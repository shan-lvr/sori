// Admin-password crypto, shared by build.rs (via `include!`) and the app. Keep it free of
// crate-internal imports: only pbkdf2, sha2, aes-gcm and base64.
//
// key      = PBKDF2-HMAC-SHA256(password, salt, 600k rounds) → 32 bytes
// verifier = SHA-256("sori-admin-v1" ‖ key)          (baked in; proves the password)
// sealed   = AES-256-GCM(key, nonce, {"elevenlabs":…, "openrouter":…})   (personal builds only)

#[allow(dead_code)]
pub mod admin_crypto {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    pub const ROUNDS: u32 = 600_000;

    pub fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, ROUNDS, &mut key);
        key
    }

    pub fn verifier(key: &[u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"sori-admin-v1");
        h.update(key);
        h.finalize().into()
    }

    /// Constant-time comparison.
    pub fn same(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    /// `base64(nonce).base64(ciphertext)`
    pub fn seal(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8]) -> Option<String> {
        let ct = Aes256Gcm::new_from_slice(key).ok()?.encrypt(Nonce::from_slice(nonce), plaintext).ok()?;
        Some(format!("{}.{}", B64.encode(nonce), B64.encode(ct)))
    }

    pub fn open(key: &[u8; 32], sealed: &str) -> Option<Vec<u8>> {
        let (n, c) = sealed.split_once('.')?;
        let nonce = B64.decode(n).ok()?;
        let ct = B64.decode(c).ok()?;
        if nonce.len() != 12 {
            return None;
        }
        Aes256Gcm::new_from_slice(key).ok()?.decrypt(Nonce::from_slice(&nonce), ct.as_ref()).ok()
    }

    pub fn b64(bytes: &[u8]) -> String {
        B64.encode(bytes)
    }

    pub fn unb64(s: &str) -> Option<Vec<u8>> {
        B64.decode(s.trim()).ok()
    }
}
