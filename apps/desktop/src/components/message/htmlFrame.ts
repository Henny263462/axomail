export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 2;
export const ZOOM_STEP = 0.1;

const ZOOM_KEY = "axomail.htmlZoom";

const BASE_STYLE = [
  "html{color-scheme:light;background:#fff}",
  "body{margin:16px;color:#111;background:#fff;font:14px/1.55 system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;overflow-wrap:anywhere}",
  "img,video{max-width:100%;height:auto}",
  "table{max-width:100%}",
  "pre{white-space:pre-wrap}",
  "blockquote{margin:0 0 0 4px;padding-left:12px;border-left:3px solid #ccc;color:#444}",
  "a{color:#111}",
].join("");

export function framedHtml(html: string, zoom: number): string {
  const zoomRule = zoom === 1 ? "" : `html{zoom:${zoom}}`;
  return `<!doctype html><meta charset="utf-8"><style>${BASE_STYLE}${zoomRule}</style>${html}`;
}

export function readZoom(): number {
  try {
    const value = Number(globalThis.localStorage?.getItem(ZOOM_KEY));
    if (Number.isFinite(value) && value >= ZOOM_MIN && value <= ZOOM_MAX) return value;
  } catch {
    return 1;
  }
  return 1;
}

export function writeZoom(value: number): void {
  try {
    globalThis.localStorage?.setItem(ZOOM_KEY, String(value));
  } catch {
    return;
  }
}
