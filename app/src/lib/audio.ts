// Web Audio playback engine for reference audio tracks.

import { convertFileSrc } from "@tauri-apps/api/core";

export type LoadedAudio = {
  buffer: AudioBuffer;
  /** min/max pairs per bucket, PEAK_BUCKETS_PER_SECOND buckets/s, mixed channels */
  peaks: Float32Array;
  /** low/mid/high band magnitudes per bucket (3 floats per bucket, 0..1) */
  bandPeaks: Float32Array;
  peaksPerSecond: number;
};

export const PEAK_BUCKETS_PER_SECOND = 1000;

const ctx = new AudioContext();

export async function loadAudio(absPath: string): Promise<LoadedAudio> {
  const res = await fetch(convertFileSrc(absPath));
  if (!res.ok) throw new Error(`could not read audio file: ${res.status}`);
  const buffer = await ctx.decodeAudioData(await res.arrayBuffer());
  return {
    buffer,
    peaks: computePeaks(buffer),
    bandPeaks: await computeBandPeaks(buffer),
    peaksPerSecond: PEAK_BUCKETS_PER_SECOND,
  };
}

/**
 * DJ-style spectral coloring: render the file through three biquads
 * (low ≤200 Hz, mid ~200–2k, high ≥2 kHz) offline and keep one peak
 * magnitude per band per display bucket.
 */
async function computeBandPeaks(buffer: AudioBuffer): Promise<Float32Array> {
  const buckets = Math.ceil(buffer.duration * PEAK_BUCKETS_PER_SECOND);
  const out = new Float32Array(buckets * 3);
  const bands: { type: BiquadFilterType; freq: number; q?: number }[] = [
    { type: "lowpass", freq: 200 },
    { type: "bandpass", freq: 632, q: 0.35 }, // geometric center of 200 Hz–2 kHz
    { type: "highpass", freq: 2000 },
  ];
  const samplesPerBucket = buffer.sampleRate / PEAK_BUCKETS_PER_SECOND;
  for (let b = 0; b < bands.length; b++) {
    const off = new OfflineAudioContext(1, buffer.length, buffer.sampleRate);
    const src = off.createBufferSource();
    src.buffer = buffer;
    const filter = off.createBiquadFilter();
    filter.type = bands[b].type;
    filter.frequency.value = bands[b].freq;
    if (bands[b].q) filter.Q.value = bands[b].q!;
    src.connect(filter).connect(off.destination);
    src.start();
    const rendered = await off.startRendering();
    const data = rendered.getChannelData(0);
    for (let i = 0; i < buckets; i++) {
      let m = 0;
      const start = Math.floor(i * samplesPerBucket);
      const end = Math.min(Math.floor((i + 1) * samplesPerBucket), data.length);
      for (let j = start; j < end; j += 4) {
        const v = Math.abs(data[j]);
        if (v > m) m = v;
      }
      out[i * 3 + b] = m;
    }
  }
  return out;
}

function computePeaks(buffer: AudioBuffer): Float32Array {
  const buckets = Math.ceil(buffer.duration * PEAK_BUCKETS_PER_SECOND);
  const peaks = new Float32Array(buckets * 2);
  const samplesPerBucket = buffer.sampleRate / PEAK_BUCKETS_PER_SECOND;
  const channels = Array.from({ length: buffer.numberOfChannels }, (_, c) =>
    buffer.getChannelData(c),
  );
  for (let b = 0; b < buckets; b++) {
    let min = 0, max = 0;
    const start = Math.floor(b * samplesPerBucket);
    const end = Math.min(Math.floor((b + 1) * samplesPerBucket), buffer.length);
    for (const data of channels) {
      for (let i = start; i < end; i += 2) {
        const v = data[i];
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
    peaks[b * 2] = min;
    peaks[b * 2 + 1] = max;
  }
  return peaks;
}

type PlayingSource = {
  source: AudioBufferSourceNode;
  gain: GainNode;
  pan: StereoPannerNode;
};

export type TrackPlayback = {
  audio: LoadedAudio;
  /** timeline second at which the audio file starts */
  offsetSeconds: number;
  volume: number;
  pan: number;
  audible: boolean; // mute/solo already resolved by the caller
};

export class Player {
  private playing: PlayingSource[] = [];
  private startedAt = 0; // ctx.currentTime when playback started
  private startPos = 0; // timeline seconds at start
  private _isPlaying = false;

  get isPlaying() {
    return this._isPlaying;
  }

  /** Current playhead position in timeline seconds. */
  position(whenStopped: number): number {
    return this._isPlaying ? this.startPos + (ctx.currentTime - this.startedAt) : whenStopped;
  }

  async play(tracks: TrackPlayback[], fromSeconds: number) {
    this.stop();
    await ctx.resume();
    this.startedAt = ctx.currentTime + 0.05; // small scheduling headroom
    this.startPos = fromSeconds;
    for (const t of tracks) {
      const source = ctx.createBufferSource();
      source.buffer = t.audio.buffer;
      const gain = ctx.createGain();
      gain.gain.value = t.audible ? t.volume : 0;
      const pan = ctx.createStereoPanner();
      pan.pan.value = t.pan;
      source.connect(pan).connect(gain).connect(ctx.destination);

      const intoBuffer = fromSeconds - t.offsetSeconds;
      if (intoBuffer >= t.audio.buffer.duration) continue;
      if (intoBuffer >= 0) {
        source.start(this.startedAt, intoBuffer);
      } else {
        source.start(this.startedAt - intoBuffer, 0);
      }
      this.playing.push({ source, gain, pan });
    }
    this._isPlaying = true;
  }

  /** Applies volume/pan/mute changes while playing (same track order as play()). */
  update(tracks: TrackPlayback[]) {
    for (let i = 0; i < Math.min(tracks.length, this.playing.length); i++) {
      this.playing[i].gain.gain.value = tracks[i].audible ? tracks[i].volume : 0;
      this.playing[i].pan.pan.value = tracks[i].pan;
    }
  }

  stop(): number {
    const pos = this.position(this.startPos);
    for (const p of this.playing) {
      try {
        p.source.stop();
      } catch {
        // already ended
      }
      p.source.disconnect();
    }
    this.playing = [];
    this._isPlaying = false;
    return pos;
  }
}
