import { ReactNode, useEffect, useState } from "react";
import { isWin, on as listen, PASTE, DEVICE } from "../bridge";
import { AdminStatus, api, isOk, keyLabel, KeyCheck, PermState, Settings, Shortcut, sortKeys, TranslationTarget } from "../api";
import { type L, useL, useLang } from "../i18n";
import { LocalModelControl } from "./ModelDownload";
import { useToast } from "./toast";
import { IconGear, IconGlobe, IconInfo, IconKeyboard, IconMic, IconSpark, IconUser, IconX } from "../icons";

type Tab = "general" | "shortcuts" | "language" | "audio" | "ai" | "personal" | "about";

const tabs = (L: L): [Tab, string, ReactNode][] => [
  ["general", L("General", "일반"), <IconGear />],
  ["shortcuts", L("Shortcuts", "단축키"), <IconKeyboard />],
  ["language", L("Language", "언어"), <IconGlobe />],
  ["audio", L("Audio", "오디오"), <IconMic />],
  ["ai", L("AI", "AI"), <IconSpark />],
  ["personal", L("Personalization", "개인화"), <IconUser />],
  ["about", L("About", "정보"), <IconInfo />],
];

export const TARGET_OPTIONS: (TranslationTarget & { label: string })[] = [
  { code: "en-US", name: "English (US)", label: "영어 (미국)" },
  { code: "en-GB", name: "English (UK)", label: "영어 (영국)" },
  { code: "en-CA", name: "English (Canada)", label: "영어 (캐나다)" },
  { code: "ja-JP", name: "Japanese", label: "일본어" },
  { code: "zh-CN", name: "Simplified Chinese", label: "중국어 (간체)" },
  { code: "zh-TW", name: "Traditional Chinese", label: "중국어 (번체)" },
  { code: "es-ES", name: "Spanish", label: "스페인어" },
  { code: "fr-FR", name: "French", label: "프랑스어" },
  { code: "de-DE", name: "German", label: "독일어" },
  { code: "pt-BR", name: "Portuguese (Brazil)", label: "포르투갈어 (브라질)" },
  { code: "it-IT", name: "Italian", label: "이탈리아어" },
  { code: "vi-VN", name: "Vietnamese", label: "베트남어" },
  { code: "id-ID", name: "Indonesian", label: "인도네시아어" },
  { code: "th-TH", name: "Thai", label: "태국어" },
  { code: "ru-RU", name: "Russian", label: "러시아어" },
  { code: "ko-KR", name: "Korean", label: "한국어" },
];

/** Target language name in the UI language (English names unless the UI is Korean). */
export const targetLabel = (t: TranslationTarget, lang: string = "en") =>
  lang === "ko" ? (TARGET_OPTIONS.find((o) => o.code === t.code)?.label ?? t.name) : (TARGET_OPTIONS.find((o) => o.code === t.code)?.name ?? t.name);

const llmPresets = (L: L): [string, string][] => [
  ["google/gemini-3.8-flash", L("Gemini 3.8 Flash — recommended, best quality (~0.9s)", "Gemini 3.8 Flash — 추천, 최고 품질 (~0.9초)")],
  ["google/gemini-3.1-flash-lite", L("Gemini 3.1 Flash Lite — fastest, may shift your tone (~0.8s)", "Gemini 3.1 Flash Lite — 가장 빠르지만 말투가 바뀔 수 있음 (~0.8초)")],
  ["google/gemini-3.5-flash-lite", L("Gemini 3.5 Flash Lite (~0.8s)", "Gemini 3.5 Flash Lite (~0.8초)")],
  ["deepseek/deepseek-v4.1-flash", L("DeepSeek V4.1 Flash (~0.8s)", "DeepSeek V4.1 Flash (~0.8초)")],
  ["~anthropic/claude-haiku-latest", L("Claude Haiku (latest)", "Claude Haiku (최신)")],
  ["openai/gpt-5.6-luna", L("GPT-5.6 Luna (~2s)", "GPT-5.6 Luna (~2초)")],
];
const askPresets = (L: L): [string, string][] => [
  ["google/gemini-3.8-flash", L("Gemini 3.8 Flash — recommended", "Gemini 3.8 Flash — 추천")],
  ["~anthropic/claude-haiku-latest", L("Claude Haiku (latest)", "Claude Haiku (최신)")],
  ["anthropic/claude-sonnet-5", L("Claude Sonnet 5 — slower but smarter", "Claude Sonnet 5 — 느리지만 똑똑함")],
  ["openai/gpt-5.6-sol", "GPT-5.6 Sol"],
  ["google/gemini-3.1-flash-lite", L("Gemini 3.1 Flash Lite — fastest", "Gemini 3.1 Flash Lite — 가장 빠름")],
];
const oneStepPresets = (L: L): [string, string][] => [
  ["google/gemini-3.8-flash", L("Gemini 3.8 Flash — recommended (~1.5s)", "Gemini 3.8 Flash — 추천 (~1.5초)")],
  ["google/gemini-3.1-flash-lite", L("Gemini 3.1 Flash Lite — fast, weaker on tech terms", "Gemini 3.1 Flash Lite — 빠르지만 개발 용어 약함")],
  ["google/gemini-3.5-flash-lite", "Gemini 3.5 Flash Lite"],
];
/** On-device text models (ids match desktop/src-tauri/src/models.rs). */
const localLlms = (L: L): [string, string][] => [
  ["gemma-4-e2b", L("Gemma — recommended (3.1 GB)", "Gemma — 추천 (3.1 GB)")],
  ["kanana-2-1.3b", L("Kanana — fastest (0.85 GB)", "Kanana — 가장 빠름 (0.85 GB)")],
];
/** Cleanup levels 1–5 (ids match sori-core `prompts::cleanup_level`). */
const cleanupLevels = (L: L): [Settings["cleanup_style"], string, string][] => [
  ["minimal", L("Minimal", "최소"), L("Fillers and recognition errors only — your exact words", "군더더기·인식 오류만 — 말한 그대로")],
  ["light", L("Light", "가볍게"), L("Also stutters, repeats, self-corrections — your phrasing", "더듬기·반복·말 바꾸기까지 — 표현은 그대로")],
  ["clean", L("Clean", "깔끔하게"), L("Natural written sentences, same order", "자연스러운 문장으로, 순서는 그대로")],
  ["polished", L("Polished", "정돈"), L("Clear and concise; lists for several points (recommended)", "명확하고 간결하게, 여러 요점은 목록으로 (추천)")],
  ["agent", L("Agent", "에이전트"), L("A structured prompt for coding agents: goal → tasks → constraints", "코딩 에이전트용 프롬프트 구조: 목표 → 할 일 → 제약")],
];

