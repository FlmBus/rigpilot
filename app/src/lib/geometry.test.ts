import { describe, expect, it } from "vitest";
import {
  AUDIO_ROW_H,
  AUTO_LANE_DEFAULT_H,
  LANE_H,
  RULER_H,
  SECTIONS_H,
  SPARE_LANE_H,
  laneAt,
  resizeZoneWidth,
  midiLanes,
  trackLayout,
  type AudioTrack,
  type DefinitionInfo,
  type MidiTrack,
  type RpEvent,
  type Track,
} from "./types";

const ev = (lane: number): RpEvent => ({
  kind: "hold",
  commandId: "volume",
  tick: 0,
  length: 960,
  lane,
});

const midi = (...lanes: number[]): MidiTrack => ({
  type: "midi",
  name: "Helix",
  definitionId: "helix-floor",
  midiChannel: 1,
  latencyMs: 0,
  mute: false,
  solo: false,
  events: lanes.map(ev),
  automation: [],
  automationView: { command: null, height: AUTO_LANE_DEFAULT_H },
});

const audio: AudioTrack = {
  type: "audio",
  name: "Mix",
  file: "mix.wav",
  volume: 1,
  pan: 0,
  mute: false,
  solo: false,
  offsetTicks: 0,
  waveformGain: 1,
};

/** No Device Definitions — no track gets an Automation Lane, so heights are lanes only. */
const noDefs = new Map<string, DefinitionInfo>();

const heightOf = (t: Track) => trackLayout([t], noDefs)[0].height;

describe("midiLanes", () => {
  it("keeps one lane for an empty track", () => {
    expect(midiLanes(midi())).toBe(1);
  });

  it("counts up to the deepest used lane, without a trailing empty one", () => {
    expect(midiLanes(midi(0))).toBe(1);
    expect(midiLanes(midi(0, 3))).toBe(4);
  });
});

describe("track height", () => {
  it("is the used lanes plus the slim spare strip", () => {
    expect(heightOf(midi(0))).toBe(LANE_H + SPARE_LANE_H + 6);
    expect(heightOf(midi(0, 1))).toBe(2 * LANE_H + SPARE_LANE_H + 6);
  });

  it("leaves no blank lane under the last clip (issue #39)", () => {
    const t = midi(0);
    const lastClipBottom = 2 + (midiLanes(t) - 1) * LANE_H + (LANE_H - 3);
    expect(heightOf(t) - lastClipBottom).toBeLessThan(LANE_H);
  });

  it("is unchanged for audio tracks", () => {
    expect(heightOf(audio)).toBe(AUDIO_ROW_H);
  });
});

describe("laneAt", () => {
  it("maps a y offset to the lane rendered there", () => {
    const t = midi(0, 1, 2);
    expect(laneAt(t, 2)).toBe(0);
    expect(laneAt(t, 2 + LANE_H - 1)).toBe(0);
    expect(laneAt(t, 2 + LANE_H)).toBe(1);
    expect(laneAt(t, 2 + 2 * LANE_H)).toBe(2);
  });

  it("maps the spare strip to a new lane below the last one", () => {
    const t = midi(0);
    expect(laneAt(t, heightOf(t) - 10)).toBe(1);
    expect(laneAt(t, heightOf(t) - 1)).toBe(1);
  });

  it("never returns a lane outside the track", () => {
    const t = midi(0, 1);
    expect(laneAt(t, -50)).toBe(0);
    expect(laneAt(t, 10_000)).toBe(midiLanes(t));
  });
});

describe("trackLayout", () => {
  it("lays out rows without gaps, starting under the ruler and sections strip", () => {
    const tracks = [audio, midi(0), midi(0, 2)];
    const rows = trackLayout(tracks, noDefs);
    expect(rows[0].top).toBe(RULER_H + SECTIONS_H);
    rows.forEach((row, i) => {
      const next = i + 1 < rows.length ? rows[i + 1].top : row.top + row.height;
      expect(row.top + row.height).toBe(next);
    });
  });
});

describe("resizeZoneWidth", () => {
  it("gives roomy clips the full 5px edge", () => {
    expect(resizeZoneWidth(20)).toBe(5);
    expect(resizeZoneWidth(13)).toBe(5);
  });

  it("keeps short holds resizable instead of body-only", () => {
    // The old 16px gate made all of these unresizable.
    expect(resizeZoneWidth(11)).toBe(4);
    expect(resizeZoneWidth(9)).toBe(3);
    expect(resizeZoneWidth(5)).toBe(1);
  });

  it("always leaves a body zone to drag the clip by", () => {
    for (let w = 3; w <= 40; w++) {
      expect(w - 2 * resizeZoneWidth(w)).toBeGreaterThanOrEqual(3);
    }
  });

  it("falls back to body-only when the clip is down to the render floor", () => {
    expect(resizeZoneWidth(3)).toBe(0);
    expect(resizeZoneWidth(0)).toBe(0);
  });
});
