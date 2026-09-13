# Design sandbox

Two standalone HTML files, no build step — open them in a browser.

| File | What it is |
| --- | --- |
| **`reboot.html`** | **Start here.** Three complete visual languages on the current layout, `feat/automation-lanes` included. |
| `mockup.html` | Earlier study: today's "Embedded Console" look vs. a straight de-skeuomorphised version of it. Kept as the *before* reference — note it predates `feat/automation-lanes`, so it still shows automation as a clip. |

## reboot.html — three identities

Same layout (top bar · command palette · track headers + timeline · inspector · status
line), same markup tree, three committed visual languages. Switch with the bar along the
bottom.

### Studio — light · neutral · utilitarian

Paper canvas, ink type, colour only where it carries data. The action accent is **ink, not a
hue** — buttons and the play control are near-black, so the only saturated things on screen are
the commands themselves. Clips are tinted pills with a hairline border. Roomy density (60px
rows, 28px controls). This is the "credible professional tool" read — closest to Linear/Vercel.

### Graphite — dark · cool · quiet

Cool neutral greys, hairline structure, one desaturated blue accent. Clips are solid blocks
with a 3px colour spine on the left edge and a label row — the block is the object, the spine
is its type, nothing glows. Tightest density (52px rows, 26px controls, 12.5px base) so more
song fits on screen. Native to a DAW workflow; zero decoration.

### Signal — dark · high-contrast · technical

Zero radius everywhere, mono structural type (uppercase, wide tracking), 2px key rules,
near-black `#08090a` canvas. Keeps **pink as the one signal colour** — so the brand survives —
but spends it on the playhead, the position readout and command accents rather than on chrome.
Clips are measurement blocks: flat wash, 2px colour rule on top, mono uppercase label.
One-shots are diamonds — timeline point and palette swatch alike. Reads like
instrumentation, not like glossy hardware.

### The Automation Lane

`feat/automation-lanes` changed the model, so all three identities render it the new way:
automation is **not a clip**. Each MIDI track owns one lane showing one curve at a time.

- The **selector** sits in the header column (`.autohd`) with the ON/OFF pill, the `⋯` curve
  menu and the resize grip — mirroring `AutoSelector.svelte`.
- The lane itself is a **recessed display**, not an object: sunken ground, horizontal value
  guides at quarters of the range over the same beat/bar grid the event lanes use, the curve as
  a 1.5px stroke over a 12 % area fill, and the stretches before the first and after the last
  breakpoint **dashed at 55 %** — held, not drawn.
- Breakpoints are 7px rings; the selected one fills with `--fg` and shows its value. A hollow
  handle at a segment's midpoint is the bend grip.
- Quad Cortex shows the **collapsed** state: a 26px strip, the selector on *No automation* with
  a badge counting curves that still have points, and 30 %-opacity miniatures behind it.
- In the palette, automation commands are no longer a drag source — they are click-to-show, with
  a mark on the ones that already have breakpoints.
- The inspector shows the new primary editing surface: a **breakpoint** (bar · beat · value ·
  curve type · bend), not an automation clip.

One structural rule makes this hold up: **every horizontal separator is a `border-top` on the box
beneath it, in the header column and the timeline alike.** With `border-box` sizing, N event lanes
are then exactly `N × --lane` tall and the automation lane exactly `--auto`, so the two columns
cannot drift — which is the same invariant `trackLayout()` enforces in `types.ts`.

### What changed versus today, in all three

- **No imitated depth.** No specular rims, recessed wells, tonal gradients, LED bloom or
  backdrop-blur. Structure is carried by hairlines and spacing.
- **The fake LCD is gone.** Position is large mono type set directly on the bar; BPM and time
  signature are borderless scrub fields with micro-labels under them.
- **One-shots read as points, not stems.** A one-shot is an instant, so it is a dot plus its
  name — no vertical stem across the lane. Far less visual noise at typical event density.
- **Automation is the curve.** No framed graph inside a glass block; the curve *is* the clip.
- **Selection is one gesture** (ring or inset ring in the type colour), not a per-element
  invention.
- **Both themes are real.** Every identity ships a light and a dark palette.
- **Command-type colour is calmer.** Hold/one-shot/automation keep distinct hues but at
  professional saturation, and they are the only hues in Studio and Graphite.

## Layout changes in the mockup (and why)

These depart from the shipped layout on purpose. Each is reversible — they are separate blocks
in the markup.

1. **Snap · grid mode · zoom moved out of the top bar** into a 34px **timeline toolbar** above
   the ruler. They are timeline-scoped, not app-scoped; in the shipped layout they sit in the
   far top-right corner, ~900px from the thing they affect, and they are a big part of why that
   one bar carries ~15 controls. The toolbar also hosts **Follow** (keep the playhead in view)
   and **Loop**.
2. **A section strip** under the ruler — named regions (Intro, Verse 1, Chorus 1, Bridge,
   Outro) with a matching `Sections` cell in the header column. Rationale below.
3. **The command palette is split into two labelled sections** — *Drag onto timeline* for
   One-Shot/Hold, *Automate — click to open* for Automation. Since `feat/automation-lanes`
   these are two different verbs, and a type dot was the only thing distinguishing them.
4. **Ghost curves in the expanded automation lane** — the other in-use curves on that track,
   dashed at 16 %. The selector shows one curve at a time, so otherwise "what else is
   automated here?" is invisible until you open the dropdown.
5. **Event resize grips removed.** The edge hit-zone stays (7px, `cursor: ew-resize`); the drawn
   grips only added chrome to every selected clip. It's pulled out by each side's own
   border-width so it starts flush with the visual edge — inset it only to the padding box and
   the border pixels themselves fall back to the `grab` cursor, which reads as a bug.

