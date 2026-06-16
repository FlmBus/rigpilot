# Hold-Command Audit — should toggles be One-Shots?

*Research for [#5](https://github.com/FlmBus/rigpilot/issues/5) (and informs
[#4](https://github.com/FlmBus/rigpilot/issues/4)). 2026-06-16.*

## Question

The bundled definitions contain **38 `Hold` commands**. Almost all of them model
effect on/off, tuner show/hide, looper/gig-view, and footswitch emulation. The
suspicion ([#5](https://github.com/FlmBus/rigpilot/issues/5)): `Hold` is the wrong
tool for these — they should be **One-Shot** commands.

This document audits every `Hold` command across the five bundled devices and gives
a per-command verdict.

## What a `Hold` actually models

A `Hold` event is a **region** on the timeline: it emits its `Engage` messages at the
start tick and its `Disengage` messages at the end tick, and the Reset Block emits the
`Disengage` at t=0 so the device starts clean.

That is the right model only when the *region itself* is the musical intent — "this
state is active **from here until there**, and I care about both edges." Genuine
momentary footswitches (press-and-hold) and freeze/hold-style effects fit.

It is the **wrong** model when the device simply holds a persistent on/off state that
the user sets once. There, forcing the user to draw a block (and invent an end) is
friction, and the auto-`Disengage` at the block end / reset is surprising.

### Decision criteria

| Model as… | When |
|---|---|
| **One-Shot** (set-and-forget) | The CC sets a persistent state the device keeps until changed. Turning an effect on, showing the tuner, selecting a footswitch. The common case. |
| **Hold** (region) | The state is intrinsically momentary or section-scoped *and* both edges matter — e.g. a freeze/hold effect held across a passage. Rare. |

## The MIDI reality

Every "toggle" CC surveyed is **value-based** (the value sets the state) or, on some
hardware, **edge-triggered** (any value flips state). Neither is momentary at the
device level:

- **Quad Cortex** — CC35–42 "Enable/bypass Footswitch", CC45 Tuner (`0–63 off / 64–127
  on`), CC46 Gig View, CC48 Looper open/close are all value-based states. The looper's
  momentary behaviour is a *controller-side* setting, not the device model.
  ([voes.be CC list](https://voes.be/midi-cc/neuraldsp_quadcortex.html))
- **Kemper** — plain stomp CCs 16–29 *toggle* (non-deterministic); deterministic
  absolute on/off exists **only via NRPN** (page/number + value 1/0). The definition
  already uses the NRPN form. Rotary/Delay-Infinity/Delay-Hold/Tuner are binary `0/1`
  value states. (Profiler MIDI Parameter Documentation rev. 145)
- **DNAfx GiT Pro** — module CCs 10–20, footswitches 21–24, tuner/looper 26–27,
  pedal-enable 51 are listed `0–127` with semantics unverified. The definition's own
  `Description` says: *assume value-switched (0=off, 127=on); if edge-triggered,
  remodel as non-deterministic One-Shots.* **Both branches end at One-Shot.**
- **Axe-FX III** — Tuner CC15 `0–63 off / 64–127 on`, value-based.

**Consequence:** none of these need the region semantics of `Hold`. Whether a CC is
value-based or an edge toggle only changes the One-Shot's `deterministic` flag and
whether it carries an On/Off param — not whether it should be a `Hold`.

## Per-device audit

Verdict legend:
- **→ One-Shot (On/Off)** — single command with a 2-value `state` param (`Off`=0,
  `On`=127 / NRPN 1/0); deterministic.
- **→ One-Shot (toggle, non-det)** — single press command, `deterministic="false"`
  (use only if the hardware is confirmed edge-triggered).
- **Keep Hold** — region semantics genuinely primary.

### Kemper Profiler Stage (11 Holds)

| Command | CC / NRPN | Today | Verdict |
|---|---|---|---|
| `stomp-a` … `stomp-x`, `stomp-mod`, `stomp-delay` (7) | NRPN page 50–60, param 3 = on/off | Hold | **→ One-Shot (On/Off)** — NRPN value `1`/`0` is absolute & deterministic. |
| `rotary-fast` | CC33 `0/1` | Hold | **→ One-Shot (On/Off)** ("Slow/Fast"). |
| `delay-infinity` | CC34 `0/1` | Hold | **→ One-Shot (On/Off)**, *or* Keep Hold if "infinite feedback across this passage" is the intended region UX. Author's call. |
| `delay-hold` | CC35 `0/1` | Hold | **Keep Hold candidate** — freeze held across a passage is the textbook region case. |
| `show-tuner` | CC31 `1/0` | Hold | **→ One-Shot (On/Off)** ("Show/Hide Tuner"). |

### Neural DSP Quad Cortex (11 Holds)

| Command | CC | Today | Verdict |
|---|---|---|---|
| `footswitch-a` … `footswitch-h` (8) | 35–42 value-based | Hold | **→ One-Shot (On/Off)** ("Enable/Bypass"). |
| `tuner` | 45 `0–63/64–127` | Hold | **→ One-Shot (On/Off)**. |
| `gig-view` | 46 `0–63/64–127` | Hold | **→ One-Shot (On/Off)**. |
| `looper-open` | 48 `open/close` | Hold | **→ One-Shot (Open/Close)**. |

### Axe-FX III (1 Hold)

| Command | CC | Today | Verdict |
|---|---|---|---|
| `tuner` | 15 `0–63/64–127` | Hold | **→ One-Shot (On/Off)**. |

### DNAfx GiT Pro (15 Holds)

| Command | CC | Today | Verdict |
|---|---|---|---|
| `module-dynamic` … `module-reverb` (11) | 10–20 | Hold ⚠ | **→ One-Shot** — On/Off if value-based, toggle(non-det) if edge-triggered. Verify on hardware. |
| `pedal-1-active` | 51 | Hold | **→ One-Shot (On/Off)**. |
| `tuner` | 27 | Hold | **→ One-Shot (On/Off)**. |
| `looper` | 26 | Hold | **→ One-Shot (Open/Close)**. |
| `rhythm` | 49 | Hold | **→ One-Shot (On/Off)** (drum machine run/stop). |

## Verdict

| Device | Holds today | Keep Hold (candidate) | → One-Shot |
|---|---|---|---|
| Kemper | 11 | 1 (`delay-hold`), maybe `delay-infinity` | 9–10 |
| Quad Cortex | 11 | 0 | 11 |
| Axe-FX III | 1 | 0 | 1 |
| DNAfx GiT Pro | 15 | 0 | 15 |
| **Total** | **38** | **≤2** | **≥36** |

The suspicion holds: **~95% of `Hold` commands are misused** and should be One-Shots.
`Hold` survives only for genuine freeze/region effects (at most Kemper `delay-hold`,
arguably `delay-infinity`).

## Recommended remodeling pattern

Model a state toggle as a One-Shot carrying a 2-value `state` param, so the message
value comes from a **labelled** param (this is exactly the [#4](https://github.com/FlmBus/rigpilot/issues/4)
"coarse CC" concern — labels keep the value discrete and the Inspector already renders
a dropdown for labelled params, never a free 0–127 number):

```xml
<!-- DNAfx Wah module, value-based on/off -->
<OneShot id="module-wah" name="Wah Module" short="Wah" group="Modules"
         description="Turns the Wah module on or off.">
  <Param id="state" name="State" min="0" max="127" default="127">
    <Label value="127" short="On">On</Label>
    <Label value="0" short="Off">Off</Label>
  </Param>
  <ControlChange controller="11" value="$state"/>
</OneShot>
```

```xml
<!-- Kemper Stomp A, deterministic on/off via NRPN -->
<OneShot id="stomp-a" name="Stomp A" group="Stomps"
         description="Switches Stomp A on or off (absolute, via NRPN).">
  <Param id="state" name="State" min="0" max="1" default="1">
    <Label value="1" short="On">On</Label>
    <Label value="0" short="Off">Off</Label>
  </Param>
  <ControlChange controller="99" value="50"/>
  <ControlChange controller="98" value="3"/>
  <ControlChange controller="6"  value="0"/>
  <ControlChange controller="38" value="$state"/>
</OneShot>
```

For **confirmed edge-triggered** CCs (a press flips state), drop the param and use a
single non-deterministic press instead:

```xml
<OneShot id="module-wah" name="Toggle Wah" group="Modules" deterministic="false"
         description="Flips the Wah module (state depends on current device state).">
  <ControlChange controller="11" value="127"/>
</OneShot>
```

### Side effects to handle in the same change

1. **Reset Block / clean start.** Today the Reset Block emits each Hold's `Disengage`
   at t=0. One-Shots have no disengage, so a device no longer auto-starts "all off".
   Options: (a) rely on the definition's `Init` block to set a known state, or
   (b) keep emitting an explicit "off" for known toggles in the reset. Decide before
   migrating, or shows could start with stale effect states.
2. **`deterministic` warning.** One-Shot already supports `deterministic="false"`; the
   editor warns. Use it for edge-triggered CCs (Kemper plain stomps, unverified DNAfx).
3. **Project migration.** Existing `.rigpilot` files store `Hold` events with a
   `length`. If commands change type, old events referencing them need migration or a
   load-time shim. Bundled examples (`examples/*.rigpilot`) must be updated too.
4. **`short` labels on the timeline — already supported.** `eventLabel` (`types.ts:92`)
   appends each selected param's `short` (or short label text) to the command `short`,
   so a One-Shot `module-wah` with `state=On` renders as "Wah On" with no frontend
   change. The Inspector likewise renders a dropdown for labelled params
   (`Inspector.svelte:114`) and only falls back to a free `0–127` number when a param
   has no labels — so the On/Off pattern needs **no UI work**.

## Hardware verification still open (⚠)

- **DNAfx 10–20, 21–24, 26–27, 51:** value-based vs edge-triggered. Decides On/Off-param
  vs non-deterministic-toggle. (One cable test.)
- **Kemper `delay-hold` / `delay-infinity`:** confirm whether musicians want these as a
  drawn region (Keep Hold) or a one-shot.

## Proposed next steps

1. Agree the policy: *state toggles → One-Shot with On/Off label param; `Hold` reserved
   for freeze/region effects.* (This also resolves #4.)
2. Decide the Reset-Block strategy (item 1 above) — blocker for migration.
3. Rewrite the four definitions (keep ≤2 Kemper Holds), update `examples/*.rigpilot`,
   add the project-migration shim.
4. Verify the ⚠ DNAfx/Kemper items on hardware and set `deterministic` accordingly.
