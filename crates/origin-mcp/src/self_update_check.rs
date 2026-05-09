use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const RELEASES_URL: &str = "https://api.github.com/repos/7xuanlu/origin-mcp/releases/latest";

/// Process-wide in-memory fallback for environments where on-disk cache writes
/// fail (locked-down sandboxes, missing dirs::cache_dir, etc). Without this,
/// `store_cache` would silently no-op and every invocation in the same
/// long-lived process (e.g. an MCP server hosting many sessions) would re-hit
/// the GitHub API, risking the 60-req/hr unauthenticated rate limit.
static MEMORY_FALLBACK: Mutex<Option<CacheEntry>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CacheEntry {
    latest_tag: String,
    checked_at_secs: u64,
}

fn cache_path() -> Option<PathBuf> {
    // ORIGIN_MCP_CACHE_DIR override exists so tests can point at a temp dir
    // instead of polluting the user's real cache (~/Library/Caches/origin-mcp/...).
    let base = std::env::var_os("ORIGIN_MCP_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::cache_dir().map(|d| d.join("origin-mcp")))?;
    std::fs::create_dir_all(&base).ok()?;
    Some(base.join("version-check.json"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_cache() -> Option<CacheEntry> {
    if let Some(path) = cache_path() {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry>(&bytes) {
                if now_secs().saturating_sub(entry.checked_at_secs) < CACHE_TTL.as_secs() {
                    return Some(entry);
                }
            }
        }
    }
    // Fall back to the in-memory cache if disk read failed or was stale.
    let guard = MEMORY_FALLBACK.lock().ok()?;
    let entry = guard.as_ref()?;
    if now_secs().saturating_sub(entry.checked_at_secs) < CACHE_TTL.as_secs() {
        Some(entry.clone())
    } else {
        None
    }
}

fn store_cache(entry: &CacheEntry) {
    if let Some(path) = cache_path() {
        if let Ok(bytes) = serde_json::to_vec(entry) {
            if std::fs::write(&path, bytes).is_ok() {
                return;
            }
        }
    }
    // Disk write failed (no cache_dir, read-only FS, etc) — fall back to memory.
    if let Ok(mut guard) = MEMORY_FALLBACK.lock() {
        *guard = Some(entry.clone());
    }
}

async fn fetch_latest_tag() -> Option<String> {
    let resp = reqwest::Client::new()
        .get(RELEASES_URL)
        .header(
            "User-Agent",
            concat!("origin-mcp/", env!("CARGO_PKG_VERSION")),
        )
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body["tag_name"]
        .as_str()
        .map(|s| s.trim_start_matches('v').to_string())
}

/// Check for a newer published release. Returns Some(message) if behind,
/// None otherwise. Uses a 24h on-disk cache so this never adds startup latency
/// after the first run.
pub async fn check() -> Option<String> {
    let mcp_version = env!("CARGO_PKG_VERSION");
    let mcp = Version::parse(mcp_version).ok()?;

    let latest_tag = match load_cache() {
        Some(entry) => entry.latest_tag,
        None => {
            let tag = fetch_latest_tag().await?;
            store_cache(&CacheEntry {
                latest_tag: tag.clone(),
                checked_at_secs: now_secs(),
            });
            tag
        }
    };

    let latest = Version::parse(&latest_tag).ok()?;
    if latest > mcp {
        Some(format!(
            "A newer origin-mcp is available (v{latest}, you are on v{mcp}). \
             Run `brew upgrade origin-mcp`."
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tests touch process-wide state (`ORIGIN_MCP_CACHE_DIR` env var + the
    /// resulting on-disk cache file). Cargo runs tests in parallel by default,
    /// so we serialize the disk-touching tests through this lock. The env
    /// override is per-test (set inside the lock) so each disk-test gets its
    /// own temp dir — no pollution of the user's real cache.
    static CACHE_LOCK: Mutex<()> = Mutex::new(());

    fn set_temp_cache(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("origin-mcp-test-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("ORIGIN_MCP_CACHE_DIR", &dir);
        dir
    }

    #[test]
    fn cache_path_under_user_cache_dir() {
        // No override → falls back to dirs::cache_dir().
        let _g = CACHE_LOCK.lock().unwrap();
        std::env::remove_var("ORIGIN_MCP_CACHE_DIR");
        let p = cache_path().expect("cache dir should resolve on this platform");
        assert!(p.ends_with("origin-mcp/version-check.json"), "got {p:?}");
    }

    #[test]
    fn cache_round_trip_within_ttl() {
        let _g = CACHE_LOCK.lock().unwrap();
        let dir = set_temp_cache("round-trip");
        let entry = CacheEntry {
            latest_tag: "9.9.9".to_string(),
            checked_at_secs: now_secs(),
        };
        store_cache(&entry);
        let loaded = load_cache().expect("cache should load");
        assert_eq!(loaded.latest_tag, "9.9.9");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ORIGIN_MCP_CACHE_DIR");
    }

    #[test]
    fn cache_expires_after_ttl() {
        let _g = CACHE_LOCK.lock().unwrap();
        let dir = set_temp_cache("expires");
        let entry = CacheEntry {
            latest_tag: "9.9.9".to_string(),
            checked_at_secs: now_secs().saturating_sub(CACHE_TTL.as_secs() + 60),
        };
        store_cache(&entry);
        assert!(
            load_cache().is_none(),
            "expired entry should not be returned"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ORIGIN_MCP_CACHE_DIR");
    }
}
