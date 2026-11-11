# Jyroxa brand

Jyroxa (pronounced *jy-ROK-suh*, from "gyro") uses a flat, two-color planet-and-orbit mark: a teal ring with a solid core, crossed by a tilted coral orbit that passes behind the ring at the top and in front of it at the bottom. It suggests spin, momentum and many parallel streams moving around one core. The mark uses **no gradients**.

## Colors

| Role | Hex |
|------|-----|
| Teal (primary, on light) | `#0F766E` |
| Teal light (primary, on dark) | `#2DD4BF` |
| Coral (orbit / accent) | `#FF5A3C` |
| Navy (dark surfaces, wordmark text) | `#0B1020` |
| White | `#FFFFFF` |

## Type

The wordmark "Jyroxa" is set in **Outfit Bold** (SIL Open Font License 1.1) and converted to outlines, so no font is needed to render it. Text color is solid `#0B1020` on light and `#FFFFFF` on dark.

## Files

| File | Use |
|------|-----|
| `jyroxa-icon-light.svg` | Primary app icon: white rounded tile (Windows .ico, installer, Android legacy icons) |
| `jyroxa-icon-dark.svg` | Navy tile with light teal, for dark contexts |
| `jyroxa-mark.svg` | Transparent mark for light surfaces (the gaps are real cut-outs) |
| `jyroxa-mark-on-dark.svg` | Transparent mark for dark surfaces |
| `jyroxa-mark-mono.svg` | Single-color white mark |
| `jyroxa-tray.svg` | Heavier-stroke tile for 16 to 24 px (tray, small .ico frames) |
| `jyroxa-android-foreground.svg` / `jyroxa-android-monochrome.svg` | 108 dp adaptive-icon layers, mark inside the 66 dp safe zone |
| `jyroxa-wordmark*.svg` | Mark plus wordmark (`-dark` = for dark backgrounds) |
| `png/` | Rendered PNGs (16 to 1024 px), `jyroxa.ico` (16/24/32/48/64/128/256) and `jyroxa-tray.ico` |
| `preview.png` | Overview sheet |

## Regenerating

Run `node generate-brand.js <output dir>` after installing `@resvg/resvg-js` and `opentype.js` in a scratch folder, with the Outfit Bold TTF available (the font path is set near the top of the script).

## Usage rules

- Keep the two colors flat. Don't add gradients, shadows or glows to the mark.
- Keep clear space of at least 1/4 of the mark width around it.
- Below 24 px, use the tray variant.
- Don't recolor the orbit and ring to the same color, except in the mono variant.
