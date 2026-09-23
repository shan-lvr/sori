//! Two-step (STT → LLM) vs experimental one-step (audio → multimodal LLM), dev mode on.
//! cargo run -p sori-core --example onestep_eval -- a.wav b.wav …   (keys from env)
use sori_core::{pipeline, settings::Settings, Context, Mode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();
    let mut s = Settings::default();
    s.elevenlabs_api_key = std::env::var("ELEVENLABS_API_KEY")?;
    s.openrouter_api_key = std::env::var("OPENROUTER_API_KEY")?;
    let ctx = Context { app_name: "Slack".into(), bundle_id: "com.tinyspeck.slackmacgap".into(), ..Default::default() };
    for path in std::env::args().skip(1) {
        let wav = std::fs::read(&path)?;
        println!("\n=== {path}");
        for one_step in [false, true] {
            let mut s = s.clone();
            s.pipeline_mode = if one_step { "one_step".into() } else { "two_step".into() };
            let t = std::time::Instant::now();
            let o = pipeline::run(&http, &sori_core::llm::OpenRouter { http: &http, api_key: &s.openrouter_api_key }, &s, &[], wav.clone(), &Mode::Dictate, &ctx).await?;
            println!("  {} {:>5}ms  {}", if one_step { "1-step" } else { "2-step" }, t.elapsed().as_millis(), o.output.replace('\n', " / "));
        }
    }
    Ok(())
}
