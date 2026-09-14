// Automation curve maths and geometry.
//
// `valueAt` and `ease` mirror `app/src-tauri/src/project.rs` exactly — the Rust
// side is what Export emits, so the two must agree value for value. The Rust
// tests pin the numbers; change one side and you must change the other.

import {
  DEFAULT_AUTOMATION_RESOLUTION_MS,
  snapToSteps,
  type AutomationCurve,
  type Breakpoint,
  type CommandInfo,
  type DefinitionInfo,
  type MidiTrack,
  type Shape,
} from "./types";

/**
 * Inset above and below the curve inside its lane. Only wide enough to keep the
 * 1.5px stroke off the lane edges — the value scale otherwise spans the full lane,
 * so a breakpoint at the top of the range sits at the top of the lane.
 */
export const AUTO_PAD = 1;

/**
 * Samples per `curve` segment when drawing. The chord error of a sampled function
 * scales with (vertical extent)/N², so it depends on the lane height, never on
 * horizontal zoom: at the tallest lane (`AUTO_LANE_MAX_H`) and the sharpest
 * tension, 64 samples keep the chords roughly a tenth of a pixel off the curve.
 */
const CURVE_SAMPLES = 64;

/** Power ease for shape "curve". Closed form in u, so a value can be read at any tick. */
export function ease(u: number, k: number): number {
  const t = Math.min(1, Math.max(-1, k));
  const p = 1 + 3 * Math.abs(t);
  return t >= 0 ? Math.pow(u, p) : 1 - Math.pow(1 - u, p);
}

/**
 * Drawing value: continuous. Rounding here would stair-step every bent segment by
 * one MIDI value — truer to the stream that gets exported, but it reads as a
 * jagged line rather than a curve.
 */
const lerpExact = (v0: number, v1: number, u: number) => v0 + (v1 - v0) * u;

/** Exported value: 7-bit and integral, all a CC message can carry. */
const lerp = (v0: number, v1: number, u: number) =>
  Math.min(127, Math.max(0, Math.round(lerpExact(v0, v1, u))));

/**
 * Value of a curve at an absolute tick, or null when it has no breakpoints.
 * Once a curve has any point it has a value everywhere: the first value is held
 * back to the song start, the last one to the end. `forced` overrides every
 * segment's stored shape, for targets the definition restricts to one curve type.
 */
export function valueAt(bps: Breakpoint[], tick: number, forced: Shape | null): number | null {
  if (bps.length === 0) return null;
  const first = bps[0];
  const last = bps[bps.length - 1];
  if (tick <= first.tick) return first.value;
  if (tick >= last.tick) return last.value;

  let lo = 0;
  let hi = bps.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (bps[mid].tick <= tick) lo = mid;
    else hi = mid - 1;
  }
  const a = bps[lo];
  const b = bps[lo + 1];
  if (b.tick <= a.tick) return b.value;
  const u = (tick - a.tick) / (b.tick - a.tick);
  // The shape belongs to the point the segment arrives at (FL-Studio style).
  const shape: Shape = forced ?? b.shape;
  if (shape === "hold") return a.value;
  if (shape === "curve") return lerp(a.value, b.value, ease(u, b.tension));
  return lerp(a.value, b.value, u);
}

// ---- commands & curves ----

export const isStepped = (cmd: CommandInfo) => cmd.steps.length > 0;

const ALL_SHAPES: Shape[] = ["linear", "curve", "hold"];

/** Curve types the definition lets the user pick for this command. */
export function allowedShapes(cmd: CommandInfo): Shape[] {
  const picked = (cmd.shapes ?? []).filter((s) => ALL_SHAPES.includes(s));
  return picked.length ? picked : ALL_SHAPES;
}

/**
 * The shape every segment must take when there is no choice: a discrete value set
 * can only jump, and a definition may allow just one type.
 */
export function forcedShape(cmd: CommandInfo): Shape | null {
  if (isStepped(cmd)) return "hold";
  const allowed = allowedShapes(cmd);
  return allowed.length === 1 ? allowed[0] : null;
}

/** Declared value range of an Automation target (full 7-bit when unspecified). */
export function rangeOf(cmd: CommandInfo): [number, number] {
  return cmd.range ?? [0, 127];
}

export function automationCommands(def: DefinitionInfo | undefined): CommandInfo[] {
  return def?.commands.filter((c) => c.commandType === "automation") ?? [];
}

export function findCurve(t: MidiTrack, commandId: string): AutomationCurve | undefined {
  return t.automation?.find((c) => c.commandId === commandId);
}

export function newCurve(commandId: string): AutomationCurve {
  return {
    commandId,
    enabled: true,
    resolutionMs: DEFAULT_AUTOMATION_RESOLUTION_MS,
    resendMs: 0,
    breakpoints: [],
  };
}

/** The track's curve for a command, created on the spot if it has none yet. */
export function curveFor(t: MidiTrack, commandId: string): AutomationCurve {
  if (!t.automation) t.automation = [];
  let curve = findCurve(t, commandId);
  if (!curve) {
    curve = newCurve(commandId);
    t.automation.push(curve);
  }
  return curve;
}

