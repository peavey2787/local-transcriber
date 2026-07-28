//! Best-effort Ctrl+V synthesis for Linux desktops.
//!
//! X11 uses xdotool and remembers the window focused when recording began.
//! Wayland tries wtype, then ydotool. Clipboard copy is always performed even
//! if a desktop compositor blocks synthetic keyboard input.

use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct PasteTarget {
    x11_window: Option<String>,
}

impl PasteTarget {
    pub fn capture() -> Self {
        Self {
            x11_window: capture_x11_window(),
        }
    }

    pub fn paste_ctrl_v(&self) -> Result<&'static str> {
        // Give clipboard managers and the target application a moment to see
        // the new clipboard contents before Ctrl+V is emitted.
        thread::sleep(Duration::from_millis(100));

        if let Some(window) = self.x11_window.as_deref() {
            if command_exists("xdotool") {
                run_backend(
                    "xdotool",
                    ["key", "--window", window, "--clearmodifiers", "ctrl+v"],
                )?;
                return Ok("xdotool");
            }
        }
        if is_wayland_session() && command_exists("wtype") {
            run_backend("wtype", ["-M", "ctrl", "-k", "v", "-m", "ctrl"])?;
            return Ok("wtype");
        }
        if env::var_os("DISPLAY").is_some() && command_exists("xdotool") {
            run_backend("xdotool", ["key", "--clearmodifiers", "ctrl+v"])?;
            return Ok("xdotool");
        }
        if command_exists("ydotool") {
            // Linux input-event codes: KEY_LEFTCTRL=29, KEY_V=47.
            run_backend("ydotool", ["key", "29:1", "47:1", "47:0", "29:0"])?;
            return Ok("ydotool");
        }

        bail!("no paste backend found; install xdotool for X11, or wtype/ydotool for Wayland")
    }
}

fn capture_x11_window() -> Option<String> {
    if !command_exists("xdotool") || env::var_os("DISPLAY").is_none() {
        return None;
    }
    Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|window| window.trim().to_string())
        .filter(|window| !window.is_empty())
}

fn is_wayland_session() -> bool {
    env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or_else(|_| env::var_os("WAYLAND_DISPLAY").is_some())
}

fn run_backend<const N: usize>(name: &str, args: [&str; N]) -> Result<()> {
    let output = Command::new(name)
        .args(args)
        .output()
        .with_context(|| format!("run {name}"))?;
    ensure_success(name, output)
}

fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| {
        fs::metadata(directory.join(name))
            .map(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
            .unwrap_or(false)
    })
}

fn ensure_success(name: &str, output: Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{name} exited with {}", output.status);
    }
    bail!("{name} failed: {stderr}")
}
