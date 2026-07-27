# Linux conversion

This source archive replaces the Windows-only application path with a Linux-first implementation.

## Added

- Configurable global recording shortcut, defaulting to the physical backquote/tilde key.
- Immediate hotkey validation and rebinding from Tray -> Settings.
- Optional Ctrl+V auto-paste after every successful transcription.
- X11 paste targeting through `xdotool`; Wayland support through `wtype`, with `ydotool` as an optional fallback.
- Visible states for model checking, download progress, extraction, model loading, warmup, recording, transcription, completion, and errors.
- Linux GTK tray event integration.
- Debian/Ubuntu/Devuan/Mint install script that checks dependencies before requesting root access.
- Run-only launcher and Linux release packager.
- Linux GitHub release workflow.
- Nonfatal hotkey-conflict handling that clears the unavailable shortcut and keeps the tray app open for reconfiguration.
- Editable transcription results with a Copy / Done action that writes the edited text back to the clipboard.
- Interaction-aware result timeout: untouched results close automatically, while clicked or edited results remain open.
- Correctly constrained overlay layout with a wider seven-bar microphone meter.
- Expanded `.gitignore` coverage for Rust targets, packages, logs, temporary files, editor state, and crash/profiler output.
- Auto-paste completion now bypasses the editable transcript popup; only compact failure or no-speech notices remain.
- Replaced all shortcut presets and manual shortcut-string entry with live keyboard capture from a Set/Change shortcut button.
- Added physical-key normalization and live Ctrl, Alt, and Shift combination capture.
- Split visual notifications into independent loading, recording, transcribing, and result controls, including migration of the former single toggle.

## Removed

- Windows PowerShell packaging.
- MSVC-only linker configuration.
- Windows-only release workflow.
- Redundant top-level `install-linux.sh` and `run-linux.sh` wrappers; the canonical commands now live only under `scripts/`.

## Validation performed in the delivery environment

- All shell scripts pass `bash -n` parsing.
- Cargo manifest parses as TOML.
- GitHub Actions workflow parses as YAML.
- Rust source delimiters and the modified code paths were statically reviewed.

A full Rust build could not be executed in the delivery sandbox because it has no Rust toolchain or Cargo registry access. `scripts/install-linux.sh` performs the real dependency installation and release build on the target Linux machine.
