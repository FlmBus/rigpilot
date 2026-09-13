// Types mirroring the Rust model (serde camelCase / kebab-case tags).

export const PPQN = 960;

/** Project file format. Older files are refused, never converted. */
export const PROJECT_FORMAT_VERSION = 2;

/** Default automation export resolution (ms between CC steps); mirrors the Rust serde default. */
export const DEFAULT_AUTOMATION_RESOLUTION_MS = 10;

export type LabelInfo = { value: number; text: string; short: string | null };

/** Shape of the segment *arriving at* a breakpoint (FL-Studio style). */
export type Shape = "linear" | "curve" | "hold";

export type ParamInfo = {
  id: string;
  name: string;
  min: number;
  max: number;
  default: number | null;
  labels: LabelInfo[];
};

export type CommandInfo = {
  id: string;
  name: string;
  short: string | null;
  latency: number;
  commandType: "one-shot" | "hold" | "automation";
  group: string | null;
  description: string | null;
  deterministic: boolean;
  params: ParamInfo[];
  /** Discrete value set for a stepped Automation (empty = continuous). */
  steps: LabelInfo[];
  /** Value range of an Automation target; null for other command types. */
  range: [number, number] | null;
  /** Curve types this Automation allows; empty for other command types.
   *  One entry = the definition leaves no choice. */
  shapes: Shape[];
};

export type DefinitionInfo = {
  id: string;
  manufacturer: string;
  model: string;
  description: string | null;
  defaultLatency: number;
  commands: CommandInfo[];
  /** Problems with the definition itself (e.g. two Automations on one CC). */
  warnings: string[];
};

export type Breakpoint = {
  /** Absolute song tick. */
  tick: number;
  value: number;
  shape: Shape;
  /** "curve" only: -1..1, 0 = straight. */
  tension: number;
};

/** The value curve of one Automation Command. No breakpoints = not automated. */
export type AutomationCurve = {
  commandId: string;
  enabled: boolean;
  resolutionMs: number;
  /** Re-send the standing value every N ms (0 = off), so a dropped CC can't strand the device. */
  resendMs: number;
  /** Sorted by tick, strictly increasing. */
  breakpoints: Breakpoint[];
};

/** Which curve a track's Automation Lane shows, and how tall it is. View state. */
export type AutomationView = {
  /** null = the lane is collapsed. */
  command: string | null;
  height: number;
};

export type RpEvent = {
  kind: "one-shot" | "hold";
  commandId: string;
  tick: number;
  length?: number;
  params?: Record<string, number>;
  lane: number;
};

export type MidiTrack = {
  type: "midi";
  name: string;
  definitionId: string;
  midiChannel: number;
  latencyMs: number;
  mute: boolean;
  solo: boolean;
  events: RpEvent[];
  automation: AutomationCurve[];
  automationView: AutomationView;
};

export type AudioTrack = {
  type: "audio";
  name: string;
  file: string;
  volume: number;
  pan: number;
  mute: boolean;
  solo: boolean;
  offsetTicks: number;
  waveformGain: number;
};

export type Track = MidiTrack | AudioTrack;

export type Section = { tick: number; name: string };

export type Project = {
  formatVersion: number;
  name: string;
  bpm: number;
  timeSignature: [number, number];
  tracks: Track[];
  /** Named regions (Intro, Verse 1, …). Read-only display for now — no editor yet. */
  sections?: Section[];
};

// Command Type drives the event color (docs/terminology.md).
// Mirrors the --hold/--shot/--auto tokens in theme.css — this app ships one
// identity/theme, so it's simplest to keep these as a manual copy rather than
// reading CSS custom properties at runtime. If a theme switcher ever ships,
// read these from getComputedStyle(document.documentElement) instead.
export const COMMAND_TYPE_COLORS: Record<CommandInfo["commandType"], string> = {
  "one-shot": "#e0a44a",
  hold: "#8b85ff",
  automation: "#3fbfa8",
};

/** Compact timeline label: short name + short param value labels. */
export function eventLabel(cmd: CommandInfo, ev: RpEvent): string {
  const parts: string[] = [cmd.short ?? cmd.name];
  for (const p of cmd.params) {
    const v = ev.params?.[p.id];
    if (v === undefined) continue;
    const l = p.labels.find((x) => x.value === v);
    parts.push(l?.short ?? (l && l.text.length <= 8 ? l.text : String(v)));
  }
  return parts.join(" ");
}

