#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_DIR="$ROOT_DIR/target/release"
BINARY="$RELEASE_DIR/local-stt-rs"
OUT_DIR="$ROOT_DIR/dist/local-stt-linux-x86_64"
ARCHIVE="$ROOT_DIR/dist/local-stt-linux-x86_64.tar.gz"

if [[ ! -x "$BINARY" ]]; then
  echo "Missing $BINARY — run scripts/install-linux.sh first." >&2
  exit 69
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/lib"
install -m 0755 "$BINARY" "$OUT_DIR/local-stt"

library_count=0
while IFS= read -r -d '' library; do
  cp -L "$library" "$OUT_DIR/lib/$(basename "$library")"
  ((library_count += 1))
done < <(
  find "$RELEASE_DIR" -maxdepth 8 \
    \( -type f -o -type l \) \
    \( -name 'libonnxruntime*.so*' -o -name 'libsherpa-onnx*.so*' -o -name 'libsherpa_onnx*.so*' \) \
    -print0 2>/dev/null
)

if ((library_count == 0)); then
  cat >&2 <<MSG
No sherpa-onnx or ONNX Runtime shared libraries were found under:
  $RELEASE_DIR

The release binary was built, but a portable package cannot be created without
its native runtime libraries. Check the sherpa-onnx build output and try again.
MSG
  exit 70
fi

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

Default hotkey: physical ` / ~ key
Tray -> Settings: change hotkey, enable/disable Ctrl+V auto-paste, and visual notifications.

Required Linux runtime libraries include GTK 3, AppIndicator/Ayatana AppIndicator,
ALSA, X11/Wayland libraries, and libxdo. Run the source project's
scripts/install-linux.sh on Debian/Ubuntu/Devuan to install them.

First run downloads the Parakeet INT8 model to ~/.local-stt/models/.
Audio is processed locally.
TXT

mkdir -p "$ROOT_DIR/dist"
rm -f "$ARCHIVE"
tar -C "$ROOT_DIR/dist" -czf "$ARCHIVE" "$(basename "$OUT_DIR")"
sha256sum "$ARCHIVE"
printf 'Packed: %s\n' "$ARCHIVE"
