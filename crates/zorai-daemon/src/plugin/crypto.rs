//! AES-256-GCM encryption for plugin credential storage.
//!
//! Provides symmetric encrypt/decrypt using a 32-byte key stored in `plugin-key`
//! within the data directory. The key file is created with 0600 permissions on Unix.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm,
};
use anyhow::{Context, Result};
use std::path::Path;

/// Name of the key file within the data directory.
const KEY_FILE_NAME: &str = "plugin-key";

/// Load an existing 32-byte key from `data_dir/plugin-key`, or create a new
/// random key file if it does not exist.
///
/// On Unix the key file is created with mode 0600 (owner read/write only).
pub fn load_or_create_key(data_dir: &Path) -> Result<[u8; 32]> {
    let key_path = data_dir.join(KEY_FILE_NAME);

    if key_path.exists() {
        let bytes = std::fs::read(&key_path)
            .with_context(|| format!("failed to read key file: {}", key_path.display()))?;
        if bytes.len() != 32 {
            anyhow::bail!(
                "key file has invalid length {} (expected 32): {}",
                bytes.len(),
                key_path.display()
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    } else {
        // Ensure parent directory exists
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create key directory: {}", parent.display()))?;
        }

        let key: [u8; 32] = rand::random();

        // Write key file with restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true).mode(0o600);
            let mut file = opts
                .open(&key_path)
                .with_context(|| format!("failed to create key file: {}", key_path.display()))?;
            std::io::Write::write_all(&mut file, &key)
                .with_context(|| format!("failed to write key file: {}", key_path.display()))?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&key_path, &key)
                .with_context(|| format!("failed to write key file: {}", key_path.display()))?;
        }

        Ok(key)
    }
}

/// Encrypt plaintext using AES-256-GCM. Returns `nonce(12 bytes) || ciphertext`.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM encryption failed: {e}"))?;
    let mut blob = Vec::with_capacity(12 + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypt a blob produced by [`encrypt`]. Expects `nonce(12 bytes) || ciphertext`.
pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < 12 {
        anyhow::bail!(
            "encrypted blob too short ({} bytes, minimum 12 for nonce)",
            blob.len()
        );
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM decryption failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key: [u8; 32] = rand::random();
        let plaintext = b"hello world";
        let blob = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &blob).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn different_nonces_produce_different_ciphertexts() {
        let key: [u8; 32] = rand::random();
        let plaintext = b"same input";
        let blob1 = encrypt(&key, plaintext).unwrap();
        let blob2 = encrypt(&key, plaintext).unwrap();
        // The nonce portion (first 12 bytes) should differ with overwhelming probability
        assert_ne!(blob1, blob2);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1: [u8; 32] = rand::random();
        let key2: [u8; 32] = rand::random();
        let blob = encrypt(&key1, b"secret").unwrap();
        let result = decrypt(&key2, &blob);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_with_truncated_blob_fails() {
        let key: [u8; 32] = rand::random();
        // Less than 12 bytes -> must fail
        let result = decrypt(&key, &[0u8; 5]);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too short"), "msg: {msg}");
    }

    #[test]
    fn load_or_create_key_creates_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let key1 = load_or_create_key(dir.path()).unwrap();
        let key2 = load_or_create_key(dir.path()).unwrap();
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[cfg(unix)]
    #[test]
    fn key_file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let _key = load_or_create_key(dir.path()).unwrap();
        let key_path = dir.path().join("plugin-key");
        let metadata = std::fs::metadata(&key_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "key file mode: {:o}", mode);
    }
}
