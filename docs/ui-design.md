# RigPilot — UI Design ("Embedded Console")

*Landed 2026-06-15 on `feat/native-ui-experiment`. This is the design language + the
non-obvious implementation decisions behind it. Canonical vocabulary: [terminology.md](terminology.md);
layout/feature scope: [plan.md](plan.md) §2.*

The band's M3 feedback was blunt: the UI "reads as the 91239874612334th bootstrap.css app —
you can smell the browser on it". The redesign answers that with a dark **instrument console**
look that does not feel like a web page or like Tauri/Electron.

## 1. Design thesis

- **Identity:** near-black industrial console, hot-pink accent `#ff2e88`, white text. UI font
  **Archivo** (variable), all numeric/positional values in **IBM Plex Mono**.
- **Abstract depth, NOT skeuomorphism.** No imitation of real materials (brushed metal, chrome,
  glossy plastic, leather, grain). Depth is conveyed the way modern macOS does it: **one light
  source from the top**, whisper-subtle tonal gradients, a 1px **specular rim** on the top edge
  (carries most of the depth), **shadows only downward**, consistent rounded radii, restrained
  colour. Glass (backdrop-blur) is allowed — it is an *abstract* material, used only for holds
  and floating overlays (menus, dropdowns).
- **Controls embedded, not "chips".** Buttons are carved **cells** in a surface (transport bar,
  toolbars) separated by hairlines — not floating bordered web-buttons. Toggles light up with a
  subtle **LED** (thin accent underline + faint bloom); a weakened version of that bloom is the
  signature hover.
- **Restraint:** colour = action/type/selection only; neutral greys carry structure. The "clean"
  comes from typographic rigor and a consistent spacing/stroke scale, not from effects.

## 2. Design system (`app/src/lib/theme.css`)

`theme.css` is the single source of truth — tokens + base element styling + shared utility
classes, all global so every component draws from them.

- **Tokens:** background ramp `--bg0…3`, `--line`/`--line-strong`, `--fg`/`--fg-dim`/`--fg-faint`,
  `--accent`/`--accent-down`/`--accent-soft`, `--green`, `--warn`; radii `--radius` (8) /
  `--radius-sm` (5) / `--radius-lg` (11).
- **Depth tokens:** `--rim` (specular top edge), `--well-in` (recessed inset), `--lift` (downward
  contact shadow), `--float` (overlay shadow), `--raise`/`--raise-hi` (tonal button gradients),
  `--bar-bg` (lighter unified bar).
- **Interaction grammar tokens:** `--hover-glow` (pink LED warm-up), `--led` (toggle on),
  `--clip-depth` (subtle 3D for clips).
- **Utility classes:** `.faceplate` (raised matte panel), `.well` (recessed display), `.cell` +
  `.cellgroup` (carved control cells), `.seg` (segmented control), `.tdot` (`.diamond`/`.square`/
  `.circle` type markers), `.glass` (blurred overlay).
- **Stroke scale:** 1px structural borders · 1.5px data strokes (automation curve + nodes) ·
  2px handles/markers.

## 3. Layout

```
┌ topbar: ☰ · song · ▶⏹ · LCD(POS/BPM/SIG) · SNAP▾ · Bars|Time · Color · Export · ◧◨ · win ┐
├──────────┬───────────────────────────────────────────────┬───────────┤
│ Command  │ Tracks (headers + lanes)                       │ Inspector │
│ Palette  │  header: ▍colour · name · ch/device · M/S|⚙    │ (faceplate│
│ (MIDI    │  lanes: clips on anonymous lanes               │  + wells) │
│  track)  │  ＋Audio ＋MIDI ghost row under last track      │           │
├──────────┴───────────────────────────────────────────────┴───────────┤
│ status line                                                           │
└───────────────────────────────────────────────────────────────────────┘
```

- **Unified topbar** (one ~50px bar, lighter than the rest so it lifts off): traffic-less custom
  titlebar with brand + ☰ file menu + editable song name; centered transport (play/stop cells +
  a recessed **LCD** showing position and editable BPM / time-signature); right side has the
  **split snap control** (SNAP toggle + ▾ that opens the grid-size menu), Bars/Time, Color, Export,
  and the **panel toggles** ◧ (Palette) / ◨ (Inspector).
