// Types mirroring the Rust model (serde camelCase / kebab-case tags).

export const PPQN = 960;

/** Default automation export resolution (ms between CC steps); mirrors the Rust serde default. */
export const DEFAULT_AUTOMATION_RESOLUTION_MS = 10;

export type LabelInfo = { value: number; text: string; short: string | null };

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
};

export type DefinitionInfo = {
  id: string;
  manufacturer: string;
  model: string;
  description: string | null;
  defaultLatency: number;
  commands: CommandInfo[];
};

export type RpEvent = {
  kind: "one-shot" | "hold" | "automation";
  commandId: string;
  tick: number;
  length?: number;
  params?: Record<string, number>;
  breakpoints?: [number, number][];
  /** Automation only: min ms between exported CC steps (defaults to DEFAULT_AUTOMATION_RESOLUTION_MS). */
  resolutionMs?: number;
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

export type Project = {
  name: string;
  bpm: number;
  timeSignature: [number, number];
  tracks: Track[];
};

// Command Type drives the event color (docs/terminology.md).
export const COMMAND_TYPE_COLORS: Record<RpEvent["kind"], string> = {
  "one-shot": "#ffb02e",
  hold: "#ff2e88",
  automation: "#2ee08a",
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
export const LANE_H = 72;
export const AUDIO_ROW_H = 72;
/** Slim strip under the last used lane — the drop target that grows a track by one lane.
 *  A full empty lane there used to read as a gap of blank space under the last clip. */
export const SPARE_LANE_H = 20;

/** Lanes of a MIDI track that can hold events: up to the deepest event, at least one. */
export function midiLanes(t: MidiTrack): number {
  return Math.max(1, ...t.events.map((e) => e.lane + 1));
}

export function trackHeight(t: Track): number {
  return t.type === "audio" ? AUDIO_ROW_H : midiLanes(t) * LANE_H + SPARE_LANE_H + 6;
}

/** Lane a y offset *inside* a MIDI track points at; the spare strip maps to a new
 *  lane (index `midiLanes(t)`), which is how dropping below the last lane grows a track. */
export function laneAt(t: MidiTrack, yInTrack: number): number {
  return Math.min(Math.max(0, Math.floor((yInTrack - 2) / LANE_H)), midiLanes(t));
}

export function trackTops(tracks: Track[]): number[] {
  const tops: number[] = [];
  let y = RULER_H;
  for (const t of tracks) {
    tops.push(y);
    y += trackHeight(t);
  }
  return tops;
}

/** Ticks of one bar (PPQN is per quarter note). */
export function barTicks(timeSignature: [number, number]): number {
  return Math.round(timeSignature[0] * (4 / timeSignature[1]) * PPQN);
}

export type EventRef = { ti: number; ei: number };

export const secondsPerBeat = (bpm: number) => 60 / bpm;
export const tickToSeconds = (tick: number, bpm: number) => (tick / PPQN) * secondsPerBeat(bpm);
export const secondsToTick = (sec: number, bpm: number) => Math.round((sec / secondsPerBeat(bpm)) * PPQN);
