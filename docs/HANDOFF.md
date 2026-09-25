# Sori — status & handoff

_Last updated: 2026-09-25 (first run on a real Windows PC — see §2 for results). Written for whoever (human or coding agent) picks this up next._

Sori is a voice-dictation app: press a shortcut in any text field, speak, and a cleaned-up version is pasted at the cursor. The owner uses it **mainly to dictate prompts to AI coding agents** (Claude Code, Codex, Cursor) instead of typing them; chat, email and notes are secondary. Everything runs on-device by default. See [README.md](../README.md) for the user-facing overview.

---

## 1. Where things stand

| Area | macOS (Apple Silicon) | Windows 10/11 |
|---|---|---|
| Builds | ✅ locally + CI (`.dmg`) | ✅ locally (Windows 11, VS 2022 Build Tools) + CI (`nsis`); see §2 Build for the path-length note |
| Runs / tested | ✅ daily use | ✅ first real PC (2026-09-25): automated end-to-end with injected input over RDP · ⏳ human test with a real mic/keyboard at the PC |
| Global shortcuts | ✅ CGEventTap, `Fn` (swallows the 🌐 action) | ✅ `Ctrl+Win` start/stop, no Start menu, lone Win still opens Start, Esc, Ask notice (injected input) |
| Paste at cursor | ✅ ⌘V via CGEvent | ✅ Notepad, Chrome textarea, Windows Terminal; focus stays; clipboard restore ✅ |
| Focused-field / selection detection | ✅ Accessibility API | ⚠️ only the system caret (`GetGUIThreadInfo`) + Ctrl+C for selection |
| HUD voice bar / answer card | ✅ | ✅ bottom-center, never takes focus (`WS_EX_NOACTIVATE`); answer card untested (Ask is admin-only) |
| On-device speech (Whisper) | ✅ Metal | ✅ Vulkan: RTX 5080 0.2–0.4 s · ⚠️ Intel iGPU 8 s · ❌ CPU ~3 min per clip |
| On-device text model (llama-server) | ✅ Metal, measured | ✅ Vulkan: RTX 5080 0.16–0.3 s per call · Intel iGPU ~2 s · CPU fallback works (no Vulkan driver) |
| Model downloads (resume/verify) | ✅ tested (pause, kill -9, corrupt file) | ✅ parallel, resume after a forced quit, SHA-256 |
| UI (English/Korean) | ✅ | same code; Windows-specific wording handled (`isWin`) |
| Code signing | self-signed "Sori Dev" (local) / ad-hoc (CI); not notarized | unsigned → SmartScreen warning |

