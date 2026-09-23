//! Compare developer mode off vs on over real audio.
//! cargo run -p sori-core --example dev_eval -- a.wav b.wav …   (keys from env)
use sori_core::{pipeline, settings::Settings, Context, Mode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();
    let mut base = Settings::default();
    base.elevenlabs_api_key = std::env::var("ELEVENLABS_API_KEY")?;
    base.openrouter_api_key = std::env::var("OPENROUTER_API_KEY")?;
    let ctx = Context { app_name: "Slack".into(), bundle_id: "com.tinyspeck.slackmacgap".into(), ..Default::default() };
    for path in std::env::args().skip(1) {
        let wav = std::fs::read(&path)?;
        println!("\n=== {path}");
        for dev in [false, true] {
            let mut s = base.clone();
            s.dev_mode = dev;
            let t = std::time::Instant::now();
            let (stt, stt_ms) = pipeline::transcribe(&http, &s, &[], wav.clone()).await?;
            let p = pipeline::process_text(&sori_core::llm::OpenRouter { http: &http, api_key: &s.openrouter_api_key }, &s, &[], &stt.text, &Mode::Dictate, &ctx).await?;
            println!("[dev {}] STT {stt_ms}ms: {}\n          OUT {}ms: {}  (total {}ms)",
                if dev { "ON " } else { "OFF" }, stt.text, p.llm_ms, p.output.replace('\n', " / "), t.elapsed().as_millis());
        }
    }
    Ok(())
}
