//! Run dictation cleanup through the app's on-device text model path (bundled llama-server).
//! cargo run --release -p sori --example local_llm -- <models_dir> [model_id]
use std::sync::Arc;
use std::time::Instant;

use sori_core::settings::Settings;
use sori_core::{pipeline, Context, Mode};
use sori_lib::local_llm::{LlmServer, LocalLlm};
use sori_lib::models::ModelStore;

const CASES: &[&str] = &[
    "음 그러니까 내일 회의는 3시, 아니 4시로 하고요, 어 준비물은 첫째 노트북 둘째 발표 자료 셋째 명함이에요",
    "so um basically the deploy failed last night because the env variables were missing in staging, and uh I fixed it this morning",
    "이 컴포넌트에서 유즈 이펙트가 두 번 불리는 것 같은데 음 아마 스트릭트 모드 때문인 거 같긴 한데 리액트 쿼리로 옮기는 게 나을 것 같아",
    "아 그 내일 회의 말인데 음 세 시로 하자고 했었나 아 아니다 네 시로 하자",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let store = Arc::new(ModelStore::new(args[1].clone().into()));
    let id = args.get(2).cloned().unwrap_or_else(|| "gemma-4-e2b".into());
    let log = std::env::temp_dir().join("sori-llama-server.log");
    let server = Arc::new(LlmServer::new(store, log.clone()));
    let t = Instant::now();
    server.ensure_running(&id).await?;
    println!("server up in {}ms (log: {})", t.elapsed().as_millis(), log.display());
    let mut s = Settings::default();
    s.llm_provider = "local".into();
    s.local_llm_model = id.clone();
    let llm = LocalLlm { server: server.clone(), model: id };
    let ctx = Context::default();
    for round in 0..2 {
        for raw in CASES {
            let p = pipeline::process_text(&llm, &s, &[], raw, &Mode::Dictate, &ctx).await?;
            println!("[r{round}] {}ms{} → {}", p.llm_ms, p.fallback_reason.map(|r| format!(" [{r}]")).unwrap_or_default(), p.output.replace('\n', " / "));
        }
    }
    let target = sori_core::settings::TranslationTarget { code: "en-US".into(), name: "English (US)".into() };
    for raw in [CASES[0], CASES[2]] {
        let p = pipeline::process_text(&llm, &s, &[], raw, &Mode::Translate { target: target.clone() }, &ctx).await?;
        println!("[translate] {}ms → {}", p.llm_ms, p.output.replace('\n', " / "));
    }
    let sel = Context { selected_text: Some("hey can u send me the report by tmrw thx".into()), selection_editable: true, field_focused: Some(true), app_name: "Slack".into(), bundle_id: "com.tinyspeck.slackmacgap".into(), ..Default::default() };
    for (raw, c) in [("이거 좀 더 정중하게 바꿔 줘", &sel), ("리액트에서 유즈 메모랑 유즈 콜백 차이가 뭐야", &ctx)] {
        match pipeline::process_text(&llm, &s, &[], raw, &Mode::Ask, c).await {
            Ok(p) => println!("[ask {:?}] {}ms → {}", p.action, p.llm_ms, p.output.chars().take(160).collect::<String>().replace('\n', " / ")),
            Err(e) => println!("[ask] ERROR {e:#}"),
        }
    }
    server.stop().await;
    Ok(())
}
