//! Keeps expected ALSA/JACK probe failures out of the user's terminal.
//!
//! CPAL's ALSA discovery walks compatibility PCM aliases. Many Linux systems
//! intentionally leave some of those aliases unconfigured, and ALSA prints a
//! diagnostic for every rejected alias directly to stderr. Device discovery is
//! still allowed to fail normally; this module only silences that probe noise
//! while the iterator is consumed.

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::OpenOptions;
    use std::io::{self, Write};
    use std::os::fd::{AsRawFd, RawFd};
    use std::sync::{Mutex, MutexGuard};

    const STDERR_FD: RawFd = 2;
    static STDERR_REDIRECT_LOCK: Mutex<()> = Mutex::new(());

    extern "C" {
        fn close(fd: RawFd) -> i32;
        fn dup(fd: RawFd) -> RawFd;
        fn dup2(old_fd: RawFd, new_fd: RawFd) -> RawFd;
    }

    struct StderrRedirect {
        saved_stderr: RawFd,
        _lock: MutexGuard<'static, ()>,
    }

    impl StderrRedirect {
        fn to_dev_null() -> Option<Self> {
            let lock = STDERR_REDIRECT_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let null = OpenOptions::new().write(true).open("/dev/null").ok()?;
            let _ = io::stderr().flush();

            // SAFETY: dup and dup2 are called with valid process file
            // descriptors. The saved descriptor is restored and closed by Drop.
            let saved_stderr = unsafe { dup(STDERR_FD) };
            if saved_stderr < 0 {
                return None;
            }
            if unsafe { dup2(null.as_raw_fd(), STDERR_FD) } < 0 {
                // SAFETY: saved_stderr was returned by dup above.
                unsafe {
                    close(saved_stderr);
                }
                return None;
            }

            Some(Self {
                saved_stderr,
                _lock: lock,
            })
        }
    }

    impl Drop for StderrRedirect {
        fn drop(&mut self) {
            let _ = io::stderr().flush();
            // SAFETY: saved_stderr remains open for the lifetime of this guard.
            // Restoring fd 2 before closing the duplicate re-establishes the
            // process's original stderr destination.
            unsafe {
                dup2(self.saved_stderr, STDERR_FD);
                close(self.saved_stderr);
            }
        }
    }

    pub(super) fn during<T>(operation: impl FnOnce() -> T) -> T {
        let _redirect = StderrRedirect::to_dev_null();
        operation()
    }
}

pub(super) fn during_device_enumeration<T>(operation: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "linux")]
    {
        linux::during(operation)
    }

    #[cfg(not(target_os = "linux"))]
    {
        operation()
    }
}
