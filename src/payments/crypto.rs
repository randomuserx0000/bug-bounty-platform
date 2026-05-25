//! Cifrado simétrico de los detalles sensibles de un método de pago.
//!
//! `payment_methods.details_enc` es BYTEA. Guardamos ahí el JSON de los datos
//! específicos del rail (dirección USDT, número de cuenta bancaria, etc.)
//! cifrado con ChaCha20-Poly1305 usando la key fija de la app
//! (`PAYMENT_METHODS_KEY_HEX`, 32 bytes).
//!
//! Formato del blob: `[nonce(12) || ciphertext || tag(16)]`. Sin headers ni
//! versionado todavía — si rotamos la key habrá que migrar, pero por ahora
//! no es problema porque la tabla está vacía.

use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};

const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("payment_methods_key_hex inválido: {0}")]
    BadKey(String),
    #[error("cifrado/descifrado falló")]
    Aead,
    #[error("blob demasiado corto")]
    ShortCiphertext,
}

/// Decodifica `PAYMENT_METHODS_KEY_HEX` a una key de 32 bytes.
///
/// Se llama una vez al arranque para fallar temprano si la key está mal.
pub fn key_from_hex(hex_key: &SecretString) -> Result<[u8; 32], CryptoError> {
    let bytes = hex::decode(hex_key.expose_secret().trim())
        .map_err(|e| CryptoError::BadKey(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(CryptoError::BadKey(format!(
            "se esperaban 32 bytes, llegaron {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Aead)?;

    let mut blob = Vec::with_capacity(NONCE_LEN + ct.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ct);
    Ok(blob)
}

pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < NONCE_LEN + 16 {
        return Err(CryptoError::ShortCiphertext);
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| CryptoError::Aead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = [7u8; 32];
        let blob = encrypt(&key, b"hello").unwrap();
        let out = decrypt(&key, &blob).unwrap();
        assert_eq!(out, b"hello");
    }

    #[test]
    fn tampered_blob_fails() {
        let key = [7u8; 32];
        let mut blob = encrypt(&key, b"hello").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(decrypt(&key, &blob).is_err());
    }
}
