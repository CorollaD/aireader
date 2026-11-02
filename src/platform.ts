// Phones and tablets get the touch layout (styles under `html.mobile`). Adding `?mobile` to the
// address forces it, to work on that layout in a desktop browser.

export const mobile =
  /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) ||
  // iPadOS reports itself as a Mac; touch gives it away.
  (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1) ||
  new URLSearchParams(location.search).has("mobile");
