# local-stt-rs for Linux

Background speech-to-text powered by **NVIDIA Parakeet TDT 0.6B v3 INT8** through sherpa-onnx.

Press the physical **` / ~ key** by default to start recording and press it again to stop. The transcription is copied to the clipboard. Optional **Ctrl+V auto-paste** can paste it directly into the application that was focused when recording began.

## Linux install

On Debian, Ubuntu, Devuan, Linux Mint, and related systems:

```bash
chmod +x scripts/*.sh
./scripts/install-linux.sh
./scripts/run-linux.sh
```

The installer checks first. It asks for a sudo password only when required operating-system packages are missing, explains why, installs them, installs Rust for the current user only when Rust is absent, and builds the release binary.

`scripts/run-linux.sh` only runs the already-built application; it never installs packages or requests root access.

## Shortcut capture

Open the tray menu and choose **Settings…**.

The default shortcut is the physical backquote/tilde key, but the Settings window does not contain shortcut presets or a manually typed shortcut field. To change it:

1. Click **Set shortcut** or **Change shortcut**.
2. Optionally hold Ctrl, Alt, or Shift.
3. Press one main key.

The application captures the keyboard event, validates the combination, registers it immediately, and saves it automatically. Every other Settings control is also persisted as soon as it changes; there is no Save or Apply button.

If the configured shortcut is already owned by another application, local-stt does not exit. It disables and clears that shortcut, keeps the tray application running, and displays a persistent message directing you to **Tray → Settings…**. A conflicting replacement is also left disabled rather than silently restoring an old key.

Settings are stored in `~/.local-stt/config.json`.

## Recording device, auto-paste, and visual notifications

The **Recording device** drop-down lists the currently available input devices plus **System default**. Selecting a device rebuilds the idle recorder immediately and saves that choice automatically. If a saved device is unavailable at startup, local-stt safely falls back to the system default.

The Settings window also contains **Automatically paste the transcription with Ctrl+V**. The result is always copied first.

Successful auto-paste does not show the editable transcription textbox. When auto-paste is disabled, the result textbox can be edited; clicking or editing keeps it open until **Copy / Done** or `Esc` is used.

Visual states are controlled independently, so any mixture can be enabled:

- **Show model loading and ready notifications**
- **Show recording notification and microphone meter**
- **Show transcribing notification**
- **Show transcription result and result/error notifications**

The **Temporary notification duration** control accepts 1–60 seconds. It controls temporary notices and untouched transcription results; recording, loading, and transcribing indicators remain visible for the duration of their active operation.

Critical shortcut-conflict guidance remains visible because recording cannot work until another shortcut is selected.

## Linux desktop support

Global shortcuts use X11 directly and the XDG GlobalShortcuts portal on supported Wayland desktops. On Wayland desktops without portal support, running through XWayland remains a fallback.

Auto-paste uses:

- `xdotool` on X11, remembering the window focused when recording starts;
- `wtype` on Wayland when available;
- `ydotool` as a final optional fallback.

Clipboard copy still succeeds when a compositor refuses synthetic Ctrl+V, and an optional compact result/error notice explains the failure.

## Usage

| Action | Result |
|---|---|
| `` ` / ~ `` by default | Start recording |
| Same captured shortcut again | Stop and transcribe |
| Edit the result text with auto-paste off | Keeps the result open |
| **Copy / Done** with auto-paste off | Copies the edited result and closes it |
| `Esc` | Dismisses the result without changing the clipboard again |
| Tray → Settings… | Select a microphone and configure shortcut, paste, and notification behavior |
| Tray → Quit | Exit |

While speaking, audio is decoded in live 10-second chunks, so stopping usually waits only for the final tail.

## Model and privacy

| | |
|---|---|
| Model | sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8 |
| Runtime | sherpa-onnx, CPU |
| Sample rate | 16 kHz mono |
| Model cache | `~/.local-stt/models/` |
| Config | `~/.local-stt/config.json` |

The first launch downloads roughly 500 MB of model files. The download is accepted only when its fixed SHA-256 matches the expected release asset, and extraction rejects unsafe archive paths and non-file entries. Audio transcription runs locally.

No microphone input stream exists while the app is idle. The stream is created only when recording starts and is dropped completely before transcription begins. Transcribed words are not written to stdout or application logs.

## Architecture and code-quality checks

`src/app.rs` is a thin facade. The eframe composition root, recording/session
control, bounded transcription worker, result delivery, settings state,
settings application, settings UI, and viewport placement live in focused
modules under `src/app/`. Audio device discovery, callback processing, and
recorder lifecycle are separated under `src/audio/`, while single-instance
ownership lives in `src/instance_lock.rs`.

After installation, run the complete offline quality gate with:

```bash
./scripts/audit-linux.sh
```

It checks formatting, locked metadata, Clippy with warnings denied, all tests,
and the release build without permitting network access.

## Build and package manually

```bash
./scripts/prepare-sherpa-runtime.sh
SHERPA_ONNX_LIB_DIR="$PWD/.native/lib" cargo build --release --locked
./scripts/package-linux.sh
```

The package is written to `dist/local-stt-linux-x86_64.tar.gz`.

`Cargo.lock` is committed and installer and documented local builds use `--locked`. The native Sherpa/ONNX release archive is downloaded by `scripts/prepare-sherpa-runtime.sh`, checked against a fixed SHA-256 before extraction, and supplied through `SHERPA_ONNX_LIB_DIR` so the dependency build does not perform its own native-library download.

See `SECURITY.md` for the complete microphone, download-integrity, dependency-lock, and network-boundary description.
