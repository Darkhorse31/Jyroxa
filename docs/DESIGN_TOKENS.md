# Jyroxa desktop design

Author: Prateek Kumar

The approved desktop composition is a link field and Download action, a command
bar, a counted filter rail, a list of downloads, and a status bar. Technical
metadata stays behind Show details. Surfaces use a subtle 4px corner radius on
controls and cards; dialogs use 7px. Flat fills and one-pixel warm borders
apply throughout.

## Color tokens

The authoritative definitions are in `apps/desktop/src/styles.css`, with control
boundaries and layout in `apps/desktop/src/redesign.css`.

| Purpose | Light | Dark |
| --- | --- | --- |
| App background | `#f4f0eb` | `#14100d` |
| Surface / panel | `#ffffff` | `#1c1712` |
| Inner surface | `#f7f4ef` | `#211c16` |
| Hover | `#e8e2da` | `#2a241d` |
| Primary text | `#1c1712` | `#f3eadf` |
| Secondary text | `#5c5349` | `#b9ac9c` |
| Muted text | `#6f655a` | `#a89b8b` |
| Gold accent (links, progress, nav) | `#7a5c2e` | `#c9a46a` |
| Accent hover | `#684e26` | `#b89255` |
| Primary action (light fill) | `#7a5c2e` / `#faf6f0` text | cream `#f1e6d6` / `#14100d` text |
| Dividers | `#e6e1da` | `#2e2620` (hover `#3a3028`) |
| Focus ring | `#c9a46a` | `#c9a46a` |
| Progress fill | `#c9a46a` | `#c9a46a` |
| Completed text | `#15803d` | `#4ade80` |
| Error text | `#b91c1c` | `#f87171` |

Muted gold is the sole interactive accent. Status colors communicate success,
caution and errors; text labels accompany them. The automated contrast check
reads the actual CSS tokens and checks primary, secondary, muted and accent
text against the app, surface, inner and hover backgrounds, plus primary action
text. Text must reach 4.5:1. Focus uses a two-pixel gold outline.

## Type, space and motion

- Segoe UI Variable Text, then Segoe UI and system fallbacks.
- Main download names: 17px, weight 600; summaries: 14px; commands: 15px.
- Spacing follows 4px increments. Link controls are 48px high, navigation rows
  50px, row action buttons 42 by 38px. Download progress is 3px high.
- Corner radii: 4px on buttons, inputs, rows and cards; 7px on dialogs. No
  gradients, glows or elevated shadows.
- Keyboard focus remains visible. Reduced-motion preferences disable transitions.
- Minimum window size remains 900 by 560.

New installations start in light mode. Explicit light, dark and system
preferences remain supported.

## Icons and logo

UI action icons use Microsoft's Fluent System Icons regular 20px catalog,
embedded locally without a runtime dependency. The flat mark uses warm gold
fills (`#7a5c2e` light, `#c9a46a` dark). Regenerate rasters with
`npm run brand:design` from `apps/desktop`.

Run `npm test`, `npm run lint`, `npm run build`, and `npm run test:design` to
verify the frontend.