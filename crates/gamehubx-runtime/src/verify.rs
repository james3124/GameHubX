use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

const CHUNK: usize = 64 * 1024;

pub fn sha256_hex(reader: &mut impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_reader(reader: &mut impl Read, expected_hex: &str) -> io::Result<bool> {
    let actual = sha256_hex(reader)?;
    Ok(actual.eq_ignore_ascii_case(expected_hex))
}

pub fn verify_file(path: impl AsRef<Path>, expected_hex: &str) -> io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    verify_reader(&mut file, expected_hex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn hashes_streamed_input() {
        let mut cursor = Cursor::new(b"abc");
        assert_eq!(sha256_hex(&mut cursor).unwrap(), ABC_SHA256);
    }

    #[test]
    fn verification_is_case_insensitive() {
        let mut cursor = Cursor::new(b"abc");
        assert!(verify_reader(&mut cursor, &ABC_SHA256.to_uppercase()).unwrap());

        let mut cursor = Cursor::new(b"abd");
        assert!(!verify_reader(&mut cursor, ABC_SHA256).unwrap());
    }

    #[test]
    fn verifies_a_file() {
        let path = std::env::temp_dir().join(format!(
            "gamehubx-sha-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"abc").unwrap();
        assert!(verify_file(&path, ABC_SHA256).unwrap());
        assert!(!verify_file(&path, &"0".repeat(64)).unwrap());
        let _ = std::fs::remove_file(&path);
    }
}
