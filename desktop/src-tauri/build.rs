use std::path::Path;

fn main() {
    // Bake the owner's API keys in as defaults (from the git-ignored repo-root .env.local).
    // Team builds (`SORI_TEAM_BUILD=1`, see scripts/build-team.sh) ship without keys: they
    // default to on-device speech recognition + each user's own Claude Code sign-in.
    let env_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env.local");
    println!("cargo:rerun-if-changed={}", env_file.display());
    println!("cargo:rerun-if-env-changed=SORI_TEAM_BUILD");
    let team = std::env::var("SORI_TEAM_BUILD").map(|v| v == "1").unwrap_or(false);
    if let Some(s) = (!team).then(|| std::fs::read_to_string(&env_file).ok()).flatten() {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "ELEVENLABS_API_KEY" => println!("cargo:rustc-env=SORI_DEFAULT_ELEVENLABS_KEY={}", v.trim()),
                    "OPENROUTER_API_KEY" => println!("cargo:rustc-env=SORI_DEFAULT_OPENROUTER_KEY={}", v.trim()),
                    _ => {}
                }
            }
        }
    }
    tauri_build::build()
}
