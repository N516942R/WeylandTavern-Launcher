# WeylandTavern Launcher

Tauri-based desktop launcher that wraps the bundled SillyTavern server, applies updates, and opens the UI inside a WebView.

## Prerequisites

- Node.js and npm available in `PATH`.
- Rust stable toolchain for the Tauri backend.

## Launcher workflow

1. **Vendor update prompt** – On startup the UI asks whether to run a `git pull` in the bundled WeylandTavern checkout. If the update fails, the launcher streams the contents of `WTUpdate.log`, offers a retry that stashes and overwrites local changes, and exposes a *Manage stashed changes* button so you can restore or discard any stash created during the retry.
2. **Character updater prompt** – After the vendor step you can run the optional `character-downloader.js` sync. Failures are non-fatal; the UI reports the error and lets you retry or continue to server launch.
3. **Server launch** – Once you continue, the backend performs the npm preflight according to `RUN_NPM_INSTALL`, starts `node server.js`, and waits for the health check before rendering the SillyTavern UI in an `<iframe>`. Environment variables `NO_BROWSER=1` and `BROWSER=none` are set automatically and the default CLI flags `--listen true --listenAddressIPv4 127.0.0.1 --listen-host 127.0.0.1 --browserLaunchEnabled=false --no-open` prevent the vendor script from opening an external browser.

### Update step & stash handling

- `.env` flag `ALLOW_GIT_PULL_IN_APP` controls whether the launcher is allowed to run the vendor `git pull`. Disable it if you prefer to update via the PowerShell script referenced by `UPDATE_SCRIPT`.
- `WTUpdate.log` is written to the SillyTavern directory on every failed update. The UI displays the log inline and links to the on-disk path for deeper inspection.
- If you choose to retry with overwrite, the launcher stashes local changes before pulling. After a successful pull—or after a failure with a stash present—the *Manage stashed changes* prompt lets you either `git stash pop` (restore) or `git stash clear` (discard).

### Character updater

- `RUN_CHARACTER_SYNC` toggles whether the launcher automatically offers the sync step. When run, stdout/stderr from `character-downloader.js` is streamed into the in-app log overlay.
- Failures produce a warning and present buttons to retry the sync or continue launching the server without new characters.

### Server launch & npm handling

- The npm install policy is governed by `RUN_NPM_INSTALL` (`auto` compares timestamps, `always` runs, `never` skips). `NPM_MODE` decides between `npm ci` and `npm install` when a lock file is present.
- If npm installation fails, the UI surfaces the error and asks whether to retry the install or continue launching with the existing `node_modules` (skipping npm on the next attempt).
- Runtime server logs stream to `Launcher/logs/server-YYYYMMDD.log`. Use <kbd>Ctrl</kbd>+<kbd>L</kbd> to toggle the live log overlay in the WebView.

## Logs

- **Vendor update** – `WTUpdate.log` inside `SILLYTAVERN_DIR` captures the `git pull` output and a compact diff summary. The file is overwritten on each failed update attempt.
- **Server runtime** – Logs live in `Launcher/logs/` (one file per day). These include npm output, SillyTavern startup logs, and any server-side errors.

## Configuration (`Launcher/.env`)

The repo ships with a ready-to-use `.env`. Adjust values as needed:

| Variable | Description |
| --- | --- |
| `WEYLANDTAVERN_DIR` | Path to the bundled WeylandTavern checkout. |
| `SILLYTAVERN_DIR` | Path to the SillyTavern app inside the vendor checkout. Must exist before launch. |
| `SERVER_HOST` | Hostname passed to `node server.js`. |
| `SERVER_PORT` | Preferred listening port (auto-fallback if unavailable). |
| `SERVER_ARGS` | Additional command-line flags appended to `node server.js`. Defaults to `--listen true --listenAddressIPv4 127.0.0.1 --listen-host 127.0.0.1 --browserLaunchEnabled=false --no-open`. |
| `RUN_NPM_INSTALL` | `auto`, `always`, or `never` to control npm installs. |
| `NPM_MODE` | `ci` or `install` to choose between `npm ci` and `npm install`. |
| `RUN_CHARACTER_SYNC` | `true`/`false` to offer the character updater step. |
| `ALLOW_GIT_PULL_IN_APP` | Enables in-app vendor updates when `true`; set to `false` to require the external script specified by `UPDATE_SCRIPT`. |
| `UPDATE_SCRIPT` | Path to the helper script for manual vendor updates (informational when in-app pulls are disabled). |

Optional environment variables:

- `NPM_BIN` – Override the npm executable if it is not on `PATH`.
- `SERVER_ARGS` can include additional SillyTavern switches as needed.

## Troubleshooting

- **Vendor update failures** – Review the in-app `WTUpdate.log` preview. Use *Retry with overwrite* to attempt a stashed pull, or *Manage stashed changes* to restore/discard the stash before continuing. The log also lives on disk at `<SILLYTAVERN_DIR>/WTUpdate.log`.
- **npm install failures** – The launcher reports the error and offers to retry or continue launching without reinstalling. Continuing skips npm for that attempt; if SillyTavern fails to start afterwards, rerun the launcher and retry npm.
- **Character sync failures** – The warning dialog allows you to retry the sync or continue launching SillyTavern anyway. Check the live log overlay for the underlying Node output.
- **Server health check failures** – If the health probe times out, inspect `Launcher/logs/server-*.log` or toggle the in-app log overlay (<kbd>Ctrl</kbd>+<kbd>L</kbd>) for details.

## Security

The Tauri configuration grants only the permissions required for the launcher flow:

- `shell.execute` for running git/npm/node commands inside `SILLYTAVERN_DIR`.
- `fs.readFile` for reading configuration files and logs.
- `path` for resolving application directories.
- `process.exit` for quitting the app.

A strict content security policy limits resources to `self` and connections to `http://127.0.0.1:*`.

