import { useEffect, useMemo, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, HistoryEntry, Settings } from "../api";
import { confirmDialog, PASTE, DEVICE } from "../bridge";
import { dateLocale, L, Lang, noteText, useL, useLang, useT } from "../i18n";
import { IconCheck, IconCopy, IconDownload, IconLock, IconRetry, IconTrash } from "../icons";
import { useToast } from "./toast";

const retention = (L: L): [string, string][] => [
  ["forever", L("Forever", "영원히")],
  ["1y", L("1 year", "1년")],
  ["1m", L("1 month", "1개월")],
  ["1w", L("1 week", "1주")],
  ["24h", L("24 hours", "24시간")],
  ["never", L("Never", "안 함")],
];

function dayLabel(ts: number, L: L, lang: Lang) {
  const d = new Date(ts);
  const today = new Date();
  const y = new Date();
  y.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return L("Today", "오늘");
  if (d.toDateString() === y.toDateString()) return L("Yesterday", "어제");
  return d.toLocaleDateString(dateLocale(lang), { year: "numeric", month: "long", day: "numeric", weekday: "short" });
}

function timeLabel(ts: number) {
  return new Date(ts).toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" });
}

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

export default function History({ settings, save, tick }: { settings: Settings; save: (s: Settings) => Promise<Settings>; tick: number }) {
  const t = useT();
  const L = useL();
  const lang = useLang();
  const toast = useToast();
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<HistoryEntry[]>([]);
  const [retrying, setRetrying] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [updated, setUpdated] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const load = () =>
    api
      .listHistory(filter, query)
      .then(setItems)
      .catch((e) => toast(`${L("Couldn't load history", "기록을 불러오지 못했어요")}: ${errText(e)}`, { tone: "error" }));
  useEffect(() => {
    load();
  }, [filter, query, tick]);

  const groups = useMemo(() => {
    const g: [string, HistoryEntry[]][] = [];
    for (const it of items) {
      const l = dayLabel(it.created_at, L, lang);
      if (!g.length || g[g.length - 1][0] !== l) g.push([l, []]);
      g[g.length - 1][1].push(it);
    }
    return g;
  }, [items, lang]);

  const copy = async (e: HistoryEntry) => {
    const text = e.output || e.raw_text;
    if (!text) return toast(L("Nothing to copy", "복사할 텍스트가 없어요"), { tone: "info" });
    try {
      await api.copyText(text);
      setCopied(e.id);
      setTimeout(() => setCopied((c) => (c === e.id ? null : c)), 1600);
      toast(L(`Copied to clipboard — paste with ${PASTE}`, `클립보드에 복사했어요 — ${PASTE}로 붙여넣으세요`));
    } catch (err) {
      toast(`${L("Couldn't copy", "복사하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    }
  };

  const retry = async (e: HistoryEntry) => {
    setRetrying(e.id);
    try {
      const r = await api.retryHistory(e.id);
      await load();
      if (r.status === "ok") {
        setUpdated(e.id);
        setTimeout(() => setUpdated((u) => (u === e.id ? null : u)), 2000);
        toast(L("Reprocessed from the same audio", "같은 오디오로 다시 처리했어요"));
      } else if (r.status === "empty") {
        toast(L("Reprocessed, but no speech was detected", "다시 처리했지만 인식된 말이 없었어요"), { tone: "info" });
      } else {
        toast(`${L("Couldn't reprocess", "다시 처리하지 못했어요")}: ${r.error ?? L("Unknown error", "알 수 없는 오류")}`, {
          tone: "error",
        });
      }
    } catch (err) {
      toast(`${L("Couldn't reprocess", "다시 처리하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    } finally {
      setRetrying(null);
    }
  };

  const download = async (e: HistoryEntry) => {
    try {
      const dest = await saveDialog({
        defaultPath: `sori-${new Date(e.created_at).toISOString().slice(0, 19).replace(/[:T]/g, "-")}.wav`,
        filters: [{ name: L("WAV audio", "WAV 오디오"), extensions: ["wav"] }],
      });
      if (!dest) return; // cancelled
      await api.exportAudio(e.id, dest);
      toast(`${L("Audio saved", "오디오를 저장했어요")}: ${dest.split("/").pop()}`, {
        action: { label: L("Show in Finder", "Finder에서 보기"), onClick: () => revealItemInDir(dest) },
        ms: 5000,
      });
    } catch (err) {
      toast(`${L("Couldn't save audio", "오디오를 저장하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    }
  };

  const del = async (e: HistoryEntry) => {
    setRemoving(e.id);
    try {
      await api.deleteHistory(e.id);
      await new Promise((r) => setTimeout(r, 180)); // let the fade play
      await load();
      toast(L("Entry deleted", "기록을 삭제했어요"));
    } catch (err) {
      toast(`${L("Couldn't delete", "삭제하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    } finally {
      setRemoving(null);
    }
  };

  const deleteAll = async () => {
    if (!items.length) return toast(L("No history to delete", "삭제할 기록이 없어요"), { tone: "info" });
    const ok = await confirmDialog(
      L("Delete all history and recordings? This can't be undone.", "모든 기록과 녹음 파일을 삭제할까요? 되돌릴 수 없습니다."),
      L("Delete all", "모두 삭제"),
    );
    if (!ok) return;
    try {
      await api.deleteAllHistory();
      await load();
      toast(L("All history deleted", "모든 기록을 삭제했어요"));
    } catch (err) {
      toast(`${L("Couldn't delete", "삭제하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    }
  };

  const changeRetention = async (v: string) => {
    try {
      await save({ ...settings, history_retention: v });
      toast(
        v === "never"
          ? L("History will no longer be saved", "이제 기록을 저장하지 않아요")
          : `${L("Keep history for", "기록 보관 기간")}: ${retention(L).find(([k]) => k === v)?.[1]}`,
      );
    } catch (err) {
      toast(`${L("Couldn't save settings", "설정을 저장하지 못했어요")}: ${errText(err)}`, { tone: "error" });
    }
  };

  const privacy = [
    settings.stt_engine === "local"
      ? L(`Speech is transcribed on this ${DEVICE}.`, `음성 인식은 이 ${DEVICE}에서 처리됩니다.`)
      : L("Audio is sent to ElevenLabs for transcription.", "음성은 ElevenLabs로 전송되어 인식됩니다."),
    settings.llm_provider === "local"
      ? L(
          `Cleanup also runs on this ${DEVICE} — nothing is sent anywhere.`,
          `텍스트 다듬기도 이 ${DEVICE}에서 처리되어 어디로도 전송되지 않습니다.`,
        )
      : L("Text is sent to OpenRouter for cleanup.", "텍스트는 OpenRouter로 전송되어 다듬어집니다."),
    L("History and recordings stay only on this device.", "기록·오디오 파일은 로컬에만 남습니다."),
  ].join(" ");

  return (
    <div className="page">
      <div style={{ display: "flex", alignItems: "center" }}>
        <h1 className="page-title" style={{ flex: 1 }}>
          {t.history}
        </h1>
        <button className="btn ghost danger" onClick={deleteAll}>
          {L("Delete all", "모두 삭제")}
        </button>
      </div>
      <div className="card privacy">
        <div className="row">
          <div className="grow">
            <h4>{L("Keep history", "기록 유지")}</h4>
            <p>
              {L(
                "Choose how long dictation history and recordings are kept on this device.",
                "이 기기에 받아쓰기 기록과 녹음을 얼마나 오래 보관할지 선택하세요.",
              )}
            </p>
          </div>
          <select className="select" style={{ width: 160 }} value={settings.history_retention} onChange={(e) => changeRetention(e.target.value)}>
            {retention(L).map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </div>
        <div className="row" style={{ borderTop: "1px solid var(--border)", paddingTop: 14 }}>
          <span style={{ color: "var(--muted)", width: 18, height: 18, display: "inline-flex" }}>
            <IconLock />
          </span>
          <div className="grow">
            <h4>{L(`History is stored only on this ${DEVICE}`, `기록은 이 ${DEVICE}에만 저장됩니다`)}</h4>
            <p>{privacy}</p>
          </div>
        </div>
      </div>
      <div className="toolbar">
        <div className="tabs">
          {[
            ["all", L("All", "모두")],
            ["dictate", L("Dictation", "받아쓰기")],
            ["ask", L("Ask anything", "무엇이든 물어보세요")],
          ].map(([v, l]) => (
            <button key={v} className={`tab ${filter === v ? "active" : ""}`} onClick={() => setFilter(v)}>
              {l}
            </button>
          ))}
        </div>
        <div className="grow" />
        <input className="input search" placeholder={L("Search", "검색")} value={query} onChange={(e) => setQuery(e.target.value)} />
      </div>
      {items.length === 0 && (
        <div className="empty">
          {query
            ? L("No results", "검색 결과가 없어요")
            : L(
                "No history yet. Hold Fn in any text field and start talking.",
                "아직 기록이 없어요. 아무 텍스트 칸에서 Fn을 누르고 말해 보세요.",
              )}
        </div>
      )}
      {groups.map(([label, list]) => (
        <div key={label}>
          <div className="day-label">{label}</div>
          <div className="hist-list">
            {list.map((e) => (
              <div
                key={e.id}
                className={`hist-item ${retrying === e.id ? "busy" : ""} ${updated === e.id ? "flash" : ""} ${removing === e.id ? "removing" : ""}`}
              >
                <div className="hist-time">{timeLabel(e.created_at)}</div>
                <div>
                  {e.mode === "ask" && e.raw_text && <div style={{ color: "var(--muted)", fontSize: 13, marginBottom: 4 }}>“{e.raw_text}”</div>}
                  <div className="hist-text selectable">
                    {retrying === e.id ? (
                      <span className="hint">{L("Reprocessing the same audio…", "같은 오디오로 다시 처리하는 중…")}</span>
                    ) : (
                      e.output || e.raw_text || (e.status === "error" ? L("Processing failed", "처리 실패") : "")
                    )}
                  </div>
                  {e.error && <div className={e.status === "error" ? "err-text" : "hint"}>{e.status === "error" ? e.error : noteText(L, e.error)}</div>}
                  {open === e.id && e.raw_text && e.mode !== "ask" && <div className="raw selectable">{L("Original", "원문")} · {e.raw_text}</div>}
                  <div className="hist-meta">
                    {e.mode === "translate" && <span className="tag translate">{L("Translation", "번역")}</span>}
                    {e.mode === "ask" && <span className="tag ask">Ask · {e.action}</span>}
                    {e.status === "error" && <span className="tag error">{L("Error", "오류")}</span>}
                    {e.app_name && <span>{e.app_name}</span>}
                    <span>{L(`${(e.duration_ms / 1000).toFixed(1)}s`, `${(e.duration_ms / 1000).toFixed(1)}초`)}</span>
                    {e.status === "ok" &&
                      (e.language === "one-step" ? (
                        <span>{L("One-step AI", "원스텝 AI")} {e.llm_ms}ms</span>
                      ) : (
                        <span>
                          STT {e.stt_ms}ms · AI {e.llm_ms}ms
                        </span>
                      ))}
                    {e.raw_text && e.mode !== "ask" && (
                      <button className="mini" onClick={() => setOpen(open === e.id ? null : e.id)}>
                        {open === e.id ? L("Hide original", "원문 숨기기") : L("Show original", "원문 보기")}
                      </button>
                    )}
                  </div>
                </div>
                <div className="hist-actions">
                  <button
                    className={`icon-btn ${copied === e.id ? "done" : ""}`}
                    title={L("Copy", "복사")}
                    aria-label={L("Copy", "복사")}
                    onClick={() => copy(e)}
                  >
                    {copied === e.id ? <IconCheck /> : <IconCopy />}
                  </button>
                  {e.audio_path && (
                    <button
                      className="icon-btn"
                      title={L("Retry with the same audio", "같은 오디오로 재시도")}
                      aria-label={L("Retry", "재시도")}
                      disabled={retrying !== null}
                      onClick={() => retry(e)}
                    >
                      <span className={retrying === e.id ? "spin" : ""} style={{ display: "inline-flex" }}>
                        <IconRetry />
                      </span>
                    </button>
                  )}
                  {e.audio_path && (
                    <button
                      className="icon-btn"
                      title={L("Download audio", "오디오 다운로드")}
                      aria-label={L("Download audio", "오디오 다운로드")}
                      onClick={() => download(e)}
                    >
                      <IconDownload />
                    </button>
                  )}
                  <button
                    className="icon-btn"
                    title={L("Delete", "삭제")}
                    aria-label={L("Delete", "삭제")}
                    disabled={removing === e.id}
                    onClick={() => del(e)}
                  >
                    <IconTrash />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
