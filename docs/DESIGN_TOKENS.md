# Jyroxa desktop design

Author: Prateek Kumar

The approved desktop composition is a link field and Download action, a command
bar, a counted filter rail, a list of downloads, and a status bar. Technical
metadata stays behind Show details. Square surfaces, flat fills and one-pixel
borders apply to dialogs, drawers, menus and notifications as well as the list.

## Color tokens

The authoritative definitions are in `apps/desktop/src/styles.css`, with control
boundaries and layout in `apps/desktop/src/redesign.css`.

| Purpose | Light | Dark |
| --- | --- | --- |
| App background | `#f3f3f3` | `#303234` |
| Surface | `#ffffff` | `#383a3c` |
| Inner surface | `#f7f7f7` | `#343638` |
| Hover | `#eaeaea` | `#46484a` |
| Primary text | `#1a1a1a` | `#f3f3f3` |
| Secondary text | `#5c5c5c` | `#cccccc` |
| Muted text | `#62676a` | `#b8bdc0` |
| Teal accent | `#0f766e` | `#2dd4bf` |
| Accent hover | `#115e59` | `#14b8a6` |
| Primary action text | `#ffffff` | `#1f1f1f` |
| Dividers | `#b7babc` | `#64686b` |
| Control boundaries | `#858b8e` | `#8c9296` |
| Completed text | `#15803d` | `#4ade80` |
| Error text | `#b91c1c` | `#f87171` |

Teal is the sole interactive accent. Status colors communicate success, caution
and errors; text labels accompany them. Main row file icons are neutral. The
automated contrast check reads the actual CSS tokens and checks primary,
secondary, muted and accent text against the app, surface, inner and hover
backgrounds, plus primary action text. Text must reach 4.5:1. Focus uses a
two-pixel teal outline; input and button boundaries must reach 3:1. Decorative
dividers use a quieter contrast. Disabled controls retain native disabled behavior.

## Type, space and motion

- Segoe UI Variable Text, then Segoe UI and system fallbacks.
- Main download names: 17px, weight 600; summaries: 14px; commands: 15px.
- Spacing follows 4px increments. Link controls are 48px high, navigation rows
  50px, row action buttons 42 by 38px. Download progress is 3px high.
- All surface and control corner radii are zero. No gradients, glows or elevated
  shadows. Speed history uses a flat translucent fill.
- Keyboard focus remains visible. Reduced-motion preferences disable transitions.
- Minimum window size remains 900 by 560. Layout screenshots cover 900, 1024,
  1280 and 1920 pixel widths in both themes.

New installations start in light mode. Explicit light, dark and system
preferences remain supported. The theme button persists the selected preference.

## Navigation and behavior

All, Downloading, Completed, Videos, Torrents, Scheduled and Failed are mutually
exclusive primary views with counts. Scheduled represents queued downloads;
the existing scheduler controls when they run. More file types exposes the
existing music, document, archive, program and other category views. Search and
sort remain in the title strip. Selections work through row clicks, checkboxes,
Shift selection and Ctrl+A. Space pauses running selections or resumes paused
ones. Delete opens the existing removal confirmation. Shortcuts leave form
controls, modal dialogs and menu keyboard handling intact.

HTTP links, magnets and full torrent paths enter the existing review dialog.
Torrent selection and native path drops use the same backend commands as before.
Video links also use the existing add flow; multiple links retain LinkGrabber.
Checksums, connection limits, source metadata, peer statistics, segment progress
and cancellation remain available in details or advanced options.

## Icons and logo

UI action icons use Microsoft's Fluent System Icons regular 20px catalog,
embedded locally without a runtime dependency. Their license is in
`docs/FLUENT-ICONS-LICENSE.txt`. Rebuild the embedded set with
`npm run brand:design` from `apps/desktop`.

The original logo is a square J-shaped receiving path and a down arrow, drawn on
a 19-unit grid. It uses a single flat teal fill and remains legible at 16px. SVG
sources, light and dark PNGs from 16 through 1024px, and a seven-resolution ICO
are in `assets/brand/v3`. The tray mark has a flat light square behind it so it
can be seen on either taskbar theme. Desktop window, Windows package, installer,
tray and browser extension icons share the mark. Previous replaced assets are
preserved under `assets/brand/v3/previous`; the older brand directory is retained.

Run `npm test`, `npm run lint`, `npm run build`, and `npm run test:design` to
verify the frontend. Screenshots exercise the built frontend with deterministic
Tauri command fixtures. They do not claim to capture a running native window or
the Windows-owned tray menu. Native compilation and Rust tests are separate.
