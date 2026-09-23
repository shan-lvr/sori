//! Dev self-test of the History / Dictionary actions, run inside the real app process
//! (real clipboard, real files, real APIs). Launch with:
//!   open --env SORI_SELFTEST=/path/to/16k-mono.wav /Applications/Sori.app
//! Results go to the log as `SELFTEST PASS|FAIL …`. Uses its own history entry and removes it;
//! the user's clipboard is put back afterwards.

use std::sync::Arc;

use serde_json::json;
use sori_core::store::HistoryEntry;
use sori_core::{Context, Mode};

use crate::commands;
use crate::controller::App;
use crate::platform;

fn report(name: &str, r: anyhow::Result<String>) -> bool {
    match r {
        Ok(detail) => {
            log::info!("SELFTEST PASS {name}: {detail}");
            true
        }
        Err(e) => {
            log::error!("SELFTEST FAIL {name}: {e:#}");
            false
        }
    }
}

pub async fn run(app: Arc<App>, wav_path: String) {
    log::info!("SELFTEST start");
    let mut ok = true;
    let id = format!("selftest-{}", uuid::Uuid::new_v4());
    let audio = app.data_dir.join("audio").join(format!("{id}.wav"));

    // Seed a history entry (status ok so retry doesn't touch the daily stats).
    ok &= report(
        "seed",
        (|| {
            std::fs::copy(&wav_path, &audio)?;
            let ctx = Context { app_name: "Sori selftest".into(), ..Default::default() };
            app.store.lock().upsert_history(&HistoryEntry {
                id: id.clone(),
                created_at: chrono::Utc::now().timestamp_millis(),
                mode: "dictate".into(),
                app_name: "Sori selftest".into(),
                audio_path: Some(audio.to_string_lossy().to_string()),
                status: "ok".into(),
                output: "(selftest placeholder)".into(),
                context_json: json!({"context": ctx, "mode": Mode::Dictate, "pid": 0}).to_string(),
                ..Default::default()
            })?;
            Ok(format!("{}", audio.display()))
        })(),
    );

    // Retry = re-run STT + LLM on the saved audio.
    let retried = app.retry(&id).await;
    let output = retried.as_ref().map(|e| e.output.clone()).unwrap_or_default();
    ok &= report(
        "retry",
        retried.map_err(anyhow::Error::from).and_then(|e| {
            anyhow::ensure!(e.status == "ok" && !e.output.is_empty() && e.output != "(selftest placeholder)", "status={} output={:?} error={:?}", e.status, e.output, e.error);
            Ok(format!("stt {}ms llm {}ms → {:?}", e.stt_ms, e.llm_ms, e.output))
        }),
    );

    // Copy = put text on the clipboard; verify by reading it back, then restore.
    ok &= report(
        "copy",
        (|| {
            let mut cb = arboard::Clipboard::new()?;
            let before = platform::clipboard_snapshot();
            let text = if output.is_empty() { "Sori selftest 복사".to_string() } else { output.clone() };
            platform::copy_to_clipboard(&text)?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            let got = cb.get_text();
            platform::clipboard_restore(&before);
            let got = got?;
            anyhow::ensure!(got == text, "clipboard had {got:?}");
            Ok("clipboard round-trip ok (previous clipboard restored)".into())
        })(),
    );

    // Download audio = copy the wav to a user-chosen path.
    ok &= report(
        "export_audio",
        (|| {
            let dest = std::env::temp_dir().join(format!("{id}-export.wav"));
            commands::history_export_audio(&app, &id, &dest.to_string_lossy())?;
            let (a, b) = (std::fs::metadata(&audio)?.len(), std::fs::metadata(&dest)?.len());
            let _ = std::fs::remove_file(&dest);
            anyhow::ensure!(a == b && a > 44, "sizes {a} vs {b}");
            Ok(format!("{b} bytes"))
        })(),
    );

    // Delete = row gone and audio file removed.
    ok &= report(
        "delete",
        (|| {
            commands::history_delete(&app, &id)?;
            anyhow::ensure!(app.store.lock().get_history(&id)?.is_none(), "row still present");
            anyhow::ensure!(!audio.exists(), "audio file still present");
            Ok("row + audio removed".into())
        })(),
    );

    // Dictionary: CSV import, edit, delete.
    ok &= report(
        "dictionary",
        (|| {
            let csv = std::env::temp_dir().join(format!("{id}.csv"));
            std::fs::write(&csv, "term\nSoriSelftestA\n\"SoriSelftestB\",note\n")?;
            let n = commands::dictionary_import(&app, &csv.to_string_lossy())?;
            let _ = std::fs::remove_file(&csv);
            anyhow::ensure!(n == 2, "imported {n}");
            let words = app.store.lock().list_dictionary()?;
            let a = words.iter().find(|w| w.term == "SoriSelftestA").ok_or_else(|| anyhow::anyhow!("A missing"))?.id;
            let b = words.iter().find(|w| w.term == "SoriSelftestB").ok_or_else(|| anyhow::anyhow!("B missing"))?.id;
            app.store.lock().update_word(a, "SoriSelftestC")?;
            anyhow::ensure!(app.store.lock().dictionary_terms()?.contains(&"SoriSelftestC".to_string()), "edit failed");
            app.store.lock().delete_words(&[a, b])?;
            anyhow::ensure!(!app.store.lock().dictionary_terms()?.iter().any(|t| t.starts_with("SoriSelftest")), "delete failed");
            Ok("import 2, edit, delete ok".into())
        })(),
    );

    // Paste path with "copy to clipboard" OFF: pasteboard is written as transient, then the
    // previous contents come back. (No ⌘V target here — just the clipboard bookkeeping.)
    ok &= report(
        "clipboard_restore",
        (|| {
            let mut cb = arboard::Clipboard::new()?;
            let before = platform::clipboard_snapshot();
            cb.set_text("sori-selftest-previous".to_string())?;
            let snap = platform::clipboard_snapshot();
            cb.set_text("sori-selftest-temporary".to_string())?;
            platform::clipboard_restore(&snap);
            let got = cb.get_text()?;
            platform::clipboard_restore(&before);
            anyhow::ensure!(got == "sori-selftest-previous", "got {got:?}");
            Ok("snapshot → overwrite → restore ok".into())
        })(),
    );

    log::info!("SELFTEST done: {}", if ok { "ALL PASS" } else { "FAILURES" });
    let _ = tauri::Emitter::emit(&app.handle, "history-updated", ());
}