/** Nearest allowed step value for a stepped automation; identity when continuous. */
export function snapToSteps(steps: LabelInfo[], value: number): number {
  if (!steps.length) return value;
  return steps.reduce(
    (best, s) => (Math.abs(s.value - value) < Math.abs(best - value) ? s.value : best),
    steps[0].value,
  );
}

/** Short label text for a discrete step value, or null if none matches. */
export function stepLabel(steps: LabelInfo[], value: number): string | null {
  const l = steps.find((s) => s.value === value);
  return l ? (l.short ?? l.text) : null;
}

// ---- timeline geometry shared by canvas and the header column ----
export const RULER_H = 28;
/** Named-sections strip, directly under the ruler. Always reserved, even with no sections yet. */
export const SECTIONS_H = 24;
export const LANE_H = 72;
export const AUDIO_ROW_H = 72;
/** Slim strip under the last used lane — the drop target that grows a track by one lane.
 *  A full empty lane there used to read as a gap of blank space under the last clip. */
export const SPARE_LANE_H = 20;

/** Automation Lane: header strip only, when the selector is on "None". */
export const AUTO_LANE_COLLAPSED_H = 26;
export const AUTO_LANE_MIN_H = 48;
export const AUTO_LANE_DEFAULT_H = 96;
export const AUTO_LANE_MAX_H = 320;

export const clampLaneHeight = (h: number) =>
  Math.min(AUTO_LANE_MAX_H, Math.max(AUTO_LANE_MIN_H, Math.round(h)));

/** Lanes of a MIDI track that can hold events: up to the deepest event, at least one. */
export function midiLanes(t: MidiTrack): number {
  return Math.max(1, ...t.events.map((e) => e.lane + 1));
}

/** Lane a y offset *inside* a MIDI track points at; the spare strip maps to a new
 *  lane (index `midiLanes(t)`), which is how dropping below the last lane grows a track. */
export function laneAt(t: MidiTrack, yInTrack: number): number {
  return Math.min(Math.max(0, Math.floor((yInTrack - 2) / LANE_H)), midiLanes(t));
}

export type TrackLayout = {
  /** Top of the whole track block. */
  top: number;
  /** Full height, Automation Lane included. */
  height: number;
  /** Top of the Automation Lane. */
  autoTop: number;
  /** 0 when the track has no Automation Lane at all. */
  autoHeight: number;
};

/** Height of a MIDI track's Automation Lane; 0 when its Device has no Automation Commands. */
export function autoLaneHeight(t: MidiTrack, def: DefinitionInfo | undefined): number {
  if (!def?.commands.some((c) => c.commandType === "automation")) return 0;
  return t.automationView?.command
    ? clampLaneHeight(t.automationView.height)
    : AUTO_LANE_COLLAPSED_H;
}

/**
 * Vertical layout of every track. The single source of truth for both the header
 * column and the Timeline — computing it twice is how the two drift apart.
 */
export function trackLayout(
  tracks: Track[],
  defs: Map<string, DefinitionInfo>,
): TrackLayout[] {
  const out: TrackLayout[] = [];
  let y = RULER_H + SECTIONS_H;
  for (const t of tracks) {
    if (t.type === "audio") {
      out.push({ top: y, height: AUDIO_ROW_H, autoTop: y + AUDIO_ROW_H, autoHeight: 0 });
      y += AUDIO_ROW_H;
      continue;
    }
    const eventsH = midiLanes(t) * LANE_H + SPARE_LANE_H + 6;
    const autoHeight = autoLaneHeight(t, defs.get(t.definitionId));
    out.push({ top: y, height: eventsH + autoHeight, autoTop: y + eventsH, autoHeight });
    y += eventsH + autoHeight;
  }
  return out;
}

/** Ticks of one bar (PPQN is per quarter note). */
export function barTicks(timeSignature: [number, number]): number {
  return Math.round(timeSignature[0] * (4 / timeSignature[1]) * PPQN);
}

export type EventRef = { ti: number; ei: number };

/** Selected breakpoints: always within one curve of one track. */
export type BpSelection = { ti: number; commandId: string; idx: number[] };

export const secondsPerBeat = (bpm: number) => 60 / bpm;
export const tickToSeconds = (tick: number, bpm: number) => (tick / PPQN) * secondsPerBeat(bpm);
export const secondsToTick = (sec: number, bpm: number) => Math.round((sec / secondsPerBeat(bpm)) * PPQN);
