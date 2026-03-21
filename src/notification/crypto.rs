use aes_gcm::aead::{Aead, KeyInit, OsRng, rand_core::RngCore};
use aes_gcm::{AeadCore, Aes256Gcm};
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const PBKDF2_ROUNDS: u32 = 100_000;

fn derive_key(salt: &[u8]) -> [u8; KEY_LEN] {
    let passphrase = format!(
        "{}:{}",
        hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default(),
        whoami(),
    );
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ROUNDS, &mut key);
    key
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Encrypt plaintext. Returns base64(salt[16] + nonce[12] + ciphertext + tag[16]).
pub fn encrypt(plaintext: &str) -> Result<String> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(&salt);
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(B64.encode(&blob))
}

/// Decrypt a base64-encoded blob produced by `encrypt`.
pub fn decrypt(encoded: &str) -> Result<String> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let blob = B64.decode(encoded).context("invalid base64")?;
    if blob.len() < SALT_LEN + NONCE_LEN {
        anyhow::bail!("ciphertext too short");
    }
    let salt = &blob[..SALT_LEN];
    let nonce = aes_gcm::Nonce::from_slice(&blob[SALT_LEN..SALT_LEN + NONCE_LEN]);
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];
    let key = derive_key(salt);
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(String::from_utf8(plaintext).context("invalid UTF-8")?)
}
