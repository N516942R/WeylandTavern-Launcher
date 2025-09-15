# WeylandTavern Launcher

Tauri based launcher that bootstraps the bundled SillyTavern server and opens it in a WebView.

## Prerequisites
- Node.js and npm available in `PATH`.
- Rust toolchain (stable) for building the Tauri side.

## `.env`
Create a `.env` file in the `Launcher/` directory with the following keys:

```
SILLYTAVERN_DIR=./vendor/WeylandTavern/SillyTavern
SERVER_HOST=127.0.0.1
SERVER_PORT=8000
SERVER_ARGS=--listen true --listen-host 0.0.0.0
RUN_NPM_INSTALL=auto   # auto|always|never
NPM_MODE=ci            # ci|install
RUN_CHARACTER_SYNC=false
UPDATE_SCRIPT=./tools/Update-WeylandTavern.ps1
```

## Usage

```sh
npm install
npm run tauri dev
```

The launcher performs a preflight check, optionally installs npm packages, starts the SillyTavern server and displays it. Logs are written to `logs/`.

## Troubleshooting
- Ensure `SILLYTAVERN_DIR` points to a valid SillyTavern checkout.
- `RUN_NPM_INSTALL=never` skips the npm step; use `auto` or `always` if dependencies are missing.
- Set `RUN_CHARACTER_SYNC=true` to invoke `character-downloader.js` (non‑fatal on failure).

