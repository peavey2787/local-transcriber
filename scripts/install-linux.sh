#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

log() { printf '\n==> %s\n' "$*"; }
command_exists() { command -v "$1" >/dev/null 2>&1; }

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This installer is for Linux." >&2
  exit 64
fi

if ! command_exists apt-get || ! command_exists dpkg-query; then
  cat >&2 <<'MSG'
This installer currently supports Debian, Ubuntu, Devuan, Linux Mint, and
other apt-based distributions. Install the packages listed in README.md for
your distribution, install Rust with rustup, then run:
  ./scripts/prepare-sherpa-runtime.sh
  SHERPA_ONNX_LIB_DIR="$PWD/.native/lib" cargo build --release --locked
MSG
  exit 69
fi

required_packages=(
  build-essential
  pkg-config
  curl
  ca-certificates
  bzip2
  tar
  coreutils
  python3
  libssl-dev
  libasound2-dev
  libgtk-3-dev
  libxdo-dev
  libx11-dev
  libxkbcommon-dev
  libxkbcommon-x11-dev
  libwayland-dev
  libudev-dev
  libxcb-shape0-dev
  libxcb-xfixes0-dev
  libayatana-appindicator3-dev
)

missing_required=()
for package in "${required_packages[@]}"; do
  if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'ok installed'; then
    missing_required+=("$package")
  fi
done

# Auto-paste helpers are optional. xdotool handles X11; wtype handles many
# Wayland sessions. ydotool remains a manually configurable fallback.
missing_optional=()
if ! command_exists xdotool && apt-cache show xdotool >/dev/null 2>&1; then
  missing_optional+=(xdotool)
fi
if ! command_exists wtype && apt-cache show wtype >/dev/null 2>&1; then
  missing_optional+=(wtype)
fi

packages_to_install=("${missing_required[@]}" "${missing_optional[@]}")
if ((${#packages_to_install[@]})); then
  cat <<MSG
The following operating-system packages are missing:
  ${packages_to_install[*]}

Root access is needed only to install those missing packages.
Your password is requested by sudo itself; this project never reads or stores it.
MSG

  if ((EUID == 0)); then
    apt-get update
    apt-get install -y "${packages_to_install[@]}"
  else
    if ! command_exists sudo; then
      cat >&2 <<'MSG'
sudo is not installed and this shell is not running as root.
Install the listed packages as root, then run this script again.
MSG
      exit 77
    fi
    sudo apt-get update
    sudo apt-get install -y "${packages_to_install[@]}"
  fi
else
  log "All required packages and available paste helpers are installed; root access is not needed"
fi

if ! command_exists cargo || ! command_exists rustc; then
  log "Installing the Rust toolchain for the current user"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
else
  log "Rust is already installed"
fi

log "Canonicalizing and validating Cargo.lock without changing its package inventory"
# Cargo.lock is security-sensitive. Cargo may need to normalize serialization or
# dependency-reference spelling after a toolchain change, but it must never be
# allowed to silently select different packages. Capture the exact package
# identities first, let Cargo normalize the lock, then reject any change to a
# package name, release, source/Git commit, or registry checksum.
lock_backup="$(mktemp)"
inventory_before="$(mktemp)"
inventory_after="$(mktemp)"
cleanup_lock_validation() {
  rm -f -- "$lock_backup" "$inventory_before" "$inventory_after"
}
trap cleanup_lock_validation EXIT

cp -- Cargo.lock "$lock_backup"
"$ROOT_DIR/scripts/cargo-lock-inventory.py" Cargo.lock >"$inventory_before"

# Prefer a fully offline normalization when all sources are already cached.
# A first installation may need Cargo to fetch the pinned Git source and crate
# index metadata, so retry online. The inventory comparison below still forbids
# Cargo from changing any selected package.
if ! cargo metadata --offline --format-version 1 >/dev/null 2>&1; then
  cargo metadata --format-version 1 >/dev/null
fi

"$ROOT_DIR/scripts/cargo-lock-inventory.py" Cargo.lock >"$inventory_after"
if ! cmp -s "$inventory_before" "$inventory_after"; then
  cp -- "$lock_backup" Cargo.lock
  echo "ERROR: Cargo attempted to change the locked package inventory." >&2
  echo "The original Cargo.lock has been restored. Refusing a non-reproducible build." >&2
  diff -u "$inventory_before" "$inventory_after" >&2 || true
  exit 65
fi

# This is the authoritative Cargo-level check. It must succeed after the
# representation-only normalization, and every subsequent command remains locked.
cargo metadata --locked --format-version 1 >/dev/null
cleanup_lock_validation
trap - EXIT

log "Preparing the SHA-256-verified Sherpa/ONNX native runtime"
"$ROOT_DIR/scripts/prepare-sherpa-runtime.sh"

log "Building local-stt from Cargo.lock"
export SHERPA_ONNX_LIB_DIR="$ROOT_DIR/.native/lib"
if [[ ! -d "$SHERPA_ONNX_LIB_DIR" ]]; then
  echo "ERROR: verified Sherpa/ONNX library directory is missing: $SHERPA_ONNX_LIB_DIR" >&2
  exit 70
fi
printf 'Using verified Sherpa/ONNX libraries from: %s\n' "$SHERPA_ONNX_LIB_DIR"
cargo build --release --locked

cat <<MSG

Installation and build completed.
Run the application with:
  $ROOT_DIR/scripts/run-linux.sh

Default recording hotkey: the physical \` / ~ key
Open the tray menu -> Settings to change the hotkey or enable Ctrl+V auto-paste.
If another application owns that key, local-stt stays open with recording disabled
and asks you to select a different shortcut in Settings.
MSG
