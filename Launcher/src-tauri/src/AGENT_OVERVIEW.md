# Launcher/src-tauri/src/main.rs Guide

## Structure
The backend is a single `main.rs` file organised around async Tauri commands plus helper utilities for git interaction, process management, and logging.

## Command handlers
- `update_vendor(app, attempt_overwrite)` – loads `.env`, resolves the SillyTavern directory, and runs `git pull` inside `vendor_dir()`. On success, reports whether the repo was already up to date; on failure it writes `SillyTavern/WTUpdate.log`, captures `git diff --compact-summary`, and either asks the UI to retry with a stash or reports a hard failure if overwrite already occurred.
- `finalize_stash(app, revert)` – runs either `git stash pop` (revert=true) or `git stash clear` against the vendor repo after an overwrite attempt, emitting log lines describing the action.
- `run_character_sync(app)` – constructs a Node command that runs `character-downloader.js <URL> -u` inside SillyTavern, forwarding stdout/stderr to the UI and reporting success/failure.
- `start_server(app, state)` – delegates to `launch`, which ensures prerequisites, optionally runs npm install, spawns `node server.js` with configured host/port/args, and waits for an HTTP health check before signalling readiness.

## Key helpers
- `load_env()` – loads `.env` from either `Launcher/.env` or repo root.
- `silly_dir()` / `vendor_dir()` – resolve configured directories and raise descriptive errors if missing.
- `run_git(dir, args)` – thin async wrapper over `tokio::process::Command` for git invocations.
- `write_update_log(log_path, pull, diff)` – saves combined `git pull` output and compact diff summary, padding blank outputs with friendly text.
- `should_npm_install(mode, dir)` – implements the `RUN_NPM_INSTALL` policy by comparing timestamps between `package-lock.json` and `node_modules` when running in `auto` mode.
- `wait_for_health(url)` – polls the SillyTavern endpoint up to 30 times with backoff via `reqwest`.
- `append_log` / `log_line` – append log lines to the current log file and emit Tauri events so the frontend can render them live.
- `shutdown(state)` – on window close, kills the spawned Node process and, on Windows, tears down the job object to avoid orphaned processes.

## Concurrency & safety
- Shared process state (child handle and Windows job object) lives inside `ServerState` guarded by `std::sync::Mutex`. File writes use `tokio::sync::Mutex` to serialise append operations.
- `tauri::async_runtime::spawn` is used for asynchronous log readers so stdout/stderr streaming continues without blocking the main command future.
