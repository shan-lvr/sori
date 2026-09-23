//! Compare cleanup styles / models on messy transcripts (text only, no audio).
//! cargo run -p sori-core --example polish_eval -- model1 model2 …   (OPENROUTER_API_KEY from env)
//! A model of the form `local:<name>@http://127.0.0.1:8080` uses a local llama-server.
use sori_core::{pipeline, settings::Settings, Context, Mode};

const CASES: &[(&str, &str, &str)] = &[
    ("Slack", "com.tinyspeck.slackmacgap", "그리고 지금 보면 약간 이거 다듬기가 아직도 좀 멍청한 수준이거든. 그리고 뭐 이렇게 추이민세나 내가 어어음 뭐 이런 거 필요 없는 것들은 이제 없어도 돼. 딱 요점 추려가지고 이렇게 말끔하게 딱 스마트해 보이게 그렇게 정돈해가지고 입력되면 좋을 것 같아. 하지만 그 말하는 거의 의도를 분명히 잘 파악하고 그쪽으로 돼야 되겠지."),
    ("Terminal", "com.apple.Terminal", "음 그러니까 이거를 팀원들한테 좀 쓰라고 배포를 할 예정인데 어 다음 사항들을 반영해 줘 일단 첫 번째는 무조건 영어를 디폴트로 해야 되고 지금 보면 영어로 바꿔도 한글로 나오는 부분이 있거든 그런 거 체크 잘 해서 두 번째는 클로드 코드 앱만 설치해 놓고 씨엘아이를 설치 안 한 팀원들이 있는데 버튼 누르면 씨엘아이가 설치되도록 하기"),
    ("Slack", "com.tinyspeck.slackmacgap", "아 그 내일 회의 말인데 음 세 시로 하자고 했었나 아 아니다 네 시로 하자 그리고 뭐지 그 자료는 어 내가 오늘 저녁까지 노션에 올려 놓을게 아 근데 혹시 그 디자인 시안도 같이 보면 좋을 거 같은데 그거 민수 씨가 가지고 있지 않나"),
    ("Mail", "com.apple.mail", "so um basically what I wanted to say is uh the deploy failed last night because of the, the env variables were missing in the staging, like, the staging config, and uh I fixed it this morning so we should be good but we need to add a check in CI so it doesn't happen again, you know"),
    ("Cursor", "com.todesktop.230313mixl4g3m", "이 컴포넌트에서 유즈 이펙트가 두 번 불리는 것 같은데 음 아마 스트릭트 모드 때문인 거 같긴 한데 어 그 리액트 쿼리로 바꾸면 이거 해결되지 않을까 그 데이터 캐칭 로직을 그 쪽으로 옮기는 게 나을 것 같아"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();
    let mut base = Settings::default();
    base.openrouter_api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    let models: Vec<String> = std::env::args().skip(1).collect();
    for (app, bundle, raw) in CASES {
        let ctx = Context { app_name: app.to_string(), bundle_id: bundle.to_string(), field_focused: Some(true), ..Default::default() };
        println!("\n=== [{app}] {raw}");
        for m in &models {
            let mut s = base.clone();
            s.llm_model = m.clone();
            if m.starts_with("local:") {
                s.llm_provider = "local".into();
            }
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
