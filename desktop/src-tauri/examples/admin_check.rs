//! Check admin mode end to end against this build's baked password (never prints keys).
//! SORI_ADMIN_PASSWORD=… cargo run -p sori --example admin_check -- <password-to-try>…
use sori_core::settings::Settings;
use sori_lib::admin::{enforce_policy, Admin};

fn main() {
    let dir = std::env::temp_dir().join("sori-admin-check");
    let _ = std::fs::remove_dir_all(&dir);
    let admin = Admin::new(dir.join("admin.key"));
    let st = admin.status();
    println!("available={} unlocked={} has_default_keys={}", st.available, st.unlocked, st.has_default_keys);
    let mut locked = Settings { stt_engine: "elevenlabs".into(), llm_provider: "openrouter".into(), openrouter_api_key: "x".into(), ..Default::default() };
    enforce_policy(&mut locked, false);
    println!("locked policy → stt={} llm={} keys_cleared={}", locked.stt_engine, locked.llm_provider, locked.openrouter_api_key.is_empty());
    for pw in std::env::args().skip(1) {
        let t = std::time::Instant::now();
        let r = admin.unlock(&pw);
        println!("unlock({}) → {:?} in {}ms", "*".repeat(pw.len()), r.as_ref().map(|_| "ok").map_err(|e| e.to_string()), t.elapsed().as_millis());
    }
    if let Some((el, or)) = admin.default_keys() {
        println!("default keys decrypted: elevenlabs {} chars, openrouter {} chars", el.len(), or.len());
    } else {
        println!("default keys: none (locked or not baked)");
    }
    let again = Admin::new(dir.join("admin.key"));
    println!("after restart: unlocked={}", again.is_unlocked());
    again.lock();
    println!("after lock: unlocked={} key file exists={}", again.is_unlocked(), dir.join("admin.key").exists());
}
