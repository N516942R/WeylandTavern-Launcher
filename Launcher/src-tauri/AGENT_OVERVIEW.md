# Launcher/src-tauri Overview

## Purpose
This is the Rust side of the launcher. Tauri loads everything from here: Cargo manifests describe dependencies, `tauri.conf.json` defines the desktop window/security posture, and `src/main.rs` implements the command handlers invoked from the React UI.

## Key files
- `Cargo.toml` – declares the crate (`weylandtavern-launcher`) and dependencies on `tauri`, `dotenvy`, `tokio`, `reqwest` (rustls build), `chrono`, `serde`, and Windows-specific `windows` APIs when compiling on Windows.
- `Cargo.lock` – locks all Rust dependencies to known versions for reproducible builds.
- `build.rs` – minimal build script delegating to `tauri_build::build()` so Tauri can embed resources.
- `tauri.conf.json` – configures the window dimensions, disables bundling by default, sets the dev/build commands, and enforces a strict CSP that only trusts `self` and `http://127.0.0.1:*` for frames and XHR.
- `icons/` – contains the application icon referenced by the bundle metadata.
- `src/` – Rust source directory housing `main.rs`.

## Runtime behaviour
- `main.rs` registers commands (`update_vendor`, `finalize_stash`, `run_character_sync`, `start_server`) exposed to the UI. Each command loads `.env` before acting so runtime overrides take effect.
- The backend supervises the spawned Node server, writes logs to `Launcher/logs/`, and broadcasts log lines/ready signals via Tauri events.
- On Windows, job objects ensure the spawned server dies with the launcher.
