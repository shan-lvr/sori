//! On-device model files (speech + text): catalog, status and a robust downloader.
//!
//! Downloads survive quitting the app (the `.part` file is resumed with an HTTP Range request),
//! retry network hiccups while bytes keep arriving, restart when a server ignores Range, check
//! free disk space up front and verify SHA-256 before a file is used. Several models can
//! download at once; each has its own state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, bail, Context as _, Result};
use parking_lot::Mutex;
use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Speech-to-text (whisper.cpp)
    Stt,
    /// Text model for cleanup/translation/Ask (llama.cpp server)
    Llm,
}

pub struct ModelInfo {
    pub id: &'static str,
    pub kind: Kind,
    pub label: &'static str,
    pub file: &'static str,
    /// Pinned to a repo commit so the checksum can't drift.
    pub url: &'static str,
    pub size: u64,
    /// SHA-256 of the file (Hugging Face `x-linked-etag`), checked after every download.
    pub sha256: &'static str,
    pub license: &'static str,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "whisper-large-v3-turbo-q5_0",
        kind: Kind::Stt,
        label: "Whisper",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
        size: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        license: "MIT",
    },
    ModelInfo {
        id: "gemma-4-e2b",
        kind: Kind::Llm,
        label: "Gemma",
        file: "gemma-4-E2B-it-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/0314792d7f1f7e229411f620751375812bb9faf2/gemma-4-E2B-it-Q4_K_M.gguf",
        size: 3_106_738_272,
        sha256: "740185b21d22ceb83a11c3aa62ad5842ef32c70f6096d756bbee85a1e4ec34b8",
        license: "Apache-2.0",
    },
    ModelInfo {
        id: "kanana-2-1.3b",
        kind: Kind::Llm,
        label: "Kanana",
        file: "Kanana-2-1.3b-instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/ssolunar/Kanana-2-1.3b-instruct-Q4_K_M-GGUF/resolve/cdf3d580c1ad5ae9bcee8028937158e8dee655e0/Kanana-2-1.3b-instruct-Q4_K_M.gguf",
        size: 853_655_776,
        sha256: "281cc80a8bbbf4b7faee402bc7fca5e7f073d2a6cb85497397014803184780c1",
        license: "Kanana Open License",
    },
];

pub fn model(id: &str) -> Result<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id).ok_or_else(|| anyhow!("Unknown local model: {id}"))
}

#[derive(Serialize, Clone)]
pub struct ModelStatus {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub size: u64,
    pub license: String,
    pub installed: bool,
    /// In memory and ready (whisper loaded / llama-server up with this model).
    pub loaded: bool,
    pub downloading: bool,
    /// Bytes already on disk from an interrupted download (resumable).
    pub partial: u64,
    /// The user paused or deleted it on purpose (survives restarts: no automatic download).
    pub paused: bool,
    pub download: Option<Download>,
}

