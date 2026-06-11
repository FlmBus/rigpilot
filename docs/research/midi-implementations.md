# MIDI Implementation Research — Band Devices

*M0 research, 2026-06-10. Sources at the bottom; items marked ⚠ must be verified against
the real hardware before the definitions are considered done.*

## Summary / schema-fit verdict

| Question | Kemper Profiler Stage | Quad Cortex | DNAfx GiT Pro |
|---|---|---|---|
| MIDI channel | 1 global channel, user-set, default **OMNI** | 1 channel, user-set, OMNI available | 1 channel, user-set, default **1**, OMNI available |
| Fixed channels? | No | No | No |
| Preset switching | Bank (CC32 LSB) + PC | Bank (CC0/CC32) + PC | Bank (CC0, values 0–1) + PC |
| Continuous control | CC (7-bit) + NRPN (14-bit) | CC | CC |
| Deterministic effect on/off | **NRPN only** (plain CCs are toggles!) | CC value-based | CC value-based ⚠ |

**Verdict:** the One-Shot / Hold / Automation model and the message vocabulary fit all
three devices. **Every surveyed device listens on exactly one user-configured channel** —
additionally confirmed for **Axe-FX III** (single channel setting; OMNI exists but
Fractal discourages it) and **Headrush Prime** (one global send/receive channel in
Global Settings → MIDI). Consequence (decided 2026-06-10): definitions carry **no
channel information**; the MIDI Channel is a universal Device Setting in the app.

---

## Kemper Profiler Stage

From the official *Profiler MIDI Parameter Documentation* (rev. 145, fw ≥ 4.2.1).

**Channel:** one global MIDI channel ("MIDI Global Channel" in System Menu), default
OMNI. NRPN messages also arrive on this channel.

**CC commands (the useful set):**

| CC | Values | Function | RigPilot mapping |
|---|---|---|---|
| 1 / 4 / 7 / 11 | 0–127 | Wah / Pitch / Volume / Morph pedal | **Automation** |
| 16 | any | Toggle ALL stomps | One-Shot (toggle — see warning) |
| 17–20, 22 | any | Toggle Stomp A/B/C/D/X | One-Shot (toggle ⚠ see NRPN note) |
| 24 / 26 / 27 | any | Toggle MOD / DELAY / DELAY w. tail | One-Shot (toggle) |
| 28 / 29 | any | Toggle Reverb / Reverb w. tail | One-Shot (toggle) |
| 30 | 1/0 | Tap Tempo (1=press, 0=release) | One-Shot (send 0; or Hold for beat scanner) |
| 31 | 1/0 | Show / Hide Tuner | Hold<31=1, 31=0> |
| 33 | 0/1 | Rotary slow / fast | Hold |
| 34 / 35 | 0/1 | Delay Feedback Infinity / Delay Hold | Hold |
| 47 | 0–124 | Preselect performance index | One-Shot w. param |
| 48 / 49 | 1/0 | Performance up / down | One-Shot |
| 50–54 | 1 | Load Slot 1–5 of current performance | One-Shot |
| 68–73 | 0–127 | Delay Mix / Delay Feedback / Reverb Mix / Reverb Time / Amp Gain / Monitor Volume | **Automation** |

