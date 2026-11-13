# Design v3 — light theme fix

## Fix (2026-11-13)

Light theme surfaces were not reliably applied because color tokens lived only on `[data-theme="light"]` without a `:root` default, and WebView could treat the page as dark while CSS still intended light (missing `color-scheme`).

**Changes:** Default Revision 2 light tokens on `:root` (`#fbfbfb` cards, `#e5e5e5` borders, `#1a1a1a` / `#5c5c5c` text); `color-scheme: light|dark` on theme selectors; sync `data-theme` on `html` + `body`; early inline theme bootstrap in `index.html`; settings theme preference applied on load/save.

**Verification:** Headless Playwright screenshots (`apps/desktop/scripts/design-v3-screenshots.mjs`) — light main/add/linkgrabber/settings show white cards and readable header secondary buttons. `npm run build` + contract check pass.
