# WeylandTavern Launcher

Tauri based launcher that bootstraps the bundled SillyTavern server and opens it in a WebView.

## Prerequisites
- Node.js and npm available in `PATH`.
- Rust toolchain (stable) for building the Tauri side.

## `.env`
The repo ships with `Launcher/.env` pre-populated for the bundled SillyTavern checkout. Adjust the values if your layout or
preferences differ.

```
WEYLANDTAVERN_DIR=./vendor/WeylandTavern
SILLYTAVERN_DIR=./vendor/WeylandTavern/SillyTavern
SERVER_HOST=127.0.0.1
SERVER_PORT=8000
SERVER_ARGS=--listen true --listen-host 0.0.0.0
RUN_NPM_INSTALL=auto   # auto|always|never
NPM_MODE=ci            # ci|install
RUN_CHARACTER_SYNC=true
ALLOW_GIT_PULL_IN_APP=true
UPDATE_SCRIPT=./tools/Update-WeylandTavern.ps1
```

`ALLOW_GIT_PULL_IN_APP` controls whether the launcher is allowed to run `git pull` against the vendor checkout during the
Update step. Leave this enabled to trigger updates from inside the app, or set it to `false` if you prefer to run the
PowerShell script manually.

The launcher also sets the following environment variables internally to prevent an external browser from opening:
```
NO_BROWSER=1
BROWSER=none
```

`SILLYTAVERN_DIR` defaults to `./vendor/WeylandTavern/SillyTavern` and the launcher will fail to start if the path does not
exist.

## Usage

```sh
npm install
npm run tauri dev
```

The launcher performs a preflight check, optionally installs npm packages, starts the SillyTavern server and displays it. Logs are written to `logs/`.

## Security

The Tauri configuration grants only the minimal permissions required:

- `shell.execute` for running commands in `SILLYTAVERN_DIR`
- `fs.readFile` for reading configuration and logs
- `path` for resolving application directories
- `process.exit` for quitting the app

A strict content security policy allows resources from `self` and connections to `http://127.0.0.1:*` only.

## Troubleshooting
- Ensure `SILLYTAVERN_DIR` points to a valid SillyTavern checkout.
- `RUN_NPM_INSTALL=never` skips the npm step; use `auto` or `always` if dependencies are missing.
- Leave `RUN_CHARACTER_SYNC=true` (default) to invoke `character-downloader.js` (non‑fatal on failure); set it to `false` to skip the sync.