/** "In use" = has at least one breakpoint. Settings alone don't count. */
export function hasPoints(t: MidiTrack, commandId: string): boolean {
  return (findCurve(t, commandId)?.breakpoints.length ?? 0) > 0;
}

export function curvesInUse(t: MidiTrack): AutomationCurve[] {
  return (t.automation ?? []).filter((c) => c.breakpoints.length > 0);
}

/** Keeps a value inside its target's range, snapped to the steps if it has any. */
export function clampValue(cmd: CommandInfo, value: number): number {
  const [min, max] = rangeOf(cmd);
  const clamped = Math.min(max, Math.max(min, Math.round(value)));
  return isStepped(cmd) ? snapToSteps(cmd.steps, clamped) : clamped;
}

// ---- relative conversion (paste between different Commands) ----

export type ValueSpace = { range: [number, number]; steps: number[] };

export function valueSpace(cmd: CommandInfo): ValueSpace {
  return {
    range: rangeOf(cmd),
    steps: cmd.steps.map((s) => s.value).sort((a, b) => a - b),
  };
}

/**
 * Carries a value across Commands by position within the range: 50 % of the
 * source range lands at 50 % of the target's, and a stepped target snaps to the
 * nearest step.
 */
export function convertValue(value: number, src: ValueSpace, dst: ValueSpace): number {
  let r: number;
  if (src.steps.length > 1) {
    const i = src.steps.indexOf(value);
    r = i >= 0 ? i / (src.steps.length - 1) : relativeIn(value, src.range);
  } else if (src.steps.length === 1) {
    r = 0;
  } else {
    r = relativeIn(value, src.range);
  }
  r = Math.min(1, Math.max(0, r));
  if (dst.steps.length > 0) {
    return dst.steps[Math.round(r * (dst.steps.length - 1))];
  }
  const [min, max] = dst.range;
  return Math.round(min + r * (max - min));
}

function relativeIn(value: number, [min, max]: [number, number]): number {
  return max > min ? (value - min) / (max - min) : 0;
}

// ---- drawing ----

/**
 * Turns a tension drag into a stored tension: the user drags the segment so its
 * midpoint sits at value `m`.
 */
export function tensionFromMidpoint(v0: number, v1: number, m: number): number {
  if (v1 === v0) return 0;
  const r = Math.min(0.98, Math.max(0.02, (m - v0) / (v1 - v0)));
  if (r <= 0.5) {
    const p = Math.log(r) / Math.log(0.5);
    return Math.min(1, Math.max(0, (p - 1) / 3));
  }
  const p = Math.log(1 - r) / Math.log(0.5);
  return -Math.min(1, Math.max(0, (p - 1) / 3));
}

export type CurvePaths = {
  /** The curve between the first and last breakpoint. */
  drawn: string;
  /** Filled region under `drawn`, empty for a single-point curve. */
  area: string;
  /** Dashed stretches where the value is held rather than drawn. */
  held: string;
};

/**
 * SVG path data for a curve in pixel space. `hold` and `linear` segments are
 * exact path commands; only `curve` segments are sampled.
 */
export function curvePaths(
  bps: Breakpoint[],
  xOf: (tick: number) => number,
  yOf: (value: number) => number,
  forced: Shape | null,
  width: number,
  bottom: number,
): CurvePaths {
  if (bps.length === 0) return { drawn: "", area: "", held: "" };

  const first = bps[0];
  const last = bps[bps.length - 1];
  const x0 = xOf(first.tick);
  const xLast = xOf(last.tick);

  const d: string[] = [`M ${x0},${yOf(first.value)}`];
  for (let i = 0; i < bps.length - 1; i++) {
    const a = bps[i];
    const b = bps[i + 1];
    const xb = xOf(b.tick);
    const yb = yOf(b.value);
    const shape: Shape = forced ?? b.shape;
    if (shape === "hold") {
      d.push(`H ${xb}`, `V ${yb}`);
    } else if (shape === "linear" || b.tick <= a.tick) {
      d.push(`L ${xb},${yb}`);
    } else {
      for (let s = 1; s <= CURVE_SAMPLES; s++) {
        const u = s / CURVE_SAMPLES;
        const tick = a.tick + (b.tick - a.tick) * u;
        d.push(`L ${xOf(tick)},${yOf(lerpExact(a.value, b.value, ease(u, b.tension)))}`);
      }
    }
  }
  const drawn = d.join(" ");

  const held: string[] = [];
  if (x0 > 0) held.push(`M 0,${yOf(first.value)} H ${x0}`);
  if (xLast < width) held.push(`M ${xLast},${yOf(last.value)} H ${width}`);

  return {
    drawn,
    area: bps.length > 1 ? `${drawn} L ${xLast},${bottom} L ${x0},${bottom} Z` : "",
    held: held.join(" "),
  };
}
