# Live data in Wenlan Dev

This opt-in development window renders the current app UI against an existing
daemon through a read-only HTTP adapter. It does not start a daemon or replace
the installed Wenlan app. Native integrations are shimmed; this mode verifies
the UI and live reads, not the production IPC or updater.

Start the frontend and keep it running:

```sh
WENLAN_PREVIEW_DAEMON=http://127.0.0.1:7878 WENLAN_NO_AUTOSTART=1 pnpm dev:live:web
```

In another terminal, build the separate development window:

```sh
pnpm dev:live:build
```

On macOS, open `target/debug/bundle/macos/Wenlan Dev.app`. UI edits update through
Vite on `127.0.0.1:1424`; shell/config changes require rebuilding and reopening.
Keep the existing daemon running. Graph data refreshes every two minutes while
the graph is open, and is fetched again when entering the view.

The frontend command guard rejects mutation commands. The server proxy also
rejects writes, allowing GET/HEAD and only the enumerated search/list POST
routes in `devLivePolicy.ts`. Editing controls may still appear but writes fail
with an explicit read-only message. Use the installed app to edit knowledge.
Normal isolated development via `dev:all` is unchanged.
