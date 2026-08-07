# local-stt-rs for Windows

Background speech-to-text powered by **NVIDIA Parakeet TDT 0.6B v3 INT8**
through sherpa-onnx.

Press the physical **` / ~ key** by default to start recording and press it
again to stop. The transcription is copied to the clipboard. Optional
**Ctrl+V auto-paste** returns focus to the application that was active when
recording began and pastes the result there.

## Requirements

- 64-bit Windows 10 or Windows 11
- Windows Command Prompt (`cmd.exe`)
- The stable `x86_64-pc-windows-msvc` Rust toolchain
- Visual Studio 2022 Build Tools with the Desktop development with C++ workload

The application and bundled Sherpa runtime currently target Windows x64 only.

## Install/build and run

From PowerShell at the repository root:

```powershell
.\scripts\windows\install-windows.cmd
.\scripts\windows\run-windows.cmd
```

The install script downloads the pinned Windows Sherpa/ONNX runtime, verifies its
SHA-256, validates `Cargo.lock`, and creates a release build. It does not create
a distribution package; packaging remains a separate explicit step. It never requests
administrator access. The repository uses native `.cmd` entrypoints, so Windows
PowerShell execution policy and script signatures are not involved.

## Shortcut capture

Open the tray menu and choose **Settings…**.

The default shortcut is the physical backquote/tilde key. To change it:

1. Click **Set shortcut** or **Change shortcut**.
2. Optionally hold Ctrl, Alt, or Shift.
3. Press one main key.

The application validates and registers the captured combination immediately.
Every setting is persisted as soon as it changes; there is no Save or Apply
button.

If another application owns the shortcut, local-stt remains open with recording
disabled and directs you to choose another shortcut in Settings.

Settings are stored at `%APPDATA%\local-stt\config.json`.

## Recording and result behavior

The **Recording device** list contains the current Windows recording endpoints
plus **System default**. Use **Refresh devices** after connecting or removing a
microphone. The selected endpoint is resolved again when each recording starts;
an unavailable saved endpoint falls back safely to the Windows default.

The result is always copied to the clipboard. With **Automatically paste the
transcription with Ctrl+V** enabled, local-stt uses the supported Windows
`SendInput` API to paste into the window that was focused when recording began.
Windows blocks synthetic input from a normal application into an elevated
application; clipboard copy still succeeds and local-stt reports the paste
failure.

With auto-paste disabled, the result remains editable until **Copy / Done** or
`Esc` is used.

Visual states are controlled independently:

- **Show model loading and ready notifications**
- **Show recording notification and microphone meter**
- **Show transcribing notification**
- **Show transcription result and result/error notifications**

Temporary notification duration accepts 1–60 seconds. Recording, loading, and
transcribing indicators remain visible while their operation is active. The
Settings window is independent: notifications and the recording shortcut keep
working while Settings is open, except while actively capturing a replacement
shortcut.

## Usage

| Action | Result |
|---|---|
| `` ` / ~ `` by default | Start recording |
| Same captured shortcut again | Stop and transcribe |
| Edit the result with auto-paste off | Keep the result open |
| **Copy / Done** | Copy the edited result and close it |
| `Esc` | Dismiss the result |
| Tray → Settings… | Configure microphone, shortcut, paste, and notifications |
| Tray → Quit | Exit |

Audio is decoded in live 10-second chunks, so stopping usually waits only for
the final tail.

## Model and privacy

| | |
|---|---|
| Model | sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8 |
| Runtime | sherpa-onnx, CPU |
| Sample rate | 16 kHz mono |
| Model cache | `%APPDATA%\local-stt\models\` |
| Config | `%APPDATA%\local-stt\config.json` |

The first launch downloads roughly 500 MB of model files. The archive is
accepted only when its fixed SHA-256 matches, and extraction rejects unsafe
paths and non-file entries. Audio and transcription remain local.

No microphone input stream exists while the app is idle. The stream is created
only when recording starts and is dropped before transcription begins.
Transcribed words are not written to logs.

## Architecture and quality checks

`apps/windows` is a thin native application. Shared audio, ASR, model,
configuration, command matching, transcription workflow, theme, overlay, and
Voice Commands UI live in `crates/transcriber-core` and `crates/transcriber-ui`.
Windows-specific Win32, tray, global-hotkey, paste, device-discovery, script,
window, and instance-lock adapters remain under `apps/windows/src/`.

Run the complete offline quality gate after the runtime and Cargo dependencies
have been downloaded:

```powershell
.\scripts\windows\audit-windows.cmd
```

It checks formatting, locked offline metadata, Clippy with warnings denied,
unit tests, and the release build.

## Package

```powershell
.\scripts\windows\package-windows.cmd
```

Run `install-windows.cmd` first. The package command then writes the unpacked
application to `dist\local-stt-windows-x64\` and the matching archive to
`dist\local-stt-windows-x64.zip`. The package contains the executable, required
runtime DLLs, the runtime verification receipt, and project documentation. It
does not include a redundant launcher; start `local-stt.exe` directly.

See [`SECURITY.md`](SECURITY.md) for the microphone, integrity, and network boundaries.
