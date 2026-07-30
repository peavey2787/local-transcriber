//! Single-instance ownership through a session-local named Windows mutex.

use std::ffi::OsStr;
use std::ptr;

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

use super::wide_null;

const MUTEX_NAME: &str = r"Local\PeaveyKoding.LocalStt.Instance";

pub(crate) struct InstanceLock {
    handle: HANDLE,
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // SAFETY: `handle` is a valid mutex handle owned by this guard.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub(crate) fn acquire() -> Result<InstanceLock> {
    let name = wide_null(OsStr::new(MUTEX_NAME));

    // SAFETY: The security-attributes pointer is null, the initial-owner flag
    // is false, and `name` is a valid nul-terminated UTF-16 buffer.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error())
            .context("create the local-stt single-instance mutex");
    }

    // CreateMutexW returns a valid handle and sets this status when the named
    // mutex already exists.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: `handle` was returned successfully by CreateMutexW above.
        unsafe {
            CloseHandle(handle);
        }
        bail!(
            "local-stt is already running. Quit it from the system tray before starting another instance."
        );
    }

    Ok(InstanceLock { handle })
}
