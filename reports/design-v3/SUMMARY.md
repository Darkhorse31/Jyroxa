# Jyroxa Desktop UI Redesign (v3 - Revision 2) Summary

## Overview
The desktop UI has been redesigned in a modern, perfectly symmetric, native Windows 11 Fluent aesthetic inspired by Ghost Downloader 3 and native Windows applications. Following user Revision 2 overrides, the interface avoids AI-template tells, removes all rounded corners and glowing elements, sets a calm light theme by default with crisp 1px hairline borders, and provides a soft neutral charcoal dark theme.

## Core Design Principles & Changes

### 1. Zero Rounded Corners & Hairline Borders
- Eliminated all pill shapes and rounded corners across cards, buttons, badges, toolbars, dialogs, drawers, and progress bars (`border-radius: 0`).
- Applied a minimal 2px radius strictly to form inputs, checkboxes, and select elements.
- Replaced card box-shadows and glowing accents with crisp 1px hairline borders (`#e0e0e0` / `#e5e5e5` in light mode; `#383838` in dark mode).

### 2. Light & Calm Neutral Themes
- **Light Theme (Default)**: Clean, calm off-white / neutral grey Mica-style surfaces (`#f3f3f3` app background, `#ffffff` surface/cards, `#f7f7f7` inner components).
- **Dark Theme**: Soft neutral charcoal (`#202020` app background, `#282828` surfaces/cards, `#242424` inner components), completely avoiding heavy near-black backgrounds.
- **Brand Colors**: Preserved flat Teal (`#0F766E` in light mode, `#2DD4BF` in dark mode) and flat Coral (`#FF5A3C`), strictly without gradients.

### 3. Dense, Native Windows Task List
- Dense tabular card rows with 8 px grid alignment and balanced horizontal padding.
- Thin rectangular progress lines (3 px height) running with flat solid teal fills.
- Replaced pill badges with compact rectangular uppercase status and category tags.
- Tabular figures (`tabular-nums`) for file sizes, transfer speeds, ETAs, and elapsed time.
- Standardized action buttons (28 px square with crisp 1px borders).

### 4. Compact Toolbar & Symmetrical Header
- Symmetrical 48 px header with vertically centered brand lockup, search box, and global statistics.
- Equal 32 px height across toolbar buttons, search box, and sort controls.
- Clean rectangular metric blocks with flat square indicators (removing glowing/pulsing animations).

### 5. Slim Left Navigation with Native Icons
- Slim 192 px sidebar navigation with 8 px grid alignment.
- Integrated crisp SVG icons for all status filters and category filters.
- Subtle left accent indicator (3 px solid teal) and tinted background for the active filter.
- Dense rectangular monospace count badges.

### 6. Responsive Adaptations
- **1024 px**: Slim sidebar (176 px), streamlined header hiding secondary button labels and speed graph to guarantee no text or button overlap.
- **1280 px**: Standard desktop resolution with full metrics, balanced sidebar, and complete card layout.
- **1920 px**: Widescreen layout with centered container bounds preventing awkward stretch.

## Visual Verification
Headless Playwright screenshots captured at 1280 px (and responsive viewports) in `reports/design-v3/`:
- `after-dark-main.png` / `after-light-main.png`
- `after-dark-add.png` / `after-light-add.png`
- `after-dark-linkgrabber.png` / `after-light-linkgrabber.png`
- `after-dark-settings.png` / `after-light-settings.png`
- `responsive-1024.png`
- `responsive-1920.png`

## Verification & Contract Check
- `node apps/desktop/scripts/check-contract.mjs`: Contract OK (20 commands, 19 invoke sites, 12 TS files).
- `npm run build`: Typecheck and Vite production build succeeded with 0 errors.

## Light Theme Contrast & Surface Fix (2026-11-13)
- Fixed light theme surfaces to ensure task cards/rows render with clean white/neutral surfaces (`#fbfbfb`/`#ffffff`), crisp 1px hairline borders (`#e5e5e5`), and high-contrast dark text (`#1a1a1a`, secondary `#5c5c5c`).
- Fixed header secondary buttons (`#btn-grabber`, `#btn-settings`) and icon buttons to render light surfaces with dark text and crisp `#e5e5e5` borders in light theme.
- Defined all default light theme variables directly on `:root` alongside explicit `color-scheme: light` (and `:root[data-theme="dark"]` with `color-scheme: dark`) so styles resolve consistently across all platforms and webview instances.
- Synced `data-theme` attribute across both `<html>` and `<body>` with early inline head bootstrap and reactive settings persistence.
- Re-captured all headless screenshots across main, add, linkgrabber, settings, and responsive viewports with verified readable text and high contrast.
