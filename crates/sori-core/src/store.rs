//! Local SQLite store: history, dictionary, daily stats.

use std::path::Path;

use anyhow::Result;
use chrono::{Duration, Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct HistoryEntry {
    pub id: String,
    /// Unix ms.
    pub created_at: i64,
    /// `dictate` | `translate` | `ask`
    pub mode: String,
    pub app_name: String,
    pub bundle_id: String,
    pub raw_text: String,
    pub output: String,
    pub audio_path: Option<String>,
    pub duration_ms: i64,
    pub words: i64,
    pub language: String,
    /// `ok` | `error` | `empty`
    pub status: String,
    pub error: Option<String>,
    pub stt_ms: i64,
    pub llm_ms: i64,
    /// JSON of `pipeline::Context` + mode, used for Retry.
    pub context_json: String,
    /// Ask action / delivery: `insert` | `replace` | `answer` | `open_url` | `card`
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DictWord {
    pub id: i64,
    pub term: String,
    /// `manual` | `auto`
    pub source: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DayStat {
    pub day: String,
    pub words: i64,
    pub audio_ms: i64,
    pub sessions: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Stats {
    pub total_words: i64,
    pub total_audio_ms: i64,
    pub avg_wpm: i64,
    pub time_saved_ms: i64,
    pub active_days: i64,
    pub current_streak: i64,
    pub longest_streak: i64,
    pub days: Vec<DayStat>,
}

pub struct Store {
    conn: Connection,
}

const COLS: &str = "id, created_at, mode, app_name, bundle_id, raw_text, output, audio_path, duration_ms, words, language, status, error, stt_ms, llm_ms, context_json, action";

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        created_at: r.get(1)?,
        mode: r.get(2)?,
        app_name: r.get(3)?,
        bundle_id: r.get(4)?,
        raw_text: r.get(5)?,
        output: r.get(6)?,
        audio_path: r.get(7)?,
        duration_ms: r.get(8)?,
        words: r.get(9)?,
        language: r.get(10)?,
        status: r.get(11)?,
        error: r.get(12)?,
        stt_ms: r.get(13)?,
        llm_ms: r.get(14)?,
        context_json: r.get(15)?,
        action: r.get(16)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS history(
               id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, mode TEXT NOT NULL,
               app_name TEXT NOT NULL DEFAULT '', bundle_id TEXT NOT NULL DEFAULT '',
               raw_text TEXT NOT NULL DEFAULT '', output TEXT NOT NULL DEFAULT '', audio_path TEXT,
               duration_ms INTEGER NOT NULL DEFAULT 0, words INTEGER NOT NULL DEFAULT 0,
               language TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'ok', error TEXT,
               stt_ms INTEGER NOT NULL DEFAULT 0, llm_ms INTEGER NOT NULL DEFAULT 0,
               context_json TEXT NOT NULL DEFAULT '{}', action TEXT NOT NULL DEFAULT 'insert');
             CREATE INDEX IF NOT EXISTS history_created ON history(created_at DESC);
             CREATE TABLE IF NOT EXISTS dictionary(
               id INTEGER PRIMARY KEY AUTOINCREMENT, term TEXT NOT NULL UNIQUE COLLATE NOCASE,
               source TEXT NOT NULL DEFAULT 'manual', created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS daily_stats(
               day TEXT PRIMARY KEY, words INTEGER NOT NULL DEFAULT 0,
               audio_ms INTEGER NOT NULL DEFAULT 0, sessions INTEGER NOT NULL DEFAULT 0);",
        )?;
        Ok(Self { conn })
    }

    // ---------- history ----------

    pub fn upsert_history(&self, e: &HistoryEntry) -> Result<()> {
        self.conn.execute(
            &format!("INSERT OR REPLACE INTO history({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"),
            params![
                e.id, e.created_at, e.mode, e.app_name, e.bundle_id, e.raw_text, e.output, e.audio_path,
                e.duration_ms, e.words, e.language, e.status, e.error, e.stt_ms, e.llm_ms, e.context_json, e.action
            ],
        )?;
        Ok(())
    }

    pub fn get_history(&self, id: &str) -> Result<Option<HistoryEntry>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {COLS} FROM history WHERE id=?1"), [id], row_to_entry)
            .optional()?)
    }

    /// `filter`: `all` | `dictate` (incl. translate) | `ask`.
    pub fn list_history(&self, filter: &str, query: &str, limit: i64, offset: i64) -> Result<Vec<HistoryEntry>> {
        let mode_clause = match filter {
            "dictate" => "mode IN ('dictate','translate')",
            "ask" => "mode = 'ask'",
            _ => "1=1",
        };
        let q = format!("%{}%", query.trim());
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM history WHERE {mode_clause} AND (?1 = '%%' OR output LIKE ?1 OR raw_text LIKE ?1)
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = st.query_map(params![q, limit, offset], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Returns the audio path of the deleted entry, if any.
    pub fn delete_history(&self, id: &str) -> Result<Option<String>> {
        let path: Option<Option<String>> = self
            .conn
            .query_row("SELECT audio_path FROM history WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        self.conn.execute("DELETE FROM history WHERE id=?1", [id])?;
        Ok(path.flatten())
    }

    pub fn delete_all_history(&self) -> Result<Vec<String>> {
        let paths = self.audio_paths("1=1", params![])?;
        self.conn.execute("DELETE FROM history", [])?;
        Ok(paths)
    }

    /// Delete entries older than `cutoff_ms`; returns their audio paths.
    pub fn prune_history(&self, cutoff_ms: i64) -> Result<Vec<String>> {
        let paths = self.audio_paths("created_at < ?1", params![cutoff_ms])?;
        self.conn.execute("DELETE FROM history WHERE created_at < ?1", [cutoff_ms])?;
        Ok(paths)
    }

    fn audio_paths(&self, clause: &str, p: impl rusqlite::Params) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(&format!("SELECT audio_path FROM history WHERE {clause} AND audio_path IS NOT NULL"))?;
        let rows = st.query_map(p, |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---------- dictionary ----------

    pub fn list_dictionary(&self) -> Result<Vec<DictWord>> {
        let mut st = self.conn.prepare("SELECT id, term, source, created_at FROM dictionary ORDER BY created_at DESC, id DESC")?;
        let rows = st.query_map([], |r| {
            Ok(DictWord { id: r.get(0)?, term: r.get(1)?, source: r.get(2)?, created_at: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn dictionary_terms(&self) -> Result<Vec<String>> {
        Ok(self.list_dictionary()?.into_iter().map(|w| w.term).collect())
    }

    /// Returns number of newly inserted terms.
    pub fn add_words(&self, terms: &[String], source: &str) -> Result<usize> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut n = 0;
        for t in terms {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            n += self.conn.execute(
                "INSERT OR IGNORE INTO dictionary(term, source, created_at) VALUES (?1, ?2, ?3)",
                params![t, source, now],
            )?;
        }
        Ok(n)
    }

    pub fn update_word(&self, id: i64, term: &str) -> Result<()> {
        self.conn.execute("UPDATE dictionary SET term=?1 WHERE id=?2", params![term.trim(), id])?;
        Ok(())
    }

    pub fn delete_words(&self, ids: &[i64]) -> Result<()> {
        for id in ids {
            self.conn.execute("DELETE FROM dictionary WHERE id=?1", [id])?;
        }
        Ok(())
    }

    // ---------- stats ----------

    pub fn record_session(&self, created_at_ms: i64, words: i64, audio_ms: i64) -> Result<()> {
        let day = chrono::DateTime::from_timestamp_millis(created_at_ms)
            .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        self.conn.execute(
            "INSERT INTO daily_stats(day, words, audio_ms, sessions) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(day) DO UPDATE SET words=words+?2, audio_ms=audio_ms+?3, sessions=sessions+1",
            params![day, words, audio_ms],
        )?;
        Ok(())
    }

    pub fn stats(&self, typing_wpm: u32, compose_factor: f32) -> Result<Stats> {
        let mut st = self.conn.prepare("SELECT day, words, audio_ms, sessions FROM daily_stats ORDER BY day")?;
        let days = st
            .query_map([], |r| Ok(DayStat { day: r.get(0)?, words: r.get(1)?, audio_ms: r.get(2)?, sessions: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(compute_stats(days, typing_wpm, compose_factor, Local::now().date_naive()))
    }
}

/// Time saved = time it would have taken to write the same words by hand (typing at
/// `typing_wpm`, × `compose_factor` for organizing and polishing) − time spent talking.
pub fn compute_stats(days: Vec<DayStat>, typing_wpm: u32, compose_factor: f32, today: NaiveDate) -> Stats {
    let total_words: i64 = days.iter().map(|d| d.words).sum();
    let total_audio_ms: i64 = days.iter().map(|d| d.audio_ms).sum();
    let avg_wpm = if total_audio_ms > 0 { (total_words as f64 / (total_audio_ms as f64 / 60_000.0)).round() as i64 } else { 0 };
    let factor = compose_factor.clamp(1.0, 5.0) as f64;
    let typing_ms = if typing_wpm > 0 { (total_words as f64 / typing_wpm as f64 * 60_000.0 * factor) as i64 } else { 0 };
    let time_saved_ms = (typing_ms - total_audio_ms).max(0);

    let active: Vec<NaiveDate> = days
        .iter()
        .filter(|d| d.sessions > 0)
        .filter_map(|d| NaiveDate::parse_from_str(&d.day, "%Y-%m-%d").ok())
        .collect();
    let mut longest = 0i64;
    let mut run = 0i64;
    let mut prev: Option<NaiveDate> = None;
    for d in &active {
        run = match prev {
            Some(p) if *d - p == Duration::days(1) => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        prev = Some(*d);
    }
    let set: std::collections::HashSet<NaiveDate> = active.iter().copied().collect();
    let mut cursor = if set.contains(&today) { today } else { today - Duration::days(1) };
    let mut current = 0i64;
    while set.contains(&cursor) {
        current += 1;
        cursor -= Duration::days(1);
    }
    Stats {
        total_words,
        total_audio_ms,
        avg_wpm,
        time_saved_ms,
        active_days: active.len() as i64,
        current_streak: current,
        longest_streak: longest,
        days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaks() {
        let d = |s: &str| DayStat { day: s.into(), words: 100, audio_ms: 60_000, sessions: 1 };
        let today = NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
        let s = compute_stats(vec![d("2026-09-10"), d("2026-09-11"), d("2026-09-12"), d("2026-09-21"), d("2026-09-22")], 40, 1.0, today);
        assert_eq!(s.current_streak, 2);
        assert_eq!(s.longest_streak, 3);
        assert_eq!(s.active_days, 5);
        assert_eq!(s.avg_wpm, 100);
        // 500 words at 40 wpm = 12.5 min typing; ×2 for composing = 25 min; minus 5 min talking.
        let s2 = compute_stats(vec![d("2026-09-21"), d("2026-09-22"), d("2026-09-20"), d("2026-09-19"), d("2026-09-18")], 40, 2.0, today);
        assert_eq!(s2.time_saved_ms, 20 * 60_000);
    }

    #[test]
    fn history_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sori-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = Store::open(&dir.join("t.db")).unwrap();
        let e = HistoryEntry { id: "a".into(), created_at: 1, mode: "dictate".into(), output: "안녕".into(), ..Default::default() };
        st.upsert_history(&e).unwrap();
        assert_eq!(st.list_history("all", "", 10, 0).unwrap().len(), 1);
        assert_eq!(st.list_history("ask", "", 10, 0).unwrap().len(), 0);
        assert_eq!(st.list_history("all", "안", 10, 0).unwrap().len(), 1);
        assert_eq!(st.add_words(&["OpenRouter".into(), "openrouter".into()], "manual").unwrap(), 1);
    }
}
