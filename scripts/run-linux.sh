#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT_DIR/target/release/local-stt-rs"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This launcher is for Linux." >&2
  exit 64
fi

if [[ ! -x "$BIN" ]]; then
  cat >&2 <<MSG
The release binary does not exist:
  $BIN

Run the installer/build script first:
  $ROOT_DIR/scripts/install-linux.sh
MSG
  exit 69
fi

export GLOBAL_HOTKEY_APP_ID="${GLOBAL_HOTKEY_APP_ID:-io.local-stt.parakeet}"
export LD_LIBRARY_PATH="$ROOT_DIR/target/release:$ROOT_DIR/target/release/deps${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$BIN" "$@"
