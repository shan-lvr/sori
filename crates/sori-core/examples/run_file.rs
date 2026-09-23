//! cargo run -p sori-core --example run_file -- <wav> <dictate|translate|ask> [app bundle id] [selected text]
use sori_core::{pipeline, settings::Settings, Context, Mode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let wav = std::fs::read(&args[1])?;
    let mut s = Settings::default();
    s.elevenlabs_api_key = std::env::var("ELEVENLABS_API_KEY")?;
    s.openrouter_api_key = std::env::var("OPENROUTER_API_KEY")?;
    let mode = match args.get(2).map(String::as_str).unwrap_or("dictate") {
        "translate" => Mode::Translate { target: s.active_target() },
        "ask" => Mode::Ask,
        _ => Mode::Dictate,
    };
    let ctx = Context {
        app_name: "Test".into(),
        bundle_id: args.get(3).cloned().unwrap_or_default(),
        field_focused: Some(true),
        selected_text: args.get(4).cloned(),
        selection_editable: args.get(4).is_some(),
        ..Default::default()
    };
    let http = reqwest::Client::new();
    let dict = vec!["ElevenLabs".to_string(), "OpenRouter".to_string(), "Scribe".to_string()];
    let t = std::time::Instant::now();
    let o = pipeline::run(&http, &sori_core::llm::OpenRouter { http: &http, api_key: &s.openrouter_api_key }, &s, &dict, wav, &mode, &ctx).await?;
    println!("RAW   : {}\nOUTPUT: {}\nACTION: {:?} url={:?}\nstt={}ms llm={}ms total={}ms fallback={:?}",
        o.raw, o.output, o.action, o.url, o.stt_ms, o.llm_ms, t.elapsed().as_millis(), o.fallback_reason);
    Ok(())
}
