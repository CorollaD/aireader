// macOS windows have no title bar of their own (see tauri.macos.conf.json): the top bars take its
// place, leave room for the window buttons and drag the window. Other systems keep theirs.

import { getCurrentWindow } from "@tauri-apps/api/window";

const integrated = navigator.platform.toLowerCase().includes("mac") && "__TAURI_INTERNALS__" in window;

/** Spread on a top bar. Buttons and fields inside it keep working. */
export const dragRegion = integrated ? { "data-tauri-drag-region": "deep" } : {};

/** Marks the page for the styles above and follows full screen, where the window buttons go away. */
export function watchTitleBar(): () => void {
  if (!integrated) return () => {};
  const root = document.documentElement;
  const win = getCurrentWindow();
  root.classList.add("mac-titlebar");
  const sync = () => {
    win
      .isFullscreen()
      .then((on) => root.classList.toggle("fullscreen", on))
      .catch(() => {});
  };
  sync();
  const stop = win.onResized(sync);
  return () => {
    stop.then((f) => f()).catch(() => {});
  };
}
