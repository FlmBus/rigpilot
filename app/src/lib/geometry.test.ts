import { describe, expect, it } from "vitest";
import {
  AUDIO_ROW_H,
  LANE_H,
  RULER_H,
  SPARE_LANE_H,
  laneAt,
  midiLanes,
  trackHeight,
  trackTops,
  type AudioTrack,
  type MidiTrack,
  type RpEvent,
} from "./types";

const ev = (lane: number): RpEvent => ({
  kind: "automation",
  commandId: "volume",
  tick: 0,
  length: 960,
  lane,
  breakpoints: [
    [0, 0],
    [960, 127],
  ],
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

describe("midiLanes", () => {
  it("keeps one lane for an empty track", () => {
    expect(midiLanes(midi())).toBe(1);
  });

  it("counts up to the deepest used lane, without a trailing empty one", () => {
    expect(midiLanes(midi(0))).toBe(1);
    expect(midiLanes(midi(0, 3))).toBe(4);
  });
});

describe("trackHeight", () => {
  it("is the used lanes plus the slim spare strip", () => {
    expect(trackHeight(midi(0))).toBe(LANE_H + SPARE_LANE_H + 6);
    expect(trackHeight(midi(0, 1))).toBe(2 * LANE_H + SPARE_LANE_H + 6);
  });

  it("leaves no blank lane under the last clip (issue #39)", () => {
    const t = midi(0);
    const lastClipBottom = 2 + (midiLanes(t) - 1) * LANE_H + (LANE_H - 3);
    expect(trackHeight(t) - lastClipBottom).toBeLessThan(LANE_H);
  });

  it("is unchanged for audio tracks", () => {
    expect(trackHeight(audio)).toBe(AUDIO_ROW_H);
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
    expect(laneAt(t, trackHeight(t) - 10)).toBe(1);
    expect(laneAt(t, trackHeight(t) - 1)).toBe(1);
  });

  it("never returns a lane outside the track", () => {
    const t = midi(0, 1);
    expect(laneAt(t, -50)).toBe(0);
    expect(laneAt(t, 10_000)).toBe(midiLanes(t));
  });

  it("stays inside the rows trackTops lays out", () => {
    const tracks = [audio, midi(0), midi(0, 2)];
    const tops = trackTops(tracks);
    expect(tops[0]).toBe(RULER_H);
    tracks.forEach((t, i) => {
      const next = i + 1 < tops.length ? tops[i + 1] : tops[i] + trackHeight(t);
      expect(tops[i] + trackHeight(t)).toBe(next);
    });
  });
});
