import { useEffect, useState } from "react";
import { confirmDialog, on as listen } from "../bridge";
import { api, LocalModel, ModelDownload } from "../api";
import { type L, useL } from "../i18n";
import { useToast } from "./toast";

const ACTIVE = ["connecting", "downloading", "retrying", "verifying"];
export const isActive = (d: ModelDownload | null | undefined) => !!d && ACTIVE.includes(d.state);

const mb = (n: number) => `${Math.round(n / 1e6)} MB`;
function eta(L: L, d: ModelDownload) {
  if (!d.bytes_per_sec) return "";
  const s = Math.max(1, Math.round((d.total - d.downloaded) / d.bytes_per_sec));
  const t = s >= 90 ? L(`${Math.round(s / 60)} min`, `${Math.round(s / 60)}분`) : L(`${s}s`, `${s}초`);
  return L(`about ${t} left`, `약 ${t} 남음`);
}

/** Model status + live download progress (events), refreshed on state changes. */
export function useLocalModel(id: string | undefined) {
  const [m, setM] = useState<LocalModel | null>(null);
  const refresh = () =>
    api
      .localModels()
      .then((list) => setM(list.find((x) => x.id === id) ?? list[0] ?? null))
      .catch(() => {});
  useEffect(() => {
    refresh();
    const un = listen<ModelDownload>("local-model-download", (e) => {
      const d = e.payload;
      if (id && d.id !== id) return;
      if (ACTIVE.includes(d.state)) setM((prev) => (prev ? { ...prev, download: d, downloading: true } : prev));
      else refresh(); // done / paused / error / cancelled: re-read files on disk
    });
    // Loading into memory after a download happens in the background: poll briefly.
    const iv = setInterval(refresh, 4000);
    window.addEventListener("focus", refresh);
    return () => {
      un.then((u) => u());
      clearInterval(iv);
      window.removeEventListener("focus", refresh);
    };
  }, [id]);
  return [m, refresh] as const;
}

/**
 * Everything about getting the on-device model onto disk: progress with speed/ETA, pause,
 * resume (also after quitting the app), automatic retries, retry after errors, discard.
 */
