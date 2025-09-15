# vendor Directory Guide

## Purpose
The `vendor/` folder houses the `WeylandTavern` git submodule – a shallow clone of the upstream SillyTavern fork pinned to the `nightly` branch. The launcher expects SillyTavern to live at `vendor/WeylandTavern/SillyTavern`.

## Usage notes
- Do not edit files inside `vendor/WeylandTavern` directly. Move the pointer by updating the submodule (via the PowerShell script, manual git commands, or the in-app update flow) and commit the resulting SHA change.
- `.pre-commit-config.yaml` contains a guard that rejects commits with staged changes inside the submodule.
- When cloning the repository, run `git submodule update --init --recursive` so the vendor tree is present before launching the app.
