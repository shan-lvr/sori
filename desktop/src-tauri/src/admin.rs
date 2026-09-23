//! Admin mode: cloud engines (ElevenLabs, OpenRouter) and API keys are for the owner only.
//!
//! Without admin mode Sori is fully on-device and ignores any API keys — enforced in the backend
//! (`enforce_policy`), not just hidden in the UI. Admin mode is unlocked with the owner's
//! password, whose verifier is baked in at build time (see build.rs); the owner's default keys
//! are baked only as AES-GCM ciphertext under a key derived from that password. After a
//! successful unlock the derived key is kept in `admin.key` (0600) next to the settings, so the
//! device stays unlocked across restarts until "Lock" is pressed.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::Serialize;
use sori_core::settings::Settings;

use crate::admin_crypto::admin_crypto as ac;

#[derive(Serialize, Clone)]
pub struct AdminStatus {
    /// This build has an admin password.
    pub available: bool,
    pub unlocked: bool,
    /// This build carries the owner's (encrypted) default API keys.
    pub has_default_keys: bool,
}

pub struct Admin {
    key_path: PathBuf,
    key: Mutex<Option<[u8; 32]>>,
    failures: Mutex<u32>,
}

fn baked() -> Option<(Vec<u8>, Vec<u8>)> {
    let salt = ac::unb64(option_env!("SORI_ADMIN_SALT")?)?;
    let verifier = ac::unb64(option_env!("SORI_ADMIN_VERIFIER")?)?;
    Some((salt, verifier))
}

fn sealed_keys() -> Option<&'static str> {
    option_env!("SORI_SEALED_KEYS")
}

/// Force the owner-only options off: on-device speech + text, no cloud keys.
pub fn enforce_policy(s: &mut Settings, admin: bool) {
    if admin {
        return;
    }
    s.stt_engine = "local".into();
    s.llm_provider = "local".into();
    s.pipeline_mode = "two_step".into();
    s.elevenlabs_api_key.clear();
    s.openrouter_api_key.clear();
}

impl Admin {
    pub fn new(key_path: PathBuf) -> Self {
        let admin = Self { key_path, key: Mutex::new(None), failures: Mutex::new(0) };
        // Restore a previous unlock on this device (only if it still matches this build's password).
        if let Some(k) = std::fs::read_to_string(&admin.key_path).ok().and_then(|s| ac::unb64(&s)) {
            if let Ok(k) = <[u8; 32]>::try_from(k.as_slice()) {
                if Self::check(&k) {
                    *admin.key.lock() = Some(k);
                } else {
                    let _ = std::fs::remove_file(&admin.key_path);
                }
            }
        }
        admin
    }

    fn check(key: &[u8; 32]) -> bool {
        baked().is_some_and(|(_, verifier)| ac::same(&ac::verifier(key), &verifier))
    }

    pub fn status(&self) -> AdminStatus {
        AdminStatus { available: baked().is_some(), unlocked: self.is_unlocked(), has_default_keys: sealed_keys().is_some() }
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.lock().is_some()
    }

    /// Verify the password (~0.3 s of PBKDF2), remember the unlock on this device.
    /// Repeated failures are slowed down.
    pub fn unlock(&self, password: &str) -> Result<()> {
        let Some((salt, _)) = baked() else { bail!("Admin mode isn't available in this build") };
        let fails = *self.failures.lock();
        if fails > 0 {
            std::thread::sleep(Duration::from_millis((500 * 2u64.pow(fails.min(5))).min(15_000)));
        }
        let key = ac::derive_key(password, &salt);
        if !Self::check(&key) {
            *self.failures.lock() += 1;
            bail!("Wrong password");
        }
        *self.failures.lock() = 0;
        *self.key.lock() = Some(key);
        if let Some(dir) = self.key_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&self.key_path, ac::b64(&key))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.key_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn lock(&self) {
        *self.key.lock() = None;
        let _ = std::fs::remove_file(&self.key_path);
    }

    /// The owner's baked-in keys `(elevenlabs, openrouter)`, decryptable only while unlocked.
    pub fn default_keys(&self) -> Option<(String, String)> {
        let key = (*self.key.lock())?;
        let plain = ac::open(&key, sealed_keys()?)?;
        let v: serde_json::Value = serde_json::from_slice(&plain).ok()?;
        Some((v["elevenlabs"].as_str().unwrap_or("").to_string(), v["openrouter"].as_str().unwrap_or("").to_string()))
    }

    /// Fill empty key fields with the owner's defaults (after unlocking).
    pub fn fill_default_keys(&self, s: &mut Settings) {
        if let Some((el, or)) = self.default_keys() {
            if s.elevenlabs_api_key.trim().is_empty() {
                s.elevenlabs_api_key = el;
            }
            if s.openrouter_api_key.trim().is_empty() {
                s.openrouter_api_key = or;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ac;

    #[test]
    fn seal_roundtrip_and_wrong_password() {
        let salt = [7u8; 16];
        let key = ac::derive_key("correct horse", &salt);
        let sealed = ac::seal(&key, &[1u8; 12], br#"{"elevenlabs":"a","openrouter":"b"}"#).unwrap();
        assert_eq!(ac::open(&key, &sealed).unwrap(), br#"{"elevenlabs":"a","openrouter":"b"}"#);
        let wrong = ac::derive_key("wrong", &salt);
        assert!(ac::open(&wrong, &sealed).is_none());
        assert!(!ac::same(&ac::verifier(&key), &ac::verifier(&wrong)));
        assert!(ac::same(&ac::verifier(&key), &ac::verifier(&key)));
    }
}