const sttLangs = (L: L): [string, string][] => [
  ["", L("Auto-detect (best for mixed Korean/English)", "자동 감지 (한/영 섞어 말하기 추천)")],
  ["ko", L("Korean", "한국어")],
  ["en", L("English", "영어")],
  ["ja", L("Japanese", "일본어")],
  ["zh", L("Chinese", "중국어")],
];
const fnUsage = (L: L) => [
  L("Do Nothing", "아무것도 안 함"),
  L("Change Input Source", "입력 소스 변경"),
  L("Show Emoji & Symbols", "이모티콘 및 기호 보기"),
  L("Start Dictation", "받아쓰기 시작"),
];

type Props = {
  initialTab: string;
  settings: Settings;
  perms: PermState | null;
  save: (s: Settings) => Promise<Settings>;
  refreshPerms: () => void;
  onClose: () => void;
};

export default function SettingsModal(props: Props) {
  const L = useL();
  const [tab, setTab] = useState<Tab>((props.initialTab as Tab) ?? "general");
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props.onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="modal">
        <nav className="modal-nav">
          {tabs(L).map(([id, label, icon]) => (
            <button key={id} className={`nav-item ${tab === id ? "active" : ""}`} onClick={() => setTab(id)}>
              {icon}
              {label}
            </button>
          ))}
        </nav>
        <div className="modal-body">
          <button className="icon-btn modal-close" onClick={props.onClose}>
            <IconX />
          </button>
          {tab === "general" && <General {...props} />}
          {tab === "shortcuts" && <Shortcuts {...props} />}
          {tab === "language" && <Language {...props} />}
          {tab === "audio" && <Audio {...props} />}
          {tab === "ai" && <AiKeys {...props} />}
          {tab === "personal" && <Personal {...props} />}
          {tab === "about" && <About {...props} />}
        </div>
      </div>
    </div>
  );
}

