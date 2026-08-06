# Security and network boundary

## Microphone lifecycle

The application does not create a microphone input stream at startup. It opens
one only after the recording shortcut is pressed and drops the stream before
transcription begins. Shutdown waits for callbacks already in progress before
moving the recorded buffer. Idle audio is not received, metered, buffered, or
retained by the application.

## Transcript handling

Recognized words are not printed to stdout, stderr, or Rust logs. Results exist
only in memory, the optional editable result window, the Windows clipboard, and
the target application when auto-paste is enabled.

## Local configuration

Configuration and models are stored under `%APPDATA%\local-stt`, inheriting the
current Windows profile's access controls. Configuration is written to a
staging file, flushed, and replaced with the write-through `MoveFileExW` API so
an interrupted write does not leave partially serialized JSON active.

## Authenticated downloads

The native Sherpa/ONNX archive is pinned to:

- Release: `1.13.4`
- Asset: `sherpa-onnx-v1.13.4-win-x64-shared-MT-Release-lib.tar.bz2`
- SHA-256: `f923e5eacb6bca83914d89cb31afa579e11eeaff9af39f8ead82ad19f44b2c9f`

`scripts\windows\prepare-sherpa-runtime.ps1` rejects any archive whose digest differs
before extraction. Build, audit, and package scripts supply the verified
project-local runtime through `SHERPA_ONNX_LIB_DIR`, preventing the dependency
build from choosing or downloading another native archive.

The Parakeet model archive is pinned to:

- Asset: `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2`
- SHA-256: `5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf`

The application authenticates the completed archive before extraction.
Extraction rejects absolute paths, parent traversal, links, special files, and
unexpected top-level paths. The model directory becomes active only after all
expected files are present.

## Dependency lock

`Cargo.lock` is committed. Build and release scripts use `--locked`, so Cargo
must use the exact dependency graph and registry checksums in that file.

## Expected network activity

The first build may contact crates.io, the pinned GitHub Sherpa release asset,
and Rust installation services used explicitly by the developer. The first
application launch may contact the pinned GitHub Parakeet model asset. After
the model is present, first-party application code performs no HTTP requests.

Desktop integration uses local Win32 APIs. Single-instance ownership uses a
session-local named mutex rather than a file, pipe, or network port.

## Windows input boundary

Auto-paste restores the window that was focused when recording began and emits
Ctrl+V through `SendInput`. Windows User Interface Privilege Isolation can
reject input aimed at a higher-integrity process. In that case the transcript
remains on the clipboard and the application reports the paste failure.