export function LocalModelControl({ model, language, hasEleven = true, compact = false }: { model: string; language: string; hasEleven?: boolean; compact?: boolean }) {
  const L = useL();
  const toast = useToast();
  const [m, refresh] = useLocalModel(model);
  if (!m) return null;

  const d = m.download;
  const active = isActive(d);
  const start = () => api.downloadLocalModel(m.id).then(() => setTimeout(refresh, 300));
  const pause = () => api.pauseLocalDownload();
  const discard = async () => {
    if (!(await confirmDialog(L("Discard the downloaded part? You'll have to start from zero.", "받은 부분을 버릴까요? 처음부터 다시 받아야 해요."), L("Discard", "버리기")))) return;
    await api.cancelLocalDownload(m.id);
    refresh();
  };
  const remove = async () => {
    if (!(await confirmDialog(L(`Delete the on-device model (${mb(m.size)})?`, `로컬 모델(${mb(m.size)})을 삭제할까요?`), L("Delete", "삭제")))) return;
    await api.deleteLocalModel(m.id);
    toast(L("On-device model deleted", "로컬 모델을 삭제했어요"));
    refresh();
  };

  // ---- installed
  if (m.installed) {
    return (
      <div className="dl">
        <div className="key-status ok">
          ✓ {L("Installed", "설치됨")} · {m.loaded ? L("ready", "바로 사용 가능") : L("loading into memory…", "메모리에 올리는 중…")}
        </div>
        {!compact && (
          <button className="btn small ghost" style={{ justifySelf: "end" }} onClick={remove}>
            {L("Delete model", "모델 삭제")} ({mb(m.size)})
          </button>
        )}
        {!compact && language === "" && (
          <span className="hint">
            {L(
              "Tip: pinning “Spoken language” in the Language tab makes on-device recognition about 2× faster (but hurts accuracy for the other language).",
              "팁: 언어 탭에서 ‘말하는 언어’를 고정하면 로컬 인식이 약 2배 빨라져요(다른 언어로 말할 때는 정확도가 떨어짐).",
            )}
          </span>
        )}
      </div>
    );
  }

  const done = active ? d!.downloaded : d?.downloaded || m.partial;
  const pct = Math.min(100, Math.floor((done / Math.max(1, m.size)) * 100));

  // ---- downloading / retrying / verifying
  if (active && d) {
    const indeterminate = d.state === "connecting" || d.state === "verifying";
    const line =
      d.state === "connecting"
        ? L("Connecting…", "연결 중…")
        : d.state === "verifying"
          ? L("Verifying the download…", "받은 파일 확인 중…")
          : d.state === "retrying"
            ? L(`Connection lost — retrying (${d.attempt}/5)…`, `연결이 끊겼어요 — 다시 시도 중 (${d.attempt}/5)…`)
            : [`${mb(d.downloaded)} / ${mb(d.total)}`, d.bytes_per_sec ? `${(d.bytes_per_sec / 1e6).toFixed(1)} MB/s` : "", eta(L, d)].filter(Boolean).join(" · ");
    return (
      <div className="dl">
        <div className={`meter dl-bar ${indeterminate ? "indeterminate" : ""} ${d.state === "retrying" ? "warn" : ""}`}>
          <div style={{ width: indeterminate ? undefined : `${pct}%` }} />
        </div>
        <div className="dl-row">
          <span className="hint">
            {!indeterminate && <b>{pct}% </b>}
            {line}
          </span>
          {d.state !== "verifying" && (
            <span className="dl-actions">
              <button className="btn small" onClick={pause}>
                {L("Pause", "일시정지")}
              </button>
              <button className="btn small ghost" onClick={discard}>
                {L("Cancel", "취소")}
              </button>
            </span>
          )}
        </div>
        {!compact && (
          <span className="hint">
            {L("You can keep using Sori or even quit — the download picks up where it left off.", "다른 작업을 하거나 앱을 종료해도 괜찮아요 — 받던 곳부터 이어서 받아요.")}
          </span>
        )}
      </div>
    );
  }

  // ---- failed
  if (d?.state === "error") {
    return (
      <div className="dl">
        {done > 0 && (
          <div className="meter dl-bar warn">
            <div style={{ width: `${pct}%` }} />
          </div>
        )}
        <span className="key-status bad">
          ✕ {L("Download failed", "다운로드 실패")}
          {done > 0 ? L(` at ${pct}%`, ` (${pct}%에서 멈춤)`) : ""}: {d.error}
        </span>
        <span className="dl-actions">
          <button className="btn primary small" onClick={start}>
            {done > 0 ? L("Retry — resume from here", "다시 받기 — 이어서") : L("Retry", "다시 받기")}
          </button>
          {done > 0 && (
            <button className="btn small ghost" onClick={discard}>
              {L("Start over", "처음부터")}
            </button>
          )}
        </span>
        {!compact && (
          <span className="hint">{L("Check your internet connection and free disk space, then retry.", "인터넷 연결과 남은 저장 공간을 확인한 뒤 다시 받아 주세요.")}</span>
        )}
      </div>
    );
  }

  // ---- paused / interrupted (partial file on disk)
  if (m.partial > 0) {
    return (
      <div className="dl">
        <div className="meter dl-bar paused">
          <div style={{ width: `${pct}%` }} />
        </div>
        <div className="dl-row">
          <span className="hint">
            <b>{pct}% </b>
            {m.paused ? L("Paused", "일시정지됨") : L("Interrupted", "중단됨")} · {mb(m.partial)} / {mb(m.size)}
          </span>
          <span className="dl-actions">
            <button className="btn primary small" onClick={start}>
              {L("Resume", "이어 받기")}
            </button>
            <button className="btn small ghost" onClick={discard}>
              {L("Discard", "버리기")}
            </button>
          </span>
        </div>
      </div>
    );
  }

  // ---- not downloaded
  return (
    <div className="dl">
      {!compact && (
        <span className="key-status bad">
          {hasEleven
            ? L("Not downloaded yet — ElevenLabs is used until then", "아직 받지 않았어요 — 받기 전에는 ElevenLabs로 처리해요")
            : L("Not downloaded yet — dictation needs it", "아직 받지 않았어요 — 받아쓰기에 필요해요")}
        </span>
      )}
      <button className="btn primary small" onClick={start}>
        {L("Download", "내려받기")} ({mb(m.size)})
      </button>
    </div>
  );
}

/** Sidebar pill: visible from anywhere while the model is downloading, paused or failed. */
export function DownloadPill({ model, enabled, onOpen }: { model: string; enabled: boolean; onOpen: () => void }) {
  const L = useL();
  const [m] = useLocalModel(enabled ? model : undefined);
  if (!enabled || !m || m.installed) return null;
  const d = m.download;
  const done = isActive(d) ? d!.downloaded : d?.downloaded || m.partial;
  const pct = Math.min(100, Math.floor((done / Math.max(1, m.size)) * 100));
  const label = isActive(d)
    ? d!.state === "verifying"
      ? L("Verifying model…", "모델 확인 중…")
      : d!.state === "retrying"
        ? L(`Model ${pct}% · retrying`, `모델 ${pct}% · 재시도 중`)
        : L(`Downloading model ${pct}%`, `모델 받는 중 ${pct}%`)
    : d?.state === "error"
      ? L("Model download failed", "모델 다운로드 실패")
      : m.partial > 0
        ? L(`Model paused at ${pct}%`, `모델 ${pct}%에서 멈춤`)
        : L("Speech model needed", "음성 모델 필요");
  const tone = d?.state === "error" ? "bad" : isActive(d) ? "" : "idle";
  return (
    <button className={`dl-pill ${tone}`} onClick={onOpen} title={L("Open Settings → AI", "설정 → AI 열기")}>
      <span>{label}</span>
      <span className="dl-pill-bar">
        <span style={{ width: `${pct}%` }} />
      </span>
    </button>
  );
}
