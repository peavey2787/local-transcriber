#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

fail() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo is not installed"
command -v rustfmt >/dev/null 2>&1 || fail "rustfmt is not installed; run: rustup component add rustfmt"
command -v cargo-clippy >/dev/null 2>&1 || fail "clippy is not installed; run: rustup component add clippy"
[[ -d "$ROOT_DIR/.native/lib" ]] || fail "verified Sherpa runtime is missing; run scripts/linux/install-linux.sh first"

export SHERPA_ONNX_LIB_DIR="$ROOT_DIR/.native/lib"
export CARGO_NET_OFFLINE=true

printf '\n==> Formatting check\n'
cargo fmt --all -- --check

printf '\n==> Locked dependency metadata (offline)\n'
cargo metadata --locked --offline --format-version 1 >/dev/null

printf '\n==> Clippy with warnings denied\n'
cargo clippy -p transcriber-core -p transcriber-ui -p local-transcriber-linux --all-targets --locked --offline -- -D warnings

printf '\n==> Unit tests\n'
cargo test -p transcriber-core -p transcriber-ui -p local-transcriber-linux --all-targets --locked --offline

printf '\n==> Release build\n'
cargo build -p local-transcriber-linux --release --locked --offline

printf '\nAll local code-audit gates passed.\n'
