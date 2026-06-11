# RigPilot — Terminology

The canonical glossary. UI labels, docs, code identifiers, and file-format field names
all use these terms. If a word isn't in here, it shouldn't appear in the UI.

## Project & tracks

| Term | Meaning |
|---|---|
| **Project** | One song. A `.rigpilot` file plus its assets folder. Contains tracks, events, and embedded copies of all Device Definitions in use. |
| **Track** | A horizontal row in the arrangement. Either an Audio Track or a MIDI Track. |
| **Audio Track** | Reference-only audio (song mix or stem) with volume/pan/mute/solo. Never exported. |
| **MIDI Track** | A track that controls exactly one Device. Holds Events; exports to one `.mid` file. |
| **Device Definition** | The blueprint describing how a device type is controlled via MIDI: its Commands and Init Messages. Analogous to a fixture definition in lighting software. Lives as an XML **definition file** (validated by the RigPilot XSD); shipped ones form the **Definition Library**. Contains no channel information. |
| **Device** | The *configured instance* on a MIDI Track: a chosen Device Definition plus its Device Settings. Analogous to a patched fixture. |
| **Device Settings** | The per-track configuration of a Device. Always includes the **MIDI Channel** (1–16) and **Latency Compensation** (ms — messages are sent earlier by this amount at export so slow devices switch on time). Definitions may provide defaults via their `Defaults` section. |

## Commands & events

| Term | Meaning |
|---|---|
| **Command** | A named, device-specific control declared in a Definition (e.g. "Tap Tempo", "Stomp A", "Master Volume"). Every Command has exactly one Command Type and one Group. |
| **Command Type** | One of the three app-defined behaviors a Command can have: **One-Shot**, **Hold**, or **Automation**. Fixed by RigPilot, never extended by Definitions. Drives the **event color** on the timeline. |
| **One-Shot** | Command Type: fires its Messages once at a point in time. Rendered as a marker. |
| **Hold** | Command Type: sends Engage Messages at block start and Disengage Messages at block end. Rendered as a resizable block. |
| **Automation** | Command Type: a value curve over a block, targeting a MIDI CC — for volume fades, pre-programmed wah/whammy, etc. Like an automation clip in a DAW. Edited via Breakpoints, rendered to a CC stream on export. |
| **Group** | A free-form string on each Command, purely organizational. The Command Palette derives its groups from the distinct values found in the Definition — groups are not declared anywhere. No semantic meaning beyond the UI. |
| **Parameter** | A per-Event value a Command may require (e.g. "Rig #" 0–127 on "Select Rig"), declared in the Definition with range, default, and optional value labels. |
| **Event** | A placed instance of a Command on a MIDI Track's timeline, with a start time, a Lane, optionally a length (Hold/Automation), and Parameter values. |
| **Message** | A single raw MIDI message (`NoteOn`, `NoteOff`, `ControlChange`, `ProgramChange`) inside a Definition. Carries no channel — it is sent on the Device's configured MIDI Channel. |
| **Init Messages** | Optional Definition-declared Messages that put the device into a known clean state. Used by the Reset Block. |
| **Latency** (per Command) | Optional per-Command execution time in ms declared in the Definition (preset loads are slow, stomp toggles are not), added on top of the track's Latency Compensation when shifting messages early. |
| **Breakpoint** | A time/value point inside an Automation Event; consecutive Breakpoints are connected (linear in v1). |

## Editor

| Term | Meaning |
|---|---|
| **Arrangement** | The main view: track headers left, Timeline right, Command Palette bottom. |
| **Timeline** | The time area where Events and audio waveforms live. |
| **Lane** | One anonymous horizontal row inside a MIDI Track. Y-position carries no meaning; Lanes exist only so Events can overlap in time (FL-Studio-Playlist semantics). |
| **Command Palette** | The panel showing the selected MIDI Track's Commands — organized by Group, colored by Command Type — as the drag source for placing Events. |
| **Grid** | The snapping raster, toggleable between **Musical** (bars/beats, from BPM + time signature) and **Raw Time** (seconds). |
| **Snap** | Toggle: whether dragging/placing aligns to the Grid (Alt bypasses temporarily). |
| **Song Start** | The marker defining t=0 for export and the Reset Block position. |
| **Audio Offset** | Per-Audio-Track shift so the recording lines up with the musical grid. |
| **Inspector** | The popover (double-click an Event) for exact times and Parameter values. |
| **Problem** | A validation finding (error / warning / info), listed in the Problems list. **Errors block export.** |

## Export

| Term | Meaning |
|---|---|
| **Export** | Rendering each MIDI Track to a Standard MIDI File (Type 0, 960 PPQN, tempo + time signature included). |
| **Reset Block** | Optional (default on) block of Messages prepended at t=0 on export: the Definition's Init Messages plus the Disengage Messages of every Hold Command used — so a song restart always begins from a clean device state. |

## Explicitly avoided terms

- ~~Profile~~ / ~~Rig~~ as RigPilot concepts → both are Kemper terminology for other things; we say Device Definition / Device
- ~~Fixture~~ → Device Definition / Device (we're not lighting software)
- ~~Kind~~ / ~~Type~~ unqualified → always **Command Type** (One-Shot, Hold, Automation)
- ~~Clip / Pattern / Note~~ → Event (nothing here is a musical note)
- ~~Channel~~ unqualified → "MIDI Channel" (a Device Setting) or "Track" (different things)
- ~~Channel Slot~~ → removed concept; the MIDI Channel is a universal Device Setting, definitions carry no channel info