function Row(p: { title: string; desc?: ReactNode; children: ReactNode; wide?: boolean }) {
  return (
    <div className="set-row">
      <div className="info">
        <h4>{p.title}</h4>
        {p.desc && <p>{p.desc}</p>}
      </div>
      <div className={`ctl ${p.wide ? "wide" : ""}`}>{p.children}</div>
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return <button className={`toggle ${on ? "on" : ""}`} onClick={() => onChange(!on)} aria-pressed={on} />;
}

function General({ settings: s, save, perms }: Props) {
  const L = useL();
  const fnNow = perms?.fn_usage_type != null ? (fnUsage(L)[perms.fn_usage_type] ?? L("Unknown", "알 수 없음")) : L("Unknown", "알 수 없음");
  return (
    <>
      <h2>{L("General", "일반")}</h2>
      <div className="set-section">
        <Row title={L("Appearance", "모양")} desc={L("Light, dark, or follow the system.", "라이트, 다크 또는 시스템 설정을 따릅니다.")}>
          <select className="select" value={s.appearance} onChange={(e) => save({ ...s, appearance: e.target.value as Settings["appearance"] })}>
            <option value="system">{L("System", "시스템")}</option>
            <option value="light">{L("Light", "라이트")}</option>
            <option value="dark">{L("Dark", "다크")}</option>
          </select>
        </Row>
        <Row
          title={L("Also copy results to the clipboard", "결과를 클립보드에도 복사")}
          desc={L(
            `Keeps every dictation, translation and answer on the clipboard, so you can ${PASTE} it if it didn't land in a text field. When off, your previous clipboard is restored after pasting.`,
            `받아쓰기·번역·답변 결과를 클립보드에 남겨 둡니다. 입력 칸에 자동으로 붙지 않았을 때 바로 ${PASTE}로 붙여넣을 수 있어요. 끄면 붙여넣은 뒤 원래 클립보드 내용으로 되돌립니다.`,
          )}
        >
          <Toggle on={s.copy_to_clipboard} onChange={(v) => save({ ...s, copy_to_clipboard: v })} />
        </Row>
        <Row title={L("Launch at login", "로그인 시 앱 실행")} desc={L("Sori is ready as soon as you log in.", "컴퓨터를 켜고 로그인하면 Sori가 바로 대기합니다.")}>
          <Toggle on={s.launch_at_login} onChange={(v) => save({ ...s, launch_at_login: v })} />
        </Row>
        <Row title={L("Show in Dock", "Dock에 앱 표시")} desc={L("When off, Sori lives only in the menu bar.", "끄면 메뉴 막대 아이콘으로만 접근합니다.")}>
          <Toggle on={s.show_in_dock} onChange={(v) => save({ ...s, show_in_dock: v })} />
        </Row>
        {!isWin && (
        <Row
          title={L("Disable the 🌐 key's system action (while Sori runs)", "🌐 키 시스템 동작 끄기 (Sori 실행 중에만)")}
          desc={
            <>
              {L("macOS “Press 🌐 key to” is set to ", "macOS의 “🌐 키를 눌러” 설정이 ")}
              <b>{fnNow}</b>
              {perms?.fn_managed ? L(" (temporarily set to “Do Nothing” by Sori)", " (Sori가 임시로 ‘아무것도 안 함’으로 바꿔 둠)") : ""}
              {L(
                ". When on, Sori sets it to “Do Nothing” while running and restores it on quit. Turn this off if you switch input languages with 🌐.",
                "이에요. 켜면 Sori가 실행되는 동안만 ‘아무것도 안 함’으로 바꾸고 종료할 때 되돌립니다. 🌐 키로 한/영 전환을 한다면 끄세요.",
              )}
            </>
          }
        >
          <Toggle on={s.manage_fn_key} onChange={(v) => save({ ...s, manage_fn_key: v })} />
        </Row>
        )}
      </div>
    </>
  );
}

function Shortcuts({ settings: s, save }: Props) {
  const L = useL();
  const [recording, setRecording] = useState<keyof Settings["shortcuts"] | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const un = listen<string[]>("shortcut-recorded", (e) => {
      const combo = e.payload;
      const same = (a: Shortcut, b: Shortcut) => a.length === b.length && a.every((k) => b.includes(k));
      const clash = (Object.keys(s.shortcuts) as (keyof Settings["shortcuts"])[]).find((k) => s.shortcuts[k].some((c) => same(c, combo)));
      if (clash) {
        setMsg(`${L("Already used by another action", "이미 다른 기능에 쓰이는 단축키예요")}: ${combo.map(keyLabel).join(" + ")}`);
      } else {
        save({ ...s, shortcuts: { ...s.shortcuts, [recording]: [...s.shortcuts[recording], combo] } });
        setMsg(null);
      }
      setRecording(null);
    });
    return () => {
      un.then((u) => u());
    };
  }, [recording, s, save, L]);

  const start = async (k: keyof Settings["shortcuts"]) => {
    setMsg(L("Press and release the key combination (Esc to cancel)", "원하는 키 조합을 눌렀다 떼세요 (Esc로 취소)"));
    setRecording(k);
    await api.recordShortcutStart();
  };
  const remove = (k: keyof Settings["shortcuts"], i: number) =>
    save({ ...s, shortcuts: { ...s.shortcuts, [k]: s.shortcuts[k].filter((_, j) => j !== i) } });

  const rows: [keyof Settings["shortcuts"], string, string][] = [
    ["dictate", L("Voice input", "음성 입력"), L("Press to start and stop dictation. Also finishes Translate and Ask.", "받아쓰기를 시작하고 중지하려면 누르세요. 번역·Ask도 이 키로 끝냅니다.")],
    ["translate", L("Translate", "번역하기"), L("Speak in your language; Sori pastes it in the target language.", "모국어로 말하면 번역 대상 언어로 바꿔 붙여넣습니다.")],
    ["ask", L("Ask anything", "무엇이든 물어보세요"), L("Edit selected text, ask questions, get writing help.", "선택한 텍스트 편집, 질문, 글쓰기 도우미.")],
  ];
  const askOff = s.llm_provider === "local";
  return (
    <>
      <h2>{L("Shortcuts", "단축키")}</h2>
      <div className="set-section">
        <Row
          title={L("Recording mode", "녹음 방식")}
          desc={
            s.recording_mode === "hybrid"
              ? L("Records while you hold the key and stops when you let go. A quick tap toggles instead.", "누르고 있는 동안 녹음하고 떼면 끝납니다. 짧게 탭하면 한 번 눌러 시작/다시 눌러 종료로 동작해요.")
              : L("Press once to start; Sori keeps listening after you let go. Press again to finish. (Typeless style)", "한 번 누르면 녹음이 시작되고, 손을 떼도 계속 듣다가 다시 누르면 끝내고 처리합니다. (Typeless 방식)")
          }
        >
          <select className="select" value={s.recording_mode} onChange={(e) => save({ ...s, recording_mode: e.target.value as Settings["recording_mode"] })}>
            <option value="toggle">{L("Press to start · press again to stop", "한 번 눌러 시작 · 다시 눌러 종료")}</option>
            <option value="hybrid">{L("Hold to record (tap to toggle)", "누르고 있는 동안 녹음 (탭하면 토글)")}</option>
          </select>
        </Row>
        {rows.map(([k, title, desc]) => (
          <div key={k} className={k === "ask" && askOff ? "row-disabled" : undefined}>
          <Row
            title={title}
            desc={
              k === "ask" && askOff ? (
                <>
                  {desc} <span className="tag-off">{L("API mode only — admin", "API 모드 전용 — 관리자")}</span>
                </>
              ) : (
                desc
              )
            }
          >
            <div className={`shortcut-box ${recording === k ? "recording" : ""}`}>
              {s.shortcuts[k].map((c, i) => (
                <span key={i} className="shortcut-combo">
                  {sortKeys(c).map((key) => (
                    <span key={key} className="kbd">
                      {keyLabel(key)}
                    </span>
                  ))}
                  <button className="mini x" title={L("Remove", "제거")} disabled={k === "ask" && askOff} onClick={() => remove(k, i)}>
                    ✕
                  </button>
                </span>
              ))}
              {recording === k && <span className="hint" style={{ alignSelf: "center" }}>{L("Waiting for keys…", "입력 대기 중…")}</span>}
            </div>
            <div style={{ display: "flex", gap: 6 }}>
              {recording === k ? (
                <button
                  className="btn small"
                  onClick={() => {
                    api.recordShortcutCancel();
                    setRecording(null);
                    setMsg(null);
                  }}
                >
                  {L("Cancel", "취소")}
                </button>
              ) : (
                <button className="btn small" onClick={() => start(k)} disabled={!!recording || (k === "ask" && askOff)}>
                  {L("Add", "추가하기")}
                </button>
              )}
            </div>
          </Row>
          </div>
        ))}
        {msg && <p className="hint" style={{ color: "var(--accent)" }}>{msg}</p>}
        <button
          className="btn"
          style={{ marginTop: 10 }}
          onClick={async () => {
            const info = await api.appInfo();
            save({ ...s, shortcuts: info.default_shortcuts });
          }}
        >
          {L("Reset to defaults", "기본값으로 되돌리기")}
        </button>
        <p className="hint" style={{ marginTop: 12 }}>
          {L(
            "Middle and side mouse buttons work as shortcuts too. A short press is hands-free; holding records only while held.",
            "마우스 가운데/옆 버튼도 단축키로 추가할 수 있어요. 짧게 누르면 핸즈프리, 누르고 있으면 누르는 동안만 녹음합니다.",
          )}
        </p>
      </div>
    </>
  );
}

