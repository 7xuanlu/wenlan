// SPDX-License-Identifier: Apache-2.0
//! Human-facing setup/status commands for the Origin runtime.

use clap::{Subcommand, ValueEnum};
use std::io::{self, Write};
use wenlan_core::{config, on_device_models};

use crate::client::origin_host_from_env;

#[derive(Clone, Debug)]
pub struct SetupArgs {
    pub basic: bool,
    pub model: Option<String>,
    pub anthropic_api_key_env: Option<String>,
    pub yes: bool,
}

#[derive(Subcommand)]
pub enum ModelCommand {
    /// List local models Origin can download and run.
    List,
    /// Show selected/downloaded local model state.
    Status,
    /// Download and select a local model.
    Install {
        /// Model id, for example qwen3-4b.
        model_id: Option<String>,
        /// Skip confirmation before downloading.
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum KeyCommand {
    /// Show API key status.
    Status,
    /// Store an API key.
    Set {
        /// Provider to configure.
        provider: KeyProvider,
        /// Read the key from this environment variable instead of prompting.
        #[arg(long = "env", value_name = "ENV_VAR")]
        env_var: Option<String>,
    },
    /// Clear a stored API key.
    Clear {
        /// Provider to clear.
        provider: KeyProvider,
    },
}

#[derive(Clone, Debug, ValueEnum)]
pub enum KeyProvider {
    Anthropic,
}

pub async fn run_setup(args: SetupArgs) -> anyhow::Result<()> {
    if args.basic {
        configure_basic_memory()?;
        println!("Origin is set up for local memory.");
        println!("Storage, search, recall, and MCP memory work without a local model or API key.");
        println!("Distill cycles stay off until you choose a local model or Anthropic key.");
        return Ok(());
    }

    if let Some(model_id) = args.model {
        install_model(&model_id, args.yes).await?;
        mark_setup_completed()?;
        return Ok(());
    }

    if let Some(env_name) = args.anthropic_api_key_env {
        let key = std::env::var(&env_name)
            .map_err(|_| anyhow::anyhow!("environment variable {} is not set", env_name))?;
        set_anthropic_key(key).await?;
        mark_setup_completed()?;
        return Ok(());
    }

    interactive_setup().await
}

pub async fn run_model(command: ModelCommand) -> anyhow::Result<()> {
    match command {
        ModelCommand::List => {
            print_model_list();
            Ok(())
        }
        ModelCommand::Status => {
            print_model_status();
            Ok(())
        }
        ModelCommand::Install { model_id, yes } => {
            let id = model_id.unwrap_or_else(|| on_device_models::get_default_model().id.into());
            install_model(&id, yes).await
        }
    }
}

pub async fn run_key(command: KeyCommand) -> anyhow::Result<()> {
    match command {
        KeyCommand::Status => {
            print_key_status();
            Ok(())
        }
        KeyCommand::Set {
            provider: KeyProvider::Anthropic,
            env_var,
        } => {
            let key = match env_var {
                Some(name) => std::env::var(&name)
                    .map_err(|_| anyhow::anyhow!("environment variable {} is not set", name))?,
                None => prompt_secret("Anthropic API key: ")?,
            };
            set_anthropic_key(key).await
        }
        KeyCommand::Clear {
            provider: KeyProvider::Anthropic,
        } => clear_anthropic_key().await,
    }
}

pub async fn run_doctor() -> anyhow::Result<()> {
    println!("Origin doctor");
    println!();
    print_daemon_health().await;
    print_key_status();
    print_model_status();

    let cfg = config::load_config();
    let has_key = cfg
        .anthropic_api_key
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let has_cached_model = configured_model()
        .map(on_device_models::is_cached)
        .unwrap_or(false);

    println!();
    if has_key || has_cached_model {
        println!("Distill cycles: ready for richer extraction and page synthesis.");
    } else {
        println!("Distill cycles: off until you choose a local model or Anthropic key.");
        println!("  Run: origin model install");
        println!("  Or:  origin key set anthropic");
    }

    let cwd = std::env::current_dir()?;
    print_space_resolution(&cwd);

    Ok(())
}

pub async fn print_runtime_status() -> anyhow::Result<()> {
    print_key_status();
    print_model_status();
    Ok(())
}

async fn interactive_setup() -> anyhow::Result<()> {
    println!("Set up Origin");
    println!();
    println!("1) Local Memory");
    println!("   Store, search, recall, and MCP memory. No local model or API key.");
    println!("2) Local Model");
    println!("   Download a local model for private distill cycles.");
    println!("3) Anthropic Key");
    println!("   Use your Anthropic API key for stronger distill cycles. Memory stays local.");
    println!();

    let choice = prompt_line("Choose 1, 2, or 3 [1]: ")?;
    match choice.trim() {
        "" | "1" => {
            configure_basic_memory()?;
            println!("Origin is set up for local memory.");
            Ok(())
        }
        "2" => {
            let default = on_device_models::get_default_model();
            print_model_list();
            let input = prompt_line(&format!("Model id [{}]: ", default.id))?;
            let model_id = if input.trim().is_empty() {
                default.id
            } else {
                input.trim()
            };
            install_model(model_id, false).await?;
            mark_setup_completed()
        }
        "3" => {
            let key = prompt_secret("Anthropic API key: ")?;
            set_anthropic_key(key).await?;
            mark_setup_completed()
        }
        other => Err(anyhow::anyhow!("unknown setup choice: {}", other)),
    }
}

fn print_model_list() {
    let cfg = config::load_config();
    let selected = cfg
        .on_device_model
        .as_deref()
        .map(|id| on_device_models::resolve_or_default(Some(id)))
        .map(|model| model.id);
    for model in on_device_models::MODELS {
        let cached = if on_device_models::is_cached(model) {
            "downloaded"
        } else {
            "not downloaded"
        };
        let marker = if Some(model.id) == selected { "*" } else { " " };
        println!(
            "{} {} ({}, {:.1}GB download, needs {:.0}GB RAM) - {}",
            marker, model.id, model.display_name, model.file_size_gb, model.ram_required_gb, cached
        );
    }
}

fn print_model_status() {
    let Some(selected) = configured_model() else {
        println!("Local model: not selected");
        return;
    };
    let cached = on_device_models::is_cached(selected);
    println!(
        "Local model: {} ({})",
        selected.id,
        if cached {
            "downloaded"
        } else {
            "not downloaded"
        }
    );
}

async fn install_model(model_id: &str, yes: bool) -> anyhow::Result<()> {
    let model = on_device_models::get_model(model_id)
        .ok_or_else(|| anyhow::anyhow!("unknown model id: {}", model_id))?;

    if !on_device_models::is_cached(model) && !yes {
        println!(
            "{} is a {:.1}GB download and needs about {:.0}GB RAM.",
            model.display_name, model.file_size_gb, model.ram_required_gb
        );
        let answer = prompt_line("Download now? [y/N]: ")?;
        if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let body = serde_json::json!({ "model_id": model.id });
    match post_json("/api/on-device-model/download", &body).await {
        Ok(_) => {
            println!("Local model downloaded and loaded: {}", model.id);
            Ok(())
        }
        Err(http_err) => {
            println!("Daemon not available for hot-load ({}).", http_err);
            println!("Downloading directly, then the daemon will load it on next start.");
            tokio::task::spawn_blocking(move || {
                wenlan_core::llm_provider::OnDeviceProvider::new_with_model(Some(model.id))
            })
            .await??;
            let mut cfg = config::load_config();
            cfg.setup_completed = true;
            cfg.on_device_model = Some(model.id.to_string());
            config::save_config(&cfg)?;
            println!("Local model ready: {}", model.id);
            Ok(())
        }
    }
}

fn print_key_status() {
    let cfg = config::load_config();
    let configured = cfg
        .anthropic_api_key
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    println!(
        "Anthropic key: {}",
        if configured {
            "configured"
        } else {
            "not configured"
        }
    );
}

async fn set_anthropic_key(key: String) -> anyhow::Result<()> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(anyhow::anyhow!("API key cannot be empty"));
    }

    let body = serde_json::json!({ "api_key": key });
    match put_json("/api/setup/anthropic-key", &body).await {
        Ok(_) => println!("Anthropic key saved and active in the running daemon."),
        Err(_) => {
            let mut cfg = config::load_config();
            cfg.setup_completed = true;
            cfg.anthropic_api_key = Some(body["api_key"].as_str().unwrap().to_string());
            config::save_config(&cfg)?;
            println!("Anthropic key saved. Start or restart the daemon to activate it.");
        }
    }
    Ok(())
}

async fn clear_anthropic_key() -> anyhow::Result<()> {
    match delete("/api/setup/anthropic-key").await {
        Ok(_) => println!("Anthropic key cleared from the running daemon."),
        Err(_) => {
            let mut cfg = config::load_config();
            cfg.anthropic_api_key = None;
            config::save_config(&cfg)?;
            println!("Anthropic key cleared. Start or restart the daemon to apply the change.");
        }
    }
    Ok(())
}

fn mark_setup_completed() -> anyhow::Result<()> {
    let mut cfg = config::load_config();
    cfg.setup_completed = true;
    config::save_config(&cfg)?;
    Ok(())
}

fn configure_basic_memory() -> anyhow::Result<()> {
    let mut cfg = config::load_config();
    cfg.setup_completed = true;
    cfg.on_device_model = None;
    cfg.anthropic_api_key = None;
    config::save_config(&cfg)?;
    Ok(())
}

fn configured_model() -> Option<&'static on_device_models::OnDeviceModel> {
    let cfg = config::load_config();
    cfg.on_device_model
        .as_deref()
        .map(|id| on_device_models::resolve_or_default(Some(id)))
}

