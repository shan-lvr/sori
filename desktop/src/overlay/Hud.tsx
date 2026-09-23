import { useEffect, useRef, useState } from "react";
import { inTauri, on as listen } from "../bridge";
import { api, HudState } from "../api";
import { IconX } from "../icons";
import { targetLabel } from "../main/Settings";
import { makeL, useOverlayLang, type L as LFn } from "../i18n";

const BARS = 18;
const HIDDEN: HudState = {
  phase: "hidden",
  mode: "",
  hands_free: false,
  started_at: 0,
  target: null,
  targets: [],
  target_idx: 0,
  message: null,
  tone: "",
  countdown: null,
};

function fmt(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

const THINKING = (L: LFn): Record<string, string> => ({
  dictate: L("Polishing", "다듬는 중"),
  translate: L("Translating", "번역하는 중"),
  ask: L("Thinking", "생각하는 중"),
});

export default function Hud() {
  const [st, setSt] = useState<HudState>(HIDDEN);
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const [now, setNow] = useState(Date.now());
  const [countdown, setCountdown] = useState<number | null>(null);
  const smooth = useRef(0);
  const lang = useOverlayLang();
  const L = makeL(lang);

  useEffect(() => {
    const uns = [
      listen<HudState>("hud-state", (e) => {
        setSt(e.payload);
        if (e.payload.phase !== "recording") {
          setLevels(Array(BARS).fill(0));
          setCountdown(null);
          smooth.current = 0;
        }
      }),
      listen<number>("level", (e) => {
        // Fast attack, slower release — reads like a VU meter.
        const v = e.payload;
        smooth.current = v > smooth.current ? v : smooth.current * 0.6 + v * 0.4;
        const s = smooth.current;
        setLevels((prev) => [...prev.slice(1), s]);
      }),
      listen<number>("countdown", (e) => setCountdown(e.payload)),
    ];
    const iv = setInterval(() => setNow(Date.now()), 250);
    let demo: ReturnType<typeof setInterval> | undefined;
    if (!inTauri) {
      // Browser preview: ?window=hud&phase=recording|transcribing|thinking|message&mode=translate&hf=1
      const q = new URLSearchParams(location.search);
      const phase = (q.get("phase") ?? "recording") as HudState["phase"];
      setSt({
        ...HIDDEN,
        phase,
        mode: (q.get("mode") ?? "dictate") as HudState["mode"],
        hands_free: q.get("hf") === "1",
        started_at: Date.now() - 7000,
        target: { code: "en-US", name: "English (US)" },
        targets: [
          { code: "en-US", name: "English (US)" },
          { code: "ja-JP", name: "Japanese" },
        ],
        message: phase === "message" ? "No speech detected" : null,
        tone: "info",
      });
      demo = setInterval(() => setLevels((p) => [...p.slice(1), Math.random() ** 1.5]), 70);
    }
    return () => {
      if (demo) clearInterval(demo);
      uns.forEach((p) => p.then((u) => u()));
      clearInterval(iv);
    };
  }, []);

  if (st.phase === "hidden") return <div className="hud-root" />;

  const chip =
    st.phase === "recording" && st.mode === "translate" && st.target ? (
      st.hands_free && st.targets.length > 1 ? (
        <button className="hud-chip" title={L("Click to switch target language", "눌러서 번역 대상 바꾸기")} onClick={() => api.hudSetTarget((st.target_idx + 1) % st.targets.length)}>
          {L("Translate", "번역")} → {targetLabel(st.target, lang)} ⇄
        </button>
      ) : (
        <span className="hud-chip">
          {L("Translate", "번역")} → {targetLabel(st.target, lang)}
        </span>
      )
    ) : st.phase === "recording" && st.mode === "ask" ? (
      <span className="hud-chip">{L("Ask anything", "무엇이든 물어보세요")}</span>
    ) : null;

  return (
    <div className="hud-root">
      {chip}
      <div className={`hud-pill phase-${st.phase}`}>
        {st.phase === "recording" && (
          <>
            {st.hands_free && (
              <button className="hud-btn" title={L("Cancel (Esc)", "취소 (Esc)")} onClick={() => api.hudCancel()}>
                <IconX />
              </button>
            )}
            <div className="wave">
              {levels.map((l, i) => (
                <span key={i} style={{ height: `${Math.max(4, Math.round(4 + l * 20))}px`, opacity: 0.45 + l * 0.55 }} />
              ))}
            </div>
            {st.hands_free && (
              <>
                <span className="hud-time">{countdown != null ? `-${fmt(countdown * 1000)}` : fmt(now - st.started_at)}</span>
                <button className="hud-btn stop" title={L("Done", "완료")} onClick={() => api.hudStop()}>
                  <span className="stop-square" />
                </button>
              </>
            )}
          </>
        )}
        {st.phase === "transcribing" && (
          <div className="hud-proc">
            <div className="wave sweeping">
              {Array.from({ length: 12 }, (_, i) => (
                <span key={i} style={{ animationDelay: `${i * 70}ms` }} />
              ))}
            </div>
            <span className="hud-label">{L("Transcribing", "받아쓰는 중")}</span>
          </div>
        )}
        {st.phase === "listening" && (
          <div className="hud-proc">
            <div className="wave sweeping">
              {Array.from({ length: 12 }, (_, i) => (
                <span key={i} style={{ animationDelay: `${i * 70}ms` }} />
              ))}
            </div>
            <span className="hud-label">{st.mode === "translate" ? L("Listening & translating", "듣고 번역하는 중") : L("Listening & polishing", "듣고 다듬는 중")}</span>
          </div>
        )}
        {(st.phase === "thinking" || st.phase === "processing") && (
          <div className="hud-proc">
            <span className="dots">
              <i />
              <i />
              <i />
            </span>
            <span className="hud-label">{THINKING(L)[st.mode] ?? L("Processing", "처리 중")}</span>
          </div>
        )}
        {st.phase === "message" && <div className={`hud-msg ${st.tone}`}>{st.message}</div>}
      </div>
    </div>
  );
}
