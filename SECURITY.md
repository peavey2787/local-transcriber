# Security and network boundary

## Microphone lifecycle

The application does not create a microphone input stream at startup. It opens
one only after the recording shortcut is pressed and drops the stream completely
when recording stops, before transcription begins. Shutdown tracks callbacks
that were already active and waits for their observed completion before moving
the recorded buffer. Idle audio is therefore not received, metered, buffered,
or retained by the application.

## Transcript handling

Recognized words are not printed to stdout, stderr, or Rust logs. Results exist
only in memory, the optional editable result window, the desktop clipboard, and
the target application when auto-paste is enabled.

## Local configuration protection

The application data directory is created with mode `0700`. Configuration is
written to a mode-`0600` staging file, flushed, and atomically renamed over the
active configuration. This avoids partially written JSON and prevents other
local users from reading the saved shortcut and notification preferences under
normal Unix permission enforcement.

## Authenticated downloads

The native Sherpa/ONNX archive is pinned to:

- Release: `1.13.4`
- Asset: `sherpa-onnx-v1.13.4-linux-x64-shared.tar.bz2`
- SHA-256: `18887dc13c7d313d0e0f6c164ed31715c27c1c2c4f71acd7c0147dc84cf02514`

`scripts/prepare-sherpa-runtime.sh` rejects any archive whose digest differs
before extraction. `.cargo/config.toml` supplies the verified project-local runtime as the default,
while installer and CI builds export its absolute path. This prevents the
dependency build script from performing its own native-runtime download.

The Parakeet model archive is pinned to:

- Asset: `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2`
- SHA-256: `5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf`

The application authenticates the completed archive before any entry is
extracted. Extraction rejects absolute paths, parent traversal, links, and
special files, and activates the model directory only after all expected files
are present.

## Dependency lock

`Cargo.lock` is committed. Installer and release builds use `--locked`, so Cargo
must use the exact dependency graph and registry checksums recorded there.

## Expected network activity

Installation may contact the configured operating-system package repositories,
rustup, crates.io/Git sources used by Cargo, and the pinned Sherpa release asset.
The first application launch may contact the pinned Parakeet model release asset.
After the model is present, first-party application code performs no HTTP
requests. Desktop integration still uses local X11, Wayland portal, and D-Bus
IPC. The single-instance lock uses a user-owned Unix-domain socket rather than a
TCP port.

## Cargo lock canonicalization

The installer permits Cargo to normalize the serialization of `Cargo.lock` only
when the exact package inventory remains unchanged. Before and after Cargo runs,
`scripts/cargo-lock-inventory.py` compares every package name, release, source
(including pinned Git commits), and registry checksum. Any package-selection
change restores the original lock and aborts installation. The installer then
requires `cargo metadata --locked` and builds with `cargo build --locked`.
