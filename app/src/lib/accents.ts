// Musically significant hits and accents (issue #57).
//
// Not a transient detector: every snare in a steady backbeat is a transient,
// and marking all of them is noise. A hit matters when it stands out *from its
// own musical context* — louder than what surrounds it, spectrally different
// from what came before, off the established grid, or breaking a pattern the
// listener has already learned. That is what the salience score below measures.
//
// The pipeline is deterministic signal processing, no ML, and every threshold
// is a field of `AccentOptions` so marker density and sensitivity can be tuned
// later. Defaults lean generous: per the issue discussion, a few extra markers
// are cheaper for the user than a missed fill ending.
//
// Input is the band peak data the waveform already computes (see audio.ts), so
// detection costs one pass over an existing array instead of a second decode.

/** Everything the detector needs from a loaded audio file. */
export type AccentSource = {
  /** low/mid/high magnitudes, 3 floats per bucket. */
  bandPeaks: Float32Array;
  peaksPerSecond: number;
};

/** Why a hit scored the way it did. Kept on the result for tuning and tooltips. */
export type AccentFeatures = {
  /** Onset strength against the local flux floor. */
  prominence: number;
  /** How much the low/mid/high balance changes across the hit. */
  novelty: number;
  /** Loudness step from before the hit to after it. */
  energy: number;
  /** How far off the locally established pulse the hit lands. */
  deviation: number;
  /** How unlike the previous comparable hit this one is (breaks repetition). */
  surprise: number;
};

export type Accent = {
  /** Seconds from the start of the audio file. */
  time: number;
  /** 0..1 musical salience. */
  salience: number;
  features: AccentFeatures;
};

export type AccentOptions = {
  /**
   * 0..1 master control. Higher lowers the peak-picking threshold and raises
   * the per-section budget, so one knob spans "only the drops" to "every fill
   * note". Defaults high on purpose.
   */
  sensitivity: number;
  /** Analysis frame length. 10 ms ≈ 100 fps, fine enough for drum placement. */
  frameMs: number;
  /** Relative weight of the low/mid/high onset envelopes in the summed flux. */
  bandWeights: [number, number, number];
  /** Compression applied to band magnitudes before differencing; higher = flatter dynamics. */
  logCompression: number;
  /** Flux smoothing window; removes single-frame jitter without blurring hits. */
  smoothingMs: number;
  /** Window used for the local flux median that the threshold rides on. */
  medianWindowMs: number;
  /** Flux must exceed `median * thresholdRatio` to become a candidate. */
  thresholdRatio: number;
  /**
   * Absolute guard: flux must also reach this fraction of the file's loud
   * onsets (95th percentile). Without it, room tone in a silent passage has a
   * high local *ratio* and every breath becomes a marker.
   */
  noiseFloorRatio: number;
  /** Two accepted hits are never closer than this (temporal non-max suppression). */
  minSeparationMs: number;
  /** Windows for the novelty (short) and energy (long) context features. */
  contextShortMs: number;
  contextLongMs: number;
  /** Feature weights of the salience score. Need not sum to 1. */
  weights: AccentFeatures;
  /** Hits below this salience are dropped even if the section has budget left. */
  minSalience: number;
  /** Marker budget per second of section, before the sensitivity scaling. */
  markersPerSecond: number;
  /** Hard cap per section, so a dense section cannot flood the ruler. */
  maxPerSection: number;
  /** Section length used when the caller passes no section boundaries. */
  fallbackSectionSeconds: number;
};

export const DEFAULT_ACCENT_OPTIONS: AccentOptions = {
  sensitivity: 0.7,
  frameMs: 10,
  bandWeights: [1, 1.2, 1],
  logCompression: 40,
  smoothingMs: 30,
  medianWindowMs: 1500,
  thresholdRatio: 1.4,
  noiseFloorRatio: 0.12,
  minSeparationMs: 90,
  contextShortMs: 80,
  contextLongMs: 400,
  weights: { prominence: 1, novelty: 0.8, energy: 1, deviation: 0.9, surprise: 1.2 },
  minSalience: 0.18,
  markersPerSecond: 0.5,
  maxPerSection: 24,
  fallbackSectionSeconds: 8,
};

/** Maps 0..∞ onto 0..1 with no cliff, so no single feature can dominate. */
const squash = (x: number): number => (x <= 0 ? 0 : x / (1 + x));

/** Value below which `p` of the sorted samples fall. */
const percentile = (values: Float32Array, p: number): number => {
  const s = Array.from(values).sort((a, b) => a - b);
  if (s.length === 0) return 0;
  return s[Math.min(s.length - 1, Math.max(0, Math.round(p * (s.length - 1))))];
};

