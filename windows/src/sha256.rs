//! SHA-256 helpers used to authenticate downloaded archives.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[cfg(test)]
fn digest_bytes(data: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(data);
    format!("{:x}", hash.finalize())
}

pub fn digest_file(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("open {} for SHA-256", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 256 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("read {} for SHA-256", path.display()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn verify_file(path: &Path, expected: &str) -> Result<()> {
    let actual = digest_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    anyhow::bail!(
        "SHA-256 mismatch for {}: expected {}, got {}",
        path.display(),
        expected,
        actual
    );
}

#[cfg(test)]
mod tests {
    use super::digest_bytes;

    #[test]
    fn known_sha256_vectors() {
        assert_eq!(
            digest_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