async fn print_daemon_health() {
    let url = origin_url("/api/health");
    match reqwest::get(&url).await {
        Ok(resp) if resp.status().is_success() => println!("Daemon: running on {}", url),
        Ok(resp) => println!("Daemon: unhealthy ({})", resp.status()),
        Err(_) => println!("Daemon: not reachable on {}", url),
    }
}

fn origin_url(path: &str) -> String {
    format!("{}{}", origin_host_from_env(), path)
}

async fn post_json(path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let resp = reqwest::Client::new()
        .post(origin_url(path))
        .json(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", resp.status()));
    }
    Ok(resp.json().await?)
}

async fn put_json(path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let resp = reqwest::Client::new()
        .put(origin_url(path))
        .json(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", resp.status()));
    }
    Ok(resp.json().await?)
}

async fn delete(path: &str) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .delete(origin_url(path))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", resp.status()));
    }
    Ok(())
}

fn print_space_resolution(cwd: &std::path::Path) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "\n--- Space resolution ---");

    let env = std::env::var("WENLAN_SPACE").ok().filter(|s| !s.is_empty());
    let _ = writeln!(
        out,
        "WENLAN_SPACE env:      {}",
        env.as_deref().unwrap_or("(unset)")
    );

    let cfg = dirs::home_dir().map(|h| h.join(".wenlan/spaces.toml"));
    let cfg_exists = cfg.as_ref().map(|p| p.exists()).unwrap_or(false);
    let _ = writeln!(
        out,
        "~/.origin/spaces.toml: {}",
        if cfg_exists { "present" } else { "missing" }
    );

    let _ = writeln!(out, "cwd:                   {}", cwd.display());

    let plugin_resolver = std::env::var("CLAUDE_PLUGIN_ROOT")
        .ok()
        .map(|p| format!("{}/bin/resolve-space.sh", p));
    if let Some(p) = plugin_resolver {
        if std::path::Path::new(&p).exists() {
            let _ = writeln!(out, "Plugin resolver:       {}", p);
            let output = std::process::Command::new(&p)
                .arg("--cwd")
                .arg(cwd)
                .output();
            if let Ok(o) = output {
                let s = String::from_utf8_lossy(&o.stdout);
                let s = s.trim().replace('\t', " (from ");
                let s = if s.contains(" (from ") {
                    format!("{})", s)
                } else {
                    s.to_string()
                };
                let _ = writeln!(out, "Resolved:              {}", s);
            }
        } else {
            let _ = writeln!(out, "Plugin resolver:       not found at {}", p);
        }
    } else {
        let _ = writeln!(
            out,
            "Plugin resolver:       CLAUDE_PLUGIN_ROOT not set (running outside Claude Code)"
        );
    }
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value)
}

fn prompt_secret(prompt: &str) -> anyhow::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value);
    let _ = std::process::Command::new("stty").arg("echo").status();
    println!();
    read?;
    Ok(value)
}
