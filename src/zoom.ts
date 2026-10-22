// Interface size: the whole webview is zoomed, and the reader divides its own sizes by the
// zoom so the text size set under "Reading" stays what it says.

import { getCurrentWebview } from "@tauri-apps/api/webview";

const baseDpr = window.devicePixelRatio || 1;
let scale = 1;

export const uiScale = () => scale;

export function applyUiScale(value: number) {
  scale = value;
  getCurrentWebview()
    .setZoom(value)
    .catch(() => {
      // Plain browser (development): CSS zoom gives the same layout.
      document.documentElement.style.setProperty("zoom", String(value));
    });
}

/** Device pixels per CSS pixel, including the interface zoom (for sharp page images). */
export function pixelRatio(): number {
  return Math.max(window.devicePixelRatio || 1, baseDpr * scale);
}
