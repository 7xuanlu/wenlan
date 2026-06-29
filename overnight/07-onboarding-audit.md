# Onboarding & Launch-Readiness Audit (operator-authored, from code)

Purpose: assess the "install in 60 seconds" wedge claim against the actual code, since the wedge is the
single highest-leverage move and no research agent owns the funnel. Read-only inspection of `install.sh`,
`plugin/skills/init/SKILL.md`, `plugin/skills/capture/SKILL.md`.

## The happy path is real and short

Claude Code path (README "30 seconds"):
```
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
/init
```
`/init` (plugin/skills/init/SKILL.md) is genuinely well-designed: a self-healing state machine. It probes
`/api/health`, auto-runs `install.sh` if the CLI is absent, runs `origin setup --basic && origin install`,
re-probes with a 5s retry loop, then verifies the full plugin -> MCP -> daemon round-trip via `doctor()` and
`context()`. Default backend needs no model and no API key. [VERIFIED plugin/skills/init/SKILL.md steps 1-6]

This is better onboarding than most funded dev-tools ship. That is a real asset. It is not the problem.

## Where the 60-second claim leaks (failure surfaces in the code)

These are enumerated from the code, not guessed. Each is a place a first-time user silently drops off.

1. **Daemon install requires OS service registration.** `origin install` writes a launchd plist (macOS) /
   systemd user unit (Linux) / schtasks task (Windows). [VERIFIED AGENTS.md cross-platform table]. That is a
   background service install on first run. Some users will balk; some corp laptops block it. The funnel
   converts a "try a tool" intent into a "install a daemon" commitment in step 3.

2. **Restart-in-the-middle.** README: "If Claude Code asks for a restart after installing, restart once,
   then run /init." [VERIFIED README]. Any mid-flow restart is a real drop-off point. The user has to come
   back and remember to type `/init`.

3. **macOS Tahoe Metal init can fail.** init SKILL step 3 explicitly anticipates "macOS Tahoe Metal init
   issue (daemon degrades but still binds)". [VERIFIED init SKILL]. It degrades gracefully, but a user who
   sees a scary log line may quit.

4. **Port 7878 conflict.** init SKILL step 3 names "port 7878 occupied by another process" as a likely
   failure. [VERIFIED init SKILL]. No auto-fallback port in the plugin flow; it surfaces an error and stops.

5. **PATH mutation on non-zsh/bash shells.** install.sh defaults unknown shells to `~/.zshrc` with a warn.
   [VERIFIED install.sh add_to_path]. fish/nushell users get PATH silently not set; `origin` not found next
   shell.

6. **Unsigned binary / quarantine.** install.sh runs `xattr -cr` to clear macOS quarantine [VERIFIED
   install.sh], which helps, but Gatekeeper on a fresh download of unsigned binaries is still a known scare
   surface for non-technical users.

7. **The MCP-only path is longer and manual.** Codex/Cursor/etc. users run `npx -y @7xuanlu/origin setup`
   then `~/.origin/bin/origin mcp add <client>` then hand-edit JSON. [VERIFIED README MCP-only section]. More
   steps, more abandonment.

## The capture verb has hidden complexity

`/capture` (plugin/skills/capture/SKILL.md) parses an inline `space:<name>` token with grep/sed, calls a
bundled `resolve-space.sh` resolver with a 6-layer chain, prints "Resolved space: X (from layer)" before
every capture. [VERIFIED capture SKILL]. This is powerful but it is surface the first-time user did not ask
for. The first capture should feel like one frictionless action; instead it narrates space resolution. For a
wedge, the first 3 captures should hide all of this.

## Verdict

- The build quality of onboarding is high. The claim "install in 60 seconds" is true on the macOS + Claude
  Code happy path and false-ish everywhere else.
- The bottleneck is NOT onboarding polish. It is that almost nobody is reaching the funnel at all (see
  03-footprint.md). Polishing failure surface #5 before getting 20 people to the top of the funnel is more of
  the same inward work.
- **The one onboarding change worth making before launch:** a hosted "does my setup work?" path that does not
  require installing a background daemon to feel value. Even a 30-second web demo of the capture -> page ->
  recall loop, so the daemon install is a *second* commitment after the user already wants it. Reduce the
  step-3 cliff.

## VERIFICATION
- Checked: read install.sh in full, init/SKILL.md in full, capture/SKILL.md head. Every numbered failure
  surface cites the exact file that names it. PASS.
- Did NOT run the installer (Linux container, no macOS launchd, and read-only on product per run rules). The
  failure surfaces are derived from the code's own error-handling branches, which is strong evidence they are
  real paths the author already hit. [INFERRED from defensive code that these failures occurred in practice]
- Limitation: I cannot measure actual drop-off rates without analytics. The funnel has no telemetry visible
  in the repo, so the author likely cannot either. That absence is itself a finding: he is optimizing a funnel
  he cannot measure.
