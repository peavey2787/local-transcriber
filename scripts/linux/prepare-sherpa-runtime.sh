#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNTIME_ROOT="$ROOT_DIR/.native"
CACHE_DIR="$RUNTIME_ROOT/cache"
LIB_DIR="$RUNTIME_ROOT/lib"
SHERPA_RELEASE="1.13.4"
ARCHIVE_NAME="sherpa-onnx-v${SHERPA_RELEASE}-linux-x64-shared.tar.bz2"
ARCHIVE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_RELEASE}/${ARCHIVE_NAME}"
ARCHIVE_SHA256="18887dc13c7d313d0e0f6c164ed31715c27c1c2c4f71acd7c0147dc84cf02514"
ARCHIVE_PATH="$CACHE_DIR/$ARCHIVE_NAME"
PART_PATH="$ARCHIVE_PATH.part"
RECEIPT_PATH="$RUNTIME_ROOT/runtime.sha256"

fail() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

for command_name in curl sha256sum tar find cp; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command is missing: $command_name"
done

[[ "$(uname -s)" == "Linux" ]] || fail "the verified native runtime is currently prepared only for Linux"
[[ "$(uname -m)" == "x86_64" ]] || fail "the verified native runtime is currently prepared only for Linux x86_64"

mkdir -p "$CACHE_DIR" "$RUNTIME_ROOT"

verify_archive() {
  [[ -f "$ARCHIVE_PATH" ]] || return 1
  printf '%s  %s\n' "$ARCHIVE_SHA256" "$ARCHIVE_PATH" | sha256sum --check --status
}

if [[ -f "$ARCHIVE_PATH" ]] && ! verify_archive; then
  printf 'Discarding native runtime archive with the wrong SHA-256: %s\n' "$ARCHIVE_PATH" >&2
  rm -f -- "$ARCHIVE_PATH"
fi

if [[ ! -f "$ARCHIVE_PATH" ]]; then
  rm -f -- "$PART_PATH"
  printf 'Downloading verified Sherpa/ONNX native runtime:\n  %s\n' "$ARCHIVE_URL"
  curl \
    --fail \
    --location \
    --proto '=https' \
    --tlsv1.2 \
    --retry 3 \
    --retry-all-errors \
    --output "$PART_PATH" \
    "$ARCHIVE_URL"

  printf '%s  %s\n' "$ARCHIVE_SHA256" "$PART_PATH" | sha256sum --check --status || {
    rm -f -- "$PART_PATH"
    fail "Sherpa/ONNX native runtime SHA-256 verification failed"
  }
  mv -- "$PART_PATH" "$ARCHIVE_PATH"
fi

verify_archive || fail "cached Sherpa/ONNX native runtime failed SHA-256 verification"

TEMP_DIR="$(mktemp -d "$RUNTIME_ROOT/extract.XXXXXX")"
cleanup() {
  rm -rf -- "$TEMP_DIR"
}
trap cleanup EXIT

tar -xjf "$ARCHIVE_PATH" -C "$TEMP_DIR"

C_API_LIBRARY="$(find "$TEMP_DIR" -type f -name 'libsherpa-onnx-c-api.so*' -print -quit)"
[[ -n "$C_API_LIBRARY" ]] || fail "verified Sherpa archive did not contain libsherpa-onnx-c-api.so"
SOURCE_LIB_DIR="$(dirname -- "$C_API_LIBRARY")"
find "$SOURCE_LIB_DIR" -maxdepth 1 \( -type f -o -type l \) -name 'libonnxruntime.so*' -print -quit | grep -q . \
  || fail "verified Sherpa archive did not contain libonnxruntime.so"

NEW_LIB_DIR="$RUNTIME_ROOT/lib.new"
rm -rf -- "$NEW_LIB_DIR"
mkdir -p "$NEW_LIB_DIR"
cp -a "$SOURCE_LIB_DIR"/. "$NEW_LIB_DIR"/
rm -rf -- "$LIB_DIR"
mv -- "$NEW_LIB_DIR" "$LIB_DIR"

cat > "$RECEIPT_PATH" <<EOF
archive=$ARCHIVE_NAME
sha256=$ARCHIVE_SHA256
source=$ARCHIVE_URL
lib_dir=$LIB_DIR
EOF

printf 'Verified Sherpa/ONNX runtime ready at: %s\n' "$LIB_DIR"
printf 'SHA-256: %s\n' "$ARCHIVE_SHA256"
