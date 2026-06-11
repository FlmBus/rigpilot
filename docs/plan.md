# RigPilot — Project Plan

*Derived from the confirmed intent in [intent/rigpilot.md](intent/rigpilot.md).*

RigPilot is a minimal desktop app (Windows/macOS) for programming MIDI rig-control tracks
against a reference audio track, exporting standard MIDI files that drop into the band's
show DAW (Studio One 6, Reaper). Think: QLC+ show editor + fixture definitions, for MIDI.

---

## 1. Core Concepts

### 1.1 Project
- One project = **one song**. Single file on disk (recommended extension: `.rigpilot`).
- Contains: song metadata (title, BPM, time signature), tracks, events, and **embedded
  copies of all device definitions in use** (imported definitions are copied into the
  project so it stays portable between band members' machines — QLC+ does the same).
- Audio files are **always copied on import** into a project-adjacent assets folder, so a
  project folder zipped up and sent to a bandmate just works — no broken references, no
  extra "collect" step.

### 1.2 Tracks
Two track types, shown in one arrangement view (no separate mixer):

| Track type | Purpose | Controls |
|---|---|---|
| **Audio** | Reference only — the song mix or stems, so events can be placed by ear | volume, pan, mute, solo |
| **MIDI** | Holds device command events | mute, solo; settings dialog |

Audio tracks never export. MIDI tracks each export to one `.mid` file.

**MIDI track settings dialog** (per track):
- Track name, color
- **Device Definition**: pick from the shipped library or definitions imported into the
  project
- **MIDI Channel** (1–16): a universal Device Setting, asked for every Device regardless
  of definition — RigPilot's equivalent of the DMX start address. Research confirmed all
  surveyed devices (Kemper, Quad Cortex, DNAfx, Axe-FX III, Headrush Prime) listen on
  exactly one user-configured channel, so channel declarations were removed from the
  definition format entirely (docs/research/midi-implementations.md).
- Further **Device Settings** if a definition declares any.
- (later) live output port for hardware preview

### 1.3 Command Types (fixed, app-defined)
The app defines a closed set of three **Command Types**. Device Definitions map their
commands onto these types — never the other way around. The Command Type drives the
event color on the timeline.

| Command Type | Timeline shape | Definition payload | Example |
|---|---|---|---|
| **One-Shot** | Point event (zero length, rendered as a small block/marker) | one or more MIDI messages sent at the event's time | `Tap-Tempo = OneShot<NoteOn 44>` |
| **Hold** | Block with start + end | *engage* message(s) at block start, *disengage* message(s) at block end | `Mute = Hold<NoteOn 51, NoteOff 51>` or `Mute = Hold<PC 3, PC 4>` |
| **Automation** | Block with a value curve inside | a continuous target (CC number; later NRPN) plus value range | `Master-Volume = Automation<CC 5>` |

Notes:
- Hold takes *arbitrary* engage/disengage messages, which also covers the CC-threshold case
  (`Hold<CC 80 = 127, CC 80 = 0>`) without a special mechanism.
- Automation is a command containing a value curve targeting a MIDI CC — for volume
  fades, pre-programmed wah/whammy etc., like an automation clip in any DAW. Blocks
  contain breakpoints (time + value) connected by linear segments in v1
  (curve shapes are a later enhancement). At export they are rendered to a CC event stream
  at a fixed resolution (default: one CC per MIDI tick step where the value changes,
  capped at ~100 msgs/sec to avoid flooding device input buffers).
- **Parameterized commands:** some commands need a user-supplied value per event, e.g.
  "Select Preset" = One-Shot<PC *n*> where *n* is chosen when placing the event. The
  definition declares the parameter (range, default, optional value labels like preset
  names). Without this, a Kemper definition would need 128 separate "Preset 1…128"
  commands.

### 1.4 Device definitions
Hand-authored **XML** files (the "fixture definitions" of RigPilot — same format family
as QLC+ fixtures). XML over JSON because definition files are written by humans: XML
allows comments (`<!-- per manual p. 34 -->`), and the Command Type can be the element
name itself.

- RigPilot ships a **starter library** (read-only, bundled with the app).
- Users can **import** custom definition files into a project.
- v1 has **no built-in definition editor** — a text editor + the shipped **XSD** is the
  authoring tool (gives validation + autocomplete in any XML-aware editor, e.g. VS Code
  with the Red Hat XML extension).
- The **project file** stays JSON — it is machine-written and never hand-edited.

Structure (sketch):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<DeviceDefinition id="kemper-profiler-stage" version="1.0"
                  xmlns="https://rigpilot.app/schemas/device-definition/1">
  <Meta>
    <Manufacturer>Kemper</Manufacturer>
    <Model>Profiler Stage</Model>
    <Author>Flemming Busman</Author>
  </Meta>

  <!-- No channel declarations: the MIDI channel is a universal Device Setting
       the app asks for on every Device. -->

  <!-- Optional: puts the device in a known clean state (used by the Reset Block) -->
  <Init>
    <ControlChange controller="17" value="0"/>
  </Init>

  <!-- group is a free-form string — the palette derives its groups from the
       distinct values found across commands; no declaration needed -->
  <Commands>
    <OneShot id="tap-tempo" name="Tap Tempo" group="Rig Selection">
      <ControlChange controller="30" value="127"/>
    </OneShot>

    <OneShot id="select-rig" name="Select Rig" group="Rig Selection">
      <Param id="rig" name="Rig #" min="0" max="127" default="0"/>
      <ProgramChange program="$rig"/>
    </OneShot>

    <Hold id="stomp-a" name="Stomp A" group="Effects">
      <Engage><ControlChange controller="17" value="127"/></Engage>
      <Disengage><ControlChange controller="17" value="0"/></Disengage>
    </Hold>

    <Automation id="volume" name="Volume Pedal" group="Effects">
      <Target controller="7" min="0" max="127"/>
    </Automation>
  </Commands>
</DeviceDefinition>
```

Messages carry no channel information — every message of a Device goes out on that
Device's configured MIDI channel. (Should a multi-channel device ever surface, a future
schema version can add optional channel declarations; nothing in v1 blocks that.)

Message vocabulary v1: `NoteOn`, `NoteOff`, `ControlChange`, `ProgramChange`. Reserved
for later: `SysEx`, `Nrpn`, `PitchBend`, bank-select shorthand (Kemper performances need
CC0/CC32 + PC — expressible v1 as a multi-message One-Shot, so no blocker).

**First validation task of the project:** write the three band definitions (Kemper
Profiler Stage, Neural DSP Quad Cortex, Harley Benton DNAfx Git Pro) from their MIDI
implementation charts. If anything doesn't fit the schema, fix the schema *before*
building the editor on top of it.

---

## 2. UI / UX

Guiding principle: **idiot-safe and minimal**. One window, no modes, no mixer view,
nothing the band doesn't need.

### 2.1 Main window layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ ⏵ ⏹ ⏺   00:01:23.4 | 33.2.1   BPM 132   [Snap ✓] [Grid: Bars|Time]   │ transport bar
├──────────────┬───────────────────────────────────────────────────────┤
│ Track headers│ Timeline (ruler: bars/beats or min:sec)                │
│ ┌──────────┐ │ ───────────────▼ playhead ─────────────────────────── │
│ │🔊 Mix     │ │ ╱╲╱╲╱╲╱╲ waveform ╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲    │
│ │ vol pan   │ │                                                       │
│ │ M S       │ │                                                       │
│ ├──────────┤ │ ┌────────┐      ┌─────────────────┐                    │
│ │🎛 Kemper  │ │ │Rig #5  │      │ Mute ░░░░░░░░░░ │   ◆TapTempo       │
│ │ ch 1  M S │ │ └────────┘      └─────────────────┘                    │
│ │ ⚙ settings│ │        ┌─ Volume ▁▂▄▆█▆▄▂ ─┐                          │
│ └──────────┘ │        └────────────────────┘                          │
├──────────────┴───────────────────────────────────────────────────────┤
│ Command palette (when a MIDI track is selected)                       │
│ [Rig Selection ▾]  ■Select Rig  ■Tap Tempo   [Effects ▾] ■Stomp A …  │
└──────────────────────────────────────────────────────────────────────┘
```

- **Track headers** left, **timeline** right — classic arrangement view.
- **Command palette**: appears for the selected MIDI track, shows that track's device
  commands, organized by *group* (collapsible), colored by *Command Type*. Drag a command onto the
  track to create an event. A search box filters commands.
- **Transport**: play/stop, position readout in both time and bars/beats, BPM field,
  snap toggle, grid-mode toggle (musical ↔ raw time).

### 2.2 MIDI track lanes — the "anonymous lanes" model
- Each MIDI track is a grid of N horizontal lanes (FL-Studio-Playlist semantics).
- **Y position carries no meaning.** Lanes exist only so events can overlap in time
  without drawing on top of each other. Any event can sit in any lane.
- Tracks start with a small number of visible lanes (e.g. 3) and grow on demand
  (drag below the last lane → new lane). Empty trailing lanes auto-collapse.
- Events render as colored blocks (color = Command Type), labeled with the command
  name (+ parameter value, e.g. "Rig #5").
- One-Shot = narrow fixed-width marker. Hold = resizable block. Automation = resizable
  block with the curve drawn inside; double-click opens an inline breakpoint editor
  (click to add a point, drag to move, right-click to delete).

### 2.3 Editing interactions
- Drag from palette → drop on track (snaps to grid if enabled).
- Move (drag), resize Hold/Automation (drag edges), duplicate (alt-drag / Ctrl+D),
  delete (Del), multi-select (rubber band, shift-click), cut/copy/paste.
- Double-click an event → small inspector popover: exact start/end (editable in both
  time formats), parameter value, jump-to controls.
- Full undo/redo across everything. Non-negotiable for "idiot-safe".
- Zoom: Ctrl+scroll horizontal; scroll vertical.

### 2.4 Validation (instead of layout constraints)
Because lanes are anonymous, conflicts are *detected*, not prevented:

- Two Hold events of the **same command** overlapping in time → error.
- Automation overlap on the **same CC target** → error.
- A One-Shot/Hold engage landing *inside* an Automation block that writes the same CC →
  warning.
- Two events emitting messages at the **exact same tick** → info (ordering will follow
  lane order top-to-bottom; shown so the user knows it's deterministic).

Presentation: conflicting events get a red/yellow outline + a problems list in the status
bar; clicking a problem scrolls to it. **Export is blocked on errors, allowed with
warnings** (with a confirmation dialog listing them).

### 2.5 Time & grid
- v1: single global BPM + time signature, set in the transport bar. (Tempo map /
  tempo changes = post-v1; the data model should keep a tempo *list* internally so this
  doesn't require a migration.)
- Grid: musical (bars/beats/subdivisions, adaptive to zoom) or raw time (s/100ms…),
  toggleable. Snap on/off toggle, plus temporarily bypass by holding Alt while dragging.
- An **audio offset** per audio track (the mix rarely starts exactly at beat 1.1.1) and a
  **global song start marker** that defines t=0 for export.

---

## 3. Export

The product's actual deliverable — treat as the highest-risk feature and build it early.

- **Format:** Standard MIDI File (SMF) **Type 0, one file per MIDI track** (one track,
  one device, one channel — Type 0 imports most predictably). PPQN 960. File includes the
  tempo meta event and time signature so musical positions line up if the DAW project
  shares the tempo; absolute-time alignment holds regardless because t=0 = song start.
- The MIDI channel of each message is resolved through the track's **channel slot
  assignment** (profile slot → actual channel 1–16).
- **Reset Block** (toggleable per export, default **on**): prepends at t=0 the definition's
  `init` messages plus the disengage messages of every Hold command used in the track —
  restarting the song in the DAW always begins from a clean device state, even after a
  mid-song stop left a Hold engaged.
- Automation blocks rendered to CC streams (resolution as in §1.3).
- Export dialog: choose output folder, filename pattern (`{song} - {track}.mid`),
  per-track include checkboxes.
- **Acceptance test (the v1 finish line):** export from RigPilot → import into Studio
  One 6 *and* Reaper → events land at the right positions → DAW playback correctly
  switches the Kemper Stage, Quad Cortex, and DNAfx Git Pro through a full song.

---

## 4. Tech Stack (recommendation)

| Concern | Choice | Why |
|---|---|---|
| Shell | **Tauri 2** | Small binaries, native installers for Win/macOS, Rust backend for the parts that must be exact |
| UI | **TypeScript + Svelte** | The whole app is one big custom widget; Svelte keeps the reactive state lean. (React fine too — team preference wins) |
| Timeline rendering | **Canvas 2D** (single canvas for ruler/waveform/events) | DOM nodes per event won't survive zooming/scrolling a full song; canvas will. Waveform pre-rendered to peak data |
| Audio playback | **Web Audio API** in the webview | Decoding (wav/mp3/flac), transport, gain/pan per track — all built in, no native audio stack needed since audio is reference-only |
| MIDI file writing | **Rust crate `midly`** behind a Tauri command | SMF writing where tick math bugs would be silent and fatal; Rust side is trivially unit-testable |
| Live MIDI out (nice-to-have) | **Rust `midir`** | Cross-platform port enumeration + output; UI just streams "send these messages now" |
| Device definition files | **XML + XSD** (Rust `quick-xml` + serde) | Hand-authored: comments allowed, Command Type as element name, XSD gives free editor validation/autocomplete; QLC+-familiar |
| Project file | **JSON** (serde/zod validated) | Machine-written, never hand-edited — zero-friction (de)serialization |
| Undo | Command pattern over a single immutable-ish project state | Simplest model that makes "undo everything" true |

Everything user-facing lives in the webview; Rust does file I/O, SMF export, and (later)
MIDI ports. No DSP, no plugin hosting, no native audio driver work — that's what keeps
this buildable by a small team.

---

## 5. Milestones

Ordered so the riskiest assumptions are tested first and every milestone ends with
something the band can poke at.

- **M0 — Schema validation (no app yet).** Write the device-definition XSD and
  the three band definitions from the devices' MIDI implementation charts. Adjust the
  schema until all three fit. Verify whether any device expects fixed MIDI channels.
  *Exit: three valid definition files.*
  **Status: done 2026-06-10** — `definitions/device-definition-1.xsd` + three
  definitions, all xmllint-valid. Remaining: the ⚠ hardware cable tests noted in the
  definition file headers (DNAfx/QC value semantics, QC bank arithmetic).
- **M1 — Walking skeleton + export.** Tauri app, project file new/open/save, hardcoded
  test events, SMF export. Import into Studio One + Reaper and drive a real rig. *Exit:
  a hand-built RigPilot project changes a Kemper preset via DAW playback.* (Export first
  — it's the deliverable; everything else is editing convenience.)
  **Status: code done 2026-06-10** — `app/` (Tauri 2 + Svelte 5), project save/load,
  SMF export with Reset Block, form-based event editing, 5 passing backend tests,
  export independently verified with mido. **Pending:** the DAW import + rig cable
  test (hardware unavailable for a few days); test project: `examples/cable-test.rigpilot`.
- **M2 — Audio reference + transport.** Import audio, waveform rendering, play/stop/seek,
  volume/pan/mute/solo, BPM + grid + snap + grid-mode toggle, audio offset.
  **Status: code done 2026-06-10** — copy-on-import via `import_audio`, Web Audio
  playback engine (`src/lib/audio.ts`), canvas timeline with ruler/grid/waveform/
  playhead (`src/lib/Timeline.svelte`), transport bar with snap + Bars/Time grid
  toggle, per-track volume/pan/mute/solo live during playback, audio offset in beats,
  Space = play/pause, Ctrl+wheel = zoom. Pending: visual check by the band.
- **M3 — The editor.** MIDI tracks with settings dialog, command palette, anonymous
  lanes, drag-drop/move/resize/copy/delete, One-Shot + Hold events, undo/redo,
  event inspector.
  **Status: code done 2026-06-11** — full timeline editor (drag from palette, move
  within/across lanes, edge-resize, marquee/Ctrl/Shift selection, Del/Ctrl+C/V/Z/Y/S
  keybinds), inspector sidebar for events & tracks, device-settings dialog (cogwheel),
  export-options dialog, snap selector down to 1/32, new high-contrast magenta/white
  theme (Archivo + IBM Plex Mono, custom controls), unsaved-changes confirmation,
  audio-import flow fixed (file first, then save prompt). Pending: band hands-on.
- **M4 — Automation + validation.** Automation blocks with breakpoint editing, the
  conflict validator + problems list, export gating.
- **M5 — Band test ("eat your own dog food").** All three members program one real
  song each on their own machines. Fix everything that confused anyone — this is the
  idiot-safe acceptance test. *Exit: the v1 success criterion from the intent doc.*
- **M6 — Nice-to-haves (post-v1, pick by appetite):** live MIDI output to hardware
  during playback, built-in definition editor, tempo map, sysex/NRPN messages,
  curve shapes for automation, setlist/multi-song grouping, definition sharing for the
  "other bands" path.

---

## 6. Resolved Decisions (2026-06-10)

1. **Naming:** "Device Definition" everywhere ("profile"/"rig" clash with Kemper
   terminology); a **Device** is an instance (definition + settings) on a track.
   The three behaviors (One-Shot/Hold/Automation) are **Command Types** and drive event
   color; **Group** is the (only) organizational category for the palette.
   Full glossary: [terminology.md](terminology.md).
2. **Hold safety:** automatic **Reset Block** at t=0 on export (toggleable, default on) —
   definition `init` messages + disengage of all used Hold commands (§3).
3. **Audio files:** always copied into the project's assets folder on import (§1.1).
4. **MIDI channel:** a universal Device Setting (1–16), asked for every Device —
   definitions contain no channel information at all. Backed by research: Kemper,
   Quad Cortex, DNAfx, Axe-FX III, and Headrush Prime all listen on exactly one
   user-configured channel (§1.2, §1.4, research doc).
5. **File formats:** definition files are **XML** (validated by a shipped XSD —
   comments allowed, Command Type as element name, QLC+-familiar); the project file
   stays **JSON** since it is machine-written only (§1.4, §4).
6. **No expression language in definitions:** params substitute verbatim into message
   fields. Device math is avoided via native navigation commands (Kemper CC47+CC50–54),
   per-bank commands with baked-in values (DNAfx), and param **value labels** for
   friendly names (research doc, "Schema implications").
7. **Firmware/model variants** with differing MIDI behavior get separate definition
   files (like QLC+ fixture revisions), e.g. "Quad Cortex (CorOS 4.x)".
