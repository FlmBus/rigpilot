// Track accent colour. The model has no colour field and the issue asks for no
// manual picker, so the colour is derived from the track name: the same name
// always lands on the same hue, reordering tracks never reshuffles the column,
// and two tracks only clash when the hash collides.

/** Hues that read on the dark panel. Not theme tokens — these are data colours, like the command hues. */
export const TRACK_COLORS = [
  "#ff2e88",
  "#2ee08a",
  "#ffb02e",
  "#39d3e6",
  "#c084fc",
  "#fb7185",
  "#38bdf8",
];

/** Audio references stay neutral, so the coloured bars read as "this track sends MIDI". */
export const AUDIO_TRACK_COLOR = "#6b6b78";

/**
 * FNV-1a, 32-bit. Cheap, well spread over short strings, and — unlike anything
 * built in — pinned here, so a name keeps its colour across releases.
 */
export function hashName(name: string): number {
  let h = 0x811c9dc5;
  const s = name.trim().toLowerCase();
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Deterministic accent colour for a track name. */
export const trackColor = (name: string, type?: string): string =>
  type === "audio" ? AUDIO_TRACK_COLOR : TRACK_COLORS[hashName(name) % TRACK_COLORS.length];
