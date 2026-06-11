# RigPilot — Confirmed Intent

*Confirmed by Flemming, 2026-06-10. Historical record — terminology was revised
afterwards (e.g. "types/groups" became "Command Types/Groups"); the canonical
vocabulary is [../terminology.md](../terminology.md).*

- **Outcome:** A minimal desktop app (PC/Mac) where each band member programs MIDI rig-control
  tracks for one song per project, against a reference audio track, and exports standard MIDI
  files that import cleanly into the show DAW.
- **User:** The band first (3 members; Kemper Stage, Quad Cortex, DNAfx Git Pro). Other bands
  doing timed live shows are a someday-maybe that informs design cleanliness but gates nothing.
- **Why:** Programming raw CC/PC events in a DAW from device manuals is tedious and error-prone.
  RigPilot replaces that with QLC+-style device definitions + drag-and-drop commands.
- **Success:** Every member programs their own rig controls in RigPilot; exports import correctly
  into Studio One 6 and Reaper; the DAW then controls all three rigs correctly through a live set.
- **Core model:** Fixed event kinds — **One-Shot**, **Hold**, **Automation**. Hand-authored device
  definition files (shipped starter library + import) map named commands to kinds + MIDI payloads,
  organized by *type* (semantic, drives event color) and *group* (frontend sorting only).
  Validated against the three band devices first.
- **Editor:** Tracks (audio = reference with volume/pan/mute/solo; MIDI = command events).
  MIDI editor uses **anonymous lanes** (FL-Studio-Playlist style — Y carries no meaning, only
  X/time counts), a grouped/color-coded command palette as drag source, BPM grid with snap toggle
  and musical↔raw-time grid toggle. Conflicts caught by validation, not layout constraints.
  Idiot-safe and minimal above all.
- **Constraint:** Project = 1 song. No hard deadline; rehearsal-driven.
- **Out of scope (v1):** Live show playback from RigPilot, built-in definition editor UI, mixer
  view, piano roll, CC-threshold Hold commands (likely covered by Hold-with-arbitrary-messages).
  MIDI output to real hardware = nice-to-have, not v1-blocking.