function Language({ settings: s, save }: Props) {
  const L = useL();
  const lang = useLang();
  const [adding, setAdding] = useState("");
  const targets = s.translation_targets;
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= targets.length) return;
    const next = [...targets];
    [next[i], next[j]] = [next[j], next[i]];
    let active = s.active_translation_target;
    if (active === i) active = j;
    else if (active === j) active = i;
    save({ ...s, translation_targets: next, active_translation_target: active });
  };
  return (
    <>
      <h2>{L("Language", "언어")}</h2>
      <div className="set-section">
        <Row title={L("Interface language", "인터페이스 언어")} desc={L("The language of Sori's own screens.", "Sori 화면에 표시할 언어입니다.")}>
          <select className="select" value={s.interface_language} onChange={(e) => save({ ...s, interface_language: e.target.value as "ko" | "en" })}>
            <option value="en">English</option>
            <option value="ko">한국어</option>
          </select>
        </Row>
        <Row
          title={L("Spoken language", "말하는 언어")}
          desc={L(
            "Auto-detect handles mixed Korean and English best. If you only speak one language, pinning it is slightly more accurate.",
            "자동 감지가 한국어·영어가 섞인 말을 가장 잘 받아씁니다. 한 언어만 쓴다면 고정하면 조금 더 정확해요.",
          )}
        >
          <select className="select" value={s.stt_language} onChange={(e) => save({ ...s, stt_language: e.target.value })}>
            {sttLangs(L).map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </Row>
        <Row
          title={L("Translation targets", "번역 대상")}
          desc={L(
            "Add the languages you translate into and order them. The highlighted one is active; while recording, click the language above the voice bar to switch.",
            "자주 쓰는 대상 언어를 추가하고 순서를 정하세요. 파란 테두리가 현재 대상이며, 녹음 중 음성 바 위의 언어를 눌러 바꿀 수 있어요.",
          )}
          wide
        >
          <div className="target-list">
            {targets.map((t, i) => (
              <div key={t.code} className={`target-item ${i === s.active_translation_target ? "active" : ""}`}>
                <span className="grow">{targetLabel(t, lang)}</span>
                {i !== s.active_translation_target && (
                  <button className="btn small ghost" onClick={() => save({ ...s, active_translation_target: i })}>
                    {L("Use", "사용")}
                  </button>
                )}
                <button className="mini" onClick={() => move(i, -1)}>
                  ↑
                </button>
                <button className="mini" onClick={() => move(i, 1)}>
                  ↓
                </button>
                {targets.length > 1 && (
                  <button
                    className="mini"
                    onClick={() => {
                      const next = targets.filter((_, j) => j !== i);
                      save({ ...s, translation_targets: next, active_translation_target: Math.min(s.active_translation_target, next.length - 1) });
                    }}
                  >
                    ✕
                  </button>
                )}
              </div>
            ))}
            <div style={{ display: "flex", gap: 6 }}>
              <select className="select" value={adding} onChange={(e) => setAdding(e.target.value)}>
                <option value="">{L("Add a language…", "다른 언어 추가…")}</option>
                {TARGET_OPTIONS.filter((o) => !targets.some((t) => t.code === o.code)).map((o) => (
                  <option key={o.code} value={o.code}>
                    {targetLabel(o, lang)}
                  </option>
                ))}
              </select>
              <button
                className="btn"
                disabled={!adding}
                onClick={() => {
                  const o = TARGET_OPTIONS.find((x) => x.code === adding);
                  if (o) save({ ...s, translation_targets: [...targets, { code: o.code, name: o.name }] });
                  setAdding("");
                }}
              >
                {L("Add", "추가")}
              </button>
            </div>
          </div>
        </Row>
      </div>
    </>
  );
}

