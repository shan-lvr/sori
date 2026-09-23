//! Claude Code CLI as the text model: find it, install it (official installer, no admin
//! rights), sign in (browser OAuth), and run one-shot `claude -p` completions on the user's
//! own Claude subscription.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context as _, Result};
use parking_lot::Mutex;
use serde::Serialize;
use sori_core::llm::{ChatRequest, LlmCall};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

#[derive(Serialize, Clone, Default)]
pub struct CliStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub logged_in: bool,
    pub auth_method: String,
    pub installing: bool,
    pub logging_in: bool,
}

pub struct ClaudeCli {
    installing: AtomicBool,
    login: Mutex<Option<(Child, Option<ChildStdin>)>>,
    workers: Mutex<std::collections::HashMap<String, Slot>>,
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// Where the CLI lives: the official installer's location first, then common package
/// managers, then whatever the user's login shell finds.
pub fn find_binary() -> Option<PathBuf> {
    let h = home();
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    let mut candidates = vec![h.join(".local/bin").join(exe), h.join(".claude/local").join(exe)];
    if !cfg!(windows) {
        candidates.push("/opt/homebrew/bin/claude".into());
        candidates.push("/usr/local/bin/claude".into());
    }
    if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    let out = if cfg!(windows) {
        std::process::Command::new("where").arg("claude").output().ok()
    } else {
        std::process::Command::new("/bin/zsh").args(["-lc", "command -v claude"]).output().ok()
    }?;
    let first = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    (!first.is_empty() && std::path::Path::new(&first).is_file()).then(|| first.into())
}

fn cmd(bin: &PathBuf) -> Command {
    let mut c = Command::new(bin);
    // GUI apps get a minimal PATH; the CLI may shell out (e.g. to open the browser).
    let path = std::env::var("PATH").unwrap_or_default();
    let extra = if cfg!(windows) { String::new() } else { format!("{}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin", home().display()) };
    c.env("PATH", if extra.is_empty() { path } else { format!("{extra}:{path}") });
    // Always use the subscription login, never a stray API key from the environment.
    c.env_remove("ANTHROPIC_API_KEY");
    c.kill_on_drop(true);
    #[cfg(windows)]
    {
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

impl ClaudeCli {
    pub fn new() -> Self {
        Self { installing: AtomicBool::new(false), login: Mutex::new(None), workers: Mutex::new(Default::default()) }
    }

    pub async fn status(&self) -> CliStatus {
        let mut st = CliStatus {
            installing: self.installing.load(Ordering::SeqCst),
            logging_in: self.login.lock().is_some(),
            auth_method: "none".into(),
            ..Default::default()
        };
        let Some(bin) = find_binary() else { return st };
        st.installed = true;
        st.path = Some(bin.display().to_string());
        if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(10), cmd(&bin).arg("--version").output()).await {
            st.version = String::from_utf8_lossy(&out.stdout).split_whitespace().next().map(str::to_string);
        }
        if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(10), cmd(&bin).args(["auth", "status", "--json"]).output()).await {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                st.logged_in = v["loggedIn"].as_bool().unwrap_or(false);
                st.auth_method = v["authMethod"].as_str().unwrap_or("none").to_string();
            }
        }
        st
    }

    /// Run Anthropic's official installer (installs into the home folder; no admin prompt).
    pub async fn install(&self, on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
        if self.installing.swap(true, Ordering::SeqCst) {
            bail!("Already installing");
        }
        let res = async {
            let mut c = if cfg!(windows) {
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "irm https://claude.ai/install.ps1 | iex"]);
                c
            } else {
                let mut c = Command::new("/bin/bash");
                c.args(["-c", "curl -fsSL https://claude.ai/install.sh | bash"]);
                c
            };
            c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
            let mut child = c.spawn().context("Could not start the installer")?;
            let on_line = Arc::new(on_line);
            let pumps = pump(&mut child, on_line.clone());
            let status = tokio::time::timeout(Duration::from_secs(600), child.wait()).await.map_err(|_| anyhow!("Installer timed out"))??;
            for p in pumps {
                let _ = p.await;
            }
            if !status.success() {
                bail!("Installer exited with {status}");
            }
            if find_binary().is_none() {
                bail!("Installer finished but the claude command was not found");
            }
            Ok(())
        }
        .await;
        self.installing.store(false, Ordering::SeqCst);
        res
    }

    /// Start `claude auth login` (opens the browser; completes by itself via a local callback).
    /// Output lines — including the sign-in URL — go to `on_line`. Resolves when it exits.
    pub async fn login(self: &Arc<Self>, on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
        let bin = find_binary().ok_or_else(|| anyhow!("Claude Code CLI is not installed"))?;
        if self.login.lock().is_some() {
            bail!("Sign-in already in progress");
        }
        let mut child = cmd(&bin)
            .args(["auth", "login", "--claudeai"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start claude auth login")?;
        let stdin = child.stdin.take();
        let pumps = pump(&mut child, Arc::new(on_line));
        *self.login.lock() = Some((child, stdin));
        // Poll for exit without holding the lock across awaits.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        let result = loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let mut g = self.login.lock();
            let Some((child, _)) = g.as_mut() else { break Err(anyhow!("Sign-in cancelled")) };
            match child.try_wait() {
                Ok(Some(status)) => {
                    *g = None;
                    break if status.success() { Ok(()) } else { Err(anyhow!("Sign-in failed ({status})")) };
                }
                Ok(None) if tokio::time::Instant::now() > deadline => {
                    *g = None;
                    break Err(anyhow!("Sign-in timed out"));
                }
                Ok(None) => {}
                Err(e) => {
                    *g = None;
                    break Err(e.into());
                }
            }
        };
        for p in pumps {
            let _ = p.await;
        }
        result
    }

    /// Fallback when the browser shows a code instead of returning to the app.
    pub async fn submit_code(&self, code: &str) -> Result<()> {
        let stdin = self.login.lock().as_mut().and_then(|(_, s)| s.take());
        let mut stdin = stdin.ok_or_else(|| anyhow!("No sign-in in progress"))?;
        stdin.write_all(format!("{}\n", code.trim()).as_bytes()).await?;
        stdin.flush().await?;
        self.login.lock().as_mut().map(|(_, s)| *s = Some(stdin));
        Ok(())
    }

    pub fn cancel_login(&self) {
        if let Some((mut child, _)) = self.login.lock().take() {
            let _ = child.start_kill();
        }
    }
}

