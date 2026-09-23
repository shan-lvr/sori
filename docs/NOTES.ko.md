# Sori

개인용 Typeless 스타일 받아쓰기 앱. 어느 앱의 텍스트 칸에서든 **말하면 → 음성 인식 → AI가 다듬어서 → 커서 위치에 붙여넣기 + 클립보드**.

- 음성 인식: ElevenLabs **Scribe v2** (한국어·영어 혼용, 사전 단어를 `keyterms`로 전달)
- 다듬기·번역·Ask: **OpenRouter** (기본 `google/gemini-3.1-flash-lite`, Ask는 `google/gemini-3.8-flash`)
- 앱: **Tauri 2** (Rust 코어 + React UI), 현재 macOS 구현

Typeless 기능 조사: [docs/typeless-spec.md](docs/typeless-spec.md)

## 단축키 (Typeless와 동일)

| 기능 | 단축키 | 동작 |
|---|---|---|
| 받아쓰기 | `Fn` | 한 번 누르면 녹음 시작 → 다시 `Fn`을 누르면 종료·처리 (설정 → 단축키 → 녹음 방식에서 '누르고 있는 동안'으로 변경 가능) |
| 번역 | `Fn` + `Left Shift` | 한국어로 말하면 번역 대상 언어(기본 English (US))로 붙여넣기 |
| 무엇이든 물어보기 | `Fn` + `Space` | 선택 텍스트 편집(교체) / 질문 → 답변 카드 / "~ 써줘" → 글쓰기 도우미 / "~ 찾아줘" → 웹 열기 |
| 취소 | `Esc` | 녹음 중 취소 |

설정 → 단축키에서 키보드·마우스 버튼 조합을 여러 개 추가할 수 있습니다.

## 처음 실행할 때

1. **Typeless 종료** — 둘 다 Fn에 반응합니다.
2. Sori 홈의 "시작하기 전에"에서 **손쉬운 사용 권한** → 시스템 설정에서 Sori 켜기 (Fn 감지·붙여넣기에 필요)
3. **마이크 권한 요청** → 허용
4. 아무 텍스트 칸에서 `Fn`을 누르고 말해 보기

## 빌드

```bash
./scripts/build-mac.sh
```

- 요구: Rust (`brew install rustup`), Node, Xcode Command Line Tools (Xcode 전체는 불필요)
- 첫 실행 시 로그인 키체인에 자체 서명 인증서 `Sori Dev`를 만들어 서명합니다 → 재빌드해도 macOS 권한이 유지됨. (키체인 접근 → 로그인 → 내 인증서에서 삭제 가능)
- 기본 API 키: 저장소 루트의 `.env.local`(git 제외)을 빌드 시 읽어 기본값으로 넣습니다. 앱 설정 → AI · API 키에서 변경/복원.
- UI만 고칠 때: `npm --prefix desktop run dev` 후 `http://localhost:1420/?window=main|hud|card` (목업 데이터)

개발용 훅 (키보드 없이 세션 테스트):

```bash
open --env SORI_DEBUG_WAV=$PWD/sample.wav /Applications/Sori.app   # 마이크 대신 파일 사용 (16 kHz mono s16 WAV)
kill -USR1 $(pgrep -x Sori)   # 받아쓰기 시작/종료 토글 (USR2 = 번역)
```

앱 안에서 기록·사전 기능 자체 테스트 (재시도·복사·오디오 저장·삭제·사전·클립보드 복원, 결과는 로그의 `SELFTEST` 줄):

```bash
open --env SORI_SELFTEST=$PWD/sample.wav /Applications/Sori.app
```

로그: `~/Library/Logs/com.seyoon.sori/sori.log` (세션 시작/종료, STT·LLM 지연, 오류)

테스트:

```bash
cargo test -p sori-core
```

실제 API로 파이프라인만 돌려보기 (wav → STT → LLM):

```bash
set -a; source .env.local; set +a
cargo run -p sori-core --example run_file -- sample.wav dictate com.tinyspeck.slackmacgap
```

## 구조