function Audio({ settings: s, save }: Props) {
  const L = useL();
  const [devices, setDevices] = useState<string[]>([]);
  const [level, setLevel] = useState(0);
  const [testing, setTesting] = useState(false);
  useEffect(() => {
    api.listMicrophones().then(setDevices);
    const un = listen<number>("mic-level", (e) => setLevel(e.payload));
    return () => {
      un.then((u) => u());
      api.stopMicTest();
    };
  }, []);
  const toggleTest = async () => {
    if (testing) {
      await api.stopMicTest();
      setTesting(false);
      setLevel(0);
    } else {
      await api.startMicTest(s.microphone);
      setTesting(true);
    }
  };
  return (
    <>
      <h2>{L("Audio", "오디오")}</h2>
      <div className="set-section">
        <Row
          title={L("Microphone", "마이크")}
          desc={L(
            "Auto-detect uses the system's default input. Bluetooth headsets drop to call quality and can be slow, so the built-in mic is recommended.",
            "자동 감지는 시스템 기본 입력 장치를 사용합니다. 블루투스 헤드셋은 통화 품질로 바뀌며 느려질 수 있어 내장 마이크를 권장해요.",
          )}
        >
          <select className="select" value={s.microphone ?? ""} onChange={(e) => save({ ...s, microphone: e.target.value || null })}>
            <option value="">{L("Auto-detect", "자동 감지")}</option>
            {devices.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
          <div className="meter">
            <div style={{ width: `${Math.round(level * 100)}%` }} />
          </div>
          <button className="btn small" onClick={toggleTest}>
            {testing ? L("Stop test", "테스트 중지") : L("Test microphone", "마이크 테스트")}
          </button>
        </Row>
        <Row title={L("Interaction sounds", "상호작용 사운드")} desc={L("Play a short sound when dictation starts and ends.", "받아쓰기를 시작하고 끝낼 때 짧은 소리를 냅니다.")}>
          <Toggle on={s.interaction_sounds} onChange={(v) => save({ ...s, interaction_sounds: v })} />
        </Row>
        <Row title={L("Mute audio while dictating", "받아쓰는 동안 음소거")} desc={L("Mutes system output while recording and restores it afterwards.", "녹음하는 동안 시스템 출력 소리를 끄고, 끝나면 되돌립니다.")}>
          <Toggle on={s.mute_when_dictating} onChange={(v) => save({ ...s, mute_when_dictating: v })} />
        </Row>
      </div>
    </>
  );
}

function KeyInput(p: { label: string; value: string; onSave: (v: string) => void; onReset?: () => void; placeholder: string }) {
  const L = useL();
  const [v, setV] = useState(p.value);
  const [show, setShow] = useState(false);
  useEffect(() => setV(p.value), [p.value]);
  return (
    <div style={{ width: "100%", display: "grid", gap: 6 }}>
      <div className="key-field">
        <input
          className="input"
          type={show ? "text" : "password"}
          value={v}
          placeholder={p.placeholder}
          onChange={(e) => setV(e.target.value)}
          onBlur={() => v !== p.value && p.onSave(v.trim())}
          spellCheck={false}
        />
        <button className="btn small" onClick={() => setShow(!show)}>
          {show ? L("Hide", "숨기기") : L("Show", "보기")}
        </button>
      </div>
      {p.onReset && (
        <button className="btn small ghost" style={{ justifySelf: "end" }} onClick={p.onReset}>
          {L("Restore default key", "기본 키로 복원")}
        </button>
      )}
    </div>
  );
}

function ModelPicker(p: { value: string; presets: [string, string][]; onChange: (v: string) => void }) {
  const L = useL();
  const isPreset = p.presets.some(([v]) => v === p.value);
  const [custom, setCustom] = useState(!isPreset);
  const [text, setText] = useState(p.value);
  return (
    <>
      <select
        className="select"
        value={custom ? "__custom" : p.value}
        onChange={(e) => {
          if (e.target.value === "__custom") setCustom(true);
          else {
            setCustom(false);
            p.onChange(e.target.value);
          }
        }}
      >
        {p.presets.map(([v, l]) => (
          <option key={v} value={v}>
            {l}
          </option>
        ))}
        <option value="__custom">{L("Custom…", "직접 입력…")}</option>
      </select>
      {custom && (
        <input
          className="input"
          placeholder="provider/model-id (OpenRouter)"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => text.trim() && p.onChange(text.trim())}
        />
      )}
    </>
  );
}

