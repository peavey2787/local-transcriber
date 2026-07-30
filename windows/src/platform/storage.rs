//! Windows application-data location and durable file replacement.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use super::wide_null;

pub(crate) fn app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("local-stt")
}

pub(crate) fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    let source_wide = wide_null(source.as_os_str());
    let destination_wide = wide_null(destination.as_os_str());

    // SAFETY: Both paths are valid nul-terminated UTF-16 buffers. MoveFileExW
    // performs the same-volume, write-through replacement synchronously.
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "replace {} with {}",
                destination.display(),
                source.display()
            )
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create() -> Self {
            let unique = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "local-stt-storage-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn replace_file_overwrites_an_existing_destination() {
        let directory = TestDirectory::create();
        let source = directory.0.join("config.json.part");
        let destination = directory.0.join("config.json");
        fs::write(&source, b"new").unwrap();
        fs::write(&destination, b"old").unwrap();

        replace_file(&source, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert!(!source.exists());
    }
}
