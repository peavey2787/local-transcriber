//! Best-effort Ctrl+V synthesis for Linux desktops.
//!
//! X11 uses xdotool and remembers the window focused when recording began.
//! Wayland tries wtype, then ydotool. Clipboard copy is always performed even
//! if a desktop compositor blocks synthetic keyboard input.

use anyhow::{bail, Context, Result};
use std::env;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct PasteTarget {
    x11_window: Option<String>,
}

impl PasteTarget {
    pub fn capture() -> Self {
        let x11_window = if command_exists("xdotool") && env::var_os("DISPLAY").is_some() {
            Command::new("xdotool")
                .arg("getactivewindow")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|window| window.trim().to_string())
                .filter(|window| !window.is_empty())
        } else {
            None
        };
        Self { x11_window }
    }

    pub fn paste_ctrl_v(&self) -> Result<&'static str> {
        // Give clipboard managers and the target application a moment to see
        // the new clipboard contents before Ctrl+V is emitted.
        thread::sleep(Duration::from_millis(100));

        if let Some(window) = &self.x11_window {
            if command_exists("xdotool") {
                let output = Command::new("xdotool")
                    .args(["key", "--window", window, "--clearmodifiers", "ctrl+v"])
                    .output()
                    .context("run xdotool")?;
                ensure_success("xdotool", output)?;
                return Ok("xdotool");
            }
        }

        let wayland = env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or_else(|_| env::var_os("WAYLAND_DISPLAY").is_some());

        if wayland && command_exists("wtype") {
            let output = Command::new("wtype")
                .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
                .output()
                .context("run wtype")?;
            ensure_success("wtype", output)?;
            return Ok("wtype");
        }

        if command_exists("xdotool") && env::var_os("DISPLAY").is_some() {
            let output = Command::new("xdotool")
                .args(["key", "--clearmodifiers", "ctrl+v"])
                .output()
                .context("run xdotool")?;
            ensure_success("xdotool", output)?;
            return Ok("xdotool");
        }

        if command_exists("ydotool") {
            // Linux input-event codes: KEY_LEFTCTRL=29, KEY_V=47.
            let output = Command::new("ydotool")
                .args(["key", "29:1", "47:1", "47:0", "29:0"])
                .output()
                .context("run ydotool")?;
            ensure_success("ydotool", output)?;
            return Ok("ydotool");
        }

        bail!(
            "no paste backend found; install xdotool for X11, or wtype/ydotool for Wayland"
        )
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", "command -v \"$1\" >/dev/null 2>&1", "sh", name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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