```
crates/sori-core/        플랫폼 독립 Rust (Android/Windows에서 재사용)
  hotkey.rs              단축키 상태기계 (PTT / 핸즈프리 / 조합 전환 / 취소)
  stt.rs                 ElevenLabs Scribe 클라이언트
  llm.rs                 OpenRouter 클라이언트
  prompts.rs             받아쓰기·번역·Ask 프롬프트, 앱별 톤 분류
  pipeline.rs            STT → LLM → 가드(LLM이 '답변'하면 원문 사용)
  store.rs               SQLite 기록·사전·일별 통계(연속 일수, 히트맵)
  audio.rs               16 kHz 리샘플, WAV, 레벨, 작은 목소리 증폭
desktop/src-tauri/       Tauri 셸
  controller.rs          세션 오케스트레이션, HUD/카드, 기록 저장, 재시도
  recorder.rs            cpal 마이크 캡처
  platform/macos/        CGEventTap(Fn), AX(포커스·선택 텍스트), ⌘V 붙여넣기, 오버레이 창
  platform/fallback.rs   Windows/Linux 자리표시자
desktop/src/             React UI (홈·기록·사전·설정, 음성 바 HUD, 답변 카드)
```

데이터: `~/Library/Application Support/com.seyoon.sori/` (settings.json, sori.db, audio/) · 로그: `~/Library/Logs/com.seyoon.sori/sori.log`

## 모델 선택 근거 (2026-09-22, 이 Mac에서 측정)

- ElevenLabs `scribe_v2`: 14.7초 한국어 음성 → 약 1.0–1.2초. 사전 키워드로 "일레븐랩스" → "ElevenLabs" 교정 확인.
- OpenRouter 다듬기(같은 프롬프트, 2회 측정):

| 모델 | 지연 | 비고 |
|---|---|---|
| google/gemini-3.1-flash-lite | 0.67–0.83s | 기본값. 톤 유지·리스트 정리 양호 |
| google/gemini-3.5-flash-lite | ~0.8s | |
| deepseek/deepseek-v4.1-flash | ~0.8s | |
| ~anthropic/claude-haiku-latest | ~1.6s | |
| google/gemini-3.8-flash | ~1.8s | 추론 필수, Ask 기본값 |
| openai/gpt-5.6-luna | 1.7–3.9s | |
| qwen/qwen3.8-flash, x-ai/grok-4.7 | 3–14s | 추론 모델이라 부적합 |

전체(말 끝 → 붙여넣기) 약 1.5–2.7초.

## Windows / Android 계획

- **Windows**: 같은 Tauri 앱. `platform/windows.rs`만 구현하면 됨 — `WH_KEYBOARD_LL` 훅(기본 `Right Alt` / `Right Alt+Right Shift` / `Right Alt+Space`), 클립보드 + `SendInput(Ctrl+V)`, UI Automation으로 포커스/선택 텍스트.
- **Android**: 모든 앱에 입력하려면 **키보드 앱(IME)**이어야 함 → Kotlin `InputMethodService` + `sori-core`를 JNI/UniFFI로 호출(STT·LLM·프롬프트·기록 재사용). Typeless도 모바일은 자체 키보드 방식.

## 개발자 모드 (설정 → 개인화, 기본 켜짐)

- 음성 인식: `devterms.rs`의 개발 용어 약 200개 + "내 기술 스택" + 사전을 ElevenLabs `keyterms`로 전달
- 다듬기: "화자는 개발자" 전제로 한글 음차·오인식된 기술 용어를 정식 표기로 복원(React Query, useEffect, git rebase…), 서버·배포·커밋 같은 한글 외래어는 유지, 말한 단어는 바꾸지 않음(pull request ↔ PR 변환 금지)
- 코드 에디터·AI 에이전트(터미널의 Claude Code/Codex 포함)에서는 명령어·경로·식별자를 백틱으로, 일반 셸에서는 백틱 없이
- 검증: `cargo run -p sori-core --example dev_eval -- *.wav` (개발자 모드 끔/켬 비교)

## 로컬 음성 인식 (설정 → AI · API 키 → 음성 인식 엔진)

