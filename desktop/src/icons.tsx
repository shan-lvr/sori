const base = { fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

export const IconHome = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1z" /></svg>
);
export const IconClock = () => (
  <svg viewBox="0 0 24 24" {...base}><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>
);
export const IconBook = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z" /><path d="M5 17a3 3 0 0 1 3-3h11" /></svg>
);
export const IconGear = () => (
  <svg viewBox="0 0 24 24" {...base}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
);
export const IconCopy = () => (
  <svg viewBox="0 0 24 24" {...base}><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V5a2 2 0 0 1 2-2h8" /></svg>
);
export const IconRetry = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M3 12a9 9 0 0 1 15.5-6.2L21 8" /><path d="M21 3v5h-5" /><path d="M21 12a9 9 0 0 1-15.5 6.2L3 16" /><path d="M3 21v-5h5" /></svg>
);
export const IconDownload = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M12 3v12" /><path d="m7 10 5 5 5-5" /><path d="M5 21h14" /></svg>
);
export const IconTrash = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M4 7h16" /><path d="M10 11v6M14 11v6" /><path d="M6 7l1 13h10l1-13" /><path d="M9 7V4h6v3" /></svg>
);
export const IconX = () => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={2.2}><path d="M6 6l12 12M18 6 6 18" /></svg>
);
export const IconCheck = () => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={2.4}><path d="m5 12 5 5 9-10" /></svg>
);
export const IconLock = () => (
  <svg viewBox="0 0 24 24" {...base}><rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></svg>
);
export const IconKeyboard = () => (
  <svg viewBox="0 0 24 24" {...base}><rect x="2" y="6" width="20" height="12" rx="2" /><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10" /></svg>
);
export const IconGlobe = () => (
  <svg viewBox="0 0 24 24" {...base}><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></svg>
);
export const IconMic = () => (
  <svg viewBox="0 0 24 24" {...base}><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" /></svg>
);
export const IconSpark = () => (
  <svg viewBox="0 0 24 24" {...base}><path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6" /></svg>
);
export const IconUser = () => (
  <svg viewBox="0 0 24 24" {...base}><circle cx="12" cy="8" r="4" /><path d="M4 21a8 8 0 0 1 16 0" /></svg>
);
export const IconInfo = () => (
  <svg viewBox="0 0 24 24" {...base}><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5h.01" /></svg>
);
