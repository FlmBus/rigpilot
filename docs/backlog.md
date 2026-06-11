# RigPilot — Backlog (band feedback, 2026-06-10)

*All items below implemented 2026-06-11 — pending visual/hands-on check by the band.*

Collected after the first M2 hands-on session. Items map mostly onto M3.

## Design / Theme
- [x] **Desktop-app look, not web.** Current UI reads as "the 91239874612334th
  bootstrap.css app". Goals: compact desktop density, no web-card aesthetic,
  custom-styled form controls (current ones are fine functionally, but "you can smell
  the browser on them a mile away"). Explicitly NOT wanted: more effects/bells/whistles.
- [x] **Band colors:** pink-magenta + white, high-contrast design, dark base.

## Editor (M3 scope, confirmed + sharpened by feedback)
- [x] **Selection model:** click selects an event; Ctrl (toggle) and Shift (range/add)
  modifiers are a must. Selection applies to *anything* — events, audio tracks, tracks.
- [x] **Keybinds on selection:** Delete, Copy/Paste (Ctrl+C/V), etc.
- [x] **Inspector sidebar:** shows the options of whatever is selected (event params,
  audio track settings, …). One sidebar for all selection types.
- [x] **Drag interactions:** move events within a lane and across lanes; drag block
  edges to resize Hold/Automation lengths.
- [x] **Snap grid:** user-selectable subdivision, at least down to 1/16
  (replace the current zoom-adaptive guess with an explicit selector).

## UI structure
- [x] **Track settings dialog:** Device Definition + MIDI Channel move out of the
  per-track section into their own window/dialog. Trigger: cogwheel icon button in the
  track header (replacing the "ch N" pill next to M/S).
- [x] **Export settings dialog:** "Reset Block" checkbox moves out of the toolbar into
  an export-options popup that appears after clicking "Export MIDI…".

## Bugs / UX fixes
- [x] **"+ Audio Track" opens the save-project dialog** instead of the audio file
  picker (happens when the project is unsaved — intended flow, but reads as a bug).
  Fix: pick the audio file *first*, then explain + prompt for project save if needed.
- [x] **"New" discards without confirmation.** Either confirm when there are unsaved
  changes, or go multi-project with tabs. Decision pending — confirmation is the cheap
  v1, tabs are a candidate for later.

---

# Feedback round 2 (2026-06-11, after M3 hands-on)

- [x] Number inputs: hide default spinner buttons.
- [x] Bars/Time group: proper segmented control (shared border, filled active state).
- [x] Command palette: broken/overlapping layout → fixed 4-row column grid.
- [x] Audio volume slider → progress-bar look (filled track, hairline thumb).
- [x] One-shot events: diamond shape, **centered on the dispatch tick** (was triangle,
  left-aligned), color-coded **yellow**, small AE-keyframe-like marker.
- [x] Block borders softened (alpha strokes; selection = white outline).
- [x] MIDI track header: 3 lines — name / M·S·⚙ / "ch N · <device model>".
- [x] Window titlebar with minimize/maximize/close (frameless window, drag region,
  close checks for unsaved changes). Project name + dirty marker shown in titlebar.
- [x] Dynamic event labels: short command name + selected param values ("Perf 6",
  "Preset 3B", "Scene A") via new `short` attribute on commands and labels
  (XSD + all three definitions updated).
- [x] Lanes taller (24 → 30 px).
- [ ] **In-depth automation curve editing** (add/move/delete breakpoints in the block,
  curve shapes later) — confirmed possible; scheduled as the core of **M4** together
  with the conflict validator.

---

# Feedback round 3 (2026-06-11)

- [x] Device-settings dialog: OK button; closing the dialog of a *freshly created*
  MIDI track cancels the track creation (Cancel button shown for new tracks).
- [x] Focus outline clipped on the left → replaced outline with accent border-color.
- [x] Subtle separator line between timeline tracks.
- [x] MIDI tracks start with 2 lanes (was 3).
- [x] Automation breakpoint editing (pulled forward from M4): double-click adds a
  breakpoint, drag moves (inner points: time+value, endpoints: value only),
  right-click deletes, snap applies to breakpoint times, dots rendered on the curve.
- [x] New bundled definitions: **Fractal Axe-Fx III** (convention-based — device has
  no fixed CCs; Description documents the required MIDI/Remote assignments) and
  **HeadRush Prime** (full CC chart from user guide v4.0.0 §4.22).

---

# Feedback round 4 (2026-06-11)

- [x] Track reordering by dragging track headers (insertion indicator, undo-able).
- [x] Window radius increased on top (20px / 8px bottom); inner panels square.
- [x] App logo: equal x/y spacing in the titlebar.
- [x] File actions (New/Open/Save/Save As) in a burger menu (context-menu style);
  Export stays as primary button.
- [x] Edit bar + transport bar: solid color, no border/outline.
- [x] Play button green (#2ee08a).
- [x] Per-audio-track waveform zoom ("Wave zoom" in inspector, 0.25-16x, clipped to
  lane height; view-only, persisted in project).
- [x] **Colored waveforms (DJ-style)** — implemented 2026-06-11 ("Color" toggle in
  the edit bar; offscreen-cached rendering). Original analysis:
  3-band split (low/mid/high) via OfflineAudioContext biquads at import time,
  per-band peak buckets cached next to the existing mono peaks, per-pixel color
  mix on render with offscreen-canvas caching. Kick/snare separation falls out of
  band coloring (kick = low band burst, snare = mid/high broadband). No new deps.

---

# Feedback round 5 (2026-06-11)

- [x] Track reorder / window radius / burger menu / solid bars / green play /
  waveform zoom (see round 4) refined: colored waveforms implemented with
  per-band normalization, squared weights, loudness-driven brightness;
  resolution raised to 1000 buckets/s; burger menu click race fixed;
  transport centered with tempo right-aligned.
- [x] **MIDI latency compensation**: per-track Device Setting (ms), defaulted by the
  definition's new `<Defaults latency="…">` section; per-command `latency`
  attribute adds on top (e.g. HeadRush "Select Rig" = 300 ms rough estimate, to be
  measured). Export shifts messages early accordingly (clamped at 0); unit-tested.
  Will apply to live MIDI-out playback when that feature lands.
- [ ] **Viewport rendering for the timeline canvas** — needed before long songs at
  high zoom (browser canvas width limit ~32k px); makes redraws O(visible pixels).
