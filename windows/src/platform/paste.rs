//! Focus-preserving Ctrl+V synthesis through the current Windows input API.

use std::mem::size_of;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsWindow, SetForegroundWindow,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct PasteTarget {
    window: HWND,
}

impl PasteTarget {
    pub(crate) fn capture() -> Self {
        // SAFETY: GetForegroundWindow takes no pointers and returns either a
        // borrowed window handle or null.
        let window = unsafe { GetForegroundWindow() };
        Self { window }
    }

    pub(crate) fn paste_ctrl_v(&self) -> Result<&'static str> {
        // Give clipboard consumers a moment to observe the new contents.
        thread::sleep(Duration::from_millis(100));
        self.restore_focus()?;

        let inputs = [
            keyboard_input(VK_CONTROL, 0),
            keyboard_input(VK_V, 0),
            keyboard_input(VK_V, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];

        // SAFETY: `inputs` is a contiguous array of initialized INPUT values,
        // and its element size is passed exactly as required by SendInput.
        let inserted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if inserted != inputs.len() as u32 {
            bail!(
                "Windows accepted {inserted} of {} paste input events; the target may be running at a higher integrity level",
                inputs.len()
            );
        }

        Ok("Windows SendInput")
    }

    fn restore_focus(&self) -> Result<()> {
        if self.window.is_null() {
            bail!("Windows did not report a foreground application when recording started");
        }

        // SAFETY: The stored HWND is used only for Win32 validity and focus
        // queries; no ownership of the target window is assumed.
        if unsafe { IsWindow(self.window) } == 0 {
            bail!("the application focused when recording started has closed");
        }
        if unsafe { GetForegroundWindow() } == self.window {
            return Ok(());
        }
        if unsafe { SetForegroundWindow(self.window) } == 0 {
            bail!("Windows did not allow the original application to regain focus");
        }

        thread::sleep(Duration::from_millis(50));
        if unsafe { GetForegroundWindow() } != self.window {
            bail!("the original application did not regain focus");
        }
        Ok(())
    }
}

fn keyboard_input(virtual_key: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
