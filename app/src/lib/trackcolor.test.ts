import { describe, expect, it } from "vitest";
import { AUDIO_TRACK_COLOR, TRACK_COLORS, hashName, trackColor } from "./trackcolor";

describe("trackColor", () => {
  it("is stable for the same name", () => {
    expect(trackColor("Guitar")).toBe(trackColor("Guitar"));
  });

  it("ignores case and surrounding whitespace", () => {
    expect(trackColor("  guitar ")).toBe(trackColor("Guitar"));
  });

  it("does not depend on track order", () => {
    const before = ["Drums", "Bass", "Guitar"].map((n) => trackColor(n));
    const after = ["Guitar", "Drums", "Bass"].map((n) => trackColor(n));
    expect(after[0]).toBe(before[2]);
    expect(after[1]).toBe(before[0]);
  });

  it("always returns a colour from the palette", () => {
    for (const name of ["", "a", "Lead Synth", "🎸", "Track 12"]) {
      expect(TRACK_COLORS).toContain(trackColor(name));
    }
  });

  it("spreads short names over the palette", () => {
    const used = new Set(["Drums", "Bass", "Guitar", "Keys", "Vocals", "Pads", "FX"].map((n) => trackColor(n)));
    expect(used.size).toBeGreaterThan(2);
  });

  it("keeps audio references neutral", () => {
    expect(trackColor("Guitar", "audio")).toBe(AUDIO_TRACK_COLOR);
    expect(trackColor("Guitar", "midi")).not.toBe(AUDIO_TRACK_COLOR);
  });

  it("hashes to an unsigned 32-bit integer", () => {
    const h = hashName("Guitar");
    expect(Number.isInteger(h)).toBe(true);
    expect(h).toBeGreaterThanOrEqual(0);
    expect(h).toBeLessThan(2 ** 32);
  });
});