/// Forward a child's stdout/stderr lines to a callback.
fn pump(child: &mut Child, on_line: Arc<dyn Fn(String) + Send + Sync>) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = vec![];
    if let Some(out) = child.stdout.take() {
        let f = on_line.clone();
        handles.push(tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                f(strip_ansi(&l));
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let f = on_line;
        handles.push(tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                f(strip_ansi(&l));
            }
        }));
    }
    handles
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                // CSI: ESC [ … final byte
                Some('[') => {
                    chars.next();
                    for n in chars.by_ref() {
                        if n.is_ascii_alphabetic() || n == '~' {
                            break;
                        }
                    }
                }
                // OSC (e.g. hyperlinks): ESC ] … BEL | ESC \
                Some(']') => {
                    chars.next();
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
            continue;
        }
        if c != '\r' {
            out.push(c);
        }
    }
    out
}

/// First http(s) URL in a line of CLI output.
pub fn find_url(line: &str) -> Option<String> {
    let i = line.find("https://")?;
    let url: String = line[i..].chars().take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'' && *c != ')').collect();
    (url.len() > 12).then_some(url)
}

// ---------------------------------------------------------------- completions

/// The worker's fixed system prompt; the real instructions ride along in each message so
/// one warm process can serve every mode and app.
const WORKER_SYSTEM: &str = "You are the text engine of a voice dictation app. Every message contains <instructions> followed by the input. Follow the instructions exactly and output only the requested result. Each message is independent: never refer to or reuse earlier messages.";

/// Flags that keep Claude Code lean: no tools, MCP, settings/CLAUDE.md or session files.
const LEAN: &[&str] = &["--no-session-persistence", "--strict-mcp-config", "--exclude-dynamic-system-prompt-sections", "--setting-sources", ""];

/// A long-lived `claude -p --input-format stream-json` session. Startup (~0.5 s) is paid
/// once; each request then costs only the API round trip. The conversation is `/clear`ed
/// after every request so dictations never see each other.
struct Worker {
    child: Child,
    stdin: ChildStdin,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    turns: u32,
}

impl Worker {
    fn spawn(model: &str, effort: &str) -> Result<Self> {
        let bin = find_binary().ok_or_else(|| anyhow!("Claude Code CLI is not installed (Settings → AI)"))?;
        let dir = std::env::temp_dir().join("sori-claude");
        let _ = std::fs::create_dir_all(&dir);
        let mut child = cmd(&bin)
            .current_dir(&dir)
            .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
            .args(LEAN)
            .args(["--model", model, "--effort", effort, "--system-prompt", WORKER_SYSTEM, "--tools", ""])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Could not start claude")?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        log::info!("claude worker started ({model}, effort {effort})");
        Ok(Self { child, stdin, lines: BufReader::new(stdout).lines(), turns: 0 })
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Send one user message and wait for its `result` event.
    async fn send(&mut self, content: &str) -> Result<serde_json::Value> {
        let msg = serde_json::json!({"type": "user", "message": {"role": "user", "content": content}});
        self.stdin.write_all(format!("{msg}\n").as_bytes()).await?;
        self.stdin.flush().await?;
        loop {
            let line = self.lines.next_line().await?.ok_or_else(|| anyhow!("Claude Code exited"))?;
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            if v["type"] == "result" {
                return Ok(v);
            }
        }
    }
}

type Slot = Arc<tokio::sync::Mutex<Option<Worker>>>;

impl ClaudeCli {
    fn slot(&self, model: &str, effort: &str) -> Slot {
        self.workers.lock().entry(format!("{model}/{effort}")).or_default().clone()
    }

