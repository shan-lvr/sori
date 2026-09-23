import { DEVICE, isWin } from "../bridge";
import { useEffect, useMemo, useState } from "react";
import { api, comboLabel, keyLabel, PermState, Settings, Shortcut, sortKeys, Stats } from "../api";
import { useL, useLang, useT } from "../i18n";
import { targetLabel } from "./Settings";
import { LocalModelControl, useLocalModel } from "./ModelDownload";
import { useToast } from "./toast";

export function Keys({ combos }: { combos: Shortcut[] }) {
  const L = useL();
  return (
    <div className="keys">
      {combos.map((c, i) => (
        <span key={i} style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
          {i > 0 && <span className="or">{L("or", "또는")}</span>}
          {sortKeys(c).map((k) => (
            <span key={k} className="kbd">
              {keyLabel(k)}
            </span>
          ))}
        </span>
      ))}
    </div>
  );
}

function fmtWords(n: number) {
  if (n >= 10000) return `${(n / 1000).toFixed(1)}K`;
  return n.toLocaleString();
}

function Duration({ ms }: { ms: number }) {
  const totalMin = Math.round(ms / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return (
    <>
      {h > 0 && (
        <>
          {h}
          <small>hr</small>{" "}
        </>
      )}
      {m}
      <small>min</small>
    </>
  );
}

export default function Home(props: {
  settings: Settings;
  perms: PermState | null;
  save: (s: Settings) => Promise<Settings>;
  refreshPerms: () => void;
  historyTick: number;
  openSettings: (tab: string) => void;
}) {
  const t = useT();
  const L = useL();
  const lang = useLang();
  const [stats, setStats] = useState<Stats | null>(null);
  useEffect(() => {
    api.getStats().then(setStats).catch(() => {});
  }, [props.historyTick]);

  const { settings, perms } = props;
  return (
    <div className="page">
      <h1 className="hero">{t.heroTitle}</h1>
      <Setup {...props} />
      <div className="home-grid">
        <div>
          <div className="section-head">
            <h2>{t.insights}</h2>
          </div>
          <div className="grid-2" style={{ marginBottom: 14 }}>
            <div className="stat">
              <div className="num">
                {fmtWords(stats?.total_words ?? 0)}
                <small>{t.words}</small>
              </div>
              <div className="label">{t.wordsDictated}</div>
            </div>
            <div className="stat">
              <div className="num">
                <Duration ms={stats?.time_saved_ms ?? 0} />
              </div>
              <div
                className="label"
                title={L(
                  `(words ÷ ${settings.typing_wpm} WPM typing × ${settings.compose_factor}× writing overhead) − time spent speaking`,
                  `(받아쓴 단어 ÷ 타이핑 ${settings.typing_wpm} WPM × 작성·다듬기 ${settings.compose_factor}배) − 말한 시간`,
                )}
              >
                {t.timeSaved}{" "}
                <span style={{ fontSize: 12, color: "var(--faint)" }}>
                  {L(`· at ${settings.compose_factor}× writing overhead`, `· 작성·다듬기 ${settings.compose_factor}배 기준`)}
                </span>
              </div>
            </div>
            <div className="stat">
              <div className="num">
                {stats?.avg_wpm ?? 0}
                <small>WPM</small>
              </div>
              <div className="label">{t.avgSpeed}</div>
            </div>
            <div className="stat">
              <div className="num">
                <Duration ms={stats?.total_audio_ms ?? 0} />
              </div>
              <div className="label">{t.totalTime}</div>
            </div>
          </div>
          <div className="card">
            <div className="streaks">
              {[
                [stats?.active_days ?? 0, t.activeDays],
                [stats?.current_streak ?? 0, t.currentStreak],
                [stats?.longest_streak ?? 0, t.longestStreak],
              ].map(([n, l]) => (
                <div key={l as string}>
                  <div className="num" style={{ fontSize: 30, fontWeight: 750 }}>
                    {n}
                    <small style={{ fontSize: 14, marginLeft: 3, color: "var(--muted)" }}>{t.days}</small>
                  </div>
                  <div className="label" style={{ color: "var(--muted)", marginTop: 4 }}>
                    {l}
                  </div>
                </div>
              ))}
            </div>
            <Heatmap stats={stats} />
          </div>
        </div>
        <div className="card shortcut-card">
          <div className="shortcut-row">
            <h3>{t.dictate}</h3>
            <Keys combos={settings.shortcuts.dictate} />
          </div>
          <div className="shortcut-row">
            <h3>
              {t.translate} →{" "}
              {settings.translation_targets[settings.active_translation_target]
                ? targetLabel(settings.translation_targets[settings.active_translation_target], lang)
                : "English"}
            </h3>
            <Keys combos={settings.shortcuts.translate} />
          </div>
          <div className="shortcut-row" style={{ marginBottom: 6 }}>
            <h3>{t.ask}</h3>
            <Keys combos={settings.shortcuts.ask} />
          </div>
          <p className="hint" style={{ marginTop: 14 }}>
            {settings.recording_mode === "hybrid" ? (
              <>
                {L("Hold to record and let go to finish. A quick tap keeps listening until you press ", "누르고 있는 동안 녹음하고 떼면 끝나요. 짧게 탭하면 ")}
                <b>{comboLabel(settings.shortcuts.dictate[0])}</b>
                {L(" again. ", "을 다시 누를 때까지 계속 들어요. ")}
                <b>Esc</b>
                {L(" cancels.", "는 취소.")}
              </>
            ) : (
              <>
                {L("Press once to start recording; after speaking, press ", "한 번 누르면 녹음 시작, 말한 뒤 ")}
                <b>{comboLabel(settings.shortcuts.dictate[0])}</b>
                {L(" again to finish and paste. ", "을 다시 누르면 끝내고 붙여넣어요. ")}
                <b>Esc</b>
                {L(" cancels.", "는 취소.")}
              </>
            )}
          </p>
          <p className="hint">
            {L("Select text and press ", "텍스트를 선택하고 ")}
            <b>{comboLabel(settings.shortcuts.ask[0])}</b>
            {L(
              " to say things like “make it more polite” or “summarize this” — Sori rewrites the selection or answers.",
              "로 “더 정중하게”, “요약해줘”처럼 말하면 선택 영역을 바꾸거나 답해 줍니다.",
            )}
          </p>
          {perms && !perms.fn_managed && perms.fn_usage_type !== null && perms.fn_usage_type !== 0 && (
            <p className="hint" style={{ color: "var(--warn)" }}>
              {L("The 🌐 key is set to “", "🌐 키가 “")}
              {
                [
                  "",
                  L("Change Input Source", "입력 소스 변경"),
                  L("Show Emoji & Symbols", "이모티콘 및 기호"),
                  L("Start Dictation", "받아쓰기 시작"),
                ][perms.fn_usage_type]
              }
              {L("”, which may also fire when you press Fn — ", "”에 할당돼 있어요. Fn을 누를 때 함께 동작할 수 있어요 — ")}
              <a
                href="#"
                onClick={(e) => {
                  e.preventDefault();
                  props.openSettings("general");
                }}
              >
                {L("Settings", "설정")}
              </a>
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

function Setup(props: { settings: Settings; perms: PermState | null; save: (s: Settings) => Promise<Settings>; refreshPerms: () => void; openSettings: (tab: string) => void }) {
  const { perms, settings } = props;
  const L = useL();
  const toast = useToast();
  const [model] = useLocalModel(settings.stt_engine === "local" ? settings.local_model : undefined);
  const [textModel] = useLocalModel(settings.llm_provider === "local" ? settings.local_llm_model : undefined);
  const [running, setRunning] = useState(false);
  if (!perms) return null;

  const micOk = perms.microphone === "granted";
  const local = settings.stt_engine === "local";
  const sttOk = local ? !!model?.installed : !!settings.elevenlabs_api_key.trim();
  const localText = settings.llm_provider === "local";
  const textOk = localText ? !!textModel?.installed : !!settings.openrouter_api_key.trim();
  const allOk = perms.accessibility && perms.hotkeys_ready && micOk && sttOk && textOk;
  if (allOk && !settings.onboarding_done) props.save({ ...settings, onboarding_done: true });

  const askMic = async () => {
    if (perms.microphone === "denied") {
      await api.openPrivacyPane("microphone");
    } else {
      await api.startMicTest(null).catch(() => {});
      setTimeout(() => api.stopMicTest(), 1500);
    }
    setTimeout(props.refreshPerms, 1600);
  };
  const askAccessibility = async () => {
    await api.requestAccessibility();
    await api.openPrivacyPane("accessibility");
  };
  /** Everything that needs no typing, in one go: permission prompts and both model downloads. */
  const setupAll = async () => {
    setRunning(true);
    try {
      if (!micOk && perms.microphone !== "denied") await askMic();
      if (!perms.accessibility) await api.requestAccessibility();
      const jobs: Promise<unknown>[] = [];
      if (local && model && !model.installed && !model.downloading) {
        jobs.push(api.downloadLocalModel(model.id).catch((e) => toast(String(e), { tone: "error" })));
      }
      if (localText && textModel && !textModel.installed && !textModel.downloading) {
        jobs.push(api.downloadLocalModel(textModel.id).catch((e) => toast(String(e), { tone: "error" })));
      }
      await Promise.all(jobs);
      props.refreshPerms();
    } finally {
      setRunning(false);
    }
  };

  let n = 0;
  const step = (done: boolean) => (done ? "✓" : String(++n));
  return (
    <>
      {perms.typeless_running && (
        <div className="banner warn">
          <div>
            <h4>{L("Typeless is running", "Typeless가 실행 중이에요")}</h4>
            <p>
              {L(
                "Both apps react to the Fn key, so every dictation happens twice. Quit Typeless while you use Sori.",
                "두 앱이 모두 Fn 키에 반응해서 받아쓰기가 두 번 일어납니다. Sori를 쓰는 동안에는 Typeless를 종료해 주세요.",
              )}
            </p>
          </div>
        </div>
      )}
      {!allOk && (
        <div className="banner">
          <div style={{ flex: 1 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <h4 style={{ flex: 1 }}>{L("Get set up", "시작하기 전에")}</h4>
              <button className="btn primary small" disabled={running} onClick={setupAll}>
                {running ? L("Setting up…", "설정 중…") : L("Set up everything", "한 번에 설정")}
              </button>
            </div>
            <p className="hint" style={{ margin: "2px 0 8px" }}>
              {isWin
                ? L(
                    "One click runs every step below: it allows the microphone and downloads the on-device models (resumable — you can keep working).",
                    "버튼 하나로 아래 단계를 모두 진행해요: 마이크를 허용하고 로컬 모델을 내려받아요 (이어받기 지원 — 다른 작업을 해도 돼요).",
                  )
                : L(
                    "One click runs every step below. macOS asks you to allow Microphone and Accessibility, then the on-device models download in the background (resumable).",
                    "버튼 하나로 아래 단계를 모두 진행해요. macOS가 마이크·손쉬운 사용 허용을 묻고, 로컬 모델은 백그라운드에서 내려받아요 (이어받기 지원).",
                  )}
            </p>
            <div className="checklist">
              {!isWin && (
              <div className={`check ${perms.accessibility ? "done" : ""}`}>
                <span className="mark">{step(perms.accessibility)}</span>
                <div className="grow">
                  <b>{L("Accessibility", "손쉬운 사용 권한")}</b>
                  {L(" — to detect the Fn shortcut and paste at your cursor. Turn on Sori in the list that opens.", " — Fn 단축키 감지와 커서 위치에 붙여넣기에 필요해요. 열리는 목록에서 Sori를 켜 주세요.")}
                </div>
                {!perms.accessibility && (
                  <button className="btn small" onClick={askAccessibility}>
                    {L("Allow", "허용하기")}
                  </button>
                )}
              </div>
              )}
              <div className={`check ${micOk ? "done" : ""}`}>
                <span className="mark">{step(micOk)}</span>
                <div className="grow">
                  <b>{L("Microphone", "마이크 권한")}</b>
                  {" — "}
                  {perms.microphone === "denied"
                    ? L("denied. Turn on Sori in System Settings.", "거부됨. 시스템 설정에서 Sori를 켜 주세요.")
                    : L("so Sori can hear you.", "말을 듣기 위해 필요해요.")}
                </div>
                {!micOk && (
                  <button className="btn small" onClick={askMic}>
                    {perms.microphone === "denied" ? L("Open Settings", "설정 열기") : L("Allow", "허용하기")}
                  </button>
                )}
              </div>
              <div className={`check ${sttOk ? "done" : ""}`}>
                <span className="mark">{step(sttOk)}</span>
                <div className="grow">
                  {local ? (
                    <>
                      <b>{L("Local AI · speech (Whisper)", "로컬 AI · 음성 인식 (Whisper)")}</b>
                      {L(` — one-time 574MB download; your voice never leaves this ${DEVICE}.`, ` — 574MB 한 번만 내려받으면 음성이 이 ${DEVICE} 밖으로 나가지 않아요.`)}
                      {!sttOk && (
                        <div style={{ marginTop: 6 }}>
                          <LocalModelControl model={settings.local_model} language={settings.stt_language} hasEleven={false} compact />
                        </div>
                      )}
                    </>
                  ) : (
                    <>
                      <b>{L("ElevenLabs API key", "ElevenLabs API 키")}</b>
                      {L(" — for speech recognition.", " — 음성 인식에 사용해요.")}
                    </>
                  )}
                </div>
                {!local && !sttOk && (
                  <button className="btn small" onClick={() => props.openSettings("ai")}>
                    {L("Settings", "설정")}
                  </button>
                )}
              </div>
              <div className={`check ${textOk ? "done" : ""}`}>
                <span className="mark">{step(textOk)}</span>
                <div className="grow">
                  {localText ? (
                    <>
                      <b>
                        {L("Local AI · text", "로컬 AI · 텍스트")} ({textModel?.label ?? "Gemma"})
                      </b>
                      {L(
                        ` — cleans up what you said, on this ${DEVICE}. One-time download.`,
                        ` — 말한 내용을 이 ${DEVICE}에서 다듬어요. 한 번만 내려받으면 돼요.`,
                      )}
                      {!textOk && (
                        <div style={{ marginTop: 6 }}>
                          <LocalModelControl model={settings.local_llm_model} language={settings.stt_language} compact />
                        </div>
                      )}
                    </>
                  ) : (
                    <>
                      <b>{L("OpenRouter API key", "OpenRouter API 키")}</b>
                      {L(" — for cleanup, translation and Ask.", " — 다듬기·번역·Ask에 사용해요.")}
                    </>
                  )}
                </div>
                {!localText && !textOk && (
                  <button className="btn small" onClick={() => props.openSettings("ai")}>
                    {L("Settings", "설정")}
                  </button>
                )}
              </div>
              {perms.accessibility && !perms.hotkeys_ready && (
                <p className="hint">
                  {L("Applying the permission… If this stays for more than a few seconds, quit and reopen Sori.", "권한이 반영되는 중이에요… 몇 초 뒤에도 그대로면 Sori를 다시 실행해 주세요.")}
                </p>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}

const WEEKS = 22;

function Heatmap({ stats }: { stats: Stats | null }) {
  const t = useT();
  const lang = useLang();
  const { cells, months } = useMemo(() => {
    const byDay = new Map((stats?.days ?? []).map((d) => [d.day, d.words]));
    const max = Math.max(1, ...Array.from(byDay.values()));
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const start = new Date(today);
    start.setDate(start.getDate() - today.getDay() - (WEEKS - 1) * 7);
    const cells: { key: string; level: number; future: boolean; title: string }[] = [];
    const months: { col: number; label: string }[] = [];
    for (let i = 0; i < WEEKS * 7; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      const w = byDay.get(key) ?? 0;
      const level = w === 0 ? 0 : Math.min(4, Math.ceil((w / max) * 4));
      cells.push({ key, level, future: d > today, title: `${key} · ${w} ${t.words}` });
      const col = Math.floor(i / 7);
      if ((d.getDate() === 1 || (i === 0 && d.getDate() <= 20)) && !months.some((m) => m.col === col)) {
        months.push({ col, label: lang === "ko" ? `${d.getMonth() + 1}월` : d.toLocaleString("en-US", { month: "short" }) });
      }
    }
    return { cells, months };
  }, [stats, t.words, lang]);
  const color = (l: number) => (l === 0 ? undefined : `color-mix(in srgb, var(--accent) ${[0, 30, 55, 78, 100][l]}%, var(--heat-0))`);
  return (
    <div>
      <div className="heatmap">
        <div className="heat-days">
          {["S", "M", "T", "W", "T", "F", "S"].map((d, i) => (
            <span key={i}>{d}</span>
          ))}
        </div>
        <div className="heat-grid">
          {cells.map((c) => (
            <div key={c.key} title={c.title} className={`heat-cell ${c.future ? "future" : ""}`} style={{ background: c.future ? undefined : color(c.level) }} />
          ))}
        </div>
      </div>
      <div style={{ position: "relative", height: 16, marginLeft: 26, marginTop: 6, fontSize: 11, color: "var(--faint)" }}>
        {months.map((m) => (
          <span key={m.col} style={{ position: "absolute", left: m.col * 19 }}>
            {m.label}
          </span>
        ))}
      </div>
      <div className="heat-legend">
        {t.less}
        {[1, 2, 3, 4].map((l) => (
          <span key={l} className="heat-cell" style={{ background: color(l), display: "inline-block" }} />
        ))}
        {t.more}
      </div>
    </div>
  );
}
