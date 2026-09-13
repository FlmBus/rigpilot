<script lang="ts">
  import type { LoadedAudio } from "./audio";
  import { dragPayload } from "./Palette.svelte";
  import {
    AUDIO_ROW_H,
    COMMAND_TYPE_COLORS,
    LANE_H,
    PPQN,
    MIN_EVENT_TICKS,
    RULER_H,
    SECTIONS_H,
    SPARE_LANE_H,
    barTicks,
    laneAt as eventLaneAt,
    midiLanes,
    secondsPerBeat,
    secondsToTick,
    tickToSeconds,
    eventLabel,
    stepLabel,
    type AutomationCurve,
    type BpSelection,
    type Breakpoint,
    type CommandInfo,
    type DefinitionInfo,
    type EventRef,
    type MidiTrack,
    type Project,
    type RpEvent,
    type Section,
    type Shape,
    type TrackLayout,
  } from "./types";
  import {
    AUTO_PAD,
    allowedShapes,
    clampValue,
    convertValue,
    curveFor,
    curvePaths,
    curvesInUse,
    findCurve,
    isStepped,
    forcedShape,
    rangeOf,
    tensionFromMidpoint,
    valueAt,
    valueSpace,
    type ValueSpace,
  } from "./automation";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let {
    project,
    definitions,
    layout,
    audio,
    playhead,
    gridMode,
    pxPerSecond,
    snapTicks,
    selection,
    bpSelection,
    valueClip,
    coloredWaves,
    sections = [],
    follow = false,
    playing = false,
    onseek,
    onzoom,
    onselectionchange,
    onbpselect,
    onlaneselect,
    onlaneaction,
    oneditvalue,
    oncopyvalue,
    oncommit,
    oncreate,
    onstatus,
  }: {
    project: Project;
    definitions: DefinitionInfo[];
    layout: TrackLayout[];
    audio: (LoadedAudio | null)[];
    playhead: number;
    gridMode: "musical" | "time";
    pxPerSecond: number;
    snapTicks: number | null;
    selection: EventRef[];
    bpSelection: BpSelection | null;
    valueClip: { value: number; space: ValueSpace } | null;
    coloredWaves: boolean;
    /** Named regions, read-only for now — no add/resize/move yet. */
    sections?: Section[];
    /** Keep the playhead in view while playing. */
    follow?: boolean;
    playing?: boolean;
    onseek: (seconds: number) => void;
    onzoom: (pxPerSecond: number) => void;
    onselectionchange: (refs: EventRef[]) => void;
    onbpselect: (sel: BpSelection | null) => void;
    onlaneselect: (ti: number | null) => void;
    onlaneaction: (action: "constant" | "clear" | "toggle", ti: number, commandId: string) => void;
    oneditvalue: (ti: number, commandId: string, index: number) => void;
    oncopyvalue: (value: number, space: ValueSpace) => void;
    oncommit: () => void;
    oncreate: (ti: number, commandId: string, tick: number, lane: number) => void;
    onstatus: (msg: string) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let scroller = $state<HTMLDivElement>();

  // ---------- geometry ----------
  const tops = $derived(layout.map((l) => l.top));
  const duration = $derived.by(() => {
    let end = 30;
    project.tracks.forEach((t, i) => {
      if (t.type === "audio") {
        const a = audio[i];
        if (a) end = Math.max(end, tickToSeconds(t.offsetTicks, project.bpm) + a.buffer.duration);
      } else {
        for (const ev of t.events)
          end = Math.max(end, tickToSeconds(ev.tick + (ev.length ?? 0), project.bpm));
        for (const c of t.automation ?? [])
          for (const b of c.breakpoints)
            end = Math.max(end, tickToSeconds(b.tick, project.bpm));
      }
    });
    return end + 10;
  });
  const widthPx = $derived(Math.ceil(duration * pxPerSecond));
  const heightPx = $derived(
    (layout.length ? layout[layout.length - 1].top + layout[layout.length - 1].height : RULER_H + SECTIONS_H) +
      1,
  );

  /** Zoom so the whole song fits the visible width — used by the toolbar's Fit button. */
  export function fitToSong() {
    const w = scroller?.clientWidth ?? 800;
    onzoom(Math.max(4, Math.min(800, w / duration)));
  }

  // Follow: while playing, keep the playhead from running off the visible edge
  // rather than re-centering on every frame.
  $effect(() => {
    if (!follow || !playing || !scroller) return;
    const x = playhead * pxPerSecond;
    const margin = 60;
    if (x < scroller.scrollLeft + margin || x > scroller.scrollLeft + scroller.clientWidth - margin) {
      scroller.scrollLeft = Math.max(0, x - scroller.clientWidth * 0.3);
    }
  });

  const xOf = (tick: number) => tickToSeconds(tick, project.bpm) * pxPerSecond;
  const tickAt = (x: number) => Math.max(0, secondsToTick(x / pxPerSecond, project.bpm));

  /** Each section's rendered span — from its own tick to the next one's, or to the song's end. */
  const sectionRects = $derived.by(() => {
    const sorted = [...sections].sort((a, b) => a.tick - b.tick);
    return sorted.map((s, i) => {
      const x = xOf(s.tick);
      const nextX = i + 1 < sorted.length ? xOf(sorted[i + 1].tick) : widthPx;
      return { name: s.name, x, w: Math.max(1, nextX - x) };
    });
  });
  const snap = (tick: number) =>
    snapTicks ? Math.max(0, Math.round(tick / snapTicks) * snapTicks) : tick;
  /** Alt temporarily bypasses Snap, for events and breakpoints alike. */
  const snapAlt = (tick: number, alt: boolean) => (alt ? Math.max(0, tick) : snap(tick));

  const commandsById = $derived.by(() => {
    const m = new Map<string, CommandInfo>();
    for (const d of definitions)
      for (const c of d.commands) m.set(`${d.id}/${c.id}`, c);
    return m;
  });

  // ---- Automation Lanes ----
  type LaneInfo = {
    ti: number;
    /** Absolute y of the lane's top edge. */
    top: number;
    height: number;
    cmd: CommandInfo;
    curve: AutomationCurve | undefined;
    stepped: boolean;
    /** Set when the definition leaves no choice of curve type. */
    forced: Shape | null;
    range: [number, number];
  };

  /** The expanded Automation Lane of a track, or null when collapsed or absent. */
  function laneOf(ti: number): LaneInfo | null {
    const t = project.tracks[ti];
    const lay = layout[ti];
    if (t.type !== "midi" || !lay || lay.autoHeight === 0) return null;
    const id = t.automationView.command;
    if (!id) return null;
    const cmd = commandsById.get(`${t.definitionId}/${id}`);
    if (!cmd) return null;
    return {
      ti,
      top: lay.autoTop,
      height: lay.autoHeight,
      cmd,
      curve: findCurve(t, id),
      stepped: isStepped(cmd),
      forced: forcedShape(cmd),
      range: rangeOf(cmd),
    };
  }

  const laneBottom = (l: { height: number }) => l.height - AUTO_PAD;
  const laneSpan = (l: { height: number }) => Math.max(4, l.height - 2 * AUTO_PAD);

  /** Value -> y inside the lane (local coordinates, 0 = lane top). */
  function laneY(l: LaneInfo, value: number): number {
    const [min, max] = l.range;
    const r = max > min ? (value - min) / (max - min) : 0;
    return laneBottom(l) - r * laneSpan(l);
  }

  /** y (local) -> value, clamped to the target's range. */
  function laneValue(l: LaneInfo, yLocal: number): number {
    const [min, max] = l.range;
    const r = Math.min(1, Math.max(0, (laneBottom(l) - yLocal) / laneSpan(l)));
    return Math.round(min + r * (max - min));
  }

  /** Miniatures of the in-use curves, drawn in a collapsed lane's strip. */
  function collapsedGhosts(ti: number): string[] {
    const t = project.tracks[ti];
    const lay = layout[ti];
    if (t.type !== "midi" || lay.autoHeight === 0 || t.automationView.command) return [];
    const pad = 4;
    const bottom = lay.autoHeight - pad;
    const span = Math.max(2, lay.autoHeight - 2 * pad);
    return curvesInUse(t)
      .map((c) => {
        const cmd = commandsById.get(`${t.definitionId}/${c.commandId}`);
        if (!cmd) return "";
        const [min, max] = rangeOf(cmd);
        const y = (v: number) => bottom - (max > min ? (v - min) / (max - min) : 0) * span;
        const p = curvePaths(c.breakpoints, xOf, y, forcedShape(cmd), widthPx, bottom);
        return `${p.drawn} ${p.held}`.trim();
      })
      .filter(Boolean);
  }

  // ---- lane hit-testing ----
  const NODE_HIT = 6;
  const SEG_HIT = 4;

  function laneAt(y: number): LaneInfo | null {
    const ti = trackAt(y);
    if (ti === null) return null;
    const lane = laneOf(ti);
    if (!lane || y < lane.top || y >= lane.top + lane.height) return null;
    return lane;
  }

  function nodeAt(x: number, y: number): { lane: LaneInfo; index: number } | null {
    const lane = laneAt(y);
    const bps = lane?.curve?.breakpoints;
    if (!lane || !bps) return null;
    for (let i = bps.length - 1; i >= 0; i--) {
      const nx = xOf(bps[i].tick);
      const ny = lane.top + laneY(lane, bps[i].value);
      if (Math.abs(x - nx) <= NODE_HIT && Math.abs(y - ny) <= NODE_HIT) return { lane, index: i };
    }
    return null;
  }

  type Seg = { lane: LaneInfo; index: number };

  /** The segment under the pointer, identified by its left breakpoint. */
  function segmentAt(x: number, y: number): Seg | null {
    const lane = laneAt(y);
    const bps = lane?.curve?.breakpoints;
    if (!lane || !bps || bps.length < 2) return null;
    const t = tickAt(x);
    if (t < bps[0].tick || t > bps[bps.length - 1].tick) return null;
    const v = valueAt(bps, t, lane.forced);
    if (v === null || Math.abs(y - (lane.top + laneY(lane, v))) > SEG_HIT) return null;
    let i = 0;
    while (i < bps.length - 2 && bps[i + 1].tick <= t) i++;
    return { lane, index: i };
  }

  /** Position of a segment's bend handle, or null when it cannot be bent. */
  function handlePos(lane: LaneInfo, index: number): { x: number; y: number } | null {
    const bps = lane.curve?.breakpoints;
    // Nothing to bend when the definition forbids curves or fixes the type.
    if (!bps || lane.forced || !allowedShapes(lane.cmd).includes("curve")) return null;
    const a = bps[index];
    const b = bps[index + 1];
    if (!a || !b || a.value === b.value || b.shape === "hold") return null;
    const mt = (a.tick + b.tick) / 2;
    const mv = valueAt(bps, mt, null);
    if (mv === null) return null;
    return { x: xOf(mt), y: lane.top + laneY(lane, mv) };
  }

  function handleAt(x: number, y: number): Seg | null {
    const seg = segmentAt(x, y);
    if (!seg) return null;
    const h = handlePos(seg.lane, seg.index);
    if (!h) return null;
    return Math.abs(x - h.x) <= NODE_HIT && Math.abs(y - h.y) <= NODE_HIT ? seg : null;
  }

  const bpSelected = (ti: number, commandId: string, i: number) =>
    bpSelection?.ti === ti && bpSelection.commandId === commandId && bpSelection.idx.includes(i);

  // ---- editing ----
  function curveOf(lane: LaneInfo) {
    const t = project.tracks[lane.ti];
    return t.type === "midi" ? curveFor(t, lane.cmd.id) : null;
  }

  function addPoint(lane: LaneInfo, x: number, y: number, alt: boolean) {
    const tick = Math.max(0, snapAlt(tickAt(x), alt));
    const value = clampValue(lane.cmd, laneValue(lane, y - lane.top));
    const existing = lane.curve?.breakpoints.findIndex((b) => b.tick === tick) ?? -1;
    if (existing >= 0) {
      onbpselect({ ti: lane.ti, commandId: lane.cmd.id, idx: [existing] });
      return;
    }
    oncommit();
    const curve = curveOf(lane);
    if (!curve) return;
    const bps = curve.breakpoints;
    const after = bps.findIndex((b) => b.tick > tick);
    const index = after === -1 ? bps.length : after;
    // The split segment's shape lives on its right-hand point, so the new point
    // takes it over and the one after keeps its own.
    const split = bps[index];
    const point: Breakpoint = {
      tick,
      value,
      shape: lane.forced ?? split?.shape ?? bps[index - 1]?.shape ?? allowedShapes(lane.cmd)[0],
      tension: split?.tension ?? 0,
    };
    bps.splice(index, 0, point);
    onbpselect({ ti: lane.ti, commandId: lane.cmd.id, idx: [index] });
  }

  /**
   * Puts a breakpoint just outside each end of a selected stretch, so dragging the
   * stretch does not drag the ramps leading into and out of it. Returns the
   * selection's indices after the inserts.
   */
  function insertBoundaries(lane: LaneInfo, curve: AutomationCurve, idx: number[]): number[] {
    const bps = curve.breakpoints;
    const ticks = idx.map((i) => bps[i]?.tick).filter((t) => t !== undefined) as number[];
    if (!ticks.length) return idx;
    const tmin = Math.min(...ticks);
    const tmax = Math.max(...ticks);
    const first = Math.min(...idx);
    const last = Math.max(...idx);
    const added: Breakpoint[] = [];
    if (first > 0 && bps[first - 1].tick < tmin - 1) {
      const t = tmin - 1;
      added.push({
        tick: t,
        value: valueAt(bps, t, lane.forced) ?? bps[first - 1].value,
        shape: bps[first - 1].shape,
        tension: bps[first - 1].tension,
      });
    }
    if (last < bps.length - 1 && bps[last + 1].tick > tmax + 1) {
      const t = tmax + 1;
      added.push({
        tick: t,
        value: valueAt(bps, t, lane.forced) ?? bps[last].value,
        shape: bps[last].shape,
        tension: bps[last].tension,
      });
    }
    for (const point of added) {
      const at = bps.findIndex((b) => b.tick > point.tick);
      bps.splice(at === -1 ? bps.length : at, 0, point);
    }
    if (!added.length) return idx;
    return ticks
      .map((t) => bps.findIndex((b) => b.tick === t))
      .filter((i) => i >= 0)
      .sort((a, b) => a - b);
  }

  function deletePoints(lane: LaneInfo, idx: number[]) {
    const curve = lane.curve;
    if (!curve || idx.length === 0) return;
    oncommit();
    for (const i of [...idx].sort((a, b) => b - a)) curve.breakpoints.splice(i, 1);
    onbpselect(null);
  }

  /** `index` is the breakpoint the segment arrives at — the one that owns its shape. */
  function setPointShape(lane: LaneInfo, index: number, shape: Shape) {
    const bps = lane.curve?.breakpoints;
    if (!bps?.[index]) return;
    oncommit();
    bps[index].shape = shape;
    if (shape !== "curve") bps[index].tension = 0;
  }

  function pasteValue(lane: LaneInfo, index: number) {
    const bps = lane.curve?.breakpoints;
    if (!bps?.[index] || !valueClip) return;
    oncommit();
    bps[index].value = convertValue(valueClip.value, valueClip.space, valueSpace(lane.cmd));
  }

  // ---- context menus ----
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /**
   * Curve type belongs to the breakpoint, describing the segment that arrives at
   * it. The first point has nothing arriving, and discrete targets never
   * interpolate, so neither is offered a choice.
   */
  function shapeItems(lane: LaneInfo, index: number): MenuItem[] {
    const point = lane.curve?.breakpoints[index];
    // No incoming segment, or the definition allows exactly one type.
    if (!point || index === 0 || lane.forced) return [];
    const set = (shape: Shape) => () => setPointShape(lane, index, shape);
    const labels: Record<Shape, string> = { linear: "Linear", curve: "Curve", hold: "Hold" };
    const items: MenuItem[] = allowedShapes(lane.cmd).map((shape) => ({
      label: labels[shape],
      checked: point.shape === shape,
      onselect: set(shape),
    }));
    if (point.shape === "curve" && point.tension !== 0 && allowedShapes(lane.cmd).includes("linear")) {
      items.push({ label: "Reset curve", onselect: set("linear") });
    }
    items.push({ separator: true });
    return items;
  }

  function pointMenu(lane: LaneInfo, index: number): MenuItem[] {
    const b = lane.curve!.breakpoints[index];
    return [
      ...shapeItems(lane, index),
      { label: "Edit value…", onselect: () => oneditvalue(lane.ti, lane.cmd.id, index) },
      { label: "Copy value", onselect: () => oncopyvalue(b.value, valueSpace(lane.cmd)) },
      { label: "Paste value", disabled: !valueClip, onselect: () => pasteValue(lane, index) },
      { separator: true },
      {
        label: "Delete point",
        shortcut: "Del",
        danger: true,
        onselect: () => deletePoints(lane, [index]),
      },
    ];
  }

  /** Right-clicking the line edits the point it runs into — same command, second route. */
  function segmentMenu(seg: Seg, x: number, y: number): MenuItem[] {
    return [
      ...shapeItems(seg.lane, seg.index + 1),
      { label: "Add point here", onselect: () => addPoint(seg.lane, x, y, false) },
    ];
  }

  function laneMenu(lane: LaneInfo): MenuItem[] {
    const on = lane.curve?.enabled ?? true;
    return [
      {
        label: "Set to constant…",
        onselect: () => onlaneaction("constant", lane.ti, lane.cmd.id),
      },
      {
        label: "Clear curve",
        disabled: !lane.curve?.breakpoints.length,
        danger: true,
        onselect: () => onlaneaction("clear", lane.ti, lane.cmd.id),
      },
      { separator: true },
      {
        label: on ? "Switch curve off" : "Switch curve on",
        checked: on,
        onselect: () => onlaneaction("toggle", lane.ti, lane.cmd.id),
      },
    ];
  }

  function oncontextmenu(e: MouseEvent) {
    const { x, y } = canvasPos(e);
    const lane = laneAt(y);
    if (!lane) return;
    e.preventDefault();
    const node = nodeAt(x, y);
    if (node) {
      onbpselect({ ti: lane.ti, commandId: lane.cmd.id, idx: [node.index] });
      menu = { x: e.clientX, y: e.clientY, items: pointMenu(lane, node.index) };
      return;
    }
    const seg = segmentAt(x, y);
    if (seg) {
      menu = { x: e.clientX, y: e.clientY, items: segmentMenu(seg, x, y) };
      return;
    }
    onlaneselect(lane.ti);
    menu = { x: e.clientX, y: e.clientY, items: laneMenu(lane) };
  }

  function ondblclick(e: MouseEvent) {
    const { x, y } = canvasPos(e);
    const lane = laneAt(y);
    if (!lane) return;
    const node = nodeAt(x, y);
    if (node) {
      deletePoints(lane, [node.index]);
      return;
    }
    const handle = handleAt(x, y);
    if (handle) {
      setPointShape(handle.lane, handle.index + 1, "linear");
      return;
    }
    addPoint(lane, x, y, e.altKey);
  }

  /** Grab box of a one-shot: a dot is too small to hit, so the box is slop around it. */
  const SHOT_W = 19;
  /** Distance from the dispatch tick to the left edge of the flag (dot half-width + gap). */
  const SHOT_LABEL_GAP = 9;
  /** Below this the flag is unreadable anyway, so it is dropped instead of truncated. */
  const SHOT_LABEL_MIN = 18;

  function eventRect(ti: number, ev: RpEvent) {
    let x = xOf(ev.tick);
    let w: number;
    if (ev.kind === "one-shot") {
      // box centered on the dispatch tick, so the dot inside it lands on the grid line
      w = SHOT_W;
      x -= w / 2;
    } else {
      // A floor of 9px made every short hold look identical; 3px still reads and, with the
      // ±2px slack in hitAt, stays clickable.
      w = Math.max(3, xOf(ev.tick + (ev.length ?? 0)) - x);
    }
    const y = tops[ti] + 2 + ev.lane * LANE_H;
    return { x, y, w, h: LANE_H - 3 };
  }

  /**
   * Room a one-shot's flag has before it runs into the next event on the same Lane.
   * The flag sits to the right of the dot, outside the grab box, so without this it
   * would print straight over its neighbours. Events are unordered, hence the scan;
   * ties on the same tick are broken by index so exactly one of them keeps its flag.
   */
  function shotLabelRoom(events: RpEvent[], ev: RpEvent, ei: number): number {
    let next = Infinity;
    events.forEach((o, i) => {
      if (i === ei || o.lane !== ev.lane) return;
      if (o.tick < ev.tick || (o.tick === ev.tick && i < ei)) return;
      next = Math.min(next, o.tick);
    });
    const right = next === Infinity ? widthPx : xOf(next) - 5;
    return right - (xOf(ev.tick) + SHOT_LABEL_GAP);
  }

  type Hit = { ref: EventRef; zone: "body" | "left" | "right" };
  function hitTest(x: number, y: number): Hit | null {
    for (let ti = project.tracks.length - 1; ti >= 0; ti--) {
      const t = project.tracks[ti];
      if (t.type !== "midi") continue;
      if (y < tops[ti] || y >= layout[ti].autoTop) continue;
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

  function trackAt(y: number): number | null {
    for (let ti = 0; ti < project.tracks.length; ti++) {
      if (y >= layout[ti].top && y < layout[ti].top + layout[ti].height) return ti;
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
        startX: number;
        moved: boolean;
      }
    | { mode: "marquee"; x0: number; y0: number; x1: number; y1: number; base: EventRef[] }
    | {
        mode: "node";
        ti: number;
        commandId: string;
        index: number;
        /** Restored while Shift is held, so the drag is horizontal only. */
        origValue: number;
        moved: boolean;
      }
    | {
        mode: "nodes";
        ti: number;
        commandId: string;
        idx: number[];
        orig: number[];
        startY: number;
        moved: boolean;
      }
    | { mode: "tension"; ti: number; commandId: string; index: number; moved: boolean }
    | {
        mode: "bpmarquee";
        ti: number;
        commandId: string;
        x0: number;
        y0: number;
        x1: number;
        y1: number;
      };

  let drag: Drag | null = null;
  let marquee = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let dropGhost = $state<{
    ti: number;
    lane: number;
    tick: number;
    kind: CommandInfo["commandType"];
  } | null>(null);

  // Command type of a palette command, so the drop ghost can mirror the real
  // event shape (dot for one-shot, block for hold/automation).
  function commandKind(definitionId: string, commandId: string): CommandInfo["commandType"] {
    const cmd = definitions
      .find((d) => d.id === definitionId)
      ?.commands.find((c) => c.id === commandId);
    return cmd?.commandType ?? "hold";
  }
  let hoverCursor = $state("default");
  /** Segment whose bend handle is currently offered. */
  let hoverSeg = $state<{ ti: number; index: number } | null>(null);

  function canvasPos(e: { clientX: number; clientY: number }) {
    const rect = canvas!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || !canvas) return;
    canvas.setPointerCapture(e.pointerId);
    const { x, y } = canvasPos(e);

    if (y < RULER_H + SECTIONS_H) {
      drag = { mode: "seek" };
      onseek(tickToSeconds(snap(tickAt(x)), project.bpm));
      return;
    }

    const lane = laneAt(y);
    if (lane) {
      const node = nodeAt(x, y);
      if (node) {
        const bps = lane.curve?.breakpoints ?? [];
        const sameCurve =
          bpSelection?.ti === lane.ti && bpSelection.commandId === lane.cmd.id;
        const current = sameCurve ? bpSelection!.idx : [];
        const inSel = current.includes(node.index);
        let idx: number[];
        if (e.ctrlKey || e.metaKey) {
          idx = inSel ? current.filter((i) => i !== node.index) : [...current, node.index];
        } else if (e.shiftKey) {
          idx = inSel ? current : [...current, node.index];
        } else {
          idx = inSel ? current : [node.index];
        }
        idx = [...new Set(idx)].sort((a, b) => a - b);
        onbpselect(idx.length ? { ti: lane.ti, commandId: lane.cmd.id, idx } : null);
        if (idx.length > 1) {
          drag = {
            mode: "nodes",
            ti: lane.ti,
            commandId: lane.cmd.id,
            idx,
            orig: idx.map((i) => bps[i]?.value ?? 0),
            startY: y,
            moved: false,
          };
        } else if (idx.length === 1) {
          drag = {
            mode: "node",
            ti: lane.ti,
            commandId: lane.cmd.id,
            index: idx[0],
            origValue: bps[idx[0]]?.value ?? 0,
            moved: false,
          };
        }
        return;
      }
      const handle = handleAt(x, y);
      if (handle) {
        drag = {
          mode: "tension",
          ti: lane.ti,
          commandId: lane.cmd.id,
          index: handle.index,
          moved: false,
        };
        return;
      }
      onbpselect(null);
      drag = { mode: "bpmarquee", ti: lane.ti, commandId: lane.cmd.id, x0: x, y0: y, x1: x, y1: y };
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
      const lane = laneAt(y);
      if (lane) {
        const seg = segmentAt(x, y);
        hoverSeg = seg && handlePos(seg.lane, seg.index) ? { ti: seg.lane.ti, index: seg.index } : null;
        hoverCursor = nodeAt(x, y) ? "crosshair" : hoverSeg && handleAt(x, y) ? "ns-resize" : "default";
        return;
      }
      hoverSeg = null;
      const hit = hitTest(x, y);
      hoverCursor = !hit
        ? y < RULER_H + SECTIONS_H
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
      // Never block a resize the active grid can still express — a 1/64 grid gets 1/64 holds.
      const minLen = Math.max(1, Math.min(MIN_EVENT_TICKS, snapTicks ?? MIN_EVENT_TICKS));
      if (drag.edge === "right") {
        const end = snap(drag.origTick + drag.origLen + dTicks);
        ev.length = Math.max(minLen, end - drag.origTick);
      } else {
        const start = Math.min(snap(drag.origTick + dTicks), drag.origTick + drag.origLen - minLen);
        ev.tick = Math.max(0, start);
        ev.length = drag.origTick + drag.origLen - ev.tick;
      }
      return;
    }

    if (drag.mode === "node") {
      const lane = laneOf(drag.ti);
      const bps = lane?.curve?.breakpoints;
      const b = bps?.[drag.index];
      if (!lane || !bps || !b) return;
      if (!drag.moved) {
        oncommit();
        drag.moved = true;
      }
      const prev = bps[drag.index - 1];
      const next = bps[drag.index + 1];
      const lo = prev ? prev.tick + 1 : 0;
      const hi = next ? next.tick - 1 : Number.MAX_SAFE_INTEGER;
      b.tick = Math.max(lo, Math.min(hi, snapAlt(tickAt(x), e.altKey)));
      // Shift locks the value, so the point only moves in time.
      b.value = e.shiftKey
        ? drag.origValue
        : clampValue(lane.cmd, laneValue(lane, y - lane.top));
      return;
    }

    if (drag.mode === "nodes") {
      const lane = laneOf(drag.ti);
      const curve = lane?.curve;
      if (!lane || !curve) return;
      if (!drag.moved) {
        oncommit();
        drag.moved = true;
        drag.idx = insertBoundaries(lane, curve, drag.idx);
        drag.orig = drag.idx.map((i) => curve.breakpoints[i]?.value ?? 0);
        onbpselect({ ti: drag.ti, commandId: drag.commandId, idx: drag.idx });
      }
      // vertical only: the shape and the timing are kept
      const d = drag;
      const dv = laneValue(lane, y - lane.top) - laneValue(lane, d.startY - lane.top);
      d.idx.forEach((i, n) => {
        const b = curve.breakpoints[i];
        if (b) b.value = clampValue(lane.cmd, d.orig[n] + dv);
      });
      return;
    }

    if (drag.mode === "tension") {
      const lane = laneOf(drag.ti);
      const bps = lane?.curve?.breakpoints;
      const a = bps?.[drag.index];
      const b = bps?.[drag.index + 1];
      if (!lane || !a || !b) return;
      if (!drag.moved) {
        oncommit();
        drag.moved = true;
      }
      b.shape = "curve";
      b.tension = tensionFromMidpoint(a.value, b.value, laneValue(lane, y - lane.top));
      return;
    }

    if (drag.mode === "bpmarquee") {
      drag.x1 = x;
      drag.y1 = y;
      marquee = { x0: drag.x0, y0: drag.y0, x1: x, y1: y };
      const lane = laneOf(drag.ti);
      const bps = lane?.curve?.breakpoints;
      if (!lane || !bps) return;
      const [mx0, mx1] = [Math.min(drag.x0, x), Math.max(drag.x0, x)];
      const [my0, my1] = [Math.min(drag.y0, y), Math.max(drag.y0, y)];
      const idx: number[] = [];
      bps.forEach((b, i) => {
        const nx = xOf(b.tick);
        const ny = lane.top + laneY(lane, b.value);
        if (nx >= mx0 && nx <= mx1 && ny >= my0 && ny <= my1) idx.push(i);
      });
      onbpselect(idx.length ? { ti: lane.ti, commandId: lane.cmd.id, idx } : null);
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
    if (drag?.mode === "bpmarquee") {
      const tiny = Math.abs(drag.x1 - drag.x0) < 3 && Math.abs(drag.y1 - drag.y0) < 3;
      if (tiny) {
        onlaneselect(drag.ti);
        onseek(tickToSeconds(snap(tickAt(drag.x0)), project.bpm));
      }
    }
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
    // Automation has a lane of its own — there is nothing to place.
    if (payload && commandKind(payload.definitionId, payload.commandId) === "automation") {
      dropGhost = null;
      return;
    }
    if (y >= layout[ti].autoTop) {
      dropGhost = null;
      return;
    }
    const lane = eventLaneAt(project.tracks[ti] as MidiTrack, y - tops[ti]);
    dropGhost = {
      ti,
      lane,
      tick: snap(tickAt(x)),
      kind: payload ? commandKind(payload.definitionId, payload.commandId) : "hold",
    };
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
    if (y >= layout[ti].autoTop) return;
    const lane = eventLaneAt(track, y - tops[ti]);
    oncreate(ti, commandId, snap(tickAt(x)), lane);
  }

  // ---------- rendering ----------
  // Canvas colors can't read CSS custom properties directly, so every repaint
  // takes one snapshot of the tokens it needs via getComputedStyle.
  type Tokens = ReturnType<typeof readTokens>;
  function readTokens() {
    const s = getComputedStyle(document.documentElement);
    const v = (n: string) => s.getPropertyValue(n).trim();
    return {
      canvas: v("--canvas"),
      panel: v("--panel"),
      sunken: v("--sunken"),
      line: v("--line"),
      line2: v("--line-2"),
      gridSoft: v("--grid-soft"),
      gridBar: v("--grid-bar"),
      fg: v("--fg"),
      fg3: v("--fg-3"),
      wave: v("--wave"),
      accent: v("--accent"),
    };
  }

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
    const t = readTokens();

    ctx.fillStyle = t.canvas;
    ctx.fillRect(0, 0, widthPx, heightPx);

    // track row backgrounds + lane separators
    project.tracks.forEach((track, i) => {
      const y = tops[i];
      const h = layout[i].height;
      ctx.fillStyle = i % 2 ? t.panel : t.canvas;
      ctx.fillRect(0, y, widthPx, h - 1);
      ctx.strokeStyle = t.line2;
      ctx.beginPath();
      ctx.moveTo(0, y + h - 0.5);
      ctx.lineTo(widthPx, y + h - 0.5);
      ctx.stroke();
      if (track.type === "midi") {
        ctx.strokeStyle = t.line;
        // one line per lane boundary; the last one is the top of the spare strip
        for (let l = 1; l <= midiLanes(track); l++) {
          ctx.beginPath();
          ctx.moveTo(0, y + 2 + l * LANE_H - 0.5);
          ctx.lineTo(widthPx, y + 2 + l * LANE_H - 0.5);
          ctx.stroke();
        }
      }
    });

    drawAutoLanes(ctx, t);
    drawGrid(ctx, t);
    project.tracks.forEach((track, i) => {
      if (track.type === "audio") drawWaveform(ctx, i, t);
      // MIDI events are rendered as a DOM overlay (see template) — keeps canvas for
      // waveform/grid/playhead while clips get the full CSS design + interactions stay on canvas.
    });

    // drop ghost — mirrors the shape of the event that will be created
    if (dropGhost) {
      const x = xOf(dropGhost.tick);
      const y = tops[dropGhost.ti] + 2 + dropGhost.lane * LANE_H;
      // on the spare strip the ghost is clipped to that strip; the lane grows on drop
      const spare = dropGhost.lane >= midiLanes(project.tracks[dropGhost.ti] as MidiTrack);
      const h = spare ? SPARE_LANE_H - 1 : LANE_H - 3;
      const color = COMMAND_TYPE_COLORS[dropGhost.kind];
      ctx.strokeStyle = color;
      ctx.setLineDash([4, 3]);
      if (dropGhost.kind === "one-shot") {
        // a dot centered on the dispatch tick, like a real one-shot
        const cx = x;
        const cy = y + h / 2;
        const r = 6;
        ctx.fillStyle = color + "30";
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      } else {
        // hold/automation start as a short block at the drop tick
        ctx.strokeRect(x, y, 40, h);
      }
      ctx.setLineDash([]);
    }

    // marquee
    if (marquee) {
      const x = Math.min(marquee.x0, marquee.x1);
      const y = Math.min(marquee.y0, marquee.y1);
      const w = Math.abs(marquee.x1 - marquee.x0);
      const h = Math.abs(marquee.y1 - marquee.y0);
      ctx.fillStyle = t.accent + "14";
      ctx.fillRect(x, y, w, h);
      ctx.strokeStyle = t.accent + "b3";
      ctx.strokeRect(x + 0.5, y + 0.5, w, h);
    }

    // playhead is a DOM element on top of the clip overlay (see template)
  }

  /** The Automation Lane reads as a recessed display, not as another event lane. */
  function drawAutoLanes(ctx: CanvasRenderingContext2D, tok: Tokens) {
    project.tracks.forEach((track, ti) => {
      const lay = layout[ti];
      if (track.type !== "midi" || lay.autoHeight === 0) return;
      ctx.fillStyle = tok.sunken;
      ctx.fillRect(0, lay.autoTop, widthPx, lay.autoHeight - 1);
      ctx.strokeStyle = tok.line;
      ctx.beginPath();
      ctx.moveTo(0, lay.autoTop + 0.5);
      ctx.lineTo(widthPx, lay.autoTop + 0.5);
      ctx.stroke();

      const lane = laneOf(ti);
      if (!lane) return;
      // value guides: quarters, or one per step for a discrete target
      const [min, max] = lane.range;
      // One guide per step reads well for a handful; a long list (e.g. 128 songs)
      // would fill the lane with lines, so fall back to quarters.
      const guides =
        lane.stepped && lane.cmd.steps.length <= 12
          ? lane.cmd.steps.map((st) => st.value)
          : [0.25, 0.5, 0.75].map((f) => min + f * (max - min));
      ctx.strokeStyle = tok.line;
      for (const v of guides) {
        const gy = Math.round(lane.top + laneY(lane, v)) + 0.5;
        ctx.beginPath();
        ctx.moveTo(0, gy);
        ctx.lineTo(widthPx, gy);
        ctx.stroke();
      }
    });
  }

  function drawGrid(ctx: CanvasRenderingContext2D, tok: Tokens) {
    ctx.textBaseline = "top";
    ctx.font = "10px 'IBM Plex Mono'";
    ctx.fillStyle = tok.canvas;
    ctx.fillRect(0, 0, widthPx, RULER_H);
    ctx.strokeStyle = tok.line;
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
        ctx.strokeStyle = tok.gridSoft;
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
        ctx.strokeStyle = isBar ? tok.gridBar : tok.gridSoft;
        ctx.beginPath();
        ctx.moveTo(x + 0.5, isBar ? 0 : RULER_H);
        ctx.lineTo(x + 0.5, heightPx);
        ctx.stroke();
        if (isBar && (beatPx * beatsPerBar > 34 || (tick / bt) % 4 === 0)) {
          ctx.fillStyle = tok.fg3;
          ctx.fillText(String(Math.round(tick / bt) + 1), x + 4, 5);
        }
      }
    } else {
      const step = pxPerSecond > 120 ? 0.5 : pxPerSecond > 30 ? 1 : pxPerSecond > 8 ? 5 : 15;
      for (let s = 0; s <= duration; s += step) {
        const x = s * pxPerSecond;
        ctx.strokeStyle = tok.gridSoft;
        ctx.beginPath();
        ctx.moveTo(x + 0.5, 0);
        ctx.lineTo(x + 0.5, heightPx);
        ctx.stroke();
        ctx.fillStyle = tok.fg3;
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

    const tok = readTokens();
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
          c.fillStyle = tok.wave;
        }
      } else {
        c.fillStyle = tok.fg3;
      }
      c.fillRect(x, mid + lo, 1, Math.max(1, hi - lo));
    }
    waveCache.set(ti, { key, canvas: cv });
    return cv;
  }

  function drawWaveform(ctx: CanvasRenderingContext2D, ti: number, tok: Tokens) {
    const track = project.tracks[ti];
    if (track.type !== "audio") return;
    const a = audio[ti];
    const y = tops[ti];
    if (!a) {
      ctx.fillStyle = tok.fg3;
      ctx.fillText("loading audio…", 8, y + (AUDIO_ROW_H - 1) / 2);
      return;
    }
    const startX = tickToSeconds(track.offsetTicks, project.bpm) * pxPerSecond;
    const endX = startX + a.buffer.duration * pxPerSecond;
    ctx.fillStyle = tok.fg + "0d";
    ctx.fillRect(startX, y, endX - startX, AUDIO_ROW_H - 1);
    ctx.drawImage(waveCanvas(ti, a, track.waveformGain || 1), startX, y);
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
  <div class="surface" style="width:{widthPx}px;height:{heightPx}px;">
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
    <div class="events" aria-hidden="true">
      {#each project.tracks as track, ti}
        {#if track.type === "midi"}
          {#each track.events as ev, ei (ei)}
            {@const r = eventRect(ti, ev)}
            {@const cmd = commandsById.get(`${track.definitionId}/${ev.commandId}`)}
            {@const label = cmd ? eventLabel(cmd, ev) : ev.commandId}
            {@const sel = isSelected(ti, ei)}
            {#if ev.kind === "one-shot"}
              {@const room = shotLabelRoom(track.events, ev, ei)}
              <div class="shot" class:sel style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px">
                <b></b>
                {#if label && room >= SHOT_LABEL_MIN}
                  <span class="shot-label" style="max-width:{room}px">{label}</span>
                {/if}
              </div>
            {:else if ev.kind === "hold"}
              <div class="clip hold" class:sel style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px">
                <span class="clip-label">{label}</span>
              </div>
            {/if}
          {/each}

          {@const lane = laneOf(ti)}
          {#if lane}
            {@const bps = lane.curve?.breakpoints ?? []}
            {@const on = lane.curve?.enabled ?? true}
            {@const paths = curvePaths(
              bps,
              xOf,
              (v) => laneY(lane, v),
              lane.forced,
              widthPx,
              laneBottom(lane),
            )}
            <div
              class="autolane"
              class:off={!on}
              style="left:0;top:{lane.top}px;width:{widthPx}px;height:{lane.height}px"
            >
              <svg width={widthPx} height={lane.height}>
                {#if paths.area}<path class="area" d={paths.area} />{/if}
                {#if paths.held}<path class="held" d={paths.held} />{/if}
                {#if paths.drawn}<path class="line" d={paths.drawn} />{/if}
              </svg>
              {#if hoverSeg && hoverSeg.ti === lane.ti}
                {@const h = handlePos(lane, hoverSeg.index)}
                {#if h}
                  <div class="seg-handle" style="left:{h.x}px;top:{h.y - lane.top}px"></div>
                {/if}
              {/if}
              {#each bps as b, i}
                <div
                  class="node"
                  class:sel={bpSelected(lane.ti, lane.cmd.id, i)}
                  style="left:{xOf(b.tick)}px;top:{laneY(lane, b.value)}px"
                ></div>
                {#if lane.stepped}
                  {@const sl = stepLabel(lane.cmd.steps, b.value)}
                  {#if sl}
                    <span class="node-label" style="left:{xOf(b.tick)}px;top:{laneY(lane, b.value)}px">{sl}</span>
                  {/if}
                {/if}
              {/each}
              <span class="lane-name">
                {lane.cmd.name}{#if !on}<span class="lane-off">OFF</span>{/if}
              </span>
            </div>
          {:else}
            {@const ghosts = collapsedGhosts(ti)}
            {#if ghosts.length}
              <svg
                class="autoghost"
                width={widthPx}
                height={layout[ti].autoHeight}
                style="left:0;top:{layout[ti].autoTop}px"
              >
                {#each ghosts as d}<path d={d} />{/each}
              </svg>
            {/if}
          {/if}
        {/if}
      {/each}
    </div>
    <div class="sections" aria-hidden="true" style="top:{RULER_H}px;height:{SECTIONS_H}px">
      {#each sectionRects as s}
        <div class="sec" style="left:{s.x}px;width:{s.w}px"><span>{s.name}</span></div>
      {/each}
    </div>
    <div class="playhead" style="left:{playhead * pxPerSecond}px"></div>
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .scroller {
    overflow: auto;
    flex: 1;
    min-width: 0;
    background: var(--canvas);
  }
  .surface {
    position: relative;
  }
  canvas {
    display: block;
    position: absolute;
    top: 0;
    left: 0;
  }
  .events {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  /* Named sections. Read-only for now — no add/resize/move yet. */
  .sections {
    position: absolute;
    left: 0;
    width: 100%;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    pointer-events: none;
  }
  .sec {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    padding: 0 8px;
    overflow: hidden;
    border-left: 1px solid var(--line-2);
  }
  .sec span {
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--fg);
    pointer-events: none;
    z-index: 5;
  }
  .playhead::before {
    content: "";
    position: absolute;
    top: 0;
    left: -4px;
    width: 0;
    height: 0;
    border-left: 5px solid transparent;
    border-right: 5px solid transparent;
    border-top: 7px solid var(--fg);
  }
  .events .defs {
    position: absolute;
  }
  .clip {
    position: absolute;
    box-sizing: border-box;
  }
  .clip-label {
    font-size: 10px;
    line-height: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* hold = tinted pill. Flat wash, hairline border, label inside. */
  .clip.hold {
    display: flex;
    align-items: center;
    padding: 0 8px;
    border: 1px solid color-mix(in srgb, var(--hold) 42%, transparent);
    border-radius: var(--clip-r);
    overflow: hidden;
    background: color-mix(in srgb, var(--hold) var(--tint), var(--canvas));
  }
  .clip.hold .clip-label {
    color: color-mix(in srgb, var(--hold) 75%, var(--fg));
    font-weight: 500;
  }
  .clip.hold.sel {
    border-color: var(--hold);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--hold) 25%, transparent);
  }
  /* one-shot is an instant, so it reads as a point — not a stem across the lane */
  .shot {
    position: absolute;
    display: flex;
    align-items: center;
    /* the box is grab slop — the dot itself must sit on the dispatch tick */
    justify-content: center;
    overflow: visible;
  }
  .shot b {
    width: 9px;
    height: 9px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--shot);
  }
  .shot.sel b {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--shot) 35%, transparent);
  }
  /* flag: anchored to the dot, never wider than the gap to the next event on the Lane */
  .shot-label {
    position: absolute;
    left: 50%;
    margin-left: 9px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 10px;
    color: var(--fg-2);
  }
  /* Automation Lane: a recessed display, not a clip — no glass, no grab handles. */
  .autolane {
    position: absolute;
    box-sizing: border-box;
  }
  .autolane.off {
    opacity: 0.45;
  }
  .autolane svg {
    position: absolute;
    inset: 0;
    display: block;
  }
  .autolane path {
    fill: none;
    vector-effect: non-scaling-stroke;
  }
  .autolane .line {
    stroke: var(--auto);
    stroke-width: 1.5;
  }
  /* before the first and after the last point the value is held, not drawn */
  .autolane .held {
    stroke: var(--auto);
    stroke-width: 1.5;
    stroke-dasharray: 3 4;
    opacity: 0.55;
  }
  .autolane .area {
    fill: color-mix(in srgb, var(--auto) 13%, transparent);
    stroke: none;
  }
  .lane-name {
    position: absolute;
    left: 6px;
    top: 3px;
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    color: var(--fg-3);
    pointer-events: none;
  }
  .lane-off {
    padding: 0 3px;
    border-radius: 3px;
    background: var(--sunken);
    color: var(--fg-2);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  /* collapsed strip: "there is automation here you cannot see" */
  .autoghost {
    position: absolute;
    display: block;
    pointer-events: none;
  }
  .autoghost path {
    fill: none;
    stroke: var(--auto);
    stroke-width: 1;
    opacity: 0.3;
    vector-effect: non-scaling-stroke;
  }
  .node {
    position: absolute;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--canvas);
    border: 1.5px solid var(--auto);
  }
  .node.sel {
    background: var(--fg);
    border-color: var(--fg);
  }
  .seg-handle {
    position: absolute;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: transparent;
    border: 1.5px solid var(--auto);
    opacity: 0.6;
  }
  .node-label {
    position: absolute;
    transform: translate(-50%, -150%);
    padding: 0 3px;
    font-size: 9px;
    line-height: 1.3;
    color: var(--auto);
    background: color-mix(in srgb, var(--canvas) 70%, transparent);
    border-radius: var(--r-sm);
    pointer-events: none;
    white-space: nowrap;
  }
</style>