/// Live download state, pushed to the UI as `local-model-download`.
#[derive(Serialize, Clone, Default, Debug)]
pub struct Download {
    pub id: String,
    /// `connecting` | `downloading` | `retrying` | `verifying` | `paused` | `error` | `done` | `cancelled`
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

pub struct ModelStore {
    dir: PathBuf,
    /// Latest state per model (kept after the download ends so the UI can show errors).
    state: Mutex<HashMap<String, Download>>,
    /// Models with a download running, and their stop request.
    active: Mutex<HashMap<String, u8>>,
}

impl ModelStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, state: Mutex::new(HashMap::new()), active: Mutex::new(HashMap::new()) }
    }

    pub fn path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.dir.join(model(id)?.file))
    }

    pub fn is_installed(&self, id: &str) -> bool {
        let Ok(m) = model(id) else { return false };
        std::fs::metadata(self.dir.join(m.file)).map(|md| md.len() == m.size).unwrap_or(false)
    }

    pub fn is_downloading(&self, id: &str) -> bool {
        self.active.lock().contains_key(id)
    }

    fn stop_flag(&self, id: &str) -> u8 {
        self.active.lock().get(id).copied().unwrap_or(STOP_NONE)
    }

    pub fn status(&self, loaded: impl Fn(&str) -> bool) -> Vec<ModelStatus> {
        let st = self.state.lock().clone();
        MODELS
            .iter()
            .map(|m| ModelStatus {
                id: m.id.into(),
                kind: m.kind,
                label: m.label.into(),
                size: m.size,
                license: m.license.into(),
                installed: self.is_installed(m.id),
                loaded: loaded(m.id),
                downloading: self.is_downloading(m.id),
                partial: std::fs::metadata(self.part_path(m)).map(|md| md.len()).unwrap_or(0),
                paused: self.paused_marker(m).exists(),
                download: st.get(m.id).cloned(),
            })
            .collect()
    }

    /// Current (unfinished) download of `id`, for "still downloading (43%)" messages.
    pub fn active_download(&self, id: &str) -> Option<Download> {
        self.state.lock().get(id).cloned().filter(|d| !matches!(d.state.as_str(), "done" | "cancelled"))
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
        !self.is_installed(id) && !self.is_downloading(id) && !self.paused_marker(m).exists()
    }

    /// Pause: stop now, keep the partial file; resumes from there later (also after a restart).
    pub fn pause(&self, id: &str) {
        if let Some(f) = self.active.lock().get_mut(id) {
            *f = STOP_PAUSE;
        }
    }

    /// Cancel: stop and discard the partial file.
    pub fn cancel(&self, id: &str) {
        let mut active = self.active.lock();
        if let Some(f) = active.get_mut(id) {
            *f = STOP_CANCEL;
        } else if let Ok(m) = model(id) {
            drop(active);
            let _ = std::fs::remove_file(self.part_path(m));
            let _ = std::fs::remove_file(self.paused_marker(m));
            self.state.lock().remove(id);
        }
    }

    /// Download (or resume) a model; progress and state changes go to `emit`.
    pub async fn download(&self, http: &reqwest::Client, id: &str, emit: impl Fn(&Download)) -> Result<()> {
        let m = model(id)?;
        {
            let mut active = self.active.lock();
            if active.contains_key(id) {
                return Ok(()); // already running; the caller just follows the events
            }
            active.insert(id.to_string(), STOP_NONE);
        }
        let _ = std::fs::remove_file(self.paused_marker(m));
        let set = |d: Download| {
            emit(&d);
            self.state.lock().insert(d.id.clone(), d);
        };
        let res = self.download_inner(http, m, &set).await;
        let stop = self.active.lock().remove(id).unwrap_or(STOP_NONE);
        let base = Download { id: m.id.into(), total: m.size, ..Default::default() };
        match res {
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
                self.state.lock().remove(id);
                Ok(())
            }
            Err(e) => {
                let downloaded = std::fs::metadata(self.part_path(m)).map(|md| md.len()).unwrap_or(0);
                set(Download { state: "error".into(), downloaded, error: Some(format!("{e:#}")), ..base });
                Err(e)
            }
        }
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
            if self.stop_flag(m.id) != STOP_NONE {
                bail!("stopped");
            }
            if have < m.size {
                set(Download { state: if attempt == 0 { "connecting" } else { "retrying" }.into(), downloaded: have, attempt, ..base.clone() });
                match self.fetch(http, m, &part, have, set).await {
                    Ok(()) => {}
                    Err(e) if self.stop_flag(m.id) != STOP_NONE => return Err(e),
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
                            if self.stop_flag(m.id) != STOP_NONE {
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
            retry_if_busy(|| std::fs::rename(&part, self.dir.join(m.file)))?;
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
            if self.stop_flag(m.id) != STOP_NONE {
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

    /// Delete a model file (and any partial download). Marks it as deliberately removed so it
    /// isn't silently re-downloaded on the next launch.
    pub fn delete(&self, id: &str) -> Result<()> {
        let m = model(id)?;
        let p = self.path(id)?;
        if Path::new(&p).exists() {
            retry_if_busy(|| std::fs::remove_file(&p))?;
        }
        let _ = std::fs::remove_file(self.part_path(m));
        let _ = std::fs::write(self.paused_marker(m), b"");
        self.state.lock().remove(id);
        Ok(())
    }
}

/// Windows: a freshly written multi-GB file is often held open for a moment by another process
/// (antivirus scan, indexer), and deleting/renaming it then fails with a sharing violation —
/// seen once in testing as "(os error 32)" on Delete. Retry for up to ~3 s.
fn retry_if_busy<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tries = 0;
    loop {
        match op() {
            // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
            Err(e) if cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)) && tries < 15 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            other => return other,
        }
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
