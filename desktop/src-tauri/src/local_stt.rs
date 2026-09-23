//! On-device speech-to-text with whisper.cpp (via `whisper-rs`).
//!
//! Metal on macOS, Vulkan on Windows (see Cargo.toml). The model is downloaded once into
//! the app data dir, loaded on demand and kept in memory so each dictation only pays for
//! inference (~0.5 s for a 12 s clip on an M4 Pro).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Instant;

use anyhow::{anyhow, bail, Context as _, Result};
use parking_lot::Mutex;
use serde::Serialize;
use sori_core::settings::Settings;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    /// SHA-256 of the file (Hugging Face `x-linked-etag`), checked after every download.
    pub sha256: &'static str,
}

pub const MODELS: &[ModelInfo] = &[ModelInfo {
    id: "whisper-large-v3-turbo-q5_0",
    label: "Whisper large-v3-turbo (q5, 574MB)",
    file: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    size: 574_041_195,
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
}];

pub fn model(id: &str) -> Result<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id).ok_or_else(|| anyhow!("Unknown local model: {id}"))
}

#[derive(Serialize, Clone)]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub size: u64,
    pub installed: bool,
    pub loaded: bool,
    pub downloading: bool,
    /// Bytes already on disk from an interrupted download (resumable).
    pub partial: u64,
    /// The user paused it (survives restarts: no automatic resume).
    pub paused: bool,
    pub download: Option<Download>,
}

/// Live download state, pushed to the UI as `local-model-download`.
#[derive(Serialize, Clone, Default, Debug)]
pub struct Download {
    pub id: String,
    /// `connecting` | `downloading` | `retrying` | `verifying` | `paused` | `error` | `done`
    pub state: String,
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub error: Option<String>,
    /// Automatic retry number while `retrying` (network hiccups).
    pub attempt: u32,
}

const STOP_NONE: u8 = 0;
const STOP_PAUSE: u8 = 1;
const STOP_CANCEL: u8 = 2;

struct Loaded {
    id: String,
    // `state` borrows nothing from `ctx` (it holds its own Arc), but keep ctx alive alongside.
    _ctx: WhisperContext,
    state: WhisperState,
}

pub struct LocalStt {
    dir: PathBuf,
    loaded: Mutex<Option<Loaded>>,
    /// Serializes model loading (startup preload vs. a dictation that arrives mid-load).
    load_lock: Mutex<()>,
    downloading: AtomicBool,
    stop: AtomicU8,
    download: Mutex<Option<Download>>,
}

pub struct Transcript {
    pub text: String,
    pub language: String,
    pub ms: u64,
}

