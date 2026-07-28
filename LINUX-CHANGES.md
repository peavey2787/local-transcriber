- Rebuilt `Cargo.lock` as one coherent dependency graph; removed duplicate semver-compatible patch selections that caused `cargo build --locked` to fail (including `crossbeam-channel` 0.5.15/0.5.16).
- Installer now runs `cargo metadata --locked` before downloading the native runtime, so lock-resolution defects fail immediately.

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

- Closed microphone lifecycle: no CPAL input stream exists while idle; it is created only for active recording and dropped before transcription.
- Removed successful transcript contents from terminal and application logs; only non-content completion metadata is logged.
- Added and committed `Cargo.lock`; installer and release builds use `cargo ... --locked`.
- Added fixed SHA-256 authentication for the Parakeet model archive before safe extraction.
- Added a dedicated native-runtime preparation script that authenticates the pinned Sherpa/ONNX archive and builds against its local libraries.
- Replaced the loopback TCP single-instance lock with a permission-restricted Unix-domain socket.
- Added `SECURITY.md` and Cargo environment enforcement so manual builds cannot silently fall back to the dependency's native-runtime downloader.

- Refactored the former 909-line `app.rs` into a 13-line facade with dedicated controller, recording, transcription-worker, result, settings, and viewport modules.
- Extracted Unix-socket single-instance ownership from `main.rs`.
- Replaced thread-per-chunk transcription with one bounded recognizer worker and explicit chunk errors.
- Replaced the fixed microphone callback sleep with observed active-callback synchronization.
- Removed duplicate paste-backend process logic and direct shell-based executable discovery.
- Added `scripts/audit-linux.sh`, full CI formatting/Clippy/test gates, and `CODE-AUDIT.md`.
- Compiled the byte-oriented SHA-256 test helper only in tests, eliminating its release dead-code warning.
- Filtered only the exact known AppIndicator deprecation warning while documenting the remaining tray-backend migration.

- Added a `TranscriptionWorker` facade that owns the bounded queue, recognizer thread, lifecycle, status events, and repaint wakeups.
- Removed the unused legacy `Config.model` field while preserving compatibility with old JSON configs.
- Made configuration writes staged and atomic with mode `0600`, and protected the application data directory with mode `0700`.
- Moved the narrowly scoped AppIndicator warning filter before GTK initialization so the known third-party warning is intercepted at startup without hiding unrelated GLib warnings.

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

The installer and release workflow both replay the committed dependency lock and use only the SHA-256-authenticated native runtime prepared by `scripts/prepare-sherpa-runtime.sh`.
