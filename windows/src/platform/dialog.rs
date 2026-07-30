//! Native startup-error reporting for release builds without a console.

use std::ffi::OsStr;
use std::ptr;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

use super::wide_null;

pub(crate) fn show_error(title: &str, message: &str) {
    let title = wide_null(OsStr::new(title));
    let message = wide_null(OsStr::new(message));

    // SAFETY: Both strings are valid, nul-terminated UTF-16 buffers for the
    // duration of this synchronous call. A null owner creates an app-modal box.
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