impl LocalStt {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        whisper_rs::install_logging_hooks();
        Self {
            dir,
            loaded: Mutex::new(None),
            load_lock: Mutex::new(()),
            downloading: AtomicBool::new(false),
            stop: AtomicU8::new(STOP_NONE),
            download: Mutex::new(None),
        }
    }

    pub fn path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.dir.join(model(id)?.file))
    }

    pub fn is_installed(&self, id: &str) -> bool {
        let Ok(m) = model(id) else { return false };
        std::fs::metadata(self.dir.join(m.file)).map(|md| md.len() == m.size).unwrap_or(false)
    }

    pub fn status(&self) -> Vec<ModelStatus> {
        let loaded = self.loaded.lock().as_ref().map(|l| l.id.clone());
        let dl = self.download.lock().clone();
        MODELS
            .iter()
            .map(|m| ModelStatus {
                id: m.id.into(),
                label: m.label.into(),
                size: m.size,
                installed: self.is_installed(m.id),
                loaded: loaded.as_deref() == Some(m.id),
                downloading: self.downloading.load(Ordering::SeqCst),
                partial: std::fs::metadata(self.part_path(m)).map(|md| md.len()).unwrap_or(0),
                paused: self.paused_marker(m).exists(),
                download: dl.clone().filter(|d| d.id == m.id),
            })
            .collect()
    }

    /// Current download, if any (for HUD messages while the model isn't ready yet).
    pub fn active_download(&self) -> Option<Download> {
        self.download.lock().clone().filter(|d| d.state != "done")
    }

    fn part_path(&self, m: &ModelInfo) -> PathBuf {
        self.dir.join(format!("{}.part", m.file))
    }

    fn paused_marker(&self, m: &ModelInfo) -> PathBuf {
        self.dir.join(format!("{}.paused", m.file))
    }

    /// The user picked this model again (e.g. switched the engine to local): allow auto-download.
    pub fn clear_pause(&self, id: &str) {
        if let Ok(m) = model(id) {
            let _ = std::fs::remove_file(self.paused_marker(m));
        }
    }

    /// Whether an automatic (re)start is appropriate: not installed, not running, not paused by the user.
    pub fn should_auto_download(&self, id: &str) -> bool {
        let Ok(m) = model(id) else { return false };
        !self.is_installed(id) && !self.downloading.load(Ordering::SeqCst) && !self.paused_marker(m).exists()
    }

    /// Load (or switch to) a model. Blocking; ~6 s on an M4 Pro (Metal kernels are compiled at
    /// load time), so the app preloads it in the background. ~850 MB resident while loaded.
    pub fn ensure_loaded(&self, id: &str) -> Result<()> {
        let _loading = self.load_lock.lock();
        if self.loaded.lock().as_ref().map(|l| l.id == id).unwrap_or(false) {
            return Ok(());
        }
        if !self.is_installed(id) {
            if let Some(d) = self.active_download().filter(|d| d.id == id && d.state != "error" && d.state != "paused") {
                bail!("The speech model is still downloading ({}%) — try again in a moment", d.downloaded * 100 / d.total.max(1));
            }
            bail!("The on-device speech model isn't downloaded yet (Settings → AI)");
        }
        let path = self.path(id)?;
        let t = Instant::now();
        let mut params = WhisperContextParameters::default();
        params.use_gpu(true).flash_attn(true);
        let ctx = WhisperContext::new_with_params(&path, params).map_err(|e| anyhow!("Could not load the model: {e}"))?;
        let state = ctx.create_state().map_err(|e| anyhow!("Could not initialize the model: {e}"))?;
        *self.loaded.lock() = Some(Loaded { id: id.into(), _ctx: ctx, state });
        log::info!("local stt loaded {id} in {}ms", t.elapsed().as_millis());
        Ok(())
    }

    pub fn unload(&self) {
        if self.loaded.lock().take().is_some() {
            log::info!("local stt unloaded");
        }
    }

    /// Transcribe 16 kHz mono samples. `language`: None/"" = auto-detect.
    pub fn transcribe(&self, id: &str, samples: &[f32], language: Option<&str>, prompt: &str) -> Result<Transcript> {
        self.ensure_loaded(id)?;
        let t = Instant::now();
        let mut guard = self.loaded.lock();
        let loaded = guard.as_mut().ok_or_else(|| anyhow!("Model not loaded"))?;

        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32;
        p.set_n_threads(threads);
        let lang = language.filter(|l| !l.is_empty()).unwrap_or("auto");
        p.set_language(Some(lang));
        p.set_no_timestamps(true);
        p.set_no_context(true);
        p.set_suppress_blank(true);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        loaded.state.full(p, samples).map_err(|e| anyhow!("Speech recognition failed: {e}"))?;

        let mut text = String::new();
        for seg in loaded.state.as_iter() {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(s.trim());
                text.push(' ');
            }
        }
        let language = whisper_rs::get_lang_str(loaded.state.full_lang_id_from_state()).unwrap_or("").to_string();
        Ok(Transcript { text: clean_hallucinations(text.trim()), language, ms: t.elapsed().as_millis() as u64 })
    }

    /// Pause: stop now, keep the partial file; resumes from there later (also after a restart).
    pub fn pause_download(&self) {
        self.stop.store(STOP_PAUSE, Ordering::SeqCst);
    }

    /// Cancel: stop and discard the partial file.
    pub fn cancel_download(&self, id: &str) {
        if self.downloading.load(Ordering::SeqCst) {
            self.stop.store(STOP_CANCEL, Ordering::SeqCst);
        } else if let Ok(m) = model(id) {
            let _ = std::fs::remove_file(self.part_path(m));
            let _ = std::fs::remove_file(self.paused_marker(m));
            *self.download.lock() = None;
        }
    }

    /// Download (or resume) a model. Progress and state changes go to `emit`. Robust to:
    /// quitting mid-way (the `.part` file is resumed with an HTTP Range request next time),
    /// flaky networks (automatic retries with backoff while bytes keep arriving), servers that
    /// ignore Range (restart from zero), full disks (checked up front) and corrupt data
    /// (SHA-256 verified before the model is used).
    pub async fn download(&self, http: &reqwest::Client, id: &str, emit: impl Fn(&Download)) -> Result<()> {
        let m = model(id)?;
        if self.downloading.swap(true, Ordering::SeqCst) {
            return Ok(()); // already running; the caller just follows the events
        }
        self.stop.store(STOP_NONE, Ordering::SeqCst);
        let _ = std::fs::remove_file(self.paused_marker(m));
        let set = |d: Download| {
            emit(&d);
            *self.download.lock() = Some(d);
        };
        let res = self.download_inner(http, m, &set).await;
        let stop = self.stop.swap(STOP_NONE, Ordering::SeqCst);
        let base = Download { id: m.id.into(), total: m.size, ..Default::default() };
        let res = match res {
            Ok(()) => {
                set(Download { state: "done".into(), downloaded: m.size, ..base });
                Ok(())
            }
            Err(_) if stop == STOP_PAUSE => {
                let _ = std::fs::write(self.paused_marker(m), b"");
                let downloaded = std::fs::metadata(self.part_path(m)).map(|md| md.len()).unwrap_or(0);
                set(Download { state: "paused".into(), downloaded, ..base });
                Ok(())
            }
            Err(_) if stop == STOP_CANCEL => {
                let _ = std::fs::remove_file(self.part_path(m));
                emit(&Download { state: "cancelled".into(), ..base });
                *self.download.lock() = None;
                Ok(())
            }
            Err(e) => {
                let downloaded = std::fs::metadata(self.part_path(m)).map(|md| md.len()).unwrap_or(0);
                set(Download { state: "error".into(), downloaded, error: Some(format!("{e:#}")), ..base });
                Err(e)
            }
        };
        self.downloading.store(false, Ordering::SeqCst);
        res
    }

    async fn download_inner(&self, http: &reqwest::Client, m: &ModelInfo, set: &impl Fn(Download)) -> Result<()> {
        let part = self.part_path(m);
        let base = Download { id: m.id.into(), total: m.size, ..Default::default() };
        let mut have = std::fs::metadata(&part).map(|md| md.len()).unwrap_or(0);
        if have > m.size {
            let _ = std::fs::remove_file(&part);
            have = 0;
        }
        let need = m.size - have + 64 * 1024 * 1024;
        if let Some(free) = free_space(&self.dir) {
            if free < need {
                bail!("Not enough disk space: {} MB needed, {} MB free", need / 1_000_000, free / 1_000_000);
            }
        }
        let mut attempt = 0u32;
        loop {
            if self.stop.load(Ordering::SeqCst) != STOP_NONE {
                bail!("stopped");
            }
            if have < m.size {
                set(Download { state: if attempt == 0 { "connecting" } else { "retrying" }.into(), downloaded: have, attempt, ..base.clone() });
                match self.fetch(http, m, &part, have, set).await {
                    Ok(()) => {}
                    Err(e) if self.stop.load(Ordering::SeqCst) != STOP_NONE => return Err(e),
                    Err(e) => {
                        let now = std::fs::metadata(&part).map(|md| md.len()).unwrap_or(0);
                        // Keep retrying as long as each attempt makes progress; give up after
                        // 5 attempts in a row without a single new byte.
                        attempt = if now > have { 1 } else { attempt + 1 };
                        have = now;
                        if attempt > 5 {
                            return Err(e);
                        }
                        log::warn!("model download attempt {attempt} failed: {e:#}");
                        let wait = [1u64, 2, 4, 8, 15][(attempt as usize - 1).min(4)];
                        set(Download { state: "retrying".into(), downloaded: have, attempt, error: Some(format!("{e:#}")), ..base.clone() });
                        for _ in 0..wait * 10 {
                            if self.stop.load(Ordering::SeqCst) != STOP_NONE {
                                bail!("stopped");
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        continue;
                    }
                }
                have = std::fs::metadata(&part).map(|md| md.len()).unwrap_or(0);
                if have < m.size {
                    continue; // connection closed early: resume
                }
            }
            // Verify before trusting 574 MB of weights.
            set(Download { state: "verifying".into(), downloaded: m.size, ..base.clone() });
            let (p2, want) = (part.clone(), m.sha256);
            let ok = tokio::task::spawn_blocking(move || sha256_file(&p2).map(|h| h == want)).await??;
            if !ok {
                let _ = std::fs::remove_file(&part);
                bail!("The downloaded file was corrupted (checksum mismatch). Please download again.");
            }
            std::fs::rename(&part, self.dir.join(m.file))?;
            let _ = std::fs::File::open(&self.dir).and_then(|d| d.sync_all());
            return Ok(());
        }
    }

    /// One HTTP request, appending to `.part` from byte `have`.
    async fn fetch(&self, http: &reqwest::Client, m: &ModelInfo, part: &Path, have: u64, set: &impl Fn(Download)) -> Result<()> {
        use std::io::Write;
        let mut req = http.get(m.url).timeout(std::time::Duration::from_secs(6 * 60 * 60));
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().await.context("Couldn't reach the download server")?;
        let status = resp.status();
        let mut have = have;
        let mut file = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            std::fs::OpenOptions::new().append(true).open(part)?
        } else if status.is_success() {
            have = 0; // server ignored Range: start over
            std::fs::File::create(part)?
        } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(()); // already complete; the caller verifies
        } else {
            bail!("Download server returned HTTP {}", status.as_u16());
        };
        let started = Instant::now();
        let start_bytes = have;
        let mut last = Instant::now();
        let mut speed = 0f64;
        let (mut win_t, mut win_b) = (Instant::now(), have);
        loop {
            let chunk = match tokio::time::timeout(std::time::Duration::from_secs(30), resp.chunk()).await {
                Ok(Ok(Some(c))) => c,
                Ok(Ok(None)) => break,
                Ok(Err(e)) => return Err(anyhow!(e).context("Connection interrupted")),
                Err(_) => bail!("Connection stalled"),
            };
            if self.stop.load(Ordering::SeqCst) != STOP_NONE {
                file.flush()?;
                bail!("stopped");
            }
            file.write_all(&chunk)?;
            have += chunk.len() as u64;
            if last.elapsed().as_millis() >= 250 {
                let dt = win_t.elapsed().as_secs_f64();
                if dt > 0.0 {
                    let inst = (have - win_b) as f64 / dt;
                    speed = if speed == 0.0 { inst } else { speed * 0.7 + inst * 0.3 };
                }
                (win_t, win_b) = (Instant::now(), have);
                set(Download { id: m.id.into(), state: "downloading".into(), downloaded: have, total: m.size, bytes_per_sec: speed as u64, ..Default::default() });
                last = Instant::now();
            }
        }
        file.flush()?;
        file.sync_all()?;
        log::info!(
            "model download: {} MB in {:.1}s",
            (have - start_bytes) / 1_000_000,
            started.elapsed().as_secs_f32()
        );
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if self.loaded.lock().as_ref().map(|l| l.id == id).unwrap_or(false) {
            self.unload();
        }
        let m = model(id)?;
        let p = self.path(id)?;
        if Path::new(&p).exists() {
            std::fs::remove_file(p)?;
        }
        let _ = std::fs::remove_file(self.part_path(m));
        // Deleting on purpose: don't silently re-download it on the next launch.
        let _ = std::fs::write(self.paused_marker(m), b"");
        *self.download.lock() = None;
        Ok(())
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Free bytes on the volume holding `dir` (None if unknown).
fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        Some(st.f_bavail as u64 * st.f_frsize as u64)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free = 0u64;
        let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
        (ok != 0).then_some(free)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = dir;
        None
    }
}


