//! Single-instance ownership through a user-private Unix-domain socket.

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use crate::config;

pub(crate) struct InstanceLock {
    _listener: UnixListener,
    path: PathBuf,
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn acquire() -> Result<InstanceLock> {
    let lock_dir = config::config_dir();
    config::prepare_private_dir(&lock_dir)
        .with_context(|| format!("prepare instance-lock directory {}", lock_dir.display()))?;
    let path = lock_dir.join("local-stt.instance.sock");

    match UnixListener::bind(&path) {
        Ok(listener) => finish(listener, path),
        Err(first_error) => recover_or_report(path, first_error),
    }
}

fn recover_or_report(path: PathBuf, first_error: std::io::Error) -> Result<InstanceLock> {
    if UnixStream::connect(&path).is_ok() {
        bail!(
            "already running (another instance holds the tray lock).\n\
             Quit it from the system tray, or kill the old local-stt process."
        );
    }

    // A crashed process can leave the socket pathname behind. Never remove an
    // unexpected regular file or symbolic link from the configuration folder.
    let metadata = fs::symlink_metadata(&path).with_context(|| {
        format!(
            "inspect failed instance lock {} after bind error: {first_error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_socket() {
        bail!(
            "refusing to replace non-socket instance-lock path {}",
            path.display()
        );
    }
    fs::remove_file(&path)
        .with_context(|| format!("remove stale instance lock {}", path.display()))?;
    let listener = UnixListener::bind(&path)
        .with_context(|| format!("bind instance lock {}", path.display()))?;
    finish(listener, path)
}

fn finish(listener: UnixListener, path: PathBuf) -> Result<InstanceLock> {
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("protect instance lock {}", path.display()))?;
    Ok(InstanceLock {
        _listener: listener,
        path,
    })
}
