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
  cargo build --release
MSG
  exit 69
fi

required_packages=(
  build-essential
  pkg-config
  curl
  ca-certificates
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

log "Building local-stt in release mode"
cargo build --release

cat <<MSG

Installation and build completed.
Run the application with:
  $ROOT_DIR/scripts/run-linux.sh

Default recording hotkey: the physical \` / ~ key
Open the tray menu -> Settings to change the hotkey or enable Ctrl+V auto-paste.
If another application owns that key, local-stt stays open with recording disabled
and asks you to select a different shortcut in Settings.
MSG
