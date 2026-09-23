use std::collections::HashMap;
use std::path::Path;

include!("src/admin_crypto.rs");

/// Admin mode (see src/admin.rs). The owner's password never ends up in the binary:
/// - every build with `SORI_ADMIN_PASSWORD` (from `.env.local` or the environment, e.g. a CI
///   secret) bakes only a salt + verifier, so admin mode can be unlocked with that password;
/// - personal builds (not `SORI_TEAM_BUILD=1`) also bake the owner's API keys from `.env.local`,
///   AES-256-GCM-encrypted with a key derived from the password. Nothing in plaintext.
/// Without a password, no keys are baked at all and admin mode is unavailable.
fn main() {
    let env_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env.local");
    println!("cargo:rerun-if-changed={}", env_file.display());
    println!("cargo:rerun-if-env-changed=SORI_TEAM_BUILD");
    println!("cargo:rerun-if-env-changed=SORI_ADMIN_PASSWORD");

    let mut vars: HashMap<String, String> = HashMap::new();
    if let Ok(s) = std::fs::read_to_string(&env_file) {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                vars.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
            }
        }
    }
    let team = std::env::var("SORI_TEAM_BUILD").map(|v| v == "1").unwrap_or(false);
    let password = std::env::var("SORI_ADMIN_PASSWORD").ok().filter(|p| !p.is_empty()).or_else(|| vars.get("SORI_ADMIN_PASSWORD").cloned());

    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        let mut salt = [0u8; 16];
        getrandom::getrandom(&mut salt).expect("random salt");
        let key = admin_crypto::derive_key(&pw, &salt);
        println!("cargo:rustc-env=SORI_ADMIN_SALT={}", admin_crypto::b64(&salt));
        println!("cargo:rustc-env=SORI_ADMIN_VERIFIER={}", admin_crypto::b64(&admin_crypto::verifier(&key)));

        let eleven = vars.get("ELEVENLABS_API_KEY").cloned().unwrap_or_default();
        let openrouter = vars.get("OPENROUTER_API_KEY").cloned().unwrap_or_default();
        if !team && !(eleven.is_empty() && openrouter.is_empty()) {
            let mut nonce = [0u8; 12];
            getrandom::getrandom(&mut nonce).expect("random nonce");
            let json = format!(r#"{{"elevenlabs":{:?},"openrouter":{:?}}}"#, eleven, openrouter);
            let sealed = admin_crypto::seal(&key, &nonce, json.as_bytes()).expect("seal keys");
            println!("cargo:rustc-env=SORI_SEALED_KEYS={sealed}");
        }
    }
    tauri_build::build()
}