### Done and verified on macOS
- Dictate / Translate / Ask anything, toggle or hold-to-talk, Esc cancels, 9-minute cap.
- HUD voice bar (recording / transcribing / thinking), answer card for Ask.
- History (retry with the same audio, copy, export audio, delete; retention), Dictionary, Insights (time saved, streaks, heatmap).
- English default UI with full Korean translation (`L("English", "한국어")` helper in `desktop/src/i18n.ts`; Rust side uses `controller::tr`).
- One-click "Set up everything" on Home: permissions + both model downloads.
- On-device speech: Whisper large-v3-turbo q5_0 (574 MB). Korean CER ~2.8%, ~0.5–1 s/sentence on M4 Pro.
- On-device text model: llama.cpp `llama-server` sidecar + **Gemma 4 E2B** (3.1 GB, default) or Kanana-2 1.3B (0.85 GB). 0.4–1.8 s per dictation on M4 Pro GPU.
- Model downloader: per-model state, parallel downloads, HTTP Range resume (also after quitting), automatic retries with backoff, disk-space check, SHA-256 verification, pinned Hugging Face revisions, pause/resume/cancel/delete, progress in sidebar + Home + Settings.
- **Admin mode** (owner only, `desktop/src-tauri/src/admin.rs`): cloud engines — ElevenLabs Scribe v2 (speech), OpenRouter (text; default `google/gemini-3.8-flash`) — API keys and cloud model choice require unlocking with the owner's password. Locked ⇒ the backend forces Local AI and clears keys (`admin::enforce_policy`, applied on load, save and lock). Builds bake a salted PBKDF2-SHA256 verifier (600k rounds) from `SORI_ADMIN_PASSWORD` (`.env.local` or CI secret); personal builds also bake the owner's keys AES-256-GCM-encrypted under the password-derived key (`build.rs`, `src/admin_crypto.rs`) — no plaintext keys in any binary. Unlock state persists per device in `admin.key` until Lock. Check: `SORI_ADMIN_PASSWORD=… cargo run -p sori --example admin_check -- <password>`.
- **Ask anything is disabled on Local AI** (a small local model can't answer or look things up): shown greyed out with "API mode only — admin"; `Settings::ask_available()`; the controller refuses to start an Ask session (HUD notice) and ignores a mid-recording switch to Ask; `pipeline::process_text` also refuses.
- UI names local models simply: "Local AI — Whisper (574 MB)", "Gemma (3.1 GB)", "Kanana (0.85 GB)" — no version/quantization details (owner's request).
- Developer mode: ~200 dev terms as recognition hints, tech-term restoration in cleanup, per-app tone.
- Prompts tuned for the main use case (coding-agent prompts) — see §4.

### Removed / abandoned (don't resurrect without a reason)
- **Claude Code CLI as the text model** — `claude -p` was 2.5–3.5 s (Haiku 4.5 always "thinks"; `MAX_THINKING_TOKENS=0` got it to ~1.6–2.3 s, still ~1 s of CLI overhead from its post-turn summary). Replaced by the on-device model.
- Local STT alternatives tested and rejected: SenseVoice, Moonshine-ko, Fun-ASR-Nano, Qwen3-ASR, Cohere Transcribe, Whisper small (details in `docs/NOTES.ko.md`).
- Local text models rejected: Qwen3.5 0.8B/2B (summarize, copy example text into answers), LFM2.5 1.2B and EXAONE 4.0 1.2B (answer/summarize instead of cleaning up). Qwen3.5 4B is good but ~2× slower than Gemma 4 E2B.

---

## 2. Windows verification checklist

Log file: `%LOCALAPPDATA%\com.seyoon.sori\logs\sori.log` (UTC timestamps); data (settings, models, history): `%APPDATA%\com.seyoon.sori\`; llama-server output: `%APPDATA%\com.seyoon.sori\llama-server.log`.

**First run, 2026-09-25** — Core Ultra 9 285K (Intel iGPU) + RTX 5080, Windows 11 Pro, over RDP. There was no capture device in the RDP session, so dictation ran end-to-end with injected input (`SendInput`: the LL hooks see it like real keys) and `SORI_DEBUG_WAV` (which now works without a mic) using English TTS clips; windows/focus/text were checked with Win32 + UI Automation and the WebView2 DevTools port (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N`). ✅ verified · ❌ found broken (fixed unless noted) · ⏳ still needs a person at the PC (real mic/keyboard). Measurements: `docs/NOTES.ko.md` → "Windows 실기기 측정".

### Build
- ✅ Local build with VS 2022 Build Tools (C++), CMake, LLVM (`LIBCLANG_PATH` for bindgen), Vulkan SDK, Rust stable, Node 22, Git Bash:
  ```bash
  npm --prefix desktop ci
  ./scripts/build-llama-server.sh          # Git Bash; binaries/llama-server-x86_64-pc-windows-msvc.exe (Vulkan, static CRT, no OpenMP)
  cd desktop && npx tauri build --bundles nsis
  ```
  ❌→✅ **Path length**: MSBuild fails (`MSB4018`/`MSB6003`, "260자") in ggml's nested `vulkan-shaders-gen` build when the checkout path is long (`C:\Users\<name>\…\sori\target\…` is enough). CI has long paths enabled. Locally either enable Windows long paths (`LongPathsEnabled=1`, admin) or use a short target dir: `$env:CARGO_TARGET_DIR="C:\t"` (the sidecar script follows it). Set `$env:GGML_NATIVE="OFF"` for installers you share (CI does) — otherwise whisper.cpp targets the build machine's CPU.
- ❌→✅ Clean-PC dependencies: Sori.exe needed `MSVCP140.dll`, llama-server `VCOMP140.DLL` (VC++ redistributable). Now only system DLLs + `vulkan-1.dll` (`dumpbin /dependents`).
- ✅ CI (`build` workflow) green on test/macos/windows.

### Install & first run
- ✅ Installer runs per-user without an admin prompt; HKCU uninstall entry; files in `%LOCALAPPDATA%\Sori`. SmartScreen not seen (the file had no Mark of the Web). ⏳ run it from Explorer — when launched from inside the Claude desktop app (MSIX) AppData writes are virtualized into `…\Packages\Claude_…\LocalCache`.
- ❌→✅ **Main window was blank on every first launch** (Tauri creates windows before `setup` manages the state; WebView2's first `get_settings` failed and nothing retried). Fixed with `settingsWhenReady()`.
- ✅ Settings → AI shows only Local AI (Whisper 574 MB, Gemma 3.1 GB, Kanana 0.85 GB); no Admin section in a build without `SORI_ADMIN_PASSWORD` (`admin_status.available=false`).
- ✅ Both downloads start at first launch, in parallel (574 MB in 16 s, 3.1 GB in 59 s here). ✅ Forced quit at 378/574 MB → relaunch fetched only the remaining 177 MB (Range) → SHA-256 → installed. ⏳ "Set up everything" button/pills by eye, and a pulled network cable ("retrying (n/5)").
- ❌→✅ Deleting a model once failed with `(os error 32)` (file briefly held by another process); delete/rename now retry on sharing violations.
- ✅ Startup: `local stt loaded … on NVIDIA GeForce RTX 5080`, `llama-server ready` 3.8–5.9 s (13.6 s the very first time: shader compile), `local llm primed`.

### Shortcuts (`desktop/src-tauri/src/platform/windows.rs`)
- ✅ `Ctrl+Win` starts/stops dictation; HUD bottom-center of the monitor under the mouse.
- ✅ Start menu does **not** open after `Ctrl+Win` (0xE8 mask key); a lone Win tap still opens Start.
- ✅ `Ctrl+Win+Shift` translates (English → Korean target worked). ✅ `Esc` cancels (nothing pasted).
- ❌→✅ `Ctrl+Win+Space` (Ask) on Local AI used to keep dictating silently (it arrives as a switch after `Ctrl+Win` starts a dictation); now cancels and shows the "API mode" notice.
- ✅ Shortcut recording (what Settings → Shortcuts → Add uses): Right Ctrl+D → `ControlRight+KeyD`, mouse back → `Mouse4`, middle → `MouseMiddle`, 한/영 → `Lang1`; a lone 한/영 press never starts a session.
- ⏳ Normal typing with Sori running (no lost/delayed keys, hook not dropped after a while). Injected-typing tests behaved identically with Sori on and off, but the Korean IME made them unreliable — needs real keyboard.

### Paste & context
- ✅ Pasted at the cursor, focus stays on the target: Notepad, Chrome textarea, Windows Terminal (PowerShell prompt; not executed). ⏳ Claude desktop, Orca, Cursor, Slack (not automated: they hold the owner's live sessions), VS Code (not installed), multi-line paste into Windows Terminal (its multi-line warning).
- ✅ "Also copy to clipboard" OFF → the previous clipboard text is restored.
- ✅ History: `win:notepad.exe` / `win:chrome.exe` / `win:windowsterminal.exe` with window titles; a terminal titled "…Claude Code" gets the coding-agent prompt. App names now come from the exe's FileDescription ("Google Chrome", "Windows Terminal") instead of "chrome"/"WindowsTerminal".
- ❌→✅ **Cleanup fell back to the raw transcript ("Um … uh …") for "can you check …?"** in every app: each level rewrote it as an instruction and `lost_question` rejected it. Polite requests may now become instructions; real questions still must stay questions (§4).
- ⏳ Ask with selected text (admin/API mode only now); elevated target apps (`SendInput` blocked — should fail gracefully).

### Overlays
- ✅ HUD transparent (no box), always on top, never takes focus (`WS_EX_NOACTIVATE`; foreground stayed on the target throughout), clickable only in hands-free mode.
- ⏳ Answer card (Ask is admin-only; not reachable in a team build).

### On-device models on Windows GPUs
- ❌→✅ **Whisper ran on the integrated GPU** while llama.cpp used the RTX 5080 (whisper.cpp takes Vulkan device #0 = iGPU). Now prefers the discrete GPU; the log names the device.
- ✅ RTX 5080: STT 0.3–0.4 s, cleanup 0.16–0.18 s per call → ~0.55 s from stop to paste. ❌→✅ The first dictation after install took 33 s (Vulkan shaders compiled on first use) → warm-up at load (1.1 s).
- ⚠️ Intel iGPU (Arrow Lake, 4 Xe cores; `GGML_VK_VISIBLE_DEVICES=0`): cleanup ~2 s ✅ target, but **Whisper 8.2 s** per sentence — 20 s with flash attention, which is now off on integrated GPUs. Stronger iGPUs (Arc 140V, 780M) still to measure.
- ✅ No Vulkan driver (simulated with `VK_DRIVER_FILES` pointing nowhere): the app still starts; both models fall back to the CPU. ❌ **Whisper on the CPU takes ~3 min per sentence** (llama-server on the CPU is fine, 171 tok/s prompt) — not investigated. ⏳ `vulkan-1.dll` missing entirely → Sori.exe can't start (see §3).
- ✅ Memory: RTX — Sori 1.5 GB + llama-server 2.0 GB working set; iGPU — 1.75 + 3.7 GB (shared memory). ⏳ 8 GB machine.
- ⚠️ Other first-time shapes still compile once: first translation 10 s, first agent prompt 3.5 s (cached by the driver afterwards).

### Misc
- ❌→✅ llama-server survived a Sori crash/kill (≈2 GB, and an update install silently kept the old locked `llama-server.exe`). Now in a kill-on-close job object.
- ✅ Single instance: launching again focuses the existing window.
- ⏳ Tray menu (Open / Settings / Quit), launch-at-login (HKCU Run is virtualized when started from the Claude app), "Show in taskbar", start/stop sounds (RDP audio), microphone privacy "denied" state.

---

## 3. Known gaps / not done yet

**Windows**
- Human test with a real mic and keyboard at the PC is still open (the ⏳ items in §2): typing feel with the hook, Korean speech, Claude desktop / Orca / Cursor / Slack paste, tray, launch at login, sounds, mic privacy.
- `vulkan-1.dll` missing (no GPU driver at all) → Sori.exe doesn't start (it imports the Vulkan loader). With the loader present but no Vulkan driver, both models already fall back to the CPU. Cheapest fix: ship the Khronos loader (`vulkan-1.dll`, Apache-2.0) next to Sori.exe.
- Whisper on integrated graphics is slow (8 s per sentence on an Arrow Lake iGPU, even with flash attention off) and on the CPU unusable (~3 min — looks pathological, worth profiling: the CPU build has AVX2/FMA). Options: a smaller Whisper for iGPU/CPU, or pointing such users to API mode.
- First use of a new prompt size compiles Vulkan shaders once (10 s first translation, 3.5 s first agent prompt); priming a few prompt lengths at startup would hide it.
- Building needs long paths enabled or a short `CARGO_TARGET_DIR` (§2 Build).
- Text-field / selection detection is caret-based only; UI Automation not implemented.
- "Mute while dictating" is a no-op on Windows (`mute_output`); needs the Core Audio `IAudioEndpointVolume` API.
- Installer isn't code-signed (SmartScreen). Not published to a Release yet (push a `v*` tag to make CI attach the `.exe` and `.dmg`).
- Default shortcut choice (`Ctrl+Win`) should be validated with the owner; Typeless' Windows default is reported as either Right Alt or Ctrl+Win.

**Cleanup quality seen on Windows (shared code)**
- At level 4 in a browser tab ("Other"), Gemma once translated an English dictation into Korean; the coverage guard caught it and level 3 was used. Worth a look in `LOCAL_EXAMPLES` / `language_hint`.

**On-device text model quality** (Gemma 4 E2B, 2B-class)
- Reliable: keeps language and 반말/존댓말, keeps every request/constraint/hedge, removes fillers, fixes most tech terms, formats lists.
- Weaker than cloud Gemini at deep restating: long, meta-level requests are only lightly reorganized; some misheard words survive (e.g. "폴리시" → "Politeness", "스트릭 모드" not always → Strict Mode). Owners who want the strongest polish can switch Settings → AI → Text AI to OpenRouter.
- Ask anything is intentionally disabled with the local model (see §1); it works in API mode (admin).

**Everything else**
- macOS: not notarized (Gatekeeper "Open Anyway" on first launch); Intel Macs not built.
- No auto-update.
- Streaming transcription (text appearing while speaking) not implemented.
- Dictionary auto-learning from corrections not implemented.
- Android (IME reusing `sori-core`) not started.
- First llama-server start after install/update compiles GPU shaders (~20 s on Metal) — happens in the background at app start; later starts ~1 s.

---

## 4. How cleanup prompting works (important for quality work)

Code: `crates/sori-core/src/prompts.rs`, `pipeline.rs`.

- **Purpose is stated explicitly.** Both prompt families say the speaker is a developer who mostly dictates prompts for AI coding agents; unclear words are read in that light. Per-app styles (`AppCategory::style`) turn Code/AI destinations into "a clear instruction the agent can act on: situation → request → constraints; numbered list for multiple requests".
- **Five cleanup levels** (`Settings.cleanup_style`: `minimal`, `light`, `clean`, `polished` (default), `agent`; `prompts::cleanup_level`). Each level has its own contract in `CLOUD_LEVELS` / `LOCAL_LEVELS` and its own worked-example outputs (`LOCAL_EXAMPLES`, same inputs, five outputs). "Agent" writes a structured coding-agent prompt (goal → context → numbered tasks → constraints/questions); for chat/email destinations it writes a well-organized message instead.
- **Drop guard** (`pipeline::dictate_text`): every rewrite is checked with `text::coverage_detail` (share of the transcript's content words still present; Hangul matched by stem, Latin tech terms that replaced Hangul spellings get credit) and `text::lost_question` (a real question must stay a question; a polite request phrased as one — "can you check …?", "좀 봐 줄래?" — may become an instruction, which is what an agent prompt wants). Thresholds: L1 0.8, L2 0.7, L3 0.6, L4–5 0.5. On failure it retries at a gentler level (5/4 → 3 → 2) and finally inserts the transcript as spoken (`cleanup_simplified` / `cleanup_dropped` notes in History).
- **Long transcripts on the local model** (> 260 chars) are cleaned chunk by chunk at level ≤ 3 (`text::chunk_sentences`), then restructured as a whole for levels 4–5 — a 2B model otherwise summarizes and drops the tail (seen in real use: a 490-char dictation lost 4 of 6 requests).
- **Cloud models** (OpenRouter) get `CLOUD_HEAD` (hard rules: same language/speech level, nothing lost, questions stay questions, nothing added) + the level contract + developer block, per-app style, dictionary and the user's style notes.
- **Small local models** get `LOCAL_DICTATE_SYSTEM` (short) + 5 worked examples as prior chat turns (`local_examples`). Each input carries a tag computed in code, e.g. `[Write in Korean, casual 반말 endings like ~해/~줘/~야 — no ~요 · a prompt for a coding agent]`:
  - language from the Hangul/Latin ratio (`language_hint`) — without it the model translated English input into Korean;
  - speech level from sentence endings (`korean_register`: -요, -ㅂ니다/-ㅂ니까; careful: 그러니까 / 아니다 are not polite) — without it the model drifted into 존댓말;
  - destination from the app category (`local_destination`).
  The long cloud prompt made every small model summarize, answer or translate; this format fixed it.
- The llama-server keeps the system prompt + examples in its KV cache (`cache_prompt`, one slot), so only the new words are processed per dictation.
- Safety nets in `pipeline.rs`: if the model "answers" instead of rewriting (`text::looks_like_assistant_reply`) or fails, the raw transcript is inserted and History shows why.

**Evaluate before/after any prompt change** (same code path as the app):
```bash
# cloud (needs OPENROUTER_API_KEY) and/or a local llama-server on :8080
cargo run -p sori-core --example polish_eval -- google/gemini-3.8-flash local:gemma@http://127.0.0.1:8080
CASE=3 cargo run -p sori-core --example polish_eval -- …   # one case
```
`STYLE=minimal|light|clean|polished|agent` picks the level; `EVAL_FILE=cases.json` adds private cases (keep real dictations out of git — e.g. `.scratch/`, which is ignored). Output shows coverage and any fallback note per case. The cases in `crates/sori-core/examples/polish_eval.rs` are mostly realistic coding-agent dictation plus a chat and an email case. Check: language, 반말/존댓말, every constraint ("don't commit", "바로 고치지 말고") and hedge ("maybe", "아마") survives, nothing invented, tech terms fixed.

---

## 5. Code map

```
crates/sori-core/            platform-independent (reusable for Android)
  hotkey.rs                  shortcut state machine (toggle / hold / combos / cancel)
  prompts.rs                 all prompts, app classification, language/register tags
  pipeline.rs                STT → LLM → guards; one-step (audio → Gemini) experiment
  llm.rs                     OpenRouter client, LocalServer (llama-server) client, LlmCall trait
  stt.rs, devterms.rs        ElevenLabs client, developer vocabulary
  store.rs                   SQLite history, dictionary, stats
  settings.rs                settings schema + defaults (local STT + local LLM by default)
desktop/src-tauri/src/
  controller.rs              session orchestration, HUD/card, model sync, delivery
  commands.rs                Tauri commands for the UI
  models.rs                  model catalog (ids, pinned URLs, sizes, SHA-256) + downloader
  local_stt.rs               whisper.cpp wrapper
  local_llm.rs               llama-server sidecar manager + LlmCall impl
  recorder.rs                cpal mic capture
  platform/macos/*           CGEventTap, AX, paste, overlays
  platform/windows.rs        LL hooks, SendInput, foreground app, mic privacy, overlays
desktop/src/                 React UI (main/*, overlay/Hud.tsx, overlay/Card.tsx, i18n.ts, mock.ts)
scripts/                     build-mac.sh (personal), build-team.sh (shareable DMG), build-llama-server.sh
.github/workflows/build.yml  test → macOS DMG + Windows NSIS (+ Release on v* tags)
```

Useful harnesses (same code paths as the app):
```bash
cargo test -p sori-core
npx --prefix desktop tsc --noEmit -p desktop
cargo run --release -p sori --example local_llm -- <models_dir> gemma-4-e2b          # sidecar + cleanup/translate/ask
cargo run --release -p sori --example local_stt_download -- <dir> [pause_s] [model_id]
cargo run --release -p sori --example local_stt_eval -- <models_dir> <data_dir> [prompts]
npm --prefix desktop run dev   # UI with mock backend: http://localhost:1420/?window=main|hud|card
```

## 6. Rules of the road
- Never commit `.env.local` or any key. Share `build-team.sh` / CI builds only — a `build-mac.sh` build carries the owner's keys (encrypted, but still). `build-team.sh` refuses to package if a plaintext key leaks.
- Cloud features must stay behind admin mode: anything new that uses an API key must respect `admin::enforce_policy` / `app.admin.is_unlocked()`.
- Model downloads must stay pinned (URL revision + size + SHA-256 in `models.rs`); update all three together.
- The llama.cpp version is pinned in `scripts/build-llama-server.sh` (`LLAMA_CPP_TAG`). Upgrade deliberately and re-run the evals.
- Keep UI strings bilingual via `L("English", "한국어")`.