    /// Start a worker ahead of time so the first dictation doesn't pay for startup.
    pub fn warm(self: &Arc<Self>, model: &str, effort: &str) {
        let slot = self.slot(model, effort);
        let (model, effort) = (model.to_string(), effort.to_string());
        tauri::async_runtime::spawn(async move {
            let Ok(mut g) = slot.try_lock() else { return };
            if g.as_mut().map_or(true, |w| !w.alive()) {
                *g = Worker::spawn(&model, &effort).map_err(|e| log::warn!("claude warm-up: {e:#}")).ok();
            }
        });
    }

    /// Drop all workers (after sign-in changes or when the provider is switched off).
    pub fn reset_workers(&self) {
        self.workers.lock().clear();
    }

    pub async fn complete(&self, req: &ChatRequest, effort: &str) -> Result<String> {
        if req.web {
            return one_shot(req, effort).await;
        }
        let slot = self.slot(&req.model, effort);
        // Busy (e.g. a history retry while dictating): don't queue behind it.
        let Ok(mut g) = tokio::time::timeout(Duration::from_millis(300), slot.clone().lock_owned()).await else {
            return one_shot(req, effort).await;
        };
        let content = format!("<instructions>\n{}\n</instructions>\n\n{}", req.system, req.user);
        let mut last_err = anyhow!("Claude Code failed");
        for _ in 0..2 {
            if g.as_mut().map_or(true, |w| !w.alive()) {
                *g = Some(Worker::spawn(&req.model, effort)?);
            }
            let w = g.as_mut().unwrap();
            let t = std::time::Instant::now();
            match tokio::time::timeout(Duration::from_secs(60), w.send(&content)).await {
                Ok(Ok(v)) => {
                    w.turns += 1;
                    let res = parse_result(&v, &req.model, t);
                    if matches!(&res, Err(e) if e.to_string().contains("Not signed in")) || w.turns > 500 {
                        *g = None;
                    } else {
                        // Forget this exchange in the background; the next request waits for it (~10 ms).
                        tokio::spawn(async move {
                            if let Some(w) = g.as_mut() {
                                if !matches!(tokio::time::timeout(Duration::from_secs(5), w.send("/clear")).await, Ok(Ok(_))) {
                                    *g = None;
                                }
                            }
                        });
                    }
                    return res;
                }
                Ok(Err(e)) => last_err = e,
                Err(_) => last_err = anyhow!("Claude Code timed out"),
            }
            *g = None; // broken worker: start a fresh one and retry once
        }
        Err(last_err)
    }
}

fn parse_result(v: &serde_json::Value, model: &str, t: std::time::Instant) -> Result<String> {
    let text = v["result"].as_str().unwrap_or("").to_string();
    log::info!("claude {model}: {}ms (api {}ms)", t.elapsed().as_millis(), v["duration_api_ms"].as_u64().unwrap_or(0));
    if text.starts_with("Not logged in") || text.contains("Please run /login") {
        bail!("Not signed in to Claude — open Sori → Settings → AI and click Sign in");
    }
    if v["is_error"].as_bool().unwrap_or(false) {
        bail!("Claude Code: {text}");
    }
    if text.trim().is_empty() {
        bail!("Claude Code returned an empty response");
    }
    Ok(text)
}

/// Fresh `claude -p` process for requests the worker can't serve (web search, concurrency).
async fn one_shot(req: &ChatRequest, effort: &str) -> Result<String> {
    let bin = find_binary().ok_or_else(|| anyhow!("Claude Code CLI is not installed (Settings → AI)"))?;
    let dir = std::env::temp_dir().join("sori-claude");
    let _ = std::fs::create_dir_all(&dir);
    let tools = if req.web { "WebSearch" } else { "" };
    let mut c = cmd(&bin);
    c.current_dir(&dir)
        .args(["-p", "--output-format", "json", "--disable-slash-commands"])
        .args(LEAN)
        .args(["--model", &req.model, "--effort", effort, "--system-prompt", &req.system, "--tools", tools]);
    if req.web {
        c.args(["--allowed-tools", "WebSearch"]);
    }
    let t = std::time::Instant::now();
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("Could not start claude")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(req.user.as_bytes()).await?;
    }
    let out = tokio::time::timeout(Duration::from_secs(if req.web { 120 } else { 60 }), child.wait_with_output())
        .await
        .map_err(|_| anyhow!("Claude Code timed out"))??;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| anyhow!("Claude Code failed: {}", String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()))?;
    parse_result(&v, &req.model, t)
}

/// Claude Code as the pipeline's text model.
pub struct ClaudeLlm {
    pub cli: Arc<ClaudeCli>,
    pub effort: String,
}

impl LlmCall for ClaudeLlm {
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.cli.complete(req, &self.effort))
    }
}