const median = (values: number[]): number => {
  if (values.length === 0) return 0;
  const s = [...values].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

/**
 * Stage 1: multi-band onset envelopes.
 *
 * Band magnitudes are log-compressed first — a hit over a loud passage and the
 * same hit over a quiet one then produce a comparable rise, which is what
 * "stands out locally" should mean.
 */
export function bandFlux(src: AccentSource, opts: AccentOptions): { flux: Float32Array; bands: Float32Array; frames: number; fps: number } {
  const buckets = Math.floor(src.bandPeaks.length / 3);
  const bucketsPerFrame = Math.max(1, Math.round((opts.frameMs / 1000) * src.peaksPerSecond));
  const frames = Math.max(1, Math.ceil(buckets / bucketsPerFrame));
  const fps = src.peaksPerSecond / bucketsPerFrame;

  // Frame magnitudes: peak within the frame, per band.
  const bands = new Float32Array(frames * 3);
  for (let f = 0; f < frames; f++) {
    const start = f * bucketsPerFrame;
    const end = Math.min(start + bucketsPerFrame, buckets);
    for (let i = start; i < end; i++) {
      for (let b = 0; b < 3; b++) {
        const v = src.bandPeaks[i * 3 + b];
        if (v > bands[f * 3 + b]) bands[f * 3 + b] = v;
      }
    }
  }

  // Per-band half-wave rectified difference of the compressed envelope,
  // normalised by that band's own maximum rise so no band drowns the others.
  const perBand = new Float32Array(frames * 3);
  const maxRise = [1e-6, 1e-6, 1e-6];
  for (let f = 1; f < frames; f++) {
    for (let b = 0; b < 3; b++) {
      const prev = Math.log1p(opts.logCompression * bands[(f - 1) * 3 + b]);
      const cur = Math.log1p(opts.logCompression * bands[f * 3 + b]);
      const d = Math.max(0, cur - prev);
      perBand[f * 3 + b] = d;
      if (d > maxRise[b]) maxRise[b] = d;
    }
  }

  const raw = new Float32Array(frames);
  const wsum = opts.bandWeights.reduce((a, b) => a + b, 0) || 1;
  for (let f = 0; f < frames; f++) {
    let acc = 0;
    for (let b = 0; b < 3; b++) acc += opts.bandWeights[b] * (perBand[f * 3 + b] / maxRise[b]);
    raw[f] = acc / wsum;
  }

  // Stage 1b: smooth, so one hit is one bump rather than a small cluster.
  const half = Math.max(0, Math.round((opts.smoothingMs / 1000) * fps) >> 1);
  const flux = new Float32Array(frames);
  for (let f = 0; f < frames; f++) {
    let sum = 0, n = 0;
    for (let k = f - half; k <= f + half; k++) {
      if (k < 0 || k >= frames) continue;
      sum += raw[k];
      n++;
    }
    flux[f] = n ? sum / n : 0;
  }
  return { flux, bands, frames, fps };
}

/**
 * Local flux floor, evaluated on a coarse grid.
 *
 * A true running median over a 1.5 s window costs a sort per frame, which on a
 * five-minute file is tens of thousands of sorts and a visibly stalled UI. The
 * floor moves far slower than the frame rate, so it is sampled every
 * `blockFrames` and read back with a step function — same decisions, a
 * thousandth of the work.
 */
function floorTrack(flux: Float32Array, halfWindow: number, blockFrames: number): { floors: Float32Array; blockFrames: number } {
  const frames = flux.length;
  const blocks = Math.max(1, Math.ceil(frames / blockFrames));
  const floors = new Float32Array(blocks);
  const stride = Math.max(1, Math.round(halfWindow / 200)); // ≤ ~400 samples per window
  for (let b = 0; b < blocks; b++) {
    const centre = b * blockFrames + (blockFrames >> 1);
    const window: number[] = [];
    for (let k = Math.max(0, centre - halfWindow); k < Math.min(frames, centre + halfWindow); k += stride) {
      window.push(flux[k]);
    }
    floors[b] = median(window);
  }
  return { floors, blockFrames };
}

const floorAt = (t: { floors: Float32Array; blockFrames: number }, f: number): number =>
  t.floors[Math.min(t.floors.length - 1, Math.floor(f / t.blockFrames))];

/**
 * Stage 2: peak picking against an adaptive floor.
 *
 * The threshold rides on a local median rather than a fixed level, so a quiet
 * intro and a dense chorus are judged by their own standards.
 */
function pickCandidates(flux: Float32Array, fps: number, opts: AccentOptions): number[] {
  const frames = flux.length;
  const medHalf = Math.max(1, Math.round((opts.medianWindowMs / 1000) * fps) >> 1);
  const sepFrames = Math.max(1, Math.round((opts.minSeparationMs / 1000) * fps));
  // Sensitivity relaxes the ratio: 0 → strict (2x the ratio above the floor), 1 → permissive.
  const ratio = 1 + (opts.thresholdRatio - 1) * (2 - 1.5 * clamp01(opts.sensitivity));
  // Sensitivity relaxes the absolute guard the same way, down to a third of it.
  const hard = percentile(flux, 0.95) * opts.noiseFloorRatio * (1.5 - clamp01(opts.sensitivity));
  const track = floorTrack(flux, medHalf, Math.max(1, Math.round(0.25 * fps)));
  const out: number[] = [];
  for (let f = 1; f < frames - 1; f++) {
    const v = flux[f];
    if (v <= 0) continue;
    // Local maximum within the suppression window.
    let isMax = true;
    for (let k = Math.max(0, f - sepFrames); k <= Math.min(frames - 1, f + sepFrames); k++) {
      if (flux[k] > v || (flux[k] === v && k < f)) { isMax = false; break; }
    }
    if (!isMax) continue;
    if (v < Math.max(floorAt(track, f) * ratio, hard, 1e-4)) continue;
    out.push(f);
  }
  return out;
}

const clamp01 = (x: number): number => (x < 0 ? 0 : x > 1 ? 1 : x);

/** Mean band magnitudes over [from, to) frames. */
function bandMean(bands: Float32Array, frames: number, from: number, to: number): [number, number, number] {
  const a = Math.max(0, from), b = Math.min(frames, to);
  const out: [number, number, number] = [0, 0, 0];
  if (b <= a) return out;
  for (let f = a; f < b; f++) for (let i = 0; i < 3; i++) out[i] += bands[f * 3 + i];
  for (let i = 0; i < 3; i++) out[i] /= b - a;
  return out;
}

const total = (v: [number, number, number]): number => v[0] + v[1] + v[2];

/** L1 distance between two band balances (each normalised to sum 1), 0..1. */
function profileDistance(a: [number, number, number], b: [number, number, number]): number {
  const ta = total(a), tb = total(b);
  if (ta < 1e-6 || tb < 1e-6) return ta < 1e-6 && tb < 1e-6 ? 0 : 1;
  let d = 0;
  for (let i = 0; i < 3; i++) d += Math.abs(a[i] / ta - b[i] / tb);
  return Math.min(1, d / 2);
}

/**
 * Stage 3: contextual features, stage 4: the combined salience score.
 */
function scoreCandidates(
  candidates: number[],
  flux: Float32Array,
  bands: Float32Array,
  frames: number,
  fps: number,
  opts: AccentOptions,
): Accent[] {
  const shortF = Math.max(1, Math.round((opts.contextShortMs / 1000) * fps));
  const longF = Math.max(1, Math.round((opts.contextLongMs / 1000) * fps));
  const wsum = Object.values(opts.weights).reduce((a, b) => a + b, 0) || 1;

  // Inter-onset intervals drive both rhythmic features. The local median IOI is
  // the pulse the listener has learned; a hit that lands far off it is news.
  const iois = candidates.map((f, i) => (i === 0 ? 0 : f - candidates[i - 1]));

  return candidates.map((f, i) => {
    // Prominence is measured against the neighbouring *hits*, not against the
    // silence between them: over a busy passage every onset towers over the
    // floor, and only "louder than the hits around it" separates an accent
    // from the pattern carrying it.
    const peers = candidates
      .slice(Math.max(0, i - 4), Math.min(candidates.length, i + 5))
      .filter((c) => c !== f)
      .map((c) => flux[c]);
    const peer = Math.max(median(peers), 1e-5);
    const prominence = squash((flux[f] - peer) / peer);

    const before = bandMean(bands, frames, f - shortF, f);
    const after = bandMean(bands, frames, f, f + shortF);
    const novelty = profileDistance(before, after);

    // A hit at the very start has nothing before it; claiming a loudness step
    // there would hand the file's first frame the top score.
    const haveBefore = f - shortF > 0;
    const quietBefore = total(bandMean(bands, frames, f - longF, f - shortF));
    const loudAfter = total(bandMean(bands, frames, f, f + longF));
    const energy = haveBefore
      ? squash((loudAfter - quietBefore) / Math.max(quietBefore, 1e-3))
      : 0;

    // Rhythmic deviation: distance of this IOI from the local pulse.
    const nearby = iois.slice(Math.max(1, i - 4), Math.min(iois.length, i + 5)).filter((v) => v > 0);
    const pulse = median(nearby);
    const ioi = iois[i];
    const deviation = pulse > 0 && ioi > 0 ? squash(Math.abs(ioi - pulse) / pulse) : 0;

    // Repetition: compare with the previous hit. A snare that repeats the
    // previous one in strength and colour is expected, so it scores low; one
    // that is markedly stronger or differently coloured breaks the pattern.
    // Nothing has repeated yet at the first hit, so there is no pattern for it
    // to break; it earns its markers through energy instead.
    let surprise = 0.25;
    if (i > 0) {
      const p = candidates[i - 1];
      const strengthJump = squash(Math.max(0, flux[f] - flux[p]) / Math.max(flux[p], 1e-5));
      const colour = profileDistance(
        bandMean(bands, frames, p, p + shortF),
        bandMean(bands, frames, f, f + shortF),
      );
      surprise = clamp01(0.6 * strengthJump + 0.4 * colour);
    }

    const features: AccentFeatures = { prominence, novelty, energy, deviation, surprise };
    const w = opts.weights;
    const salience = clamp01(
      (prominence * w.prominence +
        novelty * w.novelty +
        energy * w.energy +
        deviation * w.deviation +
        surprise * w.surprise) /
        wsum,
    );
    return { time: f / fps, salience, features };
  });
}

/**
 * Stage 5: temporal non-max suppression. Greedy by salience, so when a fill and
 * its ringing tail both peak, the marker lands on the stronger of the two.
 */
function suppress(accents: Accent[], minSeparationMs: number): Accent[] {
  const kept: Accent[] = [];
  const gap = minSeparationMs / 1000;
  for (const a of [...accents].sort((x, y) => y.salience - x.salience || x.time - y.time)) {
    if (kept.some((k) => Math.abs(k.time - a.time) < gap)) continue;
    kept.push(a);
  }
  return kept.sort((x, y) => x.time - y.time);
}

/**
 * Stage 6: sparse selection per musical section.
 *
 * Selecting globally would spend every marker on the loudest chorus and leave
 * the intro and the breakdown bare, so each section gets its own budget and
 * its own ranking.
 */
function selectPerSection(
  accents: Accent[],
  boundaries: number[],
  duration: number,
  opts: AccentOptions,
): Accent[] {
  const edges = [0, ...boundaries.filter((b) => b > 0 && b < duration).sort((a, b) => a - b), duration];
  const out: Accent[] = [];
  // Sensitivity scales the budget between half and double the configured rate.
  const rate = opts.markersPerSecond * (0.5 + 1.5 * clamp01(opts.sensitivity));
  // Sensitivity also relaxes the salience gate, so turning it up actually
  // yields more markers instead of raising a budget nothing can fill.
  const minSalience = opts.minSalience * (1.5 - clamp01(opts.sensitivity));
  for (let i = 0; i < edges.length - 1; i++) {
    const from = edges[i], to = edges[i + 1];
    const inSection = accents.filter((a) => a.time >= from && a.time < to && a.salience >= minSalience);
    const budget = Math.min(opts.maxPerSection, Math.max(1, Math.round((to - from) * rate)));
    out.push(...[...inSection].sort((a, b) => b.salience - a.salience).slice(0, budget));
  }
  return out.sort((a, b) => a.time - b.time);
}

/**
 * Detects musically significant hits in a loaded audio file.
 *
 * @param sectionBoundaries Section starts in seconds from the file's own start.
 *   Empty falls back to fixed-length windows, which still beats a global
 *   ranking at keeping markers spread over the whole file.
 */
export function detectAccents(
  src: AccentSource,
  sectionBoundaries: number[] = [],
  options: Partial<AccentOptions> = {},
): Accent[] {
  const opts: AccentOptions = {
    ...DEFAULT_ACCENT_OPTIONS,
    ...options,
    weights: { ...DEFAULT_ACCENT_OPTIONS.weights, ...(options.weights ?? {}) },
  };
  if (src.bandPeaks.length < 6 || src.peaksPerSecond <= 0) return [];

  const { flux, bands, frames, fps } = bandFlux(src, opts);
  const duration = frames / fps;
  const scored = scoreCandidates(pickCandidates(flux, fps, opts), flux, bands, frames, fps, opts);
  const kept = suppress(scored, opts.minSeparationMs);
  const edges = sectionBoundaries.length
    ? sectionBoundaries
    : Array.from(
        { length: Math.max(0, Math.ceil(duration / opts.fallbackSectionSeconds) - 1) },
        (_, i) => (i + 1) * opts.fallbackSectionSeconds,
      );
  return selectPerSection(kept, edges, duration, opts);
}