/// Whisper's initial prompt: biases spelling toward the user's vocabulary (English terms in
/// Latin script). Phrased as a plain sentence — a "용어:" label style leaked into transcripts
/// in testing (FLEURS-ko CER 2.76% → 4.17%), this form stays at ~3.0% with no leaks.
pub fn build_prompt(s: &Settings, dictionary: &[String]) -> String {
    let korean = match s.stt_language.as_str() {
        "ko" => true,
        "" => s.interface_language == "ko",
        _ => false,
    };
    build_prompt_lang(s, dictionary, korean)
}

/// Prompt sentence in Korean or English (the prompt's language nudges the decoder's style).
pub fn build_prompt_lang(s: &Settings, dictionary: &[String], korean: bool) -> String {
    let mut terms: Vec<String> = dictionary.to_vec();
    if s.dev_mode {
        terms.extend(sori_core::devterms::tech_stack(s));
        terms.extend(PROMPT_DEV_TERMS.iter().map(|t| t.to_string()));
    }
    let mut seen = std::collections::HashSet::new();
    terms.retain(|t| seen.insert(t.to_lowercase()));
    let mut list = String::new();
    for t in terms {
        if list.chars().count() + t.chars().count() > 360 {
            break;
        }
        if !list.is_empty() {
            list.push_str(", ");
        }
        list.push_str(&t);
    }
    if list.is_empty() {
        return String::new();
    }
    let (lead, tail) = PROMPTS[if korean { 0 } else { 1 }];
    format!("{lead}{list}{tail}")
}

