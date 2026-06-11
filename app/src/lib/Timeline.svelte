<script lang="ts">
  import type { LoadedAudio } from "./audio";
  import { dragPayload } from "./Palette.svelte";
  import {
    AUDIO_ROW_H,
    COMMAND_TYPE_COLORS,
    LANE_H,
    PPQN,
    RULER_H,
    barTicks,
    midiLanes,
    secondsPerBeat,
    secondsToTick,
    tickToSeconds,
    trackHeight,
    trackTops,
    eventLabel,
    type CommandInfo,
    type DefinitionInfo,
    type EventRef,
    type Project,
    type RpEvent,
  } from "./types";

  let {
    project,
    definitions,
    audio,
    playhead,
    gridMode,
    pxPerSecond,
    snapTicks,
    selection,
    coloredWaves,
    onseek,
    onzoom,
    onselectionchange,
    oncommit,
    oncreate,
    onstatus,
  }: {
    project: Project;
    definitions: DefinitionInfo[];
    audio: (LoadedAudio | null)[];
    playhead: number;
    gridMode: "musical" | "time";
    pxPerSecond: number;
    snapTicks: number | null;
    selection: EventRef[];
    coloredWaves: boolean;
    onseek: (seconds: number) => void;
    onzoom: (pxPerSecond: number) => void;
    onselectionchange: (refs: EventRef[]) => void;
    oncommit: () => void;
    oncreate: (ti: number, commandId: string, tick: number, lane: number) => void;
    onstatus: (msg: string) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let scroller = $state<HTMLDivElement>();

  // ---------- geometry ----------
  const tops = $derived(trackTops(project.tracks));
  const duration = $derived.by(() => {
    let end = 30;
    project.tracks.forEach((t, i) => {
      if (t.type === "audio") {
        const a = audio[i];
        if (a) end = Math.max(end, tickToSeconds(t.offsetTicks, project.bpm) + a.buffer.duration);
      } else {
        for (const ev of t.events)
          end = Math.max(end, tickToSeconds(ev.tick + (ev.length ?? 0), project.bpm));
      }
    });
    return end + 10;
  });
  const widthPx = $derived(Math.ceil(duration * pxPerSecond));
  const heightPx = $derived(
    RULER_H + project.tracks.reduce((s, t) => s + trackHeight(t), 0) + 1,
  );

  const xOf = (tick: number) => tickToSeconds(tick, project.bpm) * pxPerSecond;
  const tickAt = (x: number) => Math.max(0, secondsToTick(x / pxPerSecond, project.bpm));
  const snap = (tick: number) =>
    snapTicks ? Math.max(0, Math.round(tick / snapTicks) * snapTicks) : tick;

  const commandsById = $derived.by(() => {
    const m = new Map<string, CommandInfo>();
    for (const d of definitions)
      for (const c of d.commands) m.set(`${d.id}/${c.id}`, c);
    return m;
  });

  function eventRect(ti: number, ev: RpEvent) {
    let x = xOf(ev.tick);
    let w: number;
    if (ev.kind === "one-shot") {
      // diamond centered on the dispatch tick
      w = 19;
      x -= w / 2;
    } else {
      w = Math.max(9, xOf(ev.tick + (ev.length ?? 0)) - x);
    }
    const y = tops[ti] + 2 + ev.lane * LANE_H;
    return { x, y, w, h: LANE_H - 3 };
  }

  type Hit = { ref: EventRef; zone: "body" | "left" | "right" };
  function hitTest(x: number, y: number): Hit | null {
    for (let ti = project.tracks.length - 1; ti >= 0; ti--) {
      const t = project.tracks[ti];
      if (t.type !== "midi") continue;
      if (y < tops[ti] || y >= tops[ti] + trackHeight(t)) continue;
      for (let ei = t.events.length - 1; ei >= 0; ei--) {
        const ev = t.events[ei];
        const r = eventRect(ti, ev);
        if (x < r.x - 2 || x > r.x + r.w + 2 || y < r.y || y > r.y + r.h) continue;
        let zone: Hit["zone"] = "body";
        if (ev.kind !== "one-shot" && r.w > 16) {
          if (x <= r.x + 5) zone = "left";
          else if (x >= r.x + r.w - 5) zone = "right";
        }
        return { ref: { ti, ei }, zone };
      }
    }
    return null;
  }

  function breakpointAt(x: number, y: number): { ref: EventRef; index: number } | null {
    for (let ti = project.tracks.length - 1; ti >= 0; ti--) {
      const tr = project.tracks[ti];
      if (tr.type !== "midi") continue;
      for (let ei = tr.events.length - 1; ei >= 0; ei--) {
        const ev = tr.events[ei];
        if (ev.kind !== "automation" || !ev.breakpoints || !ev.length) continue;
        const r = eventRect(ti, ev);
        for (let i = 0; i < ev.breakpoints.length; i++) {
          const [bt, bv] = ev.breakpoints[i];
          const bx = r.x + (bt / ev.length) * r.w;
          const by = r.y + r.h - 2 - (bv / 127) * (r.h - 4);
          if (Math.abs(x - bx) <= 5 && Math.abs(y - by) <= 5) return { ref: { ti, ei }, index: i };
        }
      }
    }
    return null;
  }

  function trackAt(y: number): number | null {
    for (let ti = 0; ti < project.tracks.length; ti++) {
      if (y >= tops[ti] && y < tops[ti] + trackHeight(project.tracks[ti])) return ti;
    }
    return null;
  }

  const isSelected = (ti: number, ei: number) =>
    selection.some((r) => r.ti === ti && r.ei === ei);

  // ---------- interaction state ----------
  type Drag =
    | { mode: "seek" }
    | {
        mode: "move";
        refs: EventRef[];
        orig: { tick: number; lane: number }[];
        anchor: { tick: number };
        startX: number;
        startY: number;
        moved: boolean;
      }
    | {
        mode: "resize";
        edge: "left" | "right";
        ref: EventRef;
        origTick: number;
        origLen: number;
        origBps: [number, number][] | null;
        startX: number;
        moved: boolean;
      }
    | { mode: "marquee"; x0: number; y0: number; x1: number; y1: number; base: EventRef[] }
    | { mode: "breakpoint"; ref: EventRef; index: number; moved: boolean };

  let drag: Drag | null = null;
  let marquee = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let dropGhost = $state<{ ti: number; lane: number; tick: number } | null>(null);
  let hoverCursor = $state("default");

  function canvasPos(e: { clientX: number; clientY: number }) {
    const rect = canvas!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || !canvas) return;
    canvas.setPointerCapture(e.pointerId);
    const { x, y } = canvasPos(e);

    if (y < RULER_H) {
      drag = { mode: "seek" };
      onseek(tickToSeconds(snap(tickAt(x)), project.bpm));
      return;
    }

    const bp = breakpointAt(x, y);
    if (bp) {
      onselectionchange([bp.ref]);
      drag = { mode: "breakpoint", ref: bp.ref, index: bp.index, moved: false };
      return;
    }

    const hit = hitTest(x, y);
    if (hit) {
      const { ref, zone } = hit;
      let next: EventRef[];
      if (e.ctrlKey || e.metaKey) {
        next = isSelected(ref.ti, ref.ei)
          ? selection.filter((r) => !(r.ti === ref.ti && r.ei === ref.ei))
          : [...selection, ref];
      } else if (e.shiftKey) {
        next = isSelected(ref.ti, ref.ei) ? selection : [...selection, ref];
      } else {
        next = isSelected(ref.ti, ref.ei) ? selection : [ref];
      }
      onselectionchange(next);

      const track = project.tracks[ref.ti];
      if (track.type !== "midi") return;
      const ev = track.events[ref.ei];
      if (zone !== "body" && ev.kind !== "one-shot") {
        drag = {
          mode: "resize",
          edge: zone,
          ref,
          origTick: ev.tick,
          origLen: ev.length ?? 0,
          origBps: ev.kind === "automation" && ev.breakpoints ? ev.breakpoints.map((b) => [...b]) : null,
          startX: x,
          moved: false,
        };
      } else if (next.some((r) => r.ti === ref.ti && r.ei === ref.ei)) {
        const refs = next;
        drag = {
          mode: "move",
          refs,
          orig: refs.map((r) => {
            const t = project.tracks[r.ti];
            const e2 = t.type === "midi" ? t.events[r.ei] : null;
            return { tick: e2?.tick ?? 0, lane: e2?.lane ?? 0 };
          }),
          anchor: { tick: ev.tick },
          startX: x,
          startY: y,
          moved: false,
        };
      }
      return;
    }

    // empty area → marquee (modifiers keep the current selection as base)
    const base = e.ctrlKey || e.metaKey || e.shiftKey ? [...selection] : [];
    if (base.length === 0) onselectionchange([]);
    drag = { mode: "marquee", x0: x, y0: y, x1: x, y1: y, base };
  }

  function onpointermove(e: PointerEvent) {
    if (!canvas) return;
    const { x, y } = canvasPos(e);

    if (!drag) {
      if (breakpointAt(x, y)) {
        hoverCursor = "crosshair";
        return;
      }
      const hit = hitTest(x, y);
      hoverCursor = !hit
        ? y < RULER_H
          ? "text"
          : "default"
        : hit.zone === "body"
          ? "move"
          : "ew-resize";
      return;
    }

    if (drag.mode === "seek") {
      onseek(tickToSeconds(snap(tickAt(x)), project.bpm));
      return;
    }

    if (drag.mode === "move") {
      const dxTicks = tickAt(x) - tickAt(drag.startX);
      const snapped = snap(drag.anchor.tick + dxTicks) - drag.anchor.tick;
      const dLane = Math.round((y - drag.startY) / LANE_H);
      if (!drag.moved && (snapped !== 0 || dLane !== 0)) {
        oncommit();
        drag.moved = true;
      }
      if (!drag.moved) return;
      const d = drag;
      d.refs.forEach((r, i) => {
        const t = project.tracks[r.ti];
        if (t.type !== "midi") return;
        const ev = t.events[r.ei];
        ev.tick = Math.max(0, d.orig[i].tick + snapped);
        ev.lane = Math.max(0, d.orig[i].lane + dLane);
      });
      return;
    }

    if (drag.mode === "resize") {
      const t = project.tracks[drag.ref.ti];
      if (t.type !== "midi") return;
      const ev = t.events[drag.ref.ei];
      const dTicks = tickAt(x) - tickAt(drag.startX);
      if (!drag.moved && dTicks !== 0) {
        oncommit();
        drag.moved = true;
      }
      if (!drag.moved) return;
      const minLen = PPQN / 8;
      if (drag.edge === "right") {
        const end = snap(drag.origTick + drag.origLen + dTicks);
        ev.length = Math.max(minLen, end - drag.origTick);
      } else {
        const start = Math.min(snap(drag.origTick + dTicks), drag.origTick + drag.origLen - minLen);
        ev.tick = Math.max(0, start);
        ev.length = drag.origTick + drag.origLen - ev.tick;
      }
      if (ev.kind === "automation" && ev.breakpoints && drag.origBps && drag.origLen > 0) {
        const f = (ev.length ?? 0) / drag.origLen;
        ev.breakpoints = drag.origBps.map(([t0, v]) => [Math.round(t0 * f), v]);
      }
      return;
    }

    if (drag.mode === "breakpoint") {
      const tr = project.tracks[drag.ref.ti];
      if (tr.type !== "midi") return;
      const ev = tr.events[drag.ref.ei];
      if (ev.kind !== "automation" || !ev.breakpoints || !ev.length) return;
      if (!drag.moved) {
        oncommit();
        drag.moved = true;
      }
      const r = eventRect(drag.ref.ti, ev);
      const i = drag.index;
      const value = Math.max(0, Math.min(127, Math.round(((r.y + r.h - 2 - y) / (r.h - 4)) * 127)));
      ev.breakpoints[i][1] = value;
      // endpoints keep their time; inner points move between their neighbors
      if (i > 0 && i < ev.breakpoints.length - 1) {
        const raw = Math.max(0, Math.min(ev.length, tickAt(x) - ev.tick));
        const lo = ev.breakpoints[i - 1][0] + 1;
        const hi = ev.breakpoints[i + 1][0] - 1;
        ev.breakpoints[i][0] = Math.max(lo, Math.min(hi, snapTicks ? snap(ev.tick + raw) - ev.tick : raw));
      }
      return;
    }

    if (drag.mode === "marquee") {
      drag.x1 = x;
      drag.y1 = y;
      marquee = { x0: drag.x0, y0: drag.y0, x1: x, y1: y };
      const [mx0, mx1] = [Math.min(drag.x0, x), Math.max(drag.x0, x)];
      const [my0, my1] = [Math.min(drag.y0, y), Math.max(drag.y0, y)];
      const picked: EventRef[] = [...drag.base];
      project.tracks.forEach((t, ti) => {
        if (t.type !== "midi") return;
        t.events.forEach((ev, ei) => {
          const r = eventRect(ti, ev);
          const inside = r.x < mx1 && r.x + r.w > mx0 && r.y < my1 && r.y + r.h > my0;
          if (inside && !picked.some((p) => p.ti === ti && p.ei === ei)) picked.push({ ti, ei });
        });
      });
      onselectionchange(picked);
    }
  }

  function onpointerup(e: PointerEvent) {
    if (drag?.mode === "marquee") {
      const dx = Math.abs(drag.x1 - drag.x0);
      const dy = Math.abs(drag.y1 - drag.y0);
      if (dx < 3 && dy < 3) {
        // plain click on empty space: move the playhead
        onseek(tickToSeconds(snap(tickAt(drag.x0)), project.bpm));
      }
    }
    drag = null;
    marquee = null;
  }

  // ---------- palette drop ----------
  function ondragover(e: DragEvent) {
    if (!e.dataTransfer?.types.includes("application/x-rigpilot-command")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    const { x, y } = canvasPos(e);
    const ti = trackAt(y);
    if (ti === null || project.tracks[ti].type !== "midi") {
      dropGhost = null;
      return;
    }
    const payload = dragPayload.current;
    if (payload && project.tracks[ti].definitionId !== payload.definitionId) {
      dropGhost = null;
      return;
    }
    const lane = Math.min(
      Math.max(0, Math.floor((y - tops[ti] - 2) / LANE_H)),
      midiLanes(project.tracks[ti]) - 1,
    );
    dropGhost = { ti, lane, tick: snap(tickAt(x)) };
  }

  function ondrop(e: DragEvent) {
    const raw = e.dataTransfer?.getData("application/x-rigpilot-command");
    dropGhost = null;
    if (!raw) return;
    e.preventDefault();
    const { definitionId, commandId } = JSON.parse(raw);
    const { x, y } = canvasPos(e);
    const ti = trackAt(y);
    if (ti === null) return;
    const track = project.tracks[ti];
    if (track.type !== "midi") return;
    if (track.definitionId !== definitionId) {
      onstatus("This command belongs to a different Device Definition.");
      return;
    }
    const lane = Math.min(Math.max(0, Math.floor((y - tops[ti] - 2) / LANE_H)), midiLanes(track) - 1);
    oncreate(ti, commandId, snap(tickAt(x)), lane);
  }

  // ---------- rendering ----------
  $effect(() => {
    draw();
  });

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== widthPx * dpr || canvas.height !== heightPx * dpr) {
      canvas.width = widthPx * dpr;
      canvas.height = heightPx * dpr;
      canvas.style.width = `${widthPx}px`;
      canvas.style.height = `${heightPx}px`;
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    ctx.fillStyle = "#0b0b0d";
    ctx.fillRect(0, 0, widthPx, heightPx);

    // track row backgrounds + lane separators
    project.tracks.forEach((t, i) => {
      const y = tops[i];
      const h = trackHeight(t);
      ctx.fillStyle = i % 2 ? "#101013" : "#121215";
      ctx.fillRect(0, y, widthPx, h - 1);
      ctx.strokeStyle = "#2a2a31";
      ctx.beginPath();
      ctx.moveTo(0, y + h - 0.5);
      ctx.lineTo(widthPx, y + h - 0.5);
      ctx.stroke();
      if (t.type === "midi") {
        ctx.strokeStyle = "#1a1a1f";
        for (let l = 1; l < midiLanes(t); l++) {
          ctx.beginPath();
          ctx.moveTo(0, y + 2 + l * LANE_H - 0.5);
          ctx.lineTo(widthPx, y + 2 + l * LANE_H - 0.5);
          ctx.stroke();
        }
      }
    });

    drawGrid(ctx);
    project.tracks.forEach((track, i) => {
      if (track.type === "audio") drawWaveform(ctx, i);
      else drawEvents(ctx, i);
    });

    // drop ghost
    if (dropGhost) {
      const x = xOf(dropGhost.tick);
      const y = tops[dropGhost.ti] + 3 + dropGhost.lane * LANE_H;
      ctx.strokeStyle = "#ff2e88";
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(x, y, 40, LANE_H - 4);
      ctx.setLineDash([]);
    }

    // marquee
    if (marquee) {
      const x = Math.min(marquee.x0, marquee.x1);
      const y = Math.min(marquee.y0, marquee.y1);
      const w = Math.abs(marquee.x1 - marquee.x0);
      const h = Math.abs(marquee.y1 - marquee.y0);
      ctx.fillStyle = "rgba(255, 46, 136, 0.08)";
      ctx.fillRect(x, y, w, h);
      ctx.strokeStyle = "rgba(255, 46, 136, 0.7)";
      ctx.strokeRect(x + 0.5, y + 0.5, w, h);
    }

    // playhead
    const px = playhead * pxPerSecond;
    ctx.strokeStyle = "#f2f2f5";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(px, 0);
    ctx.lineTo(px, heightPx);
    ctx.stroke();
    ctx.lineWidth = 1;
    ctx.fillStyle = "#f2f2f5";
    ctx.beginPath();
    ctx.moveTo(px - 5, 0);
    ctx.lineTo(px + 5, 0);
    ctx.lineTo(px, 7);
    ctx.closePath();
    ctx.fill();
  }

  function drawGrid(ctx: CanvasRenderingContext2D) {
    ctx.textBaseline = "top";
    ctx.font = "10px 'IBM Plex Mono'";
    ctx.fillStyle = "#0e0e11";
    ctx.fillRect(0, 0, widthPx, RULER_H);
    ctx.strokeStyle = "#2a2a31";
    ctx.beginPath();
    ctx.moveTo(0, RULER_H - 0.5);
    ctx.lineTo(widthPx, RULER_H - 0.5);
    ctx.stroke();

    if (gridMode === "musical") {
      const bt = barTicks(project.timeSignature);
      const spb = secondsPerBeat(project.bpm);
      const beatPx = spb * pxPerSecond;
      // sub-beat lines from the snap setting, if visible
      if (snapTicks && (snapTicks / PPQN) * beatPx >= 7) {
        ctx.strokeStyle = "#17171b";
        for (let t = 0; t < secondsToTick(duration, project.bpm); t += snapTicks) {
          const x = xOf(t);
          ctx.beginPath();
          ctx.moveTo(x + 0.5, RULER_H);
          ctx.lineTo(x + 0.5, heightPx);
          ctx.stroke();
        }
      }
      const totalBeats = Math.ceil(duration / spb);
      const beatsPerBar = project.timeSignature[0];
      const beatStep = beatPx < 12 ? beatsPerBar : 1;
      for (let b = 0; b <= totalBeats; b += beatStep) {
        const tick = b * PPQN;
        const isBar = tick % bt === 0;
        const x = xOf(tick);
        ctx.strokeStyle = isBar ? "#34343d" : "#202026";
        ctx.beginPath();
        ctx.moveTo(x + 0.5, isBar ? 0 : RULER_H);
        ctx.lineTo(x + 0.5, heightPx);
        ctx.stroke();
        if (isBar && (beatPx * beatsPerBar > 34 || (tick / bt) % 4 === 0)) {
          ctx.fillStyle = "#8a8a96";
          ctx.fillText(String(Math.round(tick / bt) + 1), x + 4, 5);
        }
      }
    } else {
      const step = pxPerSecond > 120 ? 0.5 : pxPerSecond > 30 ? 1 : pxPerSecond > 8 ? 5 : 15;
      for (let s = 0; s <= duration; s += step) {
        const x = s * pxPerSecond;
        ctx.strokeStyle = "#202026";
        ctx.beginPath();
        ctx.moveTo(x + 0.5, 0);
        ctx.lineTo(x + 0.5, heightPx);
        ctx.stroke();
        ctx.fillStyle = "#8a8a96";
        const m = Math.floor(s / 60);
        const sec = (s % 60).toFixed(step < 1 ? 1 : 0).padStart(2, "0");
        ctx.fillText(`${m}:${sec}`, x + 4, 5);
      }
    }
  }

  // Waveform rendering is cached per track in an offscreen canvas: the per-pixel
  // (and per-color) work would otherwise run on every playhead frame.
  const waveCache = new Map<number, { key: string; canvas: HTMLCanvasElement }>();

  // Spectral base colors (low / mid / high) for the DJ-style coloring.
  const BAND_COLORS = [
    [255, 36, 70], // low — hot red
    [38, 255, 120], // mid — vivid green
    [64, 168, 255], // high — electric blue
  ];

  function waveCanvas(ti: number, a: NonNullable<(typeof audio)[number]>, gain: number): HTMLCanvasElement {
    const w = Math.max(1, Math.ceil(a.buffer.duration * pxPerSecond));
    const h = AUDIO_ROW_H - 1;
    const key = `${w}:${gain}:${coloredWaves}`;
    const hit = waveCache.get(ti);
    if (hit && hit.key === key) return hit.canvas;

    const cv = document.createElement("canvas");
    cv.width = w;
    cv.height = h;
    const c = cv.getContext("2d")!;
    const mid = h / 2;
    const amp = ((AUDIO_ROW_H - 12) / 2) * gain;
    const clip = (AUDIO_ROW_H - 4) / 2;

    // Per-band normalization: each band is scaled by its own track-wide
    // maximum, otherwise lows dominate and everything reads red-ish.
    const bandMax = [0.0001, 0.0001, 0.0001];
    for (let i = 0; i + 2 < a.bandPeaks.length; i += 3) {
      for (let b = 0; b < 3; b++) if (a.bandPeaks[i + b] > bandMax[b]) bandMax[b] = a.bandPeaks[i + b];
    }
    let peakMax = 0.0001;
    for (let i = 0; i < a.peaks.length; i++) {
      const v = Math.abs(a.peaks[i]);
      if (v > peakMax) peakMax = v;
    }

    for (let x = 0; x < w; x++) {
      const bucket = Math.floor(((x / pxPerSecond) * a.peaksPerSecond));
      if (bucket * 2 + 1 >= a.peaks.length) break;
      const lo = Math.max(-clip, a.peaks[bucket * 2] * amp);
      const hi = Math.min(clip, a.peaks[bucket * 2 + 1] * amp);
      if (coloredWaves && bucket * 3 + 2 < a.bandPeaks.length) {
        // squared, per-band-normalized weights exaggerate the dominant band
        const wl = (a.bandPeaks[bucket * 3] / bandMax[0]) ** 2;
        const wm = (a.bandPeaks[bucket * 3 + 1] / bandMax[1]) ** 2;
        const wh = (a.bandPeaks[bucket * 3 + 2] / bandMax[2]) ** 2;
        const tot = wl + wm + wh;
        if (tot > 0.0001) {
          // brightness follows the bucket's loudness (gamma 0.6): transients flash
          const loud = Math.min(
            1,
            (Math.max(Math.abs(a.peaks[bucket * 2]), a.peaks[bucket * 2 + 1]) / peakMax) ** 0.6,
          );
          const bright = 0.35 + 0.75 * loud;
          const r = Math.min(255, Math.round(((BAND_COLORS[0][0] * wl + BAND_COLORS[1][0] * wm + BAND_COLORS[2][0] * wh) / tot) * bright));
          const g = Math.min(255, Math.round(((BAND_COLORS[0][1] * wl + BAND_COLORS[1][1] * wm + BAND_COLORS[2][1] * wh) / tot) * bright));
          const b = Math.min(255, Math.round(((BAND_COLORS[0][2] * wl + BAND_COLORS[1][2] * wm + BAND_COLORS[2][2] * wh) / tot) * bright));
          c.fillStyle = `rgb(${r},${g},${b})`;
        } else {
          c.fillStyle = "#3a3a44";
        }
      } else {
        c.fillStyle = "#b9b9c4";
      }
      c.fillRect(x, mid + lo, 1, Math.max(1, hi - lo));
    }
    waveCache.set(ti, { key, canvas: cv });
    return cv;
  }

  function drawWaveform(ctx: CanvasRenderingContext2D, ti: number) {
    const track = project.tracks[ti];
    if (track.type !== "audio") return;
    const a = audio[ti];
    const y = tops[ti];
    if (!a) {
      ctx.fillStyle = "#5d5d68";
      ctx.fillText("loading audio…", 8, y + (AUDIO_ROW_H - 1) / 2);
      return;
    }
    const startX = tickToSeconds(track.offsetTicks, project.bpm) * pxPerSecond;
    const endX = startX + a.buffer.duration * pxPerSecond;
    ctx.fillStyle = "rgba(242, 242, 245, 0.05)";
    ctx.fillRect(startX, y, endX - startX, AUDIO_ROW_H - 1);
    ctx.drawImage(waveCanvas(ti, a, track.waveformGain || 1), startX, y);
  }

  function drawEvents(ctx: CanvasRenderingContext2D, ti: number) {
    const track = project.tracks[ti];
    if (track.type !== "midi") return;
    ctx.font = "10px 'Archivo Variable'";
    track.events.forEach((ev, ei) => {
      const r = eventRect(ti, ev);
      const color = COMMAND_TYPE_COLORS[ev.kind];
      const selected = isSelected(ti, ei);

      if (ev.kind === "one-shot") {
        const cx = r.x + r.w / 2;
        const cy = r.y + r.h / 2;
        const s = 8.5;
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.moveTo(cx, cy - s);
        ctx.lineTo(cx + s, cy);
        ctx.lineTo(cx, cy + s);
        ctx.lineTo(cx - s, cy);
        ctx.closePath();
        ctx.fill();
        if (selected) {
          ctx.strokeStyle = "#ffffff";
          ctx.stroke();
        }
      } else {
        ctx.fillStyle = color + "30";
        ctx.fillRect(r.x, r.y, r.w, r.h);
        ctx.strokeStyle = selected ? "#ffffff" : color + "77";
        ctx.strokeRect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1);
        if (ev.kind === "automation" && ev.breakpoints && ev.length) {
          ctx.strokeStyle = color;
          ctx.beginPath();
          ev.breakpoints.forEach(([t, v], i) => {
            const bx = r.x + (t / ev.length!) * r.w;
            const by = r.y + r.h - 2 - (v / 127) * (r.h - 4);
            i === 0 ? ctx.moveTo(bx, by) : ctx.lineTo(bx, by);
          });
          ctx.stroke();
          ctx.fillStyle = selected ? "#ffffff" : color;
          for (const [t, v] of ev.breakpoints) {
            const bx = r.x + (t / ev.length!) * r.w;
            const by = r.y + r.h - 2 - (v / 127) * (r.h - 4);
            ctx.beginPath();
            ctx.arc(bx, by, 2.5, 0, Math.PI * 2);
            ctx.fill();
          }
        }
      }
      // label: short command name + selected param values
      const cmd = commandsById.get(`${track.definitionId}/${ev.commandId}`);
      const label = cmd ? eventLabel(cmd, ev) : ev.commandId;
      const maxW = ev.kind === "one-shot" ? 110 : r.w - 8;
      if (maxW > 24) {
        ctx.fillStyle = ev.kind === "one-shot" ? "#b9b9c4" : "#f2f2f5";
        ctx.save();
        ctx.beginPath();
        ctx.rect(r.x + (ev.kind === "one-shot" ? r.w + 2 : 4), r.y, maxW, r.h);
        ctx.clip();
        ctx.fillText(label, r.x + (ev.kind === "one-shot" ? r.w + 3 : 5), r.y + 7);
        ctx.restore();
      }
    });
  }

  function ondblclick(e: MouseEvent) {
    const { x, y } = canvasPos(e);
    const bp = breakpointAt(x, y);
    if (bp) {
      deleteBreakpoint(bp);
      return;
    }
    const hit = hitTest(x, y);
    if (!hit) return;
    const tr = project.tracks[hit.ref.ti];
    if (tr.type !== "midi") return;
    const ev = tr.events[hit.ref.ei];
    if (ev.kind !== "automation" || !ev.breakpoints || !ev.length) return;
    const r = eventRect(hit.ref.ti, ev);
    const rawT = Math.max(0, Math.min(ev.length, tickAt(x) - ev.tick));
    const tOff = snapTicks ? Math.max(0, Math.min(ev.length, snap(ev.tick + rawT) - ev.tick)) : rawT;
    const value = Math.max(0, Math.min(127, Math.round(((r.y + r.h - 2 - y) / (r.h - 4)) * 127)));
    oncommit();
    const idx = ev.breakpoints.findIndex(([bt]) => bt > tOff);
    if (idx === -1) ev.breakpoints.push([tOff, value]);
    else ev.breakpoints.splice(idx, 0, [tOff, value]);
    onselectionchange([hit.ref]);
  }

  function deleteBreakpoint(bp: { ref: EventRef; index: number }) {
    const tr = project.tracks[bp.ref.ti];
    if (tr.type !== "midi") return;
    const ev = tr.events[bp.ref.ei];
    if (ev.kind !== "automation" || !ev.breakpoints) return;
    if (ev.breakpoints.length <= 2 || bp.index === 0 || bp.index === ev.breakpoints.length - 1) {
      onstatus("Endpoints cannot be deleted — drag them instead.");
      return;
    }
    oncommit();
    ev.breakpoints.splice(bp.index, 1);
  }

  function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    const { x, y } = canvasPos(e);
    const bp = breakpointAt(x, y);
    if (bp) deleteBreakpoint(bp);
  }

  function onwheel(e: WheelEvent) {
    if (!e.ctrlKey || !scroller || !canvas) return;
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const pointerSec = (e.clientX - rect.left) / pxPerSecond;
    const next = Math.min(800, Math.max(4, pxPerSecond * (e.deltaY < 0 ? 1.2 : 1 / 1.2)));
    onzoom(next);
    requestAnimationFrame(() => {
      if (!scroller || !canvas) return;
      const newRect = canvas.getBoundingClientRect();
      scroller.scrollLeft += pointerSec * next - (e.clientX - newRect.left);
    });
  }
</script>

<div class="scroller" bind:this={scroller} onwheel={onwheel}>
  <canvas
    bind:this={canvas}
    style:cursor={hoverCursor}
    onpointerdown={onpointerdown}
    onpointermove={onpointermove}
    onpointerup={onpointerup}
    ondblclick={ondblclick}
    oncontextmenu={oncontextmenu}
    ondragover={ondragover}
    ondragleave={() => (dropGhost = null)}
    ondrop={ondrop}
  ></canvas>
</div>

<style>
  .scroller {
    overflow: auto;
    border: 1px solid var(--line);
    border-left: none;
    flex: 1;
    min-width: 0;
    background: var(--bg0);
  }
  canvas {
    display: block;
  }
</style>
