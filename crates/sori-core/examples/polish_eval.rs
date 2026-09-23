//! Compare cleanup styles / models on messy transcripts (text only, no audio).
//! cargo run -p sori-core --example polish_eval -- model1 model2 …   (OPENROUTER_API_KEY from env)
//! A model of the form `local:<name>@http://127.0.0.1:8080` uses a local llama-server.
use sori_core::{pipeline, settings::Settings, Context, Mode};

/// (app, bundle id, window title, raw transcript). Mostly dictation into coding agents — the
/// main use — plus two chat/email cases to make sure normal messages stay messages.
const CASES: &[(&str, &str, &str, &str)] = &[
    ("Claude", "com.anthropic.claudefordesktop", "Sori", "음 그러니까 지금 보면 이 히스토리 화면에서 복사 버튼 누르면 뭐 아무 반응이 없거든 어 그거 눌렀을 때 복사됐다는 피드백 좀 보여 주고 그 다른 버튼들도 다 제대로 동작하는지 확인해 줘"),
    ("Claude", "com.anthropic.claudefordesktop", "Sori", "아 그 빌드할 때 그 윈도우 쪽에서 불칸 에스디케이 없으면 에러 나는 거 있잖아 그거 그냥 씨피유로 폴백되게 할 수 있나 아니면 설치 가이드를 리드미에 넣어 주든가 둘 중에 뭐가 나을지 먼저 알려 줘 바로 고치지 말고"),
    ("Terminal", "com.apple.Terminal", "✳ claude — sori", "그 테스트 좀 돌려 보고 실패하는 거 있으면 고쳐 줘 근데 그 스냅샷 테스트는 건드리지 말고 어 그리고 커밋은 하지 마 내가 볼 거니까"),
    ("Claude", "com.anthropic.claudefordesktop", "Sori", "그리고 주로 이거 사용을 코딩 에이전트에 타이핑하는 대용으로 사용한다 그러니까 그 용도라는 거를 다듬는 엘엘엠에다가 프롬프팅을 그렇게 하는 게 좋을 것 같아 가끔씩 잘못 알아듣고 이상하게 만들어 가지고 내가 이걸 사용하는 목적을 약간이라도 표시를 해 놓는 게 좋을 것 같고 그 다음에 폴리시 정도를 좀 더 올려도 될 것 같아 내 문맥을 파악해서 잘 정돈된 말로 다시 풀어서 해 줘도 좋을 것 같아"),
    ("Cursor", "com.todesktop.230313mixl4g3m", "Home.tsx — sori", "이 컴포넌트에서 유즈 이펙트가 두 번 불리는 것 같은데 음 아마 스트릭 모드 때문인 거 같긴 한데 어 그 데이터 캐칭 로직을 리액트 쿼리로 옮겨 줘 그 로딩 상태도 같이"),
    ("Terminal", "com.apple.Terminal", "codex", "ok so uh can you like refactor the the auth middleware so it doesn't hit the database on every request, maybe cache the session in redis or something, and um don't change the public API"),
    ("Slack", "com.tinyspeck.slackmacgap", "", "아 그 내일 회의 말인데 음 세 시로 하자고 했었나 아 아니다 네 시로 하자 그리고 뭐지 그 자료는 어 내가 오늘 저녁까지 노션에 올려 놓을게 아 근데 혹시 그 디자인 시안도 같이 보면 좋을 거 같은데 그거 민수 씨가 가지고 있지 않나"),
    ("Mail", "com.apple.mail", "", "so um basically what I wanted to say is uh the deploy failed last night because of the, the env variables were missing in the staging, like, the staging config, and uh I fixed it this morning so we should be good but we need to add a check in CI so it doesn't happen again, you know"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();
    let mut base = Settings::default();
    base.openrouter_api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    let models: Vec<String> = std::env::args().skip(1).collect();
    let only: Option<usize> = std::env::var("CASE").ok().and_then(|v| v.parse().ok());
    for (i, (app, bundle, title, raw)) in CASES.iter().enumerate() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        let ctx = Context { app_name: app.to_string(), bundle_id: bundle.to_string(), window_title: title.to_string(), field_focused: Some(true), ..Default::default() };
        println!("\n=== {i} [{app}] {raw}");
        for m in &models {
            let mut s = base.clone();
            s.llm_model = m.clone();
            s.llm_provider = if m.starts_with("local:") { "local" } else { "openrouter" }.into();
            let or = sori_core::llm::OpenRouter { http: &http, api_key: &s.openrouter_api_key };
            let local = m.strip_prefix("local:").and_then(|x| x.split_once('@')).map(|(_, url)| sori_core::llm::LocalServer { http: &http, base_url: url.to_string(), api_key: String::new() });
            let llm: &dyn sori_core::llm::LlmCall = match &local { Some(l) => l, None => &or };
            match pipeline::process_text(llm, &s, &[], raw, &Mode::Dictate, &ctx).await {
                Ok(p) => println!("--- {m} ({}ms){}\n{}", p.llm_ms, p.fallback_reason.map(|r| format!(" [{r}]")).unwrap_or_default(), p.output),
                Err(e) => println!("--- {m}: ERROR {e}"),
            }
        }
    }
    Ok(())
}
