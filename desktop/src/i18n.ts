import { createContext, useContext, useEffect, useState } from "react";
import { Settings, settingsWhenReady } from "./api";
import { inTauri, on } from "./bridge";

const ko = {
  home: "홈",
  history: "기록",
  dictionary: "사전",
  settings: "설정",
  heroTitle: "말하세요, 타이핑하지 마세요",
  insights: "인사이트",
  wordsDictated: "받아쓴 단어 수",
  timeSaved: "절약된 시간",
  avgSpeed: "평균 받아쓰기 속도",
  totalTime: "총 받아쓰기 시간",
  activeDays: "활동 일수",
  currentStreak: "현재 연속 일수",
  longestStreak: "최장 연속 일수",
  less: "적게",
  more: "더 많이",
  dictate: "받아쓰기",
  translate: "번역하기",
  ask: "무엇이든 물어보세요",
  words: "단어",
  days: "일",
  hotkeysReady: "단축키 대기 중",
  hotkeysOff: "단축키 비활성",
};

type Dict = typeof ko;

const en: Dict = {
  home: "Home",
  history: "History",
  dictionary: "Dictionary",
  settings: "Settings",
  heroTitle: "Speak, don't type",
  insights: "Insights",
  wordsDictated: "Words dictated",
  timeSaved: "Time saved",
  avgSpeed: "Average dictation speed",
  totalTime: "Total dictation time",
  activeDays: "Active days",
  currentStreak: "Current streak",
  longestStreak: "Longest streak",
  less: "Less",
  more: "More",
  dictate: "Dictate",
  translate: "Translate",
  ask: "Ask anything",
  words: "words",
  days: "days",
  hotkeysReady: "Shortcuts ready",
  hotkeysOff: "Shortcuts off",
};

export const dictionaries = { ko, en };
export type Lang = keyof typeof dictionaries;
export const LangContext = createContext<Lang>("en");
export function useT() {
  const lang = useContext(LangContext);
  return dictionaries[lang] ?? en;
}

/** Inline translation: `L("English", "한국어")`. English is the default for anything but "ko". */
export type L = (en: string, ko: string) => string;
export const makeL =
  (lang: Lang | string | undefined): L =>
  (e, k) =>
    lang === "ko" ? k : e;
export function useL(): L {
  return makeL(useContext(LangContext));
}
export function useLang(): Lang {
  return useContext(LangContext);
}
export const dateLocale = (lang: Lang | string | undefined) => (lang === "ko" ? "ko-KR" : "en-US");

/** Overlay windows (HUD, card) aren't under the main app's provider: read the setting directly. */
export function useOverlayLang(): Lang {
  const [lang, setLang] = useState<Lang>("en");
  useEffect(() => {
    settingsWhenReady().then((s) => setLang(s.interface_language === "ko" ? "ko" : "en"));
    if (!inTauri) return;
    const un = on<Settings>("settings-changed", (e) => setLang(e.payload.interface_language === "ko" ? "ko" : "en"));
    return () => {
      un.then((u) => u());
    };
  }, []);
  return lang;
}

/** History/HUD note codes from the pipeline (`assistant_reply`, `llm_failed: …`, `local_stt_failed: …`). */
export function noteText(L: L, code: string): string {
  const i = code.indexOf(": ");
  const key = i >= 0 ? code.slice(0, i) : code;
  const detail = i >= 0 ? code.slice(i + 2) : "";
  const base =
    key === "assistant_reply"
      ? L("The AI answered instead of cleaning up — inserted your words as spoken", "AI가 정리 대신 답변을 해서 원문을 그대로 넣었어요")
      : key === "llm_failed"
        ? L("AI cleanup failed — inserted the raw transcript", "AI 다듬기에 실패해 원문을 그대로 넣었어요")
        : key === "local_stt_failed"
          ? L("On-device recognition failed — used ElevenLabs instead", "로컬 음성 인식을 쓸 수 없어 ElevenLabs로 처리했어요")
          : key === "cleanup_simplified"
            ? L("Cleaned up at a gentler level so nothing was lost", "내용이 빠지지 않도록 한 단계 가볍게 다듬었어요")
            : key === "cleanup_dropped"
              ? L("Cleanup would have dropped part of what you said — inserted it as spoken", "다듬다가 내용이 빠질 것 같아 말한 그대로 넣었어요")
              : null;
  if (base === null) return code;
  return detail ? `${base} (${detail})` : base;
}
