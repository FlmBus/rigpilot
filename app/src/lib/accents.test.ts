import { describe, expect, it } from "vitest";
import {
  DEFAULT_ACCENT_OPTIONS,
  bandFlux,
  detectAccents,
  type AccentSource,
} from "./accents";

const FPS = 1000; // buckets per second, as audio.ts produces

type Hit = { at: number; amp: number; mix?: [number, number, number] };

/** Synthetic band peaks: decaying hits over a quiet noise floor. */
function makeSource(seconds: number, hits: Hit[], floor = 0.02): AccentSource {
  const buckets = Math.round(seconds * FPS);
  const bandPeaks = new Float32Array(buckets * 3);
  for (let i = 0; i < buckets; i++) {
    // Deterministic ripple instead of random, so the test cannot flake.
    const n = floor * (1 + 0.2 * Math.sin(i / 37));
    for (let b = 0; b < 3; b++) bandPeaks[i * 3 + b] = n;
  }
  for (const h of hits) {
    const mix = h.mix ?? [1, 0.8, 0.6];
    const start = Math.round(h.at * FPS);
    for (let i = start; i < Math.min(buckets, start + 250); i++) {
      const decay = Math.exp(-(i - start) / 45);
      for (let b = 0; b < 3; b++) {
        const v = h.amp * mix[b] * decay;
        if (v > bandPeaks[i * 3 + b]) bandPeaks[i * 3 + b] = v;
      }
    }
  }
  return { bandPeaks, peaksPerSecond: FPS };
}

const nearest = (times: number[], t: number) =>
  times.reduce((best, x) => (Math.abs(x - t) < Math.abs(best - t) ? x : best), Infinity);

describe("bandFlux", () => {
  it("peaks at the hit, not between hits", () => {
    const src = makeSource(4, [{ at: 1, amp: 0.8 }, { at: 3, amp: 0.8 }]);
    const { flux, fps } = bandFlux(src, DEFAULT_ACCENT_OPTIONS);
    const at = (t: number) => flux[Math.round(t * fps)];
    expect(at(1)).toBeGreaterThan(at(2));
    expect(at(3)).toBeGreaterThan(at(2));
  });

  it("is empty-safe", () => {
    expect(detectAccents({ bandPeaks: new Float32Array(0), peaksPerSecond: FPS })).toEqual([]);
  });
});

describe("detectAccents", () => {
  it("finds isolated hits", () => {
    const hits = [1, 3, 5, 7].map((at) => ({ at, amp: 0.9 }));
    const found = detectAccents(makeSource(9, hits)).map((a) => a.time);
    for (const h of hits) expect(Math.abs(nearest(found, h.at) - h.at)).toBeLessThan(0.06);
  });

  it("scores a hit that breaks the pattern above the steady pattern", () => {
    // Steady eighths with one much louder hit in the middle.
    const hits: Hit[] = [];
    for (let i = 0; i < 32; i++) hits.push({ at: 0.5 + i * 0.25, amp: i === 16 ? 1 : 0.35 });
    const accents = detectAccents(makeSource(10, hits));
    const accent = accents.reduce((a, b) => (a.salience >= b.salience ? a : b));
    expect(Math.abs(accent.time - (0.5 + 16 * 0.25))).toBeLessThan(0.08);
  });

  it("marks far fewer events than there are transients", () => {
    const hits: Hit[] = [];
    for (let i = 0; i < 80; i++) hits.push({ at: 0.5 + i * 0.25, amp: 0.4 });
    const accents = detectAccents(makeSource(24, hits));
    expect(accents.length).toBeGreaterThan(0);
    expect(accents.length).toBeLessThan(hits.length / 2);
  });

  it("keeps markers at least the suppression window apart", () => {
    const hits: Hit[] = [];
    for (let i = 0; i < 40; i++) hits.push({ at: 1 + i * 0.05, amp: 0.3 + (i % 5) * 0.12 });
    const accents = detectAccents(makeSource(8, hits));
    for (let i = 1; i < accents.length; i++) {
      expect(accents[i].time - accents[i - 1].time).toBeGreaterThanOrEqual(
        DEFAULT_ACCENT_OPTIONS.minSeparationMs / 1000 - 1e-9,
      );
    }
  });

  it("spreads markers over sections instead of piling them on the loud part", () => {
    // Quiet first half, loud second half: a global ranking would ignore the intro.
    const hits: Hit[] = [];
    for (let i = 0; i < 20; i++) hits.push({ at: 0.5 + i * 0.5, amp: i < 10 ? 0.25 : 0.95 });
    const accents = detectAccents(makeSource(12, hits), [5]);
    expect(accents.some((a) => a.time < 5)).toBe(true);
    expect(accents.some((a) => a.time >= 5)).toBe(true);
  });

  it("returns more markers at higher sensitivity", () => {
    const hits: Hit[] = [];
    for (let i = 0; i < 48; i++) hits.push({ at: 0.4 + i * 0.2, amp: 0.3 + ((i * 7) % 5) * 0.1 });
    const src = makeSource(12, hits);
    const low = detectAccents(src, [], { sensitivity: 0.1 });
    const high = detectAccents(src, [], { sensitivity: 1 });
    expect(high.length).toBeGreaterThan(low.length);
  });

  it("is deterministic and sorted in time", () => {
    const src = makeSource(8, [1, 2.3, 4, 6.5].map((at) => ({ at, amp: 0.7 })));
    const a = detectAccents(src);
    const b = detectAccents(src);
    expect(a).toEqual(b);
    for (let i = 1; i < a.length; i++) expect(a[i].time).toBeGreaterThan(a[i - 1].time);
  });

  it("reports every feature in 0..1", () => {
    const hits: Hit[] = [];
    for (let i = 0; i < 24; i++) hits.push({ at: 0.5 + i * 0.3, amp: i === 12 ? 1 : 0.4 });
    for (const a of detectAccents(makeSource(9, hits))) {
      expect(a.salience).toBeGreaterThanOrEqual(0);
      expect(a.salience).toBeLessThanOrEqual(1);
      for (const v of Object.values(a.features)) {
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThanOrEqual(1);
      }
    }
  });

  it("stays fast enough for a full-length track", () => {
    // Five minutes at 1000 buckets/s: the detector runs on the UI thread when
    // the toggle flips, so a running median per frame would be unusable.
    const hits: Hit[] = [];
    for (let i = 0; i < 1200; i++) hits.push({ at: 0.5 + i * 0.25, amp: 0.3 + ((i * 5) % 6) * 0.1 });
    const src = makeSource(300, hits);
    const started = performance.now();
    const accents = detectAccents(src);
    expect(performance.now() - started).toBeLessThan(3000);
    expect(accents.length).toBeGreaterThan(0);
  });

  it("honours the per-section cap", () => {
    const hits: Hit[] = [];
    for (let i = 0; i < 120; i++) hits.push({ at: 0.3 + i * 0.15, amp: 0.3 + ((i * 3) % 7) * 0.1 });
    const accents = detectAccents(makeSource(20, hits), [], { maxPerSection: 2, sensitivity: 1 });
    const perSection = new Map<number, number>();
    for (const a of accents) {
      const s = Math.floor(a.time / DEFAULT_ACCENT_OPTIONS.fallbackSectionSeconds);
      perSection.set(s, (perSection.get(s) ?? 0) + 1);
    }
    for (const n of perSection.values()) expect(n).toBeLessThanOrEqual(2);
  });
});
