//! Optional checksum verification (MD5 / SHA-256).

use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Supported digest algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Md5,
    Sha256,
}

impl Algo {
    pub fn name(self) -> &'static str {
        match self {
            Algo::Md5 => "md5",
            Algo::Sha256 => "sha256",
        }
    }
}

/// Compute a hex digest of a file using the given algorithm, reading in chunks
/// so arbitrarily large files use constant memory.
pub fn hash_file(path: &Path, algo: Algo) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1024 * 1024];
    match algo {
        Algo::Md5 => {
            let mut h = Md5::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
        Algo::Sha256 => {
            let mut h = Sha256::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
    }
}

/// Verify a file against an expected hex digest (case-insensitive).
pub fn verify_file(path: &Path, algo: Algo, expected: &str) -> Result<()> {
    let actual = hash_file(path, algo)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(Error::Checksum {
            algo: algo.name(),
            expected: expected.trim().to_lowercase(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn known_digests() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("abc.txt");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"abc").unwrap();
        drop(f);

        // Well-known digests of the string "abc".
        assert_eq!(hash_file(&p, Algo::Md5).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hash_file(&p, Algo::Sha256).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        verify_file(&p, Algo::Sha256, "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD").unwrap();
        assert!(verify_file(&p, Algo::Md5, "deadbeef").is_err());
    }
}
