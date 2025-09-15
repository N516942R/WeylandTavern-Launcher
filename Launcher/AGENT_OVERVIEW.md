# Launcher Directory Guide

## Role
The `Launcher/` folder houses the hybrid Vite + Tauri project that wraps SillyTavern. Frontend assets, the Rust backend, and runtime configuration all live here.

## Key files
- `README.md` – setup instructions for installing Node/Rust, configuring `.env`, and running `npm install` / `npm run tauri dev`.
- `.env` – checked-in defaults for vendor paths, server host/port, npm behaviour, character sync toggle, and the PowerShell update script reference.
- `package.json` – npm scripts (`dev`, `build`, `ui:dev`, `ui:build`, `tauri`) plus dependencies on `@tauri-apps/api`, React, and the Tauri CLI/tooling.
- `index.html` – minimal HTML shell that mounts `<div id="root">` and loads `src/main.tsx`.
- `tsconfig.json` – TypeScript compiler settings targeting ES2020 modules with React JSX support and the Tauri API typings.
- `vite.config.ts` – Vite config enabling the React plugin and suppressing the default terminal clear.

## Subdirectories
- `src/` – React single-page app that hosts the onboarding/update UI and displays the SillyTavern iframe once ready.
- `src-tauri/` – Tauri project containing Cargo manifests, runtime config, and the Rust backend entry point.
- `logs/` – destination for runtime log files written by the backend. Empty except for `.gitkeep` to keep the directory in git.
- `node_modules/` – npm dependencies (ignored by git); created by `npm install` per `.gitignore`.

## Workflow reminders
- Run all npm/Tauri commands from this directory so relative paths (e.g., to the vendor repo) resolve correctly.
- Keep `.env` in sync with the actual vendor checkout and update script path; the backend loads it via `dotenvy` before executing commands.
- Logs written by the backend use a datestamped `server-YYYYMMDD.log` naming convention; check `logs/` when diagnosing launches.