- whisper.cpp(`whisper-rs`)로 **Whisper large-v3-turbo q5 (574MB)**를 앱 안에서 실행. macOS는 Metal, Windows는 Vulkan(빌드 시 Vulkan SDK 필요, 아직 Windows 실기기 미검증).
- 모델은 처음 선택할 때 내려받음(`~/Library/Application Support/com.seyoon.sori/models`), 앱 시작 시 백그라운드 로드(약 6초), 로드 중 메모리 약 850MB. 실패·미설치 시 ElevenLabs로 자동 전환.
- 개발 용어·사전·기술 스택을 Whisper 초기 프롬프트로 전달(문장형 — "용어:" 라벨형은 결과에 새어 나와 CER 악화).
- 이 Mac 측정(FLEURS-ko 40문장, `scripts/stt_bench.py`, `cargo run --release -p sori --example local_stt_eval -- <models> <data>`):

| 엔진 | 한국어 CER | 영어 WER | 문장당 지연 |
|---|---|---|---|
| ElevenLabs Scribe v2 (클라우드) | 2.0% | 3.8% | 1.2초 |
| 로컬 Whisper turbo q5, 언어 자동 | 2.8% (프롬프트 포함 3.0%) | 5.7% | 약 1.0초 |
| 로컬 Whisper turbo q5, 한국어 고정 | 〃 | 45.7% (영어 문장이 망가짐) | 약 0.5초 |
| Qwen3-ASR 0.6B / Cohere Transcribe / Whisper small | 6.8% / 6.2% / 6.2% | 6.3% / 7.3% / 11.1% | 0.8 / 0.6 / 0.2초 |
| SenseVoice / Moonshine-ko / Fun-ASR-Nano | 사용 불가 | | |

## 실험: 한 번에 처리 (설정 → AI · API 키 → 처리 방식)

- 받아쓰기·번역을 STT 없이 오디오째 멀티모달 LLM(기본 `google/gemini-3.8-flash`)에 넣어 한 번에 정리. Ask는 항상 기존 방식. 실패 시 기존 방식으로 자동 전환.
- 개발자 모드에서는 인식 힌트 대신 개발 용어·사전을 프롬프트에 넣음. 기록에 "원스텝"으로 표시.
- 비교: `cargo run -p sori-core --example onestep_eval -- *.wav` (8개 샘플: 기존 0.9–2.2초·전부 정확 / 원스텝 1.3–2.3초(5.3초 1회)·"했어→해서" 같은 의역 발생)

## Claude Code · Codex CLI로 대체 가능한가 (2026-09-23 확인)

| | Claude Code CLI (2.1.280) | Codex CLI (0.153.4) |
|---|---|---|
| 자체 음성 입력 | `/voice` (Space 홀드/탭, 한국어 지원, Anthropic 클라우드 STT, 사용량 미차감) — **Claude Code 입력창 안에서만** | `in_app_dictation` 기능 켜짐(앱 내부), 실시간 음성 대화는 개발 중 — **앱 안에서만** |
| 오디오를 모델에 직접 | 불가 (Claude API 입력: 텍스트·이미지·PDF만) | `codex exec`는 이미지(`-i`)만, 오디오 없음 |
| 헤드리스로 다듬기 단계 대체 | `claude -p` 개인 구독 사용 허용(구독 한도 차감). 단 `--bare`(빠른 모드)는 API 키 전용. 이 Mac에서는 CLI 로그인 필요 | `codex exec` + ChatGPT 로그인으로 동작. gpt-6-astra 11–12초, gpt-5.6-luna(low) 6–8초, `gpt-6-luna`는 ChatGPT 계정 미지원. "ok"만 출력해도 5–7초 → **CLI 자체 오버헤드가 병목** |

## 아직 없는 것

- 수정한 단어를 사전에 자동 추가(Typeless의 "자동 추가됨") — 사전 필터 UI만 있음
- 스트리밍 전사(Scribe v2 Realtime) — 현재는 말이 끝난 뒤 한 번에 전송
- 개인화 학습(대신 설정 → 개인화에 "나의 글쓰기 스타일" 자유 입력)
- 클라우드 동기화, 모바일