const PROMPTS: [(&str, &str); 2] = [
    ("오늘 회의에서는 ", " 같은 용어를 섞어서 이야기했습니다."),
    ("In today's meeting we talked about ", " and related topics."),
];

/// Short list of terms Korean speakers say often and Whisper tends to write in Hangul.
const PROMPT_DEV_TERMS: &[&str] = &[
    "API", "React", "React Query", "useEffect", "TypeScript", "JavaScript", "Next.js", "Node.js", "Python", "Rust",
    "Git", "GitHub", "PR", "pull request", "commit", "merge", "rebase", "Docker", "Docker Compose", "Kubernetes",
    "PostgreSQL", "Redis", "JSON", "YAML", ".env", "CI/CD", "deploy", "LLM", "Claude Code", "Cursor", "Tauri",
    "Supabase", "Vercel", "AWS", "SDK", "CLI", "OAuth", "WebSocket", "localhost", "refactor", "prompt", "token",
];

/// Drop whisper's classic silence hallucinations, bracketed sound tags and any echo of the
/// initial prompt.
fn clean_hallucinations(text: &str) -> String {
    let mut t = text.to_string();
    for (lead, tail) in PROMPTS {
        if let (Some(a), Some(b)) = (t.find(lead.trim()), t.find(tail.trim())) {
            if a < b {
                t.replace_range(a..b + tail.trim().len(), "");
            }
        }
    }
    let mut t = t.trim().to_string();
    for (open, close) in [('[', ']'), ('(', ')')] {
        // Remove whole-transcript tags like "[음악]" or "(박수)".
        if t.starts_with(open) && t.ends_with(close) && t.chars().filter(|c| *c == open).count() == 1 {
            t.clear();
        }
    }
    const JUNK: &[&str] = &[
        "시청해주셔서 감사합니다", "시청해 주셔서 감사합니다", "구독과 좋아요 부탁드립니다", "MBC 뉴스", "Thank you for watching",
        "Thanks for watching", "자막 제공",
    ];
    if JUNK.iter().any(|j| t.contains(j)) && t.chars().count() < 40 {
        t.clear();
    }
    t.trim().to_string()
}
