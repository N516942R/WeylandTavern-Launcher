# tools Directory Guide

## Purpose
Utility scripts for managing the `vendor/WeylandTavern` submodule live here. They are referenced from `.env` and from the Windows batch wrapper at repo root.

## Update-WeylandTavern.ps1 workflow
1. Ensures it is running from the repository root and initialises the `vendor/WeylandTavern` submodule if missing.
2. Detects the requested ref type (SHA, tag, origin/<branch>, or branch name) and fetches the minimal data needed.
3. Checks out the ref – either detached, pinned to the remote commit, or as a local tracking branch depending on the arguments and `-PinExact` flag.
4. Stages the submodule pointer in the superproject and commits `chore(submodule): bump WeylandTavern to <sha>` if the ref changed.

## Related tooling
- `Update-Weyland.bat` at the repo root shells into PowerShell and calls this script with `-Ref origin/nightly -PinExact` for Windows users.
- `.pre-commit-config.yaml` enforces that commits do not accidentally modify the submodule content directly.
