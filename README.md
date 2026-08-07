# local-transcriber

Local, CPU-based speech-to-text for desktop Linux and Windows.

The repository is a Cargo workspace. Platform-independent transcription and UI
behavior lives in shared crates; Linux and Windows applications contain only
native adapters and lifecycle integration.

```text
local-transcriber/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── transcriber-core/         # audio, ASR, model, config, commands, workflow
│   └── transcriber-ui/           # shared egui theme, overlay, shortcuts, editors
├── apps/
│   ├── linux/                    # GTK/tray/hotkey/paste/script platform adapters
│   └── windows/                  # Win32/tray/hotkey/paste/script platform adapters
└── scripts/
    ├── linux/
    └── windows/
```

## Architecture

`transcriber-core` owns reusable behavior:

- microphone buffering, recorder lifecycle, silence trimming, and resampling
- Sherpa/Parakeet recognizer setup and transcription
- model download, checksum verification, and extraction
- the configuration schema, normalization, and storage workflow
- voice-command aliases, validation, matching, and ordered execution
- dual-hotkey registration policy and shortcut identity helpers
- recording-session state, chunk ordering, and the bounded transcription worker
- pure microphone-selection identity and result-delivery decisions
- a narrow `TranscriberCore` orchestration facade

Pure helpers remain directly callable, including
`transcriber_core::audio::resample_linear`,
`transcriber_core::commands::normalize_phrase`, and
`transcriber_core::config::save`.

`transcriber-ui` owns shared egui presentation:

- application theme and microphone tray-icon rendering
- notification/recording/result overlay and result-delivery workflow
- Settings form, complete Settings panel, and shortcut capture
- Voice Commands editor
- tray status/action models and menu text

The platform applications supply focused adapters for native recording-device
discovery, global hotkeys, clipboard/input injection, tray integration, file
selection, script launchers, instance locking, app-data paths, and native window
behavior. The workspace root is intentionally virtual and has no ambiguous
`src/` application.

## Linux

```bash
./scripts/linux/install-linux.sh
./scripts/linux/run-linux.sh
```

See [`apps/linux/README.md`](apps/linux/README.md) for dependencies, desktop
session notes, auditing, and packaging.

## Windows

From PowerShell:

```powershell
.\scripts\windows\install-windows.cmd
.\scripts\windows\run-windows.cmd
```

See [`apps/windows/README.md`](apps/windows/README.md) for Windows requirements,
auditing, and packaging.

## Workspace quality gates

Linux:

```bash
./scripts/linux/audit-linux.sh
```

Windows:

```powershell
.\scripts\windows\audit-windows.cmd
```

Both gates format the entire workspace and run Clippy, tests, and a release build
for the shared crates plus the selected platform application using the single
root `Cargo.lock`.

## Credits and acknowledgments

This project is directly derived from and inspired by the work of
[FirePheonix](https://github.com/FirePheonix), especially
[parakeet-tdt-v3-CPU-optimized](https://github.com/FirePheonix/parakeet-tdt-v3-CPU-optimized),
and the associated architecture write-up by Shubham.
