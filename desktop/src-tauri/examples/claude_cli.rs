//! Exercise the app's Claude Code CLI integration (same code as the app).
//! cargo run -p sori --example claude_cli -- status | install | bench [n]
use std::sync::Arc;
use std::time::Instant;

use sori_core::settings::Settings;
use sori_core::{llm, pipeline, Context, Mode};
use sori_lib::claude_cli::{ClaudeCli, ClaudeLlm};

const SAMPLES: &[&str] = &[
    "음 그러니까 내일 회의는 3시, 아니 4시로 하고요, 어 준비물은 첫째 노트북 둘째 발표 자료 셋째 명함이에요",
    "so um basically the deploy failed last night because the env variables were missing in staging, and uh I fixed it this morning",
    "이 컴포넌트에서 유즈 이펙트가 두 번 불리는 것 같은데 음 아마 스트릭트 모드 때문인 거 같긴 한데 리액트 쿼리로 옮기는 게 나을 것 같아",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = Arc::new(ClaudeCli::new());
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => println!("{}", serde_json::to_string_pretty(&cli.status().await)?),
        "install" => {
            let t = Instant::now();
            cli.install(|l| println!("  | {l}")).await?;
            println!("installed in {:.1}s\n{}", t.elapsed().as_secs_f32(), serde_json::to_string_pretty(&cli.status().await)?);
        }
        "bench" => {
            let n: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(2);
            let mut s = Settings::default();
            s.llm_provider = "claude_code".into();
            s.openrouter_api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
            let ctx = Context { app_name: "Slack".into(), bundle_id: "com.tinyspeck.slackmacgap".into(), field_focused: Some(true), ..Default::default() };
            let claude = ClaudeLlm { cli: cli.clone(), effort: s.claude_effort.clone() };
            // First call starts the worker (cold); later calls reuse it.
            for round in 0..n {
                for raw in SAMPLES {
                    let t = Instant::now();
                    let p = pipeline::process_text(&claude, &s, &[], raw, &Mode::Dictate, &ctx).await?;
                    println!("[claude r{round}] {}ms{} → {}", t.elapsed().as_millis(), p.fallback_reason.map(|r| format!(" [{r}]")).unwrap_or_default(), p.output.replace('\n', " / "));
                }
            }
            if !s.openrouter_api_key.is_empty() {
                let http = reqwest::Client::new();
                let mut o = s.clone();
                o.llm_provider = "openrouter".into();
                let or = llm::OpenRouter { http: &http, api_key: &o.openrouter_api_key };
                for raw in SAMPLES {
                    let t = Instant::now();
                    let p = pipeline::process_text(&or, &o, &[], raw, &Mode::Dictate, &ctx).await?;
                    println!("[openrouter {}] {}ms → {}", o.llm_model, t.elapsed().as_millis(), p.output.replace('\n', " / "));
                }
            }
        }
        other => anyhow::bail!("unknown command {other}"),
    }
    Ok(())
}