## UX ideas

Ordered by value for "bands that run timed live shows". The first is built into the mockup; the
rest are proposals, not implemented.

**1. Named sections — built.** Bands do not say "bar 33", they say "second chorus". A
`sections: {tick, name}[]` on the project unlocks a lot downstream: the position readout can
read `Chorus 1 · bar 17`, Loop can loop a section, and export warnings can name where the
problem is.

**2. Section-aware duplicate.** *The* repetitive task in a song is that chorus 2 needs the same
rig moves as chorus 1. "Copy everything in Chorus 1 → Chorus 2" (events on every track, plus the
breakpoints in that tick range on every curve) would remove most of the busywork the product
exists to remove. Needs #1 first.

**3. A "what actually gets sent" view.** The pitch is MIDI that just works, but nothing in the
UI shows the resolved message stream. A toggleable list — time-ordered messages with latency
compensation applied, the reset block expanded, and warnings inline (two curves on the same CC,
a non-deterministic command, a Program Change inside a 10ms window) — is what builds enough
trust to walk on stage. Probably the highest-value non-visual feature here.

**4. Make latency compensation visible.** It silently shifts messages earlier at export and you
can never see it. A faint ghost marker at each event's true send time, behind a toggle, turns a
confusing setting into an observable one.

**5. Keyboard insertion.** Type-to-search, Enter places the command at the playhead on the
selected track. Faster than dragging for repeat work, and it makes the palette optional rather
than permanent furniture.

**6. An "all curves" mode for the automation lane.** One curve at a time is the right default,
but a mode that stacks every in-use curve in the lane (active one solid, rest faint) answers
"what am I automating in this song?" at a glance. The ghosts in #4 above are the cheap half of
this.

**7. Loop region + click-anywhere-on-ruler to seek.** Table stakes for rehearsing one part;
pairs with sections.

## The lab

| Control | Effect |
| --- | --- |
| Identity | Studio / Graphite / Signal — also resets the numeric scale to that identity's defaults |
| Theme | light / dark |
| Accent | recolours the action accent + focus ring |
| Radius · Row h · Auto h · Ctl h · Zoom · Text | the geometry scale |

### Two traps this file documents by having fallen into them

- **Never give a length a colour token's name.** `--auto` is the automation *colour*; naming the
  automation-lane height `--auto` too meant `[data-id][data-theme]` (two attributes) outranked
  the geometry block (one attribute), so every `height: var(--auto)` silently resolved to
  `#0f766e`, became invalid, and collapsed the lane into the rows below it. The lengths are
  `--lane-auto` / `--lane-auto-c`.
- **Separators must use the same rule in both columns.** Header-column `border-bottom` against
  timeline `border-top` puts the divider on adjacent-but-different pixels, drifting 1px per
  track. Everything below the ruler uses border-top-on-the-box-beneath.
| Copy tokens | puts a `:root { … }` block of your overrides on the clipboard |

## The contract that keeps this CSS-only

Everything below the token layer in `reboot.html` reads **only tokens** — not one colour,
shadow or gradient literal. An identity *is* its token block. That is the shape
`app/src/lib/theme.css` has to take for a redesign to be a CSS-only change:

- **Surfaces** `--canvas --panel --raised --sunken`
- **Lines** `--line --line-2 --grid-soft --grid-bar`
- **Text** `--fg --fg-2 --fg-3`
- **Action** `--accent --accent-fg --accent-soft --focus`
- **Data** `--hold --shot --auto --wave --warn --danger --tint`
- **Depth** `--shadow --shadow-sm`
- **Scale** `--r --r-sm --clip-r --h --bar-h --lane --auto --auto-c --beat --pal-w --head-w
  --insp-w`
- **Type** `--ui --mono --fs --fs-micro --micro-f --micro-ls --micro-tt --micro-w
  --title-w --title-ls --name-w --border-key`

Note `--tint`: clips are built with `color-mix(in srgb, var(--c) var(--tint), var(--canvas))`,
so one number retunes every clip fill for a light or dark ground. Same trick for borders and
labels. No per-clip colour is ever written twice.

## Known blocker for "CSS files only"

The app is not there yet.

1. **Literals in component `<style>` blocks** — roughly 27 in `routes/+page.svelte`, 27 in
   `lib/Timeline.svelte`, 5–6 each in `Palette`/`Inspector`/`Modal`/`Num`: `#000` borders,
   `rgba(255,255,255,.0x)` rims, the `linear-gradient(180deg,#141419,#0e0e12)` panel fill.
   These have to become token references.
2. **`Timeline.svelte` paints on a canvas** — ruler, grid and waveform are drawn with
   `ctx.fillStyle = "#0e0e11"` and friends, which CSS cannot reach at all. Fix: read the tokens
   once per repaint via `getComputedStyle(document.documentElement)`.
3. **Clip construction is markup, not just paint.** Today's tall clips have a `.clip-head`
   title bar and one-shots have a `.os-stem`; Studio and Signal drop both. So `Timeline.svelte`
   needs its clip markup simplified once, after which the look is pure CSS.
   The automation lane is already close — `.autolane`, `.node`, `.seg-handle` and `.autoghost`
   map almost one-to-one onto the mockup's `.autolane`, `.bp`, `.sh` and `.autoghost`.
4. **`COMMAND_TYPE_COLORS` in `types.ts`** hard-codes the three type hues in JS. They belong in
   the token layer, read from CSS.

So adoption is: pick an identity → tokenise (steps 1–4) → then every later design change really
is only `theme.css`.
