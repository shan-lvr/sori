import { useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, DictWord } from "../api";
import { useL, useT } from "../i18n";
import { useToast } from "./toast";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

export default function Dictionary() {
  const t = useT();
  const L = useL();
  const toast = useToast();
  const [words, setWords] = useState<DictWord[]>([]);
  const [filter, setFilter] = useState<"all" | "auto" | "manual">("all");
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const load = () => api.listDictionary().then(setWords);
  useEffect(() => {
    load();
  }, []);

  const shown = useMemo(
    () => words.filter((w) => (filter === "all" || w.source === filter) && w.term.toLowerCase().includes(query.toLowerCase())),
    [words, filter, query],
  );

  const add = async () => {
    const terms = draft
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (!terms.length) return;
    try {
      const n = await api.addWords(terms);
      setDraft("");
      setAdding(false);
      load();
      toast(
        n === terms.length
          ? L(`Added ${n} ${n === 1 ? "word" : "words"}`, `${n}개 단어를 추가했어요`)
          : n === 0
            ? L("Already in your dictionary", "이미 사전에 있는 단어예요")
            : L(`Added ${n} (${terms.length - n} already there)`, `${n}개 추가 (${terms.length - n}개는 이미 있어요)`),
        { tone: n === 0 ? "info" : "success" },
      );
    } catch (e) {
      toast(`${L("Couldn't add", "추가하지 못했어요")}: ${errText(e)}`, { tone: "error" });
    }
  };

  const importCsv = async () => {
    const path = await openDialog({ multiple: false, filters: [{ name: "CSV / TXT", extensions: ["csv", "txt", "tsv"] }] });
    if (typeof path === "string") {
      try {
        const n = await api.importDictionary(path);
        load();
        toast(
          n ? L(`Imported ${n} ${n === 1 ? "word" : "words"}`, `${n}개 단어를 가져왔어요`) : L("No new words to add", "새로 추가할 단어가 없었어요"),
          { tone: n ? "success" : "info" },
        );
      } catch (e) {
        toast(`${L("Couldn't import", "가져오지 못했어요")}: ${errText(e)}`, { tone: "error" });
      }
    }
  };

  const toggle = (id: number) => {
    const s = new Set(selected);
    s.has(id) ? s.delete(id) : s.add(id);
    setSelected(s);
  };

  return (
    <div className="page">
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <h1 className="page-title" style={{ flex: 1 }}>
          {t.dictionary}
        </h1>
        <button className="btn" onClick={importCsv}>
          {L("Import CSV", "CSV 가져오기")}
        </button>
        <button className="btn primary" onClick={() => setAdding(true)}>
          {L("New word", "새 단어")}
        </button>
      </div>
      <p className="hint" style={{ marginTop: -12 }}>
        {L(
          "Add names, project names, acronyms, and jargon — speech recognition (ElevenLabs keyterms) and AI cleanup will both prefer these spellings.",
          "사람 이름, 프로젝트명, 약어, 전문 용어를 추가하면 음성 인식(ElevenLabs 키워드)과 AI 다듬기 모두에서 이 철자를 우선 사용해요.",
        )}
      </p>
      {adding && (
        <div className="card" style={{ margin: "14px 0", display: "flex", gap: 8 }}>
          <input
            autoFocus
            className="input"
            placeholder={L("Word or phrase (separate multiple with commas)", "단어나 구문 (쉼표로 여러 개)")}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") add();
              if (e.key === "Escape") setAdding(false);
            }}
          />
          <button className="btn primary" onClick={add}>
            {L("Add", "추가")}
          </button>
          <button className="btn ghost" onClick={() => setAdding(false)}>
            {L("Cancel", "취소")}
          </button>
        </div>
      )}
      <div className="toolbar">
        <div className="tabs">
          {(
            [
              ["all", L("All", "전체")],
              ["auto", L("Auto-added", "자동 추가됨")],
              ["manual", L("Manually added", "수동 추가됨")],
            ] as const
          ).map(([v, l]) => (
            <button key={v} className={`tab ${filter === v ? "active" : ""}`} onClick={() => setFilter(v)}>
              {l}
            </button>
          ))}
        </div>
        <div className="grow" />
        {selected.size > 0 && (
          <>
            <button className="btn ghost" onClick={() => setSelected(new Set())}>
              {L("Deselect", "선택 해제")}
            </button>
            <button
              className="btn danger"
              onClick={async () => {
                try {
                  await api.deleteWords(Array.from(selected));
                  toast(L(`Deleted ${selected.size} ${selected.size === 1 ? "word" : "words"}`, `${selected.size}개 단어를 삭제했어요`));
                  setSelected(new Set());
                  load();
                } catch (e) {
                  toast(`${L("Couldn't delete", "삭제하지 못했어요")}: ${errText(e)}`, { tone: "error" });
                }
              }}
            >
              {L(`Delete ${selected.size}`, `${selected.size}개 삭제`)}
            </button>
          </>
        )}
        <input className="input search" placeholder={L("Search", "검색")} value={query} onChange={(e) => setQuery(e.target.value)} />
      </div>
      {shown.length === 0 && (
        <div className="empty">{words.length ? L("No results", "검색 결과가 없어요") : L("No words added yet", "아직 추가한 단어가 없어요")}</div>
      )}
      <div className="chips">
        {shown.map((w) =>
          editing === w.id ? (
            <span key={w.id} className="chip">
              <input
                autoFocus
                className="input"
                style={{ width: 180, padding: "2px 8px" }}
                value={editText}
                onChange={(e) => setEditText(e.target.value)}
                onKeyDown={async (e) => {
                  if (e.key === "Enter") {
                    try {
                      await api.updateWord(w.id, editText);
                      toast(L("Word updated", "단어를 수정했어요"));
                      setEditing(null);
                      load();
                    } catch (err) {
                      toast(`${L("Couldn't update", "수정하지 못했어요")}: ${errText(err)}`, { tone: "error" });
                    }
                  }
                  if (e.key === "Escape") setEditing(null);
                }}
              />
            </span>
          ) : (
            <span key={w.id} className={`chip ${selected.has(w.id) ? "selected" : ""}`}>
              {(selected.size > 0 || selected.has(w.id)) && <input type="checkbox" checked={selected.has(w.id)} onChange={() => toggle(w.id)} />}
              <span className="selectable">{w.term}</span>
              {w.source === "auto" && <span className="src">{L("Auto", "자동")}</span>}
              <span className="chip-actions">
                {selected.size === 0 && (
                  <button className="mini" title={L("Select", "선택")} onClick={() => toggle(w.id)}>
                    ☐
                  </button>
                )}
                <button
                  className="mini"
                  title={L("Edit", "편집")}
                  onClick={() => {
                    setEditing(w.id);
                    setEditText(w.term);
                  }}
                >
                  ✎
                </button>
                <button
                  className="mini"
                  title={L("Delete", "삭제")}
                  onClick={async () => {
                    try {
                      await api.deleteWords([w.id]);
                      toast(L(`Deleted “${w.term}”`, `“${w.term}” 삭제했어요`));
                      load();
                    } catch (e) {
                      toast(`${L("Couldn't delete", "삭제하지 못했어요")}: ${errText(e)}`, { tone: "error" });
                    }
                  }}
                >
                  ✕
                </button>
              </span>
            </span>
          ),
        )}
      </div>
    </div>
  );
}