- **Palette left, Inspector right** (both collapsible via the topbar toggles). The Command Palette
  is a grouped vertical list with per-type dots; it only populates for a selected MIDI track.
- **Track headers:** a slim colour bar (per-track), name + `ch · device`, **M/S only on audio
  tracks** (mute = red, solo = yellow); MIDI tracks show the device-settings gear. `＋ Audio` /
  `＋ MIDI` are ghost buttons in a row beneath the last track.

## 4. Clips (commands on the timeline)

Colour per Command Type: **One-Shot = warn `#ffb02e`, Hold = accent `#ff2e88`, Automation =
green `#2ee08a`** (`COMMAND_TYPE_COLORS` in `types.ts`).

- **One-Shot:** a marker — vertical stem across the lane with a diamond; a flag (icon + name) on
  tall lanes.
- **Hold:** a liquid-glass block (backdrop-blur + subtle gradient + rim).
- **Automation:** built on the Hold block — a green glass block with the breakpoint **graph** drawn
  inside; the area under the curve is a **transparent black** fill (Grafana-style, not a loud
  colour wash). Nodes are round dots.
- **Progressive disclosure by clip height:** at/above ~50px a clip gets a **title bar** (type dot +
  label); below that it falls back to a compact inline label. `LANE_H` is currently fixed at 72px
  so clips render in the tall, title-bar style. Selection = bright ring; resize handles appear on
  selected hold/automation clips, inside the clip body.

## 5. Key implementation decisions

1. **DOM clips over a canvas waveform** (`Timeline.svelte`). The CSS clip designs (glass,
   gradients, title bars, rings, area charts) are not achievable on `<canvas>` (no backdrop-blur,
   no cheap rounded gradients). So the canvas now draws **only** the waveform, grid, ruler and
   selection marquee; **events render as a DOM overlay** positioned with the same `eventRect()`
   math. The overlay is `pointer-events:none`, so **all interactions stay on the canvas** —
   hit-testing, drag/move/resize, marquee, breakpoint editing and palette-drop are unchanged
   (verified: `elementFromPoint` over a clip returns the canvas). This revises the original
   plan.md §4 "single canvas for events" decision; the canvas-for-performance rationale still
   holds for the waveform, but event counts per song are low enough for DOM. The DOM playhead sits
   above the clip layer (`z-index`).
2. **Browser dev-guard** (`app/src/lib/devmock.ts`). The app normally requires the Tauri backend
   (`invoke`, `getCurrentWindow`, dialogs). `IS_TAURI` detects a plain browser and feeds mock
   definitions + a demo project, and guards the Tauri-only calls (Open/Save/Import/Export degrade
   to a status message). This lets the UI be developed and screenshotted with `vite dev` at `/`
   without building the native shell — a real DX win, and the only way to get a fast visual
   verification loop.
3. **Floating menus must live outside `overflow:hidden`.** `.cellgroup` clips its content to round
   the carved cells, so a dropdown rendered *inside* a cellgroup is silently clipped (it opens in
   the DOM but is invisible). Dropdowns (snap grid menu) are therefore siblings of the cellgroup,
   inside a `position:relative` wrapper. Menus close via a window `pointerdown` handler that checks
   `event.target.closest()` rather than per-element `stopPropagation` (WebKitGTK swallows the
   follow-up click when `stopPropagation` is called on `pointerdown`).
4. **`Num` is a drag-scrub field.** All numeric inputs (transport BPM/SIG, every Inspector value)
   use one `Num` component: vertical drag scrubs the value; a plain click focuses it for keyboard
   entry. A `flat` variant embeds it into the LCD (green mono, no chrome).
5. **Automation geometry reserves the title bar.** When a clip is tall it has a title bar, so the
   curve/breakpoint vertical mapping (`autoBounds`) offsets by the title height — keeping canvas
   hit-testing aligned with the DOM-drawn graph.

## 6. Prototypes (removed)

The look was developed in two scoped sandbox routes — `/sandbox` (component internals) and
`/shell` (full layout) — kept as the design source of truth during the migration and **deleted**
once everything was ported into the real components.
