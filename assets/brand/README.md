# Jyroxa Brand Identity System

> **Pronunciation:** *jy-ROK-suh* (derived from *gyro* / gyroscope)  
> **Product:** Next-Generation High-Performance Download Manager  
> **Repository:** [SwiftGet / Jyroxa](file:///E:/SwiftGet)  
> **Assets Directory:** `assets/brand/`

---

## 1. Brand Concept & Rationale

**Jyroxa** is engineered for uncompromising speed, multi-stream stability, and precision download acceleration. The brand identity reflects this high-velocity architecture:

- **The Gyroscope Gimbals:** Two interlocking elliptical orbits tilted at dynamic $\pm34^\circ$ angles embody rotational equilibrium, 3D stabilization, and concurrent multi-segment stream orchestration.
- **The Supersonic Velocity Chevron:** Integrated seamlessly into the forward trajectory of the primary orbital ring, an aerodynamic delta chevron indicates perpetual acceleration, forward throughput, and high-speed data capture.
- **The Core Rotor:** The central solid pivot represents the core download engine nucleus anchoring all concurrent threads.
- **Originality & Independence:** Purposefully designed without generic down-arrow-with-bar or globe tropes. It is completely distinct from legacy utilities such as IDM, JDownloader, ADM, 1DM, or Ghost Downloader.
- **Extreme Scalability:** Bold geometric strokes (~40px stroke width at 512px canvas) and generous negative space ensure crisp legibility from 1024px hero displays down to 16px Windows system tray and favicon sizes.

---

## 2. Color Palette

The Jyroxa identity uses an electric spectral gradient on a deep cosmic navy ground, combining high-contrast energy with dark-mode aesthetic sophistication.

| Role | Color Name | Hex Code | RGB | Usage |
| :--- | :--- | :--- | :--- | :--- |
| **Gradient Start** | Electric Violet | `#7C3AED` | `rgb(124, 58, 237)` | Orbit trailing arc, brand primary |
| **Gradient Mid** | Hyper Blue | `#2563EB` | `rgb(37, 99, 235)` | Mid-flow stream transition |
| **Gradient End** | Electric Cyan | `#06B6D4` | `rgb(6, 182, 212)` | Forward arrowhead, core velocity accent |
| **Dark Ground** | Deep Cosmic Navy | `#0B1020` | `rgb(11, 16, 32)` | App icon squircle, dark theme background |
| **Dark Container**| Slate Space Navy | `#0F172A` | `rgb(15, 23, 42)` | Elevated surfaces, cards, icon sub-frames |
| **Light Ground** | Frost White | `#F8FAFC` | `rgb(248, 250, 252)` | Light theme background |
| **Text (Dark)** | Pure White | `#FFFFFF` | `rgb(255, 255, 255)` | Wordmark on dark backgrounds, mono icon |
| **Text (Light)**| Deep Cosmic Navy | `#0B1020` | `rgb(11, 16, 32)` | Wordmark on light backgrounds |
| **Muted Text** | Slate Blue | `#94A3B8` | `rgb(148, 163, 184)` | Captions, subtitles, secondary metadata |

### Linear Gradient Definition
```xml
<linearGradient id="jyroxaGradient" x1="12%" y1="12%" x2="88%" y2="88%">
  <stop offset="0%" stop-color="#7C3AED"/>
  <stop offset="50%" stop-color="#2563EB"/>
  <stop offset="100%" stop-color="#06B6D4"/>
</linearGradient>
```

---

## 3. Typography & Wordmark

- **Font Family:** **Outfit** (Bold, weight 700)
- **Designer:** Rodrigo Fuenzalida / On Brand Investment
- **License:** **SIL Open Font License 1.1** (OFL) — Free for commercial and open-source software
- **Geometric Harmony:** Outfit's circular vowels (`o`, `a`) harmonize with the gyroscope gimbals and core rotor, while the angled cuts of `y` and `x` echo the $\pm34^\circ$ gimbal tilt angles.
- **Vector Paths:** All SVG wordmark files contain pre-rendered geometric vector `<path>` elements generated via `opentype.js`. They render identically across all platforms without requiring client-side font installations.

---

## 4. File Manifest

### Vector Source Files (`assets/brand/`)

| File Name | Format | Dimensions / ViewBox | Description & Target Purpose |
| :--- | :--- | :--- | :--- |
| [`jyroxa-mark.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-mark.svg) | SVG | `0 0 512 512` | Primary logo mark with `#0B1020` rounded squircle background (`rx="112"`). Desktop & mobile app launcher icon. |
| [`jyroxa-mark-transparent.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-mark-transparent.svg) | SVG | `0 0 512 512` | Logo mark with transparent background and true negative space notch behind the chevron. For custom composite badges & headers. |
| [`jyroxa-android-foreground.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-android-foreground.svg) | SVG | `0 0 108 108` | Android Adaptive Icon foreground asset. Mark is scaled and centered strictly within the central 66% safe zone (diameter $\le 72$dp). |
| [`jyroxa-mark-mono.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-mark-mono.svg) | SVG | `0 0 512 512` | Monochrome variant (`currentColor` / `#FFFFFF` on transparent). Designed for Android 13+ Material You themed icons and monochrome badges. |
| [`jyroxa-tray.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-tray.svg) | SVG | `0 0 32 32` | Windows / macOS / Linux system tray icon. High-contrast stroke geometry tuned for 16–32px taskbar visibility. |
| [`jyroxa-wordmark.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-wordmark.svg) | SVG | `0 0 485 160` | Full horizontal lockup on `#0B1020` container card. App about dialogue, GitHub README header, splash screen. |
| [`jyroxa-wordmark-light.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-wordmark-light.svg) | SVG | `0 0 485 136` | Transparent lockup with white wordmark for dark mode web pages and UI navigation bars. |
| [`jyroxa-wordmark-dark.svg`](file:///E:/SwiftGet/assets/brand/jyroxa-wordmark-dark.svg) | SVG | `0 0 485 136` | Transparent lockup with `#0B1020` dark wordmark for light mode documentation and paper collateral. |
| [`preview.png`](file:///E:/SwiftGet/assets/brand/preview.png) | PNG | `1200 × 800` | Comprehensive brand specification sheet displaying mark on dark/light grounds, mono mode, small scale renders (16/32/48px), wordmark lockups, and color palette. |

### Rendered PNG Previews (`assets/brand/png/`)

| File Name | Resolution | File Size | Recommended Target |
| :--- | :--- | :--- | :--- |
| [`jyroxa-mark-1024.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-1024.png) | 1024 × 1024 | ~152 KB | App Store, Google Play Store, Windows Store high-res hero icon |
| [`jyroxa-mark-512.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-512.png) | 512 × 512 | ~60 KB | Standard HD app icon, installer header |
| [`jyroxa-mark-256.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-256.png) | 256 × 256 | ~23 KB | OS file associations, desktop launcher shortcut |
| [`jyroxa-mark-128.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-128.png) | 128 × 128 | ~9 KB | Medium task switcher icon, settings panels |
| [`jyroxa-mark-64.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-64.png) | 64 × 64 | ~3.7 KB | Notification banners, compact list tiles |
| [`jyroxa-mark-32.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-32.png) | 32 × 32 | ~1.6 KB | Taskbar active icon, window corner titlebar |
| [`jyroxa-mark-16.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-mark-16.png) | 16 × 16 | ~664 B | Browser favicon, compact system tray |
| [`jyroxa-wordmark.png`](file:///E:/SwiftGet/assets/brand/png/jyroxa-wordmark.png) | 960 × 316 | ~31 KB | Social preview card, README hero banner |

---

## 5. Brand Usage Rules

### Clearspace
Always maintain clear space around the mark equal to at least **25%** of the mark's height ($0.25H$) on all four sides. Do not intrude text, borders, or foreign graphics into this perimeter.

### Minimum Sizes
- **Vector Logo Mark:** Do not render below `16 × 16` pixels. For 16–32px contexts without dark rounded backgrounds, utilize `jyroxa-tray.svg` or `jyroxa-mark-mono.svg`.
- **Wordmark Lockup:** Do not render below `120px` in total width to preserve legibility of the typography.

### Permitted Treatments
- **Primary Mark on Dark:** Use `jyroxa-mark.svg` across all dark backgrounds (`#000000` through `#1E293B`).
- **Primary Mark on Light:** Use `jyroxa-mark-transparent.svg` on light backgrounds (`#FFFFFF` or `#F8FAFC`).
- **Monochrome & Tray:** Use `jyroxa-mark-mono.svg` or `jyroxa-tray.svg` when system guidelines mandate single-color icons (e.g. Windows notification area, Android status bar).

### Prohibited Modifications
- ❌ **Do not rotate** the logo mark away from its calibrated orbital balance ($\pm34^\circ$).
- ❌ **Do not distort, skew, or disproportionately stretch** the aspect ratio (always lock 1:1).
- ❌ **Do not alter or substitute** the electric gradient stops with unapproved color combinations.
- ❌ **Do not reintroduce** generic downward download arrows or bars onto the mark.
- ❌ **Do not add heavy raster drop shadows**, bevels, or uncalibrated outer glows that impair subpixel rendering.
