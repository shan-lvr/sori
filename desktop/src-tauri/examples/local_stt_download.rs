//! Exercise the real model downloader (same code as the app) in a scratch folder.
//! cargo run --release -p sori --example local_stt_download -- <dir> [pause_after_secs]
//! Run it, let it pause (or kill it mid-way to simulate quitting the app), run it again: it resumes.
use std::sync::Arc;
use sori_lib::local_stt::LocalStt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&args[1]);
    let pause_after: Option<u64> = args.get(2).and_then(|v| v.parse().ok());
    let stt = Arc::new(LocalStt::new(dir));
    let id = "whisper-large-v3-turbo-q5_0";
    let st = &stt.status()[0];
    println!("before: installed={} partial={} MB paused={}", st.installed, st.partial / 1_000_000, st.paused);
    if let Some(secs) = pause_after {
        let s2 = stt.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            println!("-- pausing");
            s2.pause_download();
        });
    }
    let http = reqwest::Client::new();
    let t = std::time::Instant::now();
    let last = std::cell::RefCell::new(String::new());
    let res = stt
        .download(&http, id, |d| {
            let line = format!("{} {}%", d.state, d.downloaded * 100 / d.total.max(1));
            if line != *last.borrow() && (d.state != "downloading" || d.downloaded * 100 / d.total.max(1) % 10 == 0) {
                println!("  {:>5.1}s {line} {:.1} MB/s {}", t.elapsed().as_secs_f32(), d.bytes_per_sec as f64 / 1e6, d.error.clone().unwrap_or_default());
                *last.borrow_mut() = line;
            }
        })
        .await;
    let st = &stt.status()[0];
    println!("after: {res:?} installed={} partial={} MB paused={} ({:.1}s)", st.installed, st.partial / 1_000_000, st.paused, t.elapsed().as_secs_f32());
    Ok(())
}