**Performance loading, absolute:** every Slot has a fixed address = Bank Select LSB
(CC#32) + Program Change. PC#27 = Performance 6 Slot 2, regardless of current state —
i.e. `address = (performance−1)·5 + (slot−1)`, split into CC32 = address div 128,
PC = address mod 128. 125 performances × 5 slots.

**⚠ Toggle trap (key finding):** the plain stomp CCs 16–29 *toggle* — any value flips
the current state. For show programming this is non-deterministic. **Absolute on/off
exists only via NRPN** (14-bit, CC99=page / CC98=number / CC6=MSB / CC38=LSB; e.g.
Stomp A = page 50, param 3 "On/Off"). Since NRPN is literally a 4-CC sequence, our
multi-message One-Shot/Hold payloads can already express it — e.g. deterministic
"Stomp A" = `Hold<[CC99=50, CC98=3, CC6=0, CC38=1], [CC99=50, CC98=3, CC6=0, CC38=0]>`.
A dedicated `<Nrpn>` element is nicer ergonomics, but not a blocker.

---

## Neural DSP Quad Cortex

From the official User Manual 4.0.0 (CorOS 4) + community CC reference.

**Channel:** one MIDI channel, Settings → Device → MIDI, OMNI available.

**Incoming CC list:**

| CC | Values | Function | RigPilot mapping |
|---|---|---|---|
| 0 / 32 | 0–127 | Bank MSB / LSB (setlist / preset bank for PC) | part of Select Preset One-Shot |
| 1 / 2 | 0–127 | Expression pedal 1 / 2 | **Automation** |
| 35–42 | value-based | Footswitch A–H enable/bypass | Hold |
| 43 | 0–7 | **Scene select A–H** | One-Shot w. param (param in the *value* field!) |
| 44 | — | Tap tempo / tempo | One-Shot |
| 45 | 0–63 / 64–127 | Tuner close / open | Hold<45=127, 45=0> |
| 46 | 0–63 / 64–127 | Gig View close / open | Hold |
| 47 | 0 / 1 / 2 | Mode: Preset / Scene / Stomp | One-Shot w. param |
| 48–61 | various | Looper X (open/close, rec, play, half-speed, reverse, undo…) | One-Shot / Hold |
| 62 | 0–63 / 64–127 | Ignore duplicate PC off / on | One-Shot |

**Preset switching:** Bank/setlist via CC0/CC32 + PC. ⚠ The exact MSB/LSB↔setlist/bank
arithmetic should be taken from Neural's official "MIDI PC Calculator" and verified on
the device (CorOS versions have differed here).

---

## Harley Benton DNAfx GiT Pro

From the official Thomann user manual (ID 517099, 2022). Surprisingly complete MIDI docs.

**Channel:** MIDI SETTING menu — direction (MIDI IN default / MIDI OUT), channel 1–16
(default 1), OMNI on/off (default off), clock sync on/off.

**Preset switching (PC Mapping):** 2 banks via **CC#0 (value 0 or 1)** + PC 0–127:
Bank 0 PC 0–127 → presets 1A–32D, Bank 1 PC 0–127 → presets 33A–64D
(i.e. `address = (preset−1)·4 + letter`, 4 presets per numbered bank A–D).
RigPilot command: One-Shot `[CC0=$bank, PC=$program]` with a user-friendly param.

**CC reference:**

| CC | Values | Function | RigPilot mapping |
|---|---|---|---|
| 0 | 0–1 | MIDI bank | part of Select Preset |
| 10–20 | 0–127 ⚠ | Module on/off: Dynamic, Wah, DS, Amp, Cab, FX Loop, NS, EQ, Modulation, Delay, Reverb | Hold ⚠ |
| 21–24 | 0–127 | Footswitch A–D | One-Shot / Hold ⚠ |
| 25 | 0–127 | Tap tempo | One-Shot |
| 26 / 27 | 0–127 | Looper enter/exit, Tuner enter/exit | Hold ⚠ |
| 28 | 0–127 | Rec/Dub | One-Shot |
| 46–50 | 0–127 | Looper Play / Stop / Clear, Rhythm on/off, Rhythm tap | One-Shot |
| 51 | 0–127 | Pedal 1 on/off | Hold ⚠ |
| 52 / 53 | 0–127 | Pedal 1 / Pedal 2 position | **Automation** |

**⚠ Hardware verification needed:** the manual lists "0…127" for the ON/OFF CCs without
saying whether they're value-based (0–63 off / 64–127 on), edge-triggered toggles, or
0=off/127=on. This decides whether they're modeled as Hold or as toggle One-Shots.
First thing to test with a MIDI cable.

---

## Schema implications (feed into the XSD)

1. **Params must substitute into *any* message field, including CC values** — QC scene
   select is `CC43 value=$scene`, not a PC. Already compatible with the `$param` design;
   the XSD just must not restrict where `$id` placeholders may appear.
2. **No expression language (decided 2026-06-10):** params substitute verbatim into
   message fields — no arithmetic anywhere. Preset selection is modeled without math:
   - *Kemper:* use the native navigation commands — "Preselect Performance" (CC47,
     param 0–124) + "Load Slot 1–5" (CC50–54). The absolute CC32+PC scheme is not used.
   - *DNAfx:* one command **per bank** with the bank value baked in:
     `Select Preset 1A–32D = [CC0=0, PC=$p]`, `Select Preset 33A–64D = [CC0=1, PC=$p]`.
   - *Friendly names* come from param **value labels** in the definition (value 9 shown
     as "3B"), so users pick names, never raw PC numbers.
3. **NRPN:** expressible today as a 4-CC multi-message payload (sufficient for Kemper
   deterministic stomp switching); a sugar element `<Nrpn page="50" number="3" value="1"/>`
   can come later without schema breakage.
4. **No channel declarations in definitions (decided 2026-06-10):** all five surveyed
   devices use exactly one user-configured channel, so the Channel Slot concept was
   removed. The MIDI Channel is a universal Device Setting the app asks for on every
   Device. If a multi-channel device ever surfaces, a future schema version can add
   optional channel declarations.
4b. **Firmware/model variants:** when MIDI behavior genuinely differs between firmware
   versions or models (candidate: Quad Cortex CorOS versions), each variant gets its
   own definition file (e.g. "Quad Cortex (CorOS 4.x)") — like fixture revisions in
   QLC+. The ⚠ items in this doc are *not* variants, just documentation gaps to settle
   with a cable test.
5. **Toggle-style commands exist** (Kemper CC16–29, possibly DNAfx). They're One-Shots,
   but definitions should be able to flag them `deterministic="false"` so the UI can
   warn show programmers that the resulting state depends on the device's current state.

## Sources

- [Kemper Profiler MIDI Parameter Documentation rev. 145 (PDF)](https://mojagitara.com/wp-content/uploads/2018/04/Profiler-Midi-Parameter-Documentation.pdf)
- [Kemper Profiler Main Manual (pedal CCs 1/4/7/11)](https://tcfurlong.com/wp-content/uploads/KEMPER-PROFILER-Main-Manual-5.5-English.pdf)
- [Kemper forum: MIDI control of Rigs and Performances](https://forum.kemper-amps.com/forum/thread/47331-midi-control-of-rigs-and-performances/)
- [Quad Cortex User Manual 4.0.0](https://neuraldsp.com/manual/quad-cortex)
- [Quad Cortex incoming CC list (community reference)](https://voes.be/midi-cc/neuraldsp_quadcortex.html)
- [Quad Cortex MIDI PC Calculator (Neural DSP support)](https://support.neuraldsp.com/help/quad-cortex-midi-pc-calculator)
- [DNAfx GiT Pro user manual (Thomann, ID 517099)](https://images.thomann.de/pics/atg/atgdata/document/manual/c_517099_r1_en_online.pdf)
- [Fractal Audio Wiki: MIDI (Axe-FX III channel setting, scene select CC34)](https://wiki.fractalaudio.com/wiki/index.php?title=MIDI)
- [HeadRush Prime: Receiving MIDI Program Changes](https://support.headrushfx.com/en/support/solutions/articles/69000871971-headrush-prime-receiving-midi-program-changes-from-your-prime)
- [HeadRush Prime FAQ (global MIDI channel)](https://support.headrushfx.com/en/support/solutions/articles/69000834532-headrush-prime-frequently-asked-questions)