function AdminPanel({ admin, onChange }: { admin: AdminStatus | null; onChange: (s: Settings) => void }) {
  const L = useL();
  const toast = useToast();
  const [pw, setPw] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  if (!admin) return null;
  const unlock = async () => {
    if (!pw) return;
    setBusy(true);
    setErr(null);
    try {
      onChange(await api.adminUnlock(pw));
      setPw("");
      toast(L("Admin mode on", "관리자 모드 켜짐"));
    } catch (e) {
      setErr(String(e).includes("Wrong") ? L("Wrong password", "비밀번호가 틀렸어요") : String(e));
    } finally {
      setBusy(false);
    }
  };
  const lock = async () => {
    onChange(await api.adminLock());
    toast(L("Admin mode off", "관리자 모드 꺼짐"));
  };
  if (!admin.available) return null;
  return (
    <Row
      title={L("Admin mode", "관리자 모드")}
    >
      {admin.unlocked ? (
          <button className="btn small" onClick={lock}>
            {L("Lock", "잠그기")}
          </button>
        ) : (
          <div style={{ width: "100%", display: "grid", gap: 6 }}>
            <div className="key-field">
              <input
                className="input"
                type="password"
                autoComplete="off"
                placeholder={L("Admin password", "관리자 비밀번호")}
                value={pw}
                onChange={(e) => setPw(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && unlock()}
              />
              <button className="btn small primary" disabled={!pw || busy} onClick={unlock}>
                {busy ? L("Checking…", "확인 중…") : L("Unlock", "잠금 해제")}
              </button>
            </div>
            {err && <span className="key-status bad">✕ {err}</span>}
          </div>
        )}
    </Row>
  );
}

function AiKeys({ settings: s, save }: Props) {
  const L = useL();
  const [admin, setAdmin] = useState<AdminStatus | null>(null);
  const [check, setCheck] = useState<KeyCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [previewIn, setPreviewIn] = useState(
    L(
      "um so the meeting tomorrow is at three, no wait, four, and uh for the demo we need first the laptop second the slides and third the, the business cards",
      "음 그러니까 내일 회의는 3시, 아니 4시로 하고요, 어 준비물은 첫째 노트북 둘째 발표 자료 셋째 명함이에요",
    ),
  );
  const [previewMode, setPreviewMode] = useState<"dictate" | "translate">("dictate");
  const [previewOut, setPreviewOut] = useState("");
  const [previewBusy, setPreviewBusy] = useState(false);
  const refreshAdmin = () => api.adminStatus().then(setAdmin).catch(() => {});
  useEffect(() => {
    refreshAdmin();
  }, []);
  const unlocked = !!admin?.unlocked;
  const reset = async (which: "elevenlabs" | "openrouter") => {
    await api.resetApiKey(which);
    save(await api.getSettings());
  };
  const onAdminChange = (next: Settings) => {
    save(next);
    refreshAdmin();
  };
  const local = s.llm_provider === "local";
  const needEleven = s.stt_engine === "elevenlabs";
  const needOpenRouter = !local || s.pipeline_mode === "one_step";
  return (
    <>
      <h2>AI</h2>
      <div className="set-section">
        <h3>{L("Speech recognition", "음성 인식")}</h3>
        <Row
          title={L("Engine", "엔진")}
          desc={L(
            `Local AI (Whisper) runs on this ${DEVICE}: your voice never leaves it, no key needed, ~0.5–1 s per sentence.`,
            `로컬 AI(Whisper)가 이 ${DEVICE}에서 처리해요: 음성이 밖으로 나가지 않고 키도 필요 없어요. 문장당 약 0.5~1초.`,
          )}
        >
          {unlocked ? (
            <select className="select" value={s.stt_engine} onChange={(e) => save({ ...s, stt_engine: e.target.value as Settings["stt_engine"] })}>
              <option value="local">{L("Local AI — Whisper (574 MB)", "로컬 AI — Whisper (574 MB)")}</option>
              <option value="elevenlabs">{L("ElevenLabs — cloud", "ElevenLabs — 클라우드")}</option>
            </select>
          ) : (
            <span className="key-status ok">{L("Local AI — Whisper (574 MB)", "로컬 AI — Whisper (574 MB)")}</span>
          )}
          {s.stt_engine === "elevenlabs" ? (
            <select className="select" value={s.stt_model} onChange={(e) => save({ ...s, stt_model: e.target.value })}>
              <option value="scribe_v2">{L("scribe_v2 — recommended", "scribe_v2 — 추천")}</option>
              <option value="scribe_v1">scribe_v1</option>
            </select>
          ) : (
            <LocalModelControl model={s.local_model} language={s.stt_language} hasEleven={unlocked && !!s.elevenlabs_api_key.trim()} />
          )}
        </Row>
      </div>
      <div className="set-section">
        <h3>{L("Text AI — cleanup, translation, Ask", "텍스트 AI — 다듬기 · 번역 · Ask")}</h3>
        {unlocked && (
          <Row
            title={L("Engine", "엔진")}
            desc={L(
              "Local AI runs a small language model on this computer's GPU (integrated graphics work). OpenRouter uses a cloud model — sharper writing, ~1 s.",
              "로컬 AI는 이 컴퓨터 GPU(내장 그래픽 포함)에서 작은 언어 모델을 돌려요. OpenRouter는 클라우드 모델을 써요 — 글이 더 매끄럽고 약 1초.",
            )}
          >
            <select className="select" value={s.llm_provider} onChange={(e) => save({ ...s, llm_provider: e.target.value as Settings["llm_provider"] })}>
              <option value="local">{L("Local AI — private, free, offline", "로컬 AI — 비공개 · 무료 · 오프라인")}</option>
              <option value="openrouter">{L("OpenRouter — cloud", "OpenRouter — 클라우드")}</option>
            </select>
          </Row>
        )}
        {local ? (
          <Row
            title={unlocked ? L("Local AI model", "로컬 AI 모델") : L("Local AI", "로컬 AI")}
            desc={L(
              "Runs on this computer's GPU (integrated graphics work): ~1 s per sentence on Apple Silicon, ~2–3 s on integrated graphics. Kanana is about twice as fast but polishes less.",
              "이 컴퓨터 GPU(내장 그래픽 포함)에서 돌아가요: Apple Silicon에서 문장당 약 1초, 내장 그래픽에서 약 2~3초. Kanana는 약 2배 빠르지만 덜 다듬어요.",
            )}
          >
            <select className="select" value={s.local_llm_model} onChange={(e) => save({ ...s, local_llm_model: e.target.value })}>
              {localLlms(L).map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
            <LocalModelControl model={s.local_llm_model} language={s.stt_language} />
          </Row>
        ) : (
          <>
            <Row title={L("Cleanup & translation model", "다듬기 · 번역 모델")} desc={L("The faster it is, the sooner your text is pasted.", "빠를수록 붙여넣기까지 짧아요.")}>
              <ModelPicker value={s.llm_model} presets={llmPresets(L)} onChange={(v) => save({ ...s, llm_model: v })} />
            </Row>
            <Row title={L("Ask anything model", "무엇이든 물어보기 모델")} desc={L("For editing, questions and writing help. A smarter model is worth it.", "편집·질문·글쓰기 도우미용. 조금 더 똑똑한 모델을 권장해요.")}>
              <ModelPicker value={s.ask_model} presets={askPresets(L)} onChange={(v) => save({ ...s, ask_model: v })} />
            </Row>
          </>
        )}
        <Row
          title={L("Cleanup level", "다듬기 단계")}
          desc={L(
            "How much Sori rewrites what you said. If a rewrite would drop anything, Sori automatically uses a gentler level (noted in History).",
            "말한 내용을 얼마나 고쳐 쓸지 정해요. 다듬다가 내용이 빠질 것 같으면 자동으로 한 단계 가볍게 처리해요 (기록에 표시).",
          )}
          wide
        >
          <div className="levels">
            {cleanupLevels(L).map(([v, name], i) => (
              <button key={v} className={`level ${s.cleanup_style === v ? "active" : ""}`} onClick={() => save({ ...s, cleanup_style: v })}>
                <span className="level-n">{i + 1}</span>
                <span className="level-name">{name}</span>
              </button>
            ))}
          </div>
          <span className="hint">{cleanupLevels(L).find(([v]) => v === s.cleanup_style)?.[2]}</span>
        </Row>
      </div>
      {admin?.available && (
        <div className="set-section">
          <AdminPanel admin={admin} onChange={onAdminChange} />
        </div>
      )}
      {unlocked && (
      <div className="set-section">
        <h3>{L("API keys", "API 키")}</h3>
        {!needEleven && !needOpenRouter && (
          <p className="hint">{L("Not needed with your current setup — everything runs on-device.", "현재 설정에서는 필요 없어요 — 모두 기기 안에서 처리돼요.")}</p>
        )}
        <Row
          title="ElevenLabs"
          desc={
            <>
              {needEleven
                ? L("Used for speech recognition (Scribe). ", "음성 인식(Scribe)에 사용합니다. ")
                : L("Optional: backup if on-device recognition fails. ", "선택: 로컬 인식이 실패할 때 대신 사용해요. ")}
              <code>xi-api-key</code>
            </>
          }
        >
          <KeyInput
            label="ElevenLabs"
            placeholder="sk_…"
            value={s.elevenlabs_api_key}
            onSave={(v) => save({ ...s, elevenlabs_api_key: v })}
            onReset={admin?.has_default_keys ? () => reset("elevenlabs") : undefined}
          />
          {check && (
            <span className={`key-status ${isOk(check.elevenlabs) ? "ok" : "bad"}`}>
              {isOk(check.elevenlabs) ? `✓ ${check.elevenlabs.Ok}` : `✕ ${check.elevenlabs.Err}`}
            </span>
          )}
        </Row>
        <Row
          title="OpenRouter"
          desc={
            needOpenRouter
              ? L("Used for cleanup, translation and Ask.", "다듬기·번역·무엇이든 물어보기에 사용합니다.")
              : L("Optional: only needed if you switch the provider to OpenRouter.", "선택: 제공자를 OpenRouter로 바꿀 때만 필요해요.")
          }
        >
          <KeyInput
            label="OpenRouter"
            placeholder="sk-or-v1-…"
            value={s.openrouter_api_key}
            onSave={(v) => save({ ...s, openrouter_api_key: v })}
            onReset={admin?.has_default_keys ? () => reset("openrouter") : undefined}
          />
          {check && (
            <span className={`key-status ${isOk(check.openrouter) ? "ok" : "bad"}`}>
              {isOk(check.openrouter) ? `✓ ${check.openrouter.Ok}` : `✕ ${check.openrouter.Err}`}
            </span>
          )}
        </Row>
        <button
          className="btn"
          disabled={checking}
          onClick={async () => {
            setChecking(true);
            try {
              setCheck(await api.testApiKeys(s.elevenlabs_api_key, s.openrouter_api_key));
            } finally {
              setChecking(false);
            }
          }}
        >
          {checking ? L("Checking…", "확인 중…") : L("Test keys", "연결 테스트")}
        </button>
      </div>
      )}
      {unlocked && !local && (
        <div className="set-section">
          <h3>{L("Processing (experimental)", "처리 방식 (실험)")}</h3>
          <Row
            title={L("One-step", "한 번에 처리")}
            desc={L(
              "Sends the audio straight to Gemini for dictation and translation — no separate speech recognition. In tests it sometimes missed rare names (Tauri, CGEventTap) or rephrased slightly. Marked “one-step” in History; falls back automatically on failure. Ask always uses the regular path.",
              "받아쓰기·번역을 음성 인식 없이 오디오를 Gemini에 바로 넣어 한 번에 정리합니다. 테스트에서는 드문 고유명사(Tauri, CGEventTap)를 놓치거나 문장을 살짝 바꾸는 경우가 있었어요. 기록에 “원스텝”으로 표시되고, 실패하면 자동으로 기존 방식으로 처리합니다. 무엇이든 물어보기는 항상 기존 방식이에요.",
            )}
          >
            <select className="select" value={s.pipeline_mode} onChange={(e) => save({ ...s, pipeline_mode: e.target.value as Settings["pipeline_mode"] })}>
              <option value="two_step">{L("Standard: speech recognition → AI cleanup (recommended)", "기존: 음성 인식 → AI 다듬기 (추천)")}</option>
              <option value="one_step">{L("Experimental: audio → Gemini in one step", "실험: 오디오 → Gemini 한 번에")}</option>
            </select>
            {s.pipeline_mode === "one_step" && (
              <ModelPicker value={s.one_step_model} presets={oneStepPresets(L)} onChange={(v) => save({ ...s, one_step_model: v })} />
            )}
          </Row>
        </div>
      )}
      <div className="set-section">
        <h3>{L("Try it", "미리보기")}</h3>
        <Row
          title={L("Test cleanup on text", "받아쓰기 결과 시험하기")}
          desc={L("Paste a raw transcript to see what your current settings produce — no audio needed.", "음성 없이, 받아쓴 원문을 넣으면 현재 설정으로 다듬은 결과를 보여 줍니다.")}
          wide
        >
          <textarea className="textarea" value={previewIn} onChange={(e) => setPreviewIn(e.target.value)} />
          <div style={{ display: "flex", gap: 6 }}>
            <select className="select" style={{ width: 140 }} value={previewMode} onChange={(e) => setPreviewMode(e.target.value as "dictate" | "translate")}>
              <option value="dictate">{L("Dictate", "받아쓰기")}</option>
              <option value="translate">{L("Translate", "번역")}</option>
            </select>
            <button
              className="btn primary"
              disabled={previewBusy}
              onClick={async () => {
                setPreviewBusy(true);
                const t0 = performance.now();
                try {
                  const out = await api.processTextPreview(previewIn, previewMode);
                  setPreviewOut(`${out}\n\n— ${Math.round(performance.now() - t0)}ms`);
                } catch (e) {
                  setPreviewOut(String(e));
                } finally {
                  setPreviewBusy(false);
                }
              }}
            >
              {previewBusy ? L("Running…", "실행 중…") : L("Run", "실행")}
            </button>
          </div>
          {previewOut && <div className="preview-out selectable">{previewOut}</div>}
        </Row>
      </div>
    </>
  );
}

function Personal({ settings: s, save }: Props) {
  const L = useL();
  const [ci, setCi] = useState(s.custom_instructions);
  const [stack, setStack] = useState(s.tech_stack);
  return (
    <>
      <h2>{L("Personalization", "개인화")}</h2>
      <div className="set-section">
        <Row
          title={L("Developer mode", "개발자 모드")}
          desc={L(
            "Assumes you're a developer: hints ~200 dev terms to speech recognition (React, useEffect, Kubernetes, PR…) and restores misheard or Hangul-spelled tech terms to their canonical spelling. Everyday loanwords like 서버·배포·커밋 stay as they are.",
            "말하는 사람이 개발자라고 가정합니다. 음성 인식에 개발 용어 200여 개(React, useEffect, Kubernetes, PR…)를 힌트로 주고, 잘못 들리거나 한글로 적힌 기술 용어를 정식 표기로 바로잡아요. 서버·배포·커밋처럼 평소 한글로 쓰는 말은 그대로 둡니다.",
          )}
        >
          <Toggle on={s.dev_mode} onChange={(v) => save({ ...s, dev_mode: v })} />
        </Row>
        {s.dev_mode && (
          <Row
            title={L("My tech stack", "내 기술 스택")}
            desc={L(
              "Comma-separated languages, frameworks, services and internal project names get extra priority. e.g. Unity, C#, Rust, Tauri, React, Supabase",
              "자주 쓰는 언어·프레임워크·서비스·내부 프로젝트명을 쉼표로 적으면 인식 우선순위를 더 높여요. 예: Unity, C#, Rust, Tauri, React, Supabase",
            )}
            wide
          >
            <input
              className="input"
              placeholder="Unity, C#, Rust, Tauri, React …"
              value={stack}
              onChange={(e) => setStack(e.target.value)}
              onBlur={() => stack !== s.tech_stack && save({ ...s, tech_stack: stack })}
            />
          </Row>
        )}
        <Row
          title={L("Per-app tone", "앱마다 다른 톤")}
          desc={L(
            "Emails come out courteous, chats light, and code editors / AI prompts precise and structured.",
            "이메일은 격식 있게, 채팅은 가볍게, 코드 에디터·AI 프롬프트는 정확하고 구조적으로 다듬습니다.",
          )}
        >
          <Toggle on={s.per_app_tone} onChange={(v) => save({ ...s, per_app_tone: v })} />
        </Row>
        <Row
          title={L("My writing style", "나의 글쓰기 스타일")}
          desc={L(
            "Preferences applied to every dictation, in your own words. e.g. “no period at the end of chat messages”, “use British spelling”.",
            "모든 받아쓰기에 적용할 선호를 자유롭게 적어 주세요. 예: “영어 기술 용어는 영어 그대로”, “문장 끝 마침표 생략”, “존댓말은 ~요체로”.",
          )}
          wide
        >
          <textarea className="textarea" value={ci} onChange={(e) => setCi(e.target.value)} onBlur={() => ci !== s.custom_instructions && save({ ...s, custom_instructions: ci })} />
        </Row>
        <Row
          title={L("Writing overhead", "작성·다듬기 가중치")}
          desc={L(
            "Writing by hand takes more than typing — you also organize your thoughts and polish sentences. “Time saved” multiplies typing time by this factor. (1× = copying already-finished text)",
            "직접 쓸 때는 치는 시간 말고도 생각을 정리하고 문장을 다듬는 시간이 들어요. ‘절약된 시간’ 계산에서 타이핑 시간에 이 배수를 곱합니다. (1배 = 이미 정리된 글을 옮겨 치는 속도)",
          )}
        >
          <select className="select" value={String(s.compose_factor)} onChange={(e) => save({ ...s, compose_factor: Number(e.target.value) })}>
            {[1, 1.5, 2, 2.5, 3].map((f) => (
              <option key={f} value={String(f)}>
                {f}
                {L("×", "배")}
                {f === 2 ? L(" (default)", " (기본)") : f === 1 ? L(" — typing only", " — 타이핑만") : ""}
              </option>
            ))}
          </select>
        </Row>
        <Row
          title={L("Typing speed", "평소 타이핑 속도")}
          desc={L(
            "Used for “Time saved” (words per minute; for Korean, counted by 어절 — 40 is roughly 330–360 keystrokes/min).",
            "‘절약된 시간’ 계산에 사용합니다 (분당 단어 수, 한국어는 어절 기준 — 40이면 대략 330~360타/분).",
          )}
        >
          <input
            className="input"
            type="number"
            min={10}
            max={200}
            value={s.typing_wpm}
            onChange={(e) => save({ ...s, typing_wpm: Math.max(10, Number(e.target.value) || 40) })}
          />
        </Row>
      </div>
    </>
  );
}

function About({ settings: s }: Props) {
  const L = useL();
  const [info, setInfo] = useState<{ version: string; data_dir: string; config_path: string } | null>(null);
  useEffect(() => {
    api.appInfo().then(setInfo);
  }, []);
  return (
    <>
      <h2>{L("About", "정보")}</h2>
      <div className="set-section">
        <Row title="Sori" desc={L("Speak naturally; Sori writes it up cleanly wherever your cursor is.", "말하면 다듬어서 써 주는 받아쓰기 앱.")}>
          <span>v{info?.version}</span>
        </Row>
        <Row title={L("Data folder", "데이터 위치")} desc={<span className="selectable">{info?.data_dir}</span>}>
          <span />
        </Row>
        <Row title={L("Settings file", "설정 파일")} desc={<span className="selectable">{info?.config_path}</span>}>
          <span />
        </Row>
        <Row
          title={L("How it works", "처리 경로")}
          desc={`${L("Voice", "음성")} → ${s.stt_engine === "local" ? L("Local AI (Whisper)", "로컬 AI(Whisper)") : "ElevenLabs"} → ${
            s.llm_provider === "local" ? L("Local AI", "로컬 AI") : `OpenRouter (${s.llm_model})`
          } → ${L("pasted at your cursor", "커서 위치에 붙여넣기")}${s.copy_to_clipboard ? L(" + clipboard", " + 클립보드") : ""}. ${L(
            `History and audio stay on this ${DEVICE}.`,
            `기록과 오디오는 이 ${DEVICE}에만 저장됩니다.`,
          )}`}
        >
          <span />
        </Row>
      </div>
    </>
  );
}
