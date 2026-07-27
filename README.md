# local-stt-rs for Linux

Background speech-to-text powered by **NVIDIA Parakeet TDT 0.6B v3 INT8** through sherpa-onnx.

Press the physical **` / ~ key** to start recording and press it again to stop. The transcription is copied to the clipboard. Optional **Ctrl+V auto-paste** can paste it directly into the application that was focused when recording began.

The top-center visual overlay shows all important states:

- checking, downloading, extracting, loading, and warming the model;
- recording with a live microphone meter;
- transcribing;
- completion, clipboard status, and auto-paste failures.

## Linux install

On Debian, Ubuntu, Devuan, Linux Mint, and related systems:

```bash
chmod +x scripts/*.sh
./scripts/install-linux.sh
./scripts/run-linux.sh
```

The installer checks first. It asks for a sudo password only when required operating-system packages are missing, explains why, installs them, installs Rust for the current user only when Rust is absent, and builds the release binary.

`scripts/run-linux.sh` only runs the already-built application; it never installs packages or requests root access.

## Hotkey and auto-paste settings

Open the tray menu and choose **Settings…**.

The default shortcut is `Backquote`, meaning the physical key marked `` ` `` and `~`; Shift is not required. You can enter any shortcut accepted by the global-hotkey parser, including:

```text
F8
KeyR
ctrl+shift+Space
alt+KeyR
super+F9
shift+Backquote
MediaPlayPause
```

Modifiers must come before the main key. Changes are validated and applied immediately without restarting.

If the configured shortcut is already owned by another application, local-stt no longer exits. It disables and clears that shortcut, keeps the tray application running, and displays a persistent message directing you to **Tray → Settings…**. A conflicting replacement selected in Settings is also left disabled rather than silently restoring an old key.

The same Settings panel contains:

- **Automatically paste the transcription with Ctrl+V**
- **Show visual loading, recording, transcribing, and result notifications**

Settings are stored in `~/.local-stt/config.json`.

## Linux desktop support

Global shortcuts use X11 directly and the XDG GlobalShortcuts portal on supported Wayland desktops. The dependency used by this project supports X11 and Wayland. On Wayland desktops without the portal, running the app through XWayland remains a fallback.

Auto-paste uses:

- `xdotool` on X11, remembering the window focused when recording starts;
- `wtype` on Wayland when available;
- `ydotool` as a final optional fallback.

Clipboard copy still succeeds when a compositor refuses synthetic Ctrl+V; the visual result explains the auto-paste failure instead of silently losing the transcription.

## Usage

| Action | Result |
|---|---|
| `` ` / ~ `` by default | Start recording |
| Same hotkey again | Stop and transcribe |
| Edit the result text | Keeps the result open until you finish |
| **Copy / Done** | Copies the edited result to the clipboard and closes it |
| `Esc` | Dismisses the result overlay without changing the clipboard again |
| Tray → Settings… | Change hotkey, paste, and notification behavior |
| Tray → Quit | Exit |

While speaking, audio is decoded in live 10-second chunks, so stopping usually waits only for the final tail.

A completed transcription closes automatically after several seconds only when you do not interact with it. Clicking or editing the text cancels that timer. The result editor remains open until **Copy / Done** or `Esc` is used.

## Model and privacy

| | |
|---|---|
| Model | sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8 |
| Runtime | sherpa-onnx, CPU |
| Sample rate | 16 kHz mono |
| Model cache | `~/.local-stt/models/` |
| Config | `~/.local-stt/config.json` |

The first launch downloads roughly 500 MB of model files. Audio transcription runs locally; the model download is the only required network use.

## Build and package manually

```bash
cargo build --release
./scripts/package-linux.sh
```

The package is written to `dist/local-stt-linux-x86_64.tar.gz`.
