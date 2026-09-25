//! On-device text model: llama.cpp's `llama-server`, bundled as a sidecar binary (Metal on
//! macOS, Vulkan on Windows — integrated GPUs included) and kept running with the model loaded.
//!
//! A separate process rather than linking llama.cpp in: whisper.cpp is already linked in and
//! both vendor their own copy of ggml (duplicate symbols). The server listens on a random
//! localhost port behind a random API key, keeps the shared prompt prefix (instructions +
//! examples) in its KV cache, and is restarted transparently if it dies.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _, Result};
use parking_lot::Mutex;
use sori_core::llm::{ChatRequest, LlmCall, LocalServer};
use tokio::process::{Child, Command};

use crate::models::ModelStore;

struct Running {
    child: Child,
    model: String,
    base_url: String,
    api_key: String,
}

pub struct LlmServer {
    models: Arc<ModelStore>,
    log_path: PathBuf,
    /// Serializes start/stop; held across the (async) startup.
    run: tokio::sync::Mutex<Option<Running>>,
    /// Model id currently loaded and healthy (for status without awaiting the lock).
    ready: Mutex<Option<String>>,
    http: reqwest::Client,
}

/// `llama-server` next to our own executable (where Tauri puts sidecars), or the freshly
/// built one in `binaries/` during development.
pub fn server_binary() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let beside = std::env::current_exe().ok()?.parent()?.join(exe);
    if beside.is_file() {
        return Some(beside);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    std::fs::read_dir(dev).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("llama-server-"))
    })
}

fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

impl LlmServer {
    pub fn new(models: Arc<ModelStore>, log_path: PathBuf) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(120)).no_proxy().build().unwrap_or_default();
        Self { models, log_path, run: tokio::sync::Mutex::new(None), ready: Mutex::new(None), http }
    }

    /// Model id that is loaded and serving, if any.
    pub fn loaded_model(&self) -> Option<String> {
        self.ready.lock().clone()
    }

    /// Start (or switch to) `model`; returns (base_url, api_key). ~2–4 s the first time.
    pub async fn ensure_running(&self, model: &str) -> Result<(String, String)> {
        let mut run = self.run.lock().await;
        if let Some(r) = run.as_mut() {
            if r.model == model && matches!(r.child.try_wait(), Ok(None)) {
                return Ok((r.base_url.clone(), r.api_key.clone()));
            }
            log::warn!("llama-server: restarting ({} → {model})", r.model);
            let _ = r.child.start_kill();
        }
        *run = None;
        *self.ready.lock() = None;

        if !self.models.is_installed(model) {
            if let Some(d) = self.models.active_download(model).filter(|d| d.state != "error" && d.state != "paused") {
                bail!("The text model is still downloading ({}%)", d.downloaded * 100 / d.total.max(1));
            }
            bail!("The on-device text model isn't downloaded yet (Settings → AI)");
        }
        let bin = server_binary().ok_or_else(|| anyhow!("llama-server is missing from this build"))?;
        let path = self.models.path(model)?;
        let port = free_port()?;
        let api_key = uuid::Uuid::new_v4().simple().to_string();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
        let log = std::fs::File::create(&self.log_path).ok();
        let mut cmd = Command::new(&bin);
        cmd.arg("-m")
            .arg(&path)
            .args(["--host", "127.0.0.1", "--port", &port.to_string(), "--api-key", &api_key])
            // One slot so every request reuses the cached prompt prefix; all layers on the GPU
            // (falls back to CPU automatically when there's none).
            .args(["-c", "4096", "-np", "1", "-ngl", "99", "-fa", "auto", "--jinja", "--no-webui"])
            .args(["-t", &threads.to_string(), "--cache-reuse", "256"])
            .stdin(Stdio::null())
            .stdout(log.as_ref().and_then(|f| f.try_clone().ok()).map(Stdio::from).unwrap_or_else(Stdio::null))
            .stderr(log.map(Stdio::from).unwrap_or_else(Stdio::null))
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let t = Instant::now();
        let mut child = cmd.spawn().with_context(|| format!("Could not start {}", bin.display()))?;
        #[cfg(windows)]
        if let Some(h) = child.raw_handle() {
            crate::platform::kill_with_app(h);
        }
        let base_url = format!("http://127.0.0.1:{port}");
        // Wait for the model to load.
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                bail!("The text model server exited while loading ({status}); see {}", self.log_path.display());
            }
            if t.elapsed() > Duration::from_secs(90) {
                let _ = child.start_kill();
                bail!("The text model took too long to load");
            }
            let ok = self.http.get(format!("{base_url}/health")).timeout(Duration::from_secs(2)).send().await.map(|r| r.status().is_success()).unwrap_or(false);
            if ok {
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        log::info!("llama-server ready: {model} on :{port} in {}ms", t.elapsed().as_millis());
        *run = Some(Running { child, model: model.to_string(), base_url: base_url.clone(), api_key: api_key.clone() });
        *self.ready.lock() = Some(model.to_string());
        Ok((base_url, api_key))
    }

    pub async fn stop(&self) {
        if let Some(mut r) = self.run.lock().await.take() {
            let _ = r.child.start_kill();
            log::info!("llama-server stopped");
        }
        *self.ready.lock() = None;
    }

    /// Kill without awaiting (app exit).
    pub fn kill_now(&self) {
        if let Ok(mut run) = self.run.try_lock() {
            if let Some(r) = run.as_mut() {
                let _ = r.child.start_kill();
            }
        }
    }

    pub async fn complete(&self, model: &str, req: &ChatRequest) -> Result<String> {
        let (base_url, api_key) = self.ensure_running(model).await?;
        let t = Instant::now();
        let out = LocalServer { http: &self.http, base_url, api_key }.chat(req).await;
        if out.is_err() {
            // A crashed server is restarted on the next request.
            if let Some(r) = self.run.lock().await.as_mut() {
                if !matches!(r.child.try_wait(), Ok(None)) {
                    *self.ready.lock() = None;
                }
            }
        }
        log::info!("local llm {model}: {}ms", t.elapsed().as_millis());
        out
    }
}

/// The on-device model as the pipeline's text model.
pub struct LocalLlm {
    pub server: Arc<LlmServer>,
    pub model: String,
}

impl LlmCall for LocalLlm {
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.server.complete(&self.model, req))
    }
}
