//! Evaluate the in-app local Whisper engine (same code path as the app) on the benchmark set.
//! cargo run --release -p sori --example local_stt_eval -- <models_dir> <data_dir>
use sori_core::settings::Settings;
use sori_lib::local_stt::{build_prompt, build_prompt_lang, LocalStt};
use sori_lib::models::ModelStore;

fn norm(s: &str) -> Vec<char> {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

fn read(path: &str) -> Vec<f32> {
    let (s, rate) = sori_core::audio::decode_wav(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(rate, 16_000);
    s
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (models, data) = (&args[1], &args[2]);
    let stt = LocalStt::new(std::sync::Arc::new(ModelStore::new(models.into())));
    let id = "whisper-large-v3-turbo-q5_0";
    let t = std::time::Instant::now();
    stt.ensure_loaded(id)?;
    println!("load: {}ms", t.elapsed().as_millis());

    // `prompts` mode: Korean vs English prompt sentence on Korean and English speech (auto-detect).
    if args.get(3).map(String::as_str) == Some("prompts") {
        let s = Settings::default();
        for set in ["fleurs_ko", "fleurs_en"] {
            let items: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(format!("{data}/{set}.json"))?)?;
            for korean in [true, false] {
                let prompt = build_prompt_lang(&s, &[], korean);
                let (mut errs, mut chars, mut ms) = (0usize, 0usize, 0u64);
                for it in &items {
                    let file = format!("{data}/../{}", it["file"].as_str().unwrap());
                    let r = stt.transcribe(id, &read(&file), Some(""), &prompt)?;
                    let (rf, hy) = (norm(it["ref"].as_str().unwrap()), norm(&r.text));
                    errs += edit_distance(&rf, &hy);
                    chars += rf.len();
                    ms += r.ms;
                }
                println!("{set} [{} prompt] CER {:.2}%  avg {}ms", if korean { "ko" } else { "en" }, errs as f64 * 100.0 / chars as f64, ms / items.len() as u64);
            }
        }
        return Ok(());
    }

    let items: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(format!("{data}/fleurs_ko.json"))?)?;
    let mut dev_settings = Settings::default();
    dev_settings.dev_mode = true;
    let plain = Settings { dev_mode: false, ..Settings::default() };
    for (label, s, lang) in [("auto, no prompt", &plain, ""), ("ko, no prompt", &plain, "ko"), ("auto, dev prompt", &dev_settings, ""), ("ko, dev prompt", &dev_settings, "ko")] {
        let prompt = build_prompt(s, &[]);
        let (mut errs, mut chars, mut ms) = (0usize, 0usize, 0u64);
        for it in &items {
            let file = format!("{data}/../{}", it["file"].as_str().unwrap());
            let r = stt.transcribe(id, &read(&file), Some(lang), &prompt)?;
            let (rf, hy) = (norm(it["ref"].as_str().unwrap()), norm(&r.text));
            errs += edit_distance(&rf, &hy);
            chars += rf.len();
            ms += r.ms;
        }
        println!("FLEURS-ko [{label:17}] CER {:.2}%  avg {}ms", errs as f64 * 100.0 / chars as f64, ms / items.len() as u64);
        let mut dev: Vec<_> = std::fs::read_dir(format!("{data}/dev"))?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dev.sort();
        for p in dev {
            let r = stt.transcribe(id, &read(p.to_str().unwrap()), Some(lang), &prompt)?;
            println!("    [{}] {}", r.language, r.text);
        }
    }
    Ok(())
}
