export type IconShape =
  | { d: string; fill?: boolean }
  | { cx: number; cy: number; r: number; fill?: boolean };

const folderOutline =
  "M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z";
const frame = "M3.5 4.5h17v15h-17z";

export const ICONS = {
  inbox: [{ d: "M3 13l2.5-7.5h13L21 13v6.5H3z" }, { d: "M3 13h5l1.5 2.5h5L16 13h5" }],
  send: [{ d: "M21 3L3 10.5l7.5 3 3 7.5z" }, { d: "M21 3L10.5 13.5" }],
  draft: [{ d: "M14 3H6v18h12V7z" }, { d: "M14 3v4h4" }, { d: "M9 13h6M9 17h4" }],
  trash: [{ d: "M4 7h16" }, { d: "M9 7V4h6v3" }, { d: "M6 7l1 13h10l1-13" }, { d: "M10 11v5.5M14 11v5.5" }],
  junk: [{ d: "M8 3h8l5 5v8l-5 5H8l-5-5V8z" }, { d: "M12 7.5v5.5" }, { cx: 12, cy: 16.5, r: 0.6, fill: true }],
  archive: [{ d: "M3 4h18v4H3z" }, { d: "M5 8v12h14V8" }, { d: "M10 12h4" }],
  folder: [{ d: folderOutline }],
  folderMove: [{ d: folderOutline }, { d: "M9 13.5h6M13 11.5l2 2-2 2" }],
  star: [{ d: "M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" }],
  starFilled: [{ d: "M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z", fill: true }],
  tag: [{ d: "M3.5 3.5h7.5l9.5 9.5-7.5 7.5-9.5-9.5z" }, { cx: 8, cy: 8, r: 1.4 }],
  search: [{ cx: 11, cy: 11, r: 6.5 }, { d: "M20 20l-4.3-4.3" }],
  settings: [{ d: "M4 7h9M17 7h3M4 17h3M11 17h9" }, { cx: 15, cy: 7, r: 2 }, { cx: 9, cy: 17, r: 2 }],
  refresh: [
    { d: "M19.5 10.5A8 8 0 0 0 5.2 7.2L4 8.5" },
    { d: "M4 4.5v4h4" },
    { d: "M4.5 13.5a8 8 0 0 0 14.3 3.3l1.2-1.3" },
    { d: "M20 19.5v-4h-4" },
  ],
  plus: [{ d: "M12 5v14M5 12h14" }],
  compose: [{ d: "M4 20h4L19 9l-4-4L4 16z" }, { d: "M13.5 6.5l4 4" }],
  userPlus: [{ cx: 9, cy: 8, r: 3.5 }, { d: "M2.5 20c0-3.6 2.9-6 6.5-6s6.5 2.4 6.5 6" }, { d: "M19 8v6M16 11h6" }],
  outbox: [{ d: "M3 13v6.5h18V13" }, { d: "M12 15.5V4" }, { d: "M7.5 8.5L12 4l4.5 4.5" }],
  mail: [{ d: frame }, { d: "M3.5 6l8.5 6.5L20.5 6" }],
  layers: [{ d: "M12 3l9 5-9 5-9-5z" }, { d: "M3 12.5l9 5 9-5" }, { d: "M3 16.5l9 5 9-5" }],
  clock: [{ cx: 12, cy: 12, r: 8.5 }, { d: "M12 7.5V12l3 2" }],
  bookmark: [{ d: "M6.5 3.5h11v17l-5.5-4-5.5 4z" }],
  reply: [{ d: "M10 7.5L4.5 12.5l5.5 5" }, { d: "M4.5 12.5H14a6 6 0 0 1 6 6" }],
  replyAll: [{ d: "M8 7.5L2.5 12.5l5.5 5" }, { d: "M13 7.5l-5.5 5 5.5 5" }, { d: "M7.5 12.5H15a5 5 0 0 1 5 5" }],
  forward: [{ d: "M14 7.5l5.5 5-5.5 5" }, { d: "M19.5 12.5H10a6 6 0 0 0-6 6" }],
  copy: [{ d: "M8.5 8.5h11v11h-11z" }, { d: "M15.5 8.5v-4h-11v11h4" }],
  close: [{ d: "M6.5 6.5l11 11M17.5 6.5l-11 11" }],
  more: [
    { cx: 5.5, cy: 12, r: 1.3, fill: true },
    { cx: 12, cy: 12, r: 1.3, fill: true },
    { cx: 18.5, cy: 12, r: 1.3, fill: true },
  ],
  chevronRight: [{ d: "M9.5 6l6 6-6 6" }],
  chevronDown: [{ d: "M6 9.5l6 6 6-6" }],
  grip: [
    { cx: 9, cy: 6, r: 1.3, fill: true },
    { cx: 15, cy: 6, r: 1.3, fill: true },
    { cx: 9, cy: 12, r: 1.3, fill: true },
    { cx: 15, cy: 12, r: 1.3, fill: true },
    { cx: 9, cy: 18, r: 1.3, fill: true },
    { cx: 15, cy: 18, r: 1.3, fill: true },
  ],
  layoutVertical: [{ d: frame }, { d: "M8.5 4.5v15M13.5 4.5v15" }],
  layoutClassic: [{ d: frame }, { d: "M8.5 4.5v15M8.5 11h12" }],
  layoutWide: [{ d: frame }, { d: "M3.5 10.5h17M9.5 10.5v9" }],
  layoutFocus: [{ d: frame }, { d: "M10.5 4.5v15" }],
  swap: [{ d: "M4 8h15M15.5 4.5L19 8l-3.5 3.5" }, { d: "M20 16H5M8.5 12.5L5 16l3.5 3.5" }],
  reset: [{ d: "M4.5 12a7.5 7.5 0 1 0 2.2-5.3L4.5 9" }, { d: "M4.5 4.5V9H9" }],
  wifi: [
    { d: "M2.5 9a13.5 13.5 0 0 1 19 0" },
    { d: "M5.5 12.5a9 9 0 0 1 13 0" },
    { d: "M8.7 16a4.5 4.5 0 0 1 6.6 0" },
    { cx: 12, cy: 19.3, r: 1, fill: true },
  ],
  wifiOff: [
    { d: "M5.5 12.5a9 9 0 0 1 4.4-2.4M14.5 10.2a9 9 0 0 1 4 2.3" },
    { d: "M8.7 16a4.5 4.5 0 0 1 6.6 0" },
    { cx: 12, cy: 19.3, r: 1, fill: true },
    { d: "M3.5 3.5l17 17" },
  ],
  zap: [{ d: "M13 2.5L4.5 13.5h7l-1 8 8.5-11h-7z" }],
  activity: [{ d: "M3 12h4l3-7.5 4 15 3-7.5h4" }],
  download: [{ d: "M12 4v11" }, { d: "M7.5 10.5L12 15l4.5-4.5" }, { d: "M4.5 19.5h15" }],
  code: [{ d: "M8.5 7l-5 5 5 5" }, { d: "M15.5 7l5 5-5 5" }],
  headers: [{ d: "M4.5 6.5h15M4.5 12h15M4.5 17.5h9" }],
  printer: [{ d: "M7 9V3.5h10V9" }, { d: "M7 17H4V9h16v8h-3" }, { d: "M7 14h10v6.5H7z" }],
  alert: [{ d: "M12 3.5l9.5 16.5h-19z" }, { d: "M12 10v4.5" }, { cx: 12, cy: 17.3, r: 0.6, fill: true }],
  check: [{ d: "M5 12.5l4.5 4.5L19 7.5" }],
  sidebar: [{ d: frame }, { d: "M9 4.5v15" }],
  book: [{ d: "M5 4.5A1.5 1.5 0 0 1 6.5 3H19v15H6.5A1.5 1.5 0 0 0 5 19.5z" }, { d: "M5 19.5A1.5 1.5 0 0 0 6.5 21H19v-3" }, { d: "M9 8h6" }],
  externalWindow: [{ d: "M13.5 4.5h6v6" }, { d: "M19.5 4.5L11 13" }, { d: "M17.5 14v5.5h-13v-13H10" }],
  mailOpen: [{ d: "M3.5 10l8.5-6 8.5 6v9.5h-17z" }, { d: "M3.5 10l8.5 6 8.5-6" }],
  zoomIn: [{ cx: 11, cy: 11, r: 6.5 }, { d: "M20 20l-4.3-4.3" }, { d: "M11 8.5v5M8.5 11h5" }],
  zoomOut: [{ cx: 11, cy: 11, r: 6.5 }, { d: "M20 20l-4.3-4.3" }, { d: "M8.5 11h5" }],
  text: [{ d: "M5 6.5V5h14v1.5" }, { d: "M12 5v14" }, { d: "M9.5 19h5" }],
  user: [{ cx: 12, cy: 8, r: 3.5 }, { d: "M5 20c0-3.6 3.1-6 7-6s7 2.4 7 6" }],
  shield: [{ d: "M12 3l7.5 3v5.5c0 4.5-3.2 8-7.5 9.5-4.3-1.5-7.5-5-7.5-9.5V6z" }],
  keyboard: [{ d: "M3 6.5h18v11H3z" }, { d: "M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M8 14h8" }],
  palette: [{ d: "M12 3.5a8.5 8.5 0 1 0 0 17c1.2 0 1.7-.8 1.7-1.6 0-1.3-1-1.6-1-2.6 0-.9.7-1.5 1.6-1.5h2a3.7 3.7 0 0 0 3.7-3.7C20 6.9 16.4 3.5 12 3.5z" }, { cx: 8, cy: 11, r: 1, fill: true }, { cx: 11, cy: 7.5, r: 1, fill: true }, { cx: 15.5, cy: 8.5, r: 1, fill: true }],
  bell: [{ d: "M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z" }, { d: "M10 20.5a2 2 0 0 0 4 0" }],
  filter: [{ d: "M4 5h16l-6 7.5V19l-4 1.5v-8z" }],
  minus: [{ d: "M5 12h14" }],
} satisfies Record<string, IconShape[]>;

export type IconName = keyof typeof ICONS;

const ROLE_ICONS: Record<string, IconName> = {
  inbox: "inbox",
  drafts: "draft",
  sent: "send",
  trash: "trash",
  archive: "archive",
  junk: "junk",
};

export function folderIcon(role: string | null | undefined): IconName {
  return (role !== null && role !== undefined && ROLE_ICONS[role]) || "folder";
}
