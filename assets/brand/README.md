# Jyroxa brand (warm v3)

The v3 mark is a continuous square path forming a **J** whose open top reads as a download arrow. It uses flat fills only (no gradients).

## Colors

| Role | Hex |
|------|-----|
| Mark on light surfaces | `#7A5C2E` |
| Mark on dark surfaces | `#C9A46A` |
| App tile (light) | `#F4F0EB` |
| App tile (dark) | `#14100D` |
| Wordmark on light | `#3D2F1A` |
| Wordmark on dark | `#F3EADF` |

## Type

The wordmark in `jyroxa-wordmark*.svg` uses **Segoe UI / system-ui** at generation time for PNG exports.

## Files

| File | Use |
|------|-----|
| `jyroxa-icon-light.svg` | Primary app tile for README and light contexts |
| `jyroxa-icon-dark.svg` | Navy-warm tile for dark contexts |
| `jyroxa-mark.svg` / `jyroxa-mark-on-dark.svg` | Transparent marks |
| `jyroxa-tray.svg` | Taskbar and small sizes |
| `jyroxa-android-foreground.svg` / `jyroxa-android-monochrome.svg` | Adaptive icon layers |
| `jyroxa-wordmark*.svg` | Mark plus wordmark |
| `png/` | Rendered PNGs, `jyroxa.ico`, `jyroxa-tray.ico` |
| `preview.png` | Overview sheet |
| `v3/` | Generator output and archived previous bitmaps |

## Regenerating

From `apps/desktop`:

```bash
npm run brand:design
```

This runs `scripts/generate-design-brand.mjs`, which writes `assets/brand/v3/`, refreshes `assets/brand/png/`, `preview.png`, desktop/Tauri/browser icons, and Android mipmaps.

## Usage rules

- Keep fills flat; do not add gradients or glows to the mark.
- Below 24 px, prefer the tray tile (`jyroxa-tray.svg`).
- Keep clear space of at least 1/4 of the mark width around it.