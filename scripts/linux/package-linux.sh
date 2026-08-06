#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
RELEASE_DIR="$ROOT_DIR/target/release"
BINARY="$RELEASE_DIR/local-stt-rs"
VERIFIED_LIB_DIR="$ROOT_DIR/.native/lib"
RECEIPT="$ROOT_DIR/.native/runtime.sha256"
OUT_DIR="$ROOT_DIR/dist/local-stt-linux-x86_64"
ARCHIVE="$ROOT_DIR/dist/local-stt-linux-x86_64.tar.gz"

if [[ ! -x "$BINARY" ]]; then
  echo "Missing $BINARY — run scripts/linux/install-linux.sh first." >&2
  exit 69
fi
# Re-authenticate and reconstruct the bundled native libraries from the pinned
# release archive immediately before packaging.
"$ROOT_DIR/scripts/linux/prepare-sherpa-runtime.sh"
if [[ ! -d "$VERIFIED_LIB_DIR" || ! -f "$RECEIPT" ]]; then
  echo "Missing the verified native runtime — run scripts/linux/prepare-sherpa-runtime.sh first." >&2
  exit 70
fi
find "$VERIFIED_LIB_DIR" -maxdepth 1 \( -type f -o -type l \) -name 'libsherpa-onnx-c-api.so*' -print -quit | grep -q . \
  || { echo "Verified runtime is missing libsherpa-onnx-c-api.so." >&2; exit 70; }
find "$VERIFIED_LIB_DIR" -maxdepth 1 \( -type f -o -type l \) -name 'libonnxruntime.so*' -print -quit | grep -q . \
  || { echo "Verified runtime is missing libonnxruntime.so." >&2; exit 70; }

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/lib"
install -m 0755 "$BINARY" "$OUT_DIR/local-stt"
cp -a "$VERIFIED_LIB_DIR"/. "$OUT_DIR/lib"/
install -m 0644 "$RECEIPT" "$OUT_DIR/native-runtime.sha256"

cat > "$OUT_DIR/run.sh" <<'RUN'
#!/usr/bin/env bash
set -Eeuo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export GLOBAL_HOTKEY_APP_ID="${GLOBAL_HOTKEY_APP_ID:-io.local-stt.parakeet}"
export LD_LIBRARY_PATH="$HERE/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/local-stt" "$@"
RUN
chmod +x "$OUT_DIR/run.sh"

cat > "$OUT_DIR/README.txt" <<'TXT'
local-stt for Linux
===================

Run: ./run.sh

No microphone input stream exists while idle. Capture is opened only from recording
start until recording stop, then closed before transcription. Transcription text is
never printed to terminal logs.

Default hotkey: physical ` / ~ key
Tray -> Settings: change hotkey, enable/disable Ctrl+V auto-paste, and visual notifications.

The included Sherpa/ONNX libraries came from a SHA-256-verified v1.13.4 release
archive. The verification receipt is native-runtime.sha256.

First run downloads the Parakeet INT8 model to ~/.local-stt/models/ and verifies
its fixed SHA-256 before extraction. Audio and transcription stay local.
TXT

mkdir -p "$ROOT_DIR/dist"
rm -f "$ARCHIVE"
tar -C "$ROOT_DIR/dist" -czf "$ARCHIVE" "$(basename "$OUT_DIR")"
sha256sum "$ARCHIVE"
printf 'Packed: %s\n' "$ARCHIVE"
