# Launcher/src Overview

## Purpose
This folder contains the React UI rendered inside the Tauri window. It collects user consent for updates, coordinates optional character synchronization, and ultimately hosts the SillyTavern web client once the backend server is ready.

## Entry points
- `main.tsx` sets up the React root and renders `<App />` inside `React.StrictMode`.
- `App.tsx` implements the entire flow:
  - Registers listeners for `server-ready` and `log` events from the Rust backend so it can swap to an `<iframe>` view and stream runtime logs.
  - Maintains step state (`updatePrompt` → `launching`), update/character results, and error messaging. It decides when to call backend commands such as `update_vendor`, `finalize_stash`, `run_character_sync`, and `start_server` via Tauri's `invoke` API.
  - Exposes retry/skip/exit handlers and keyboard shortcuts (Ctrl+R reload, Ctrl+L toggle logs, Ctrl+Q quit). A full-screen log overlay can be toggled both before and after launch.
  - Once `server-ready` fires, renders an `<iframe>` pointing at the SillyTavern URL while optionally overlaying live logs for diagnostics.

## Implementation notes
- All UI state is local to the `App` component; there is no global state manager. This keeps the flow deterministic and easy to trace.
- Async backend calls set `isProcessing` to guard against duplicate submissions and update the current step once the promise resolves.
- Error handling ensures the user can either retry (with stash) or continue despite update failures, mirroring the backend responses.
