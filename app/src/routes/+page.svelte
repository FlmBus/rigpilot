<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import Timeline from "$lib/Timeline.svelte";
  import Palette from "$lib/Palette.svelte";
  import Inspector from "$lib/Inspector.svelte";
  import Modal from "$lib/Modal.svelte";
  import Num from "$lib/Num.svelte";
  import { loadAudio, Player, type LoadedAudio, type TrackPlayback } from "$lib/audio";
  import { detectAccents, type Accent } from "$lib/accents";
  import { History } from "$lib/undo";
  import { trackColor } from "$lib/trackcolor";
  import {
    PPQN,
    PROJECT_FORMAT_VERSION,
    RULER_H,
    SECTIONS_H,
    clampLaneHeight,
    secondsPerBeat,
    secondsToTick,
    snapGridTicks,
    snapLabel,
    tickToSeconds,
    trackLayout,
    type BpSelection,
    type Breakpoint,
    type CommandInfo,
    type DefinitionInfo,
    type EventRef,
    type Project,
    type RpEvent,
    type SnapFlavor,
  } from "$lib/types";
  import {
    automationCommands,
    clampValue,
    convertValue,
    curveFor,
    curvesInUse,
    findCurve,
    rangeOf,
    valueSpace,
    type ValueSpace,
  } from "$lib/automation";
  import AutoSelector from "$lib/AutoSelector.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
  import { IS_TAURI, mockDefinitions, mockProject } from "$lib/devmock";

  let definitions = $state<DefinitionInfo[]>([]);
  let project = $state<Project>(IS_TAURI ? newProject() : mockProject);
  let projectPath = $state<string | null>(null);
  let savedJson = $state(JSON.stringify(IS_TAURI ? newProject() : mockProject));
  let status = $state("");

  // transport / view
  let playheadSec = $state(0);
  let isPlaying = $state(false);
  let snapOn = $state(true);
  let snapDivision = $state(4); // grid = bar/N (default 1/4 note)
  let snapFlavor = $state<SnapFlavor>("straight"); // straight, dotted or triplet steps
  let snapSeconds = $state(1); // raw-time grid step
  let gridMode = $state<"musical" | "time">("musical");
  let pxPerSecond = $state(40);
  let coloredWaves = $state(true);
  let showAccents = $state(false);
  /** 0..1; the issue asks for a bias towards too many markers rather than too few. */
  let accentSensitivity = $state(0.7);
  let followPlayhead = $state(false);
  let timelineRef = $state<Timeline | undefined>();

  /** Grid-menu rows: the flavor modifies every division, so neither list has to grow. */
  const SNAP_DIVISIONS = [1, 2, 4, 8, 16, 32, 64];
  const SNAP_FLAVORS: [SnapFlavor, string, string][] = [
    ["straight", "1/n", "Straight grid"],
    ["dotted", "1/n.", "Dotted grid — one and a half steps"],
    ["triplet", "1/nT", "Triplet grid — two thirds of a step"],
  ];

  function zoomBy(factor: number) {
    pxPerSecond = Math.max(4, Math.min(800, pxPerSecond * factor));
  }

  // selection — at most one of these is ever set (see the select* helpers)
  let selection = $state<EventRef[]>([]);
  let selectedTrack = $state<number | null>(null);
  let bpSelection = $state<BpSelection | null>(null);
  let laneSelection = $state<number | null>(null);
  let clipboard: { rel: number; lane: number; ev: RpEvent }[] = [];
  /** Single value from "Copy value", carried across Commands by proportion. */
  let valueClip = $state<{ value: number; space: ValueSpace } | null>(null);
  /** Breakpoints have their own clipboard, so copying a curve never eats a copied event. */
  let bpClipboard: { rel: number; value: number; shape: Breakpoint["shape"]; tension: number }[] = [];
  let bpClipboardSpace: ValueSpace | null = null;

  // dialogs / menus
  let menuOpen = $state(false);
  let dragTrack = $state<number | null>(null);
  let dropIndex = $state<number | null>(null);
  let settingsTrack = $state<number | null>(null);
  let settingsIsNew = $state(false);
  let exportOpen = $state(false);
  let resetBlock = $state(true);
  /** End of the song in ticks: the last thing that happens on any track, audio included.
   *  Export needs it for curves that re-send their value periodically. */
  const songEndTicks = $derived.by(() => {
    let end = 0;
    project.tracks.forEach((t, i) => {
      if (t.type === "audio") {
        const a = loadedAudio[i];
        if (a) end = Math.max(end, t.offsetTicks + secondsToTick(a.buffer.duration, project.bpm));
      } else {
        for (const ev of t.events) end = Math.max(end, ev.tick + (ev.length ?? 0));
        for (const c of t.automation ?? [])
          for (const b of c.breakpoints) end = Math.max(end, b.tick);
      }
    });
    return Math.round(end);
  });

  /** Curves switched off but carrying points — Export silently skips them, so say so. */
  const skippedCurves = $derived(
    project.tracks.reduce(
      (n, t) =>
        n +
        (t.type === "midi"
          ? (t.automation ?? []).filter((c) => !c.enabled && c.breakpoints.length > 0).length
          : 0),
      0,
    ),
  );

  // Remembered custom export folder (survives restarts). When unset the export
  // defaults to the project's own directory.
  let lastExportDir = $state<string | null>(
    typeof localStorage !== "undefined" ? localStorage.getItem("rigpilot:lastExportDir") : null,
  );
  let paletteOpen = $state(true);
  let inspectorOpen = $state(true);
  let gridMenuOpen = $state(false);

  // Live MIDI output. The chosen port + on/off state are machine-level (a
  // bandmate's ports differ), so they live in localStorage, not the project.
  const ls = typeof localStorage !== "undefined" ? localStorage : null;
  let midiLive = $state(ls?.getItem("rigpilot:midiLive") === "1");
  let midiPort = $state<string | null>(ls?.getItem("rigpilot:midiPort") ?? null);
  let midiPorts = $state<string[]>([]);
  // Shared lead-in (ms) so the MIDI scheduler and audio clock start aligned.
  const MIDI_LEAD_MS = 60;

  async function refreshMidiPorts() {
    if (!IS_TAURI) return;
    try {
      midiPorts = await invoke<string[]>("list_midi_ports");
      // Drop a remembered port that has since disappeared.
      if (midiPort && !midiPorts.includes(midiPort)) midiPort = null;
    } catch (e) {
      status = String(e);
    }
  }
  function setMidiLive(on: boolean) {
    midiLive = on;
    ls?.setItem("rigpilot:midiLive", on ? "1" : "0");
    if (on) refreshMidiPorts();
    else if (isPlaying) invoke("midi_stop").catch(() => {});
  }
  function setMidiPort(name: string) {
    midiPort = name || null;
    if (midiPort) ls?.setItem("rigpilot:midiPort", midiPort);
    else ls?.removeItem("rigpilot:midiPort");
  }
  async function midiPanic() {
    if (!IS_TAURI || !midiPort) return;
    try {
      await invoke("midi_panic", { portName: midiPort });
      status = "MIDI panic — all notes off.";
    } catch (e) {
      status = String(e);
    }
  }

  // per-track accent colour for the header bar (model has no colour field)

  const appWindow = IS_TAURI
    ? getCurrentWindow()
    : ({ minimize() {}, toggleMaximize() {}, close() {}, startDragging() {} } as ReturnType<
        typeof getCurrentWindow
      >);

  // The topbar is the window's titlebar (frameless window), but it is packed with controls, so
  // `data-tauri-drag-region` — which only matches the exact element under the pointer — never
  // fires. Instead every part of the bar that is not itself interactive drags the window.
  const NO_DRAG = "button, input, select, textarea, a, .num, [role='button'], [role='menuitem']";
  const isDragSurface = (e: PointerEvent | MouseEvent) =>
    e.button === 0 && !(e.target as HTMLElement)?.closest?.(NO_DRAG);
  function startWindowDrag(e: PointerEvent) {
    if (!isDragSurface(e)) return;
    menuOpen = false;
    gridMenuOpen = false;
    appWindow.startDragging();
  }
  const player = new Player();
  const audioCache = new Map<string, LoadedAudio>();
  let loadedAudio = $state<(LoadedAudio | null)[]>([]);
  const history = new History();

  function newProject(): Project {
    return {
      formatVersion: PROJECT_FORMAT_VERSION,
      name: "Untitled Song",
      bpm: 120,
      timeSignature: [4, 4],
      tracks: [],
    };
  }

  $effect(() => {
    if (!IS_TAURI) {
      definitions = mockDefinitions;
      return;
    }
    invoke<DefinitionInfo[]>("list_definitions")
      .then((d) => (definitions = d))
      .catch((e) => (status = String(e)));
    refreshMidiPorts();
  });

  const projectDir = $derived(
    projectPath
      ? projectPath.slice(0, Math.max(projectPath.lastIndexOf("/"), projectPath.lastIndexOf("\\")))
      : null,
  );

  // Where Export writes: a remembered custom folder wins, otherwise the project dir.
  const exportDir = $derived(lastExportDir ?? projectDir);

  const snapTicks = $derived.by(() => {
    if (!snapOn) return null;
    if (gridMode === "musical") return snapGridTicks(project.timeSignature, snapDivision, snapFlavor);
    return secondsToTick(snapSeconds, project.bpm);
  });

  /** Which MIDI track the Command Palette and its "in use" marks refer to. */
  const paletteTrack = $derived.by(() => {
    const candidates = [
      selectedTrack,
      laneSelection,
      bpSelection?.ti ?? null,
      selection.length ? selection[0].ti : null,
    ];
    for (const ti of candidates) {
      if (ti !== null && project.tracks[ti]?.type === "midi") return ti;
    }
    return null;
  });
  const selectedMidiDef = $derived.by(() => {
    if (paletteTrack === null) return undefined;
    const t = project.tracks[paletteTrack];
    return t?.type === "midi" ? defsById.get(t.definitionId) : undefined;
  });
  const paletteInUse = $derived.by(() => {
    const t = paletteTrack !== null ? project.tracks[paletteTrack] : null;
    return new Set(t?.type === "midi" ? curvesInUse(t).map((c) => c.commandId) : []);
  });

  const defsById = $derived(new Map(definitions.map((d) => [d.id, d])));
  /** Single source of truth for vertical geometry, shared with the Timeline. */
  const layout = $derived(trackLayout(project.tracks, defsById));

  function commandName(track: { definitionId: string }, commandId: string): string {
    return (
      defsById.get(track.definitionId)?.commands.find((c) => c.id === commandId)?.name ??
      commandId
    );
  }

  // ---- automation lanes ----
  function setLaneCommand(ti: number, commandId: string | null) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    // View state: saved with the project, but never an undo step.
    t.automationView = { ...t.automationView, command: commandId };
  }

  function showAutomation(ti: number, commandId: string) {
    setLaneCommand(ti, commandId);
    selectTrack(ti);
    status = `Automation: ${commandName(project.tracks[ti] as any, commandId)}`;
  }

  function toggleCurve(ti: number, commandId: string) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    commit();
    const c = curveFor(t, commandId);
    c.enabled = !c.enabled;
    status = c.enabled
      ? `${commandName(t, commandId)}: curve on`
      : `${commandName(t, commandId)}: curve off — skipped on export`;
  }

  function laneAction(
    action: "constant" | "clear" | "toggle",
    ti: number,
    commandId: string,
  ) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    if (action === "toggle") return toggleCurve(ti, commandId);
    if (action === "clear") {
      commit();
      curveFor(t, commandId).breakpoints = [];
      bpSelection = null;
      status = `${commandName(t, commandId)}: curve cleared`;
      return;
    }
    const cmd = defsById.get(t.definitionId)?.commands.find((c) => c.id === commandId);
    if (!cmd) return;
    const [min, max] = rangeOf(cmd);
    const current = findCurve(t, commandId)?.breakpoints[0]?.value;
    openValuePrompt({
      title: `Set ${cmd.name} to a constant`,
      cmd,
      value: current ?? clampValue(cmd, Math.round((min + max) / 2)),
      onok: (v) => {
        commit();
        curveFor(t, commandId).breakpoints = [
          { tick: 0, value: v, shape: cmd.steps.length ? "hold" : "linear", tension: 0 },
        ];
        setLaneCommand(ti, commandId);
        status = `${cmd.name}: held at ${v} for the whole song`;
      },
    });
  }

  function editBreakpointValue(ti: number, commandId: string, index: number) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    const cmd = defsById.get(t.definitionId)?.commands.find((c) => c.id === commandId);
    const curve = findCurve(t, commandId);
    const point = curve?.breakpoints[index];
    if (!cmd || !point) return;
    openValuePrompt({
      title: `${cmd.name} — point value`,
      cmd,
      value: point.value,
      onok: (v) => {
        commit();
        point.value = v;
      },
    });
  }

  function copyValue(value: number, space: ValueSpace) {
    valueClip = { value, space };
    status = `Copied value ${value}`;
  }

  function curveAndCommand(ti: number, commandId: string) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return null;
    const cmd = defsById.get(t.definitionId)?.commands.find((c) => c.id === commandId);
    const curve = findCurve(t, commandId);
    return cmd ? { track: t, cmd, curve } : null;
  }

  function copyBreakpoints() {
    if (!bpSelection) return;
    const ctx = curveAndCommand(bpSelection.ti, bpSelection.commandId);
    if (!ctx?.curve) return;
    const pts = bpSelection.idx.map((i) => ctx.curve!.breakpoints[i]).filter(Boolean);
    if (!pts.length) return;
    const t0 = Math.min(...pts.map((p) => p.tick));
    bpClipboard = pts.map((p) => ({
      rel: p.tick - t0,
      value: p.value,
      shape: p.shape,
      tension: p.tension,
    }));
    bpClipboardSpace = valueSpace(ctx.cmd);
    status = `Copied ${pts.length} breakpoint${pts.length === 1 ? "" : "s"}`;
  }

  function pasteBreakpoints() {
    const ti = bpSelection?.ti ?? laneSelection;
    if (ti === null || !bpClipboard.length || !bpClipboardSpace) return;
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    const commandId = t.automationView.command ?? bpSelection?.commandId;
    if (!commandId) return;
    const ctx = curveAndCommand(ti, commandId);
    if (!ctx) return;
    commit();
    const curve = curveFor(t, commandId);
    const at = secondsToTick(playheadSec, project.bpm);
    const span = Math.max(...bpClipboard.map((p) => p.rel));
    // whatever sits in the pasted stretch is replaced
    curve.breakpoints = curve.breakpoints.filter((b) => b.tick < at || b.tick > at + span);
    const dst = valueSpace(ctx.cmd);
    const stepped = ctx.cmd.steps.length > 0;
    for (const p of bpClipboard) {
      curve.breakpoints.push({
        tick: at + p.rel,
        // values carry across Commands by proportion, not by raw number
        value: convertValue(p.value, bpClipboardSpace, dst),
        shape: stepped ? "hold" : p.shape,
        tension: p.tension,
      });
    }
    curve.breakpoints.sort((a, b) => a.tick - b.tick);
    const ticks = new Set(bpClipboard.map((p) => at + p.rel));
    selectBreakpoints({
      ti,
      commandId,
      idx: curve.breakpoints.map((b, i) => (ticks.has(b.tick) ? i : -1)).filter((i) => i >= 0),
    });
    setLaneCommand(ti, commandId);
    status = `Pasted ${bpClipboard.length} breakpoint${bpClipboard.length === 1 ? "" : "s"} at the playhead`;
  }

  function nudgeBreakpoints(dir: number) {
    if (!bpSelection) return;
    const ctx = curveAndCommand(bpSelection.ti, bpSelection.commandId);
    if (!ctx?.curve) return;
    commit();
    const steps = ctx.cmd.steps.map((st) => st.value).sort((a, b) => a - b);
    for (const i of bpSelection.idx) {
      const b = ctx.curve.breakpoints[i];
      if (!b) continue;
      if (steps.length) {
        const at = steps.indexOf(b.value);
        b.value = steps[Math.min(steps.length - 1, Math.max(0, (at < 0 ? 0 : at) + dir))];
      } else {
        b.value = clampValue(ctx.cmd, b.value + dir);
      }
    }
  }

  function deleteBreakpoints() {
    if (!bpSelection) return;
    const t = project.tracks[bpSelection.ti];
    if (t.type !== "midi") return;
    const curve = findCurve(t, bpSelection.commandId);
    if (!curve) return;
    commit();
    for (const i of [...bpSelection.idx].sort((a, b) => b - a)) curve.breakpoints.splice(i, 1);
    const n = bpSelection.idx.length;
    bpSelection = null;
    status = `Deleted ${n} breakpoint${n === 1 ? "" : "s"}`;
  }

  // A small prompt shared by "Set to constant…" and "Edit value…".
  let valuePrompt = $state<{
    title: string;
    cmd: CommandInfo;
    value: number;
    onok: (v: number) => void;
  } | null>(null);
  function openValuePrompt(p: NonNullable<typeof valuePrompt>) {
    valuePrompt = p;
  }
  function confirmValuePrompt() {
    if (!valuePrompt) return;
    const { onok, cmd, value } = valuePrompt;
    valuePrompt = null;
    onok(clampValue(cmd, value));
  }

  /** Lane menu opened from the header's ⋯ button (same items as right-click). */
  let headerMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function openLaneMenu(e: MouseEvent, ti: number, commandId: string) {
    e.stopPropagation();
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    const curve = findCurve(t, commandId);
    const on = curve?.enabled ?? true;
    headerMenu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Set to constant…", onselect: () => laneAction("constant", ti, commandId) },
        {
          label: "Clear curve",
          disabled: !curve?.breakpoints.length,
          danger: true,
          onselect: () => laneAction("clear", ti, commandId),
        },
        { separator: true },
        {
          label: on ? "Switch curve off" : "Switch curve on",
          checked: on,
          onselect: () => laneAction("toggle", ti, commandId),
        },
      ],
    };
  }

  let laneResize: { ti: number; startY: number; startH: number } | null = null;
  function startLaneResize(e: PointerEvent, ti: number) {
    const t = project.tracks[ti];
    if (t.type !== "midi") return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    laneResize = { ti, startY: e.clientY, startH: t.automationView.height };
  }
  function dragLaneResize(e: PointerEvent) {
    if (!laneResize) return;
    const t = project.tracks[laneResize.ti];
    if (t.type !== "midi") return;
    t.automationView = {
      ...t.automationView,
      height: clampLaneHeight(laneResize.startH + (e.clientY - laneResize.startY)),
    };
  }

  // ---- undo ----
  function commit() {
    history.push($state.snapshot(project));
  }
  function undo() {
    const before = $state.snapshot(project) as Project;
    const prev = history.undo(before);
    if (prev) {
      applyRestored(prev as Project, before);
      status = "Undo";
    }
  }
  function redo() {
    const before = $state.snapshot(project) as Project;
    const next = history.redo(before);
    if (next) {
      applyRestored(next as Project, before);
      status = "Redo";
    }
  }

  /**
   * Undo restores data, not the view: which curve a lane shows and how tall it is
   * stay put. The one exception is an edit to a curve that isn't on screen — then
   * the selector switches to it, so the user sees what just changed.
   */
  function applyRestored(next: Project, before: Project) {
    next.tracks.forEach((t, i) => {
      const cur = before.tracks[i];
      if (t.type === "midi" && cur?.type === "midi") {
        t.automationView = { ...cur.automationView };
      }
    });
    for (let i = 0; i < next.tracks.length; i++) {
      const a = next.tracks[i];
      const b = before.tracks[i];
      if (a?.type !== "midi" || b?.type !== "midi") continue;
      const ids = new Set([
        ...(a.automation ?? []).map((c) => c.commandId),
        ...(b.automation ?? []).map((c) => c.commandId),
      ]);
      for (const id of ids) {
        const one = JSON.stringify((a.automation ?? []).find((c) => c.commandId === id) ?? null);
        const two = JSON.stringify((b.automation ?? []).find((c) => c.commandId === id) ?? null);
        if (one !== two && a.automationView.command !== id) {
          a.automationView = { ...a.automationView, command: id };
          break;
        }
      }
    }
    project = next;
    selection = [];
    bpSelection = null;
    laneSelection = null;
  }

  // ---- audio loading ----
  $effect(() => {
    const dir = projectDir;
    const files = project.tracks.map((t) => (t.type === "audio" ? t.file : null));
    Promise.all(
      files.map(async (file) => {
        if (!file || !dir) return null;
        const abs = `${dir}/${file}`;
        if (!audioCache.has(abs)) {
          try {
            audioCache.set(abs, await loadAudio(abs));
          } catch (e) {
            status = `Audio: ${e}`;
            return null;
          }
        }
        return audioCache.get(abs) ?? null;
      }),
    ).then((result) => (loadedAudio = result));
  });

  // ---- accent detection ----
  // Derived, never stored in the project: the markers are a reading of the
  // audio, so they follow the file and the section grid instead of ageing into
  // stale data the user has to re-run. Sections are passed in file-local
  // seconds, since the detector budgets its markers per section.
  const accents = $derived.by<(Accent[] | null)[]>(() => {
    if (!showAccents) return project.tracks.map(() => null);
    return project.tracks.map((t, i) => {
      const a = loadedAudio[i];
      if (t.type !== "audio" || !a) return null;
      const start = tickToSeconds(t.offsetTicks, project.bpm);
      const bounds = (project.sections ?? [])
        .map((sec) => tickToSeconds(sec.tick, project.bpm) - start)
        .filter((sec) => sec > 0 && sec < a.buffer.duration);
      return detectAccents(a, bounds, { sensitivity: accentSensitivity });
    });
  });

  // ---- playback ----
  function playbackTracks(): TrackPlayback[] {
    const anySolo = project.tracks.some((t) => t.solo);
    const out: TrackPlayback[] = [];
    project.tracks.forEach((t, i) => {
      const a = loadedAudio[i];
      if (t.type !== "audio" || !a) return;
      out.push({
        audio: a,
        offsetSeconds: tickToSeconds(t.offsetTicks, project.bpm),
        volume: t.volume,
        pan: t.pan,
        audible: !t.mute && (!anySolo || t.solo),
      });
    });
    return out;
  }

  $effect(() => {
    const tracks = playbackTracks();
    if (isPlaying) player.update(tracks);
  });

  async function play() {
    await player.play(playbackTracks(), playheadSec, MIDI_LEAD_MS / 1000);
    isPlaying = true;
    if (IS_TAURI && midiLive && midiPort) {
      invoke("midi_start", {
        project: $state.snapshot(project),
        portName: midiPort,
        fromSeconds: playheadSec,
        startInMs: MIDI_LEAD_MS,
        options: { resetBlock: true, songEndTicks },
      }).catch((e) => (status = String(e)));
    } else if (IS_TAURI && midiLive && !midiPort) {
      status = "Live MIDI is on, but no output port is selected.";
    }
    const tick = () => {
      if (!isPlaying) return;
      playheadSec = player.position(playheadSec);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
  function pause() {
    playheadSec = player.stop();
    isPlaying = false;
    if (IS_TAURI && midiLive) invoke("midi_stop").catch(() => {});
  }
  function stop() {
    if (isPlaying) pause();
    else playheadSec = 0;
  }
  function seek(seconds: number) {
    playheadSec = Math.max(0, seconds);
    if (isPlaying) play();
  }

  const positionLabel = $derived.by(() => {
    const t = Math.max(0, playheadSec);
    const m = Math.floor(t / 60);
    const s = (t % 60).toFixed(1).padStart(4, "0");
    const beats = t / secondsPerBeat(project.bpm);
    const bar = Math.floor(beats / project.timeSignature[0]) + 1;
    const beat = Math.floor(beats % project.timeSignature[0]) + 1;
    return `${m}:${s} · ${bar}.${beat}`;
  });

  // ---- selection / editing ----
  function selectTrack(i: number | null) {
    selectedTrack = i;
    if (i !== null) {
      selection = [];
      bpSelection = null;
      laneSelection = null;
    }
  }
  // Selections never mix: Delete and Ctrl+C must always have one obvious meaning.
  function onselectionchange(refs: EventRef[]) {
    selection = refs;
    if (refs.length > 0) {
      selectedTrack = null;
      bpSelection = null;
      laneSelection = null;
    }
  }
  function selectBreakpoints(sel: BpSelection | null) {
    bpSelection = sel;
    if (sel) {
      selection = [];
      selectedTrack = null;
      laneSelection = null;
    }
  }
  function selectLane(ti: number | null) {
    laneSelection = ti;
    if (ti !== null) {
      selection = [];
      selectedTrack = null;
      bpSelection = null;
    }
  }

  function makeEvent(cmd: CommandInfo, tick: number, lane: number): RpEvent {
    const params: Record<string, number> = {};
    for (const p of cmd.params) params[p.id] = p.default ?? p.min;
    const base = { commandId: cmd.id, tick, params, lane };
    if (cmd.commandType === "hold") return { ...base, kind: "hold", length: PPQN * 4 };
    return { ...base, kind: "one-shot" };
  }

  function createEvent(ti: number, commandId: string, tick: number, lane: number) {
    const track = project.tracks[ti];
    if (track.type !== "midi") return;
    const cmd = definitions
      .find((d) => d.id === track.definitionId)
      ?.commands.find((c) => c.id === commandId);
    if (!cmd) return;
    if (cmd.commandType === "automation") {
      showAutomation(ti, commandId);
      return;
    }
    commit();
    track.events.push(makeEvent(cmd, tick, lane));
    selection = [{ ti, ei: track.events.length - 1 }];
  }

  function deleteSelection() {
    if (selection.length === 0) return;
    commit();
    const byTrack = new Map<number, number[]>();
    for (const r of selection) {
      if (!byTrack.has(r.ti)) byTrack.set(r.ti, []);
      byTrack.get(r.ti)!.push(r.ei);
    }
    for (const [ti, eis] of byTrack) {
      const t = project.tracks[ti];
      if (t.type !== "midi") continue;
      for (const ei of eis.sort((a, b) => b - a)) t.events.splice(ei, 1);
    }
    selection = [];
  }

  function copySelection() {
    if (selection.length === 0) return;
    const events = selection
      .map((r) => {
        const t = project.tracks[r.ti];
        return t.type === "midi" ? { r, ev: t.events[r.ei] } : null;
      })
      .filter((x) => x !== null);
    if (events.length === 0) return;
    const minTick = Math.min(...events.map((x) => x!.ev.tick));
    clipboard = events.map((x) => ({
      rel: x!.ev.tick - minTick,
      lane: x!.ev.lane,
      ev: JSON.parse(JSON.stringify($state.snapshot(x!.ev))),
    }));
    // remember source tracks so paste lands on the same ones
    clipboardTracks = events.map((x) => x!.r.ti);
    status = `Copied ${clipboard.length} event(s)`;
  }
  let clipboardTracks: number[] = [];

  function paste() {
    if (clipboard.length === 0) return;
    commit();
    const at = secondsToTick(playheadSec, project.bpm);
    const newRefs: EventRef[] = [];
    clipboard.forEach((c, i) => {
      const ti = clipboardTracks[i];
      const t = project.tracks[ti];
      if (!t || t.type !== "midi") return;
      const ev: RpEvent = JSON.parse(JSON.stringify(c.ev));
      ev.tick = at + c.rel;
      t.events.push(ev);
      newRefs.push({ ti, ei: t.events.length - 1 });
    });
    selection = newRefs;
    status = `Pasted ${newRefs.length} event(s) at playhead`;
  }

  function onkeydown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.code === "Space") {
      e.preventDefault();
      isPlaying ? pause() : play();
    } else if (e.key === "Delete" || e.key === "Backspace") {
      if (bpSelection) deleteBreakpoints();
      else deleteSelection();
    } else if (mod && e.key === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (mod && (e.key === "y" || (e.key === "z" && e.shiftKey) || e.key === "Z")) {
      e.preventDefault();
      redo();
    } else if (mod && e.key === "c") {
      if (bpSelection) copyBreakpoints();
      else copySelection();
    } else if (mod && e.key === "v") {
      if (bpSelection || laneSelection !== null) pasteBreakpoints();
      else paste();
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && bpSelection) {
      e.preventDefault();
      nudgeBreakpoints(e.key === "ArrowUp" ? 1 : -1);
    } else if (mod && e.key === "s") {
      e.preventDefault();
      saveProject(false);
    } else if (e.key === "Escape") {
      selection = [];
      selectedTrack = null;
      bpSelection = null;
      laneSelection = null;
    }
  }

  // ---- tracks ----
  function addMidiTrack() {
    if (definitions.length === 0) return;
    commit();
    project.tracks.push({
      type: "midi",
      name: `MIDI ${project.tracks.filter((t) => t.type === "midi").length + 1}`,
      definitionId: definitions[0].id,
      midiChannel: 1,
      latencyMs: definitions[0].defaultLatency,
      mute: false,
      solo: false,
      events: [],
      automation: [],
      automationView: { command: null, height: 96 },
    });
    selectTrack(project.tracks.length - 1);
    settingsTrack = project.tracks.length - 1;
    settingsIsNew = true;
  }

  async function addAudioTrack() {
    if (!IS_TAURI) {
      commit();
      project.tracks.push({
        type: "audio", name: `Audio ${project.tracks.filter((t) => t.type === "audio").length + 1}`,
        file: "demo.wav", volume: 1, pan: 0, mute: false, solo: false, offsetTicks: 0, waveformGain: 1,
      });
      selectTrack(project.tracks.length - 1);
      status = "Added a mock audio track (browser dev — import needs the desktop app).";
      return;
    }
    // pick the file FIRST, then sort out the project location
    const source = await open({
      filters: [{ name: "Audio", extensions: ["wav", "mp3", "flac", "ogg", "m4a", "aiff"] }],
    });
    if (typeof source !== "string") return;
    if (!projectPath) {
      const ok = await ask(
        "Audio files are copied into an assets folder next to the project file, so the project needs a location first. Save it now?",
        { title: "Save project", kind: "info" },
      );
      if (!ok) return;
      await saveProject(true);
      if (!projectPath) return;
    }
    try {
      const file = await invoke<string>("import_audio", { projectPath, source });
      commit();
      project.tracks.push({
        type: "audio",
        name: file.split("/").pop() ?? "Audio",
        file,
        volume: 1,
        pan: 0,
        mute: false,
        solo: false,
        offsetTicks: 0,
        waveformGain: 1,
      });
      selectTrack(project.tracks.length - 1);
      status = `Imported ${file}`;
    } catch (e) {
      status = String(e);
    }
  }

  function closeTrackSettings(confirmed: boolean) {
    if (settingsIsNew && !confirmed && settingsTrack !== null) {
      // closing the dialog of a freshly created track cancels the creation
      project.tracks.splice(settingsTrack, 1);
      selection = [];
      selectedTrack = null;
    }
    settingsTrack = null;
    settingsIsNew = false;
  }

  function reorderTrack(from: number, to: number) {
    if (from === to || from + 1 === to) {
      return;
    }
    commit();
    const [moved] = project.tracks.splice(from, 1);
    project.tracks.splice(from < to ? to - 1 : to, 0, moved);
    selection = [];
    selectedTrack = project.tracks.indexOf(moved);
  }

  async function removeTrack(ti: number) {
    const t = project.tracks[ti];
    const hasContent = t.type === "midi" ? t.events.length > 0 : true;
    if (hasContent) {
      const ok = await ask(`Remove track "${t.name}"?`, { title: "Remove track", kind: "warning" });
      if (!ok) return;
    }
    commit();
    project.tracks.splice(ti, 1);
    selection = [];
    selectedTrack = null;
  }

  /** Curves and events belong to the Device's Commands, so the Definition is
   *  fixed once a track holds anything — see the dialog, where it is disabled. */
  function trackIsEmpty(t: { events: RpEvent[]; automation?: { breakpoints: unknown[] }[] }) {
    return t.events.length === 0 && (t.automation ?? []).every((c) => c.breakpoints.length === 0);
  }

  function changeDefinition(ti: number, definitionId: string) {
    const t = project.tracks[ti];
    if (t.type !== "midi" || t.definitionId === definitionId) return;
    if (!trackIsEmpty(t)) return;
    commit();
    t.definitionId = definitionId;
    t.latencyMs = definitions.find((d) => d.id === definitionId)?.defaultLatency ?? 0;
    t.events = [];
    t.automation = [];
    t.automationView = { command: null, height: 96 };
    selection = [];
  }

  // ---- files ----
  const isDirty = () => JSON.stringify($state.snapshot(project)) !== savedJson;

  async function confirmDiscard(): Promise<boolean> {
    if (!isDirty()) return true;
    return await ask("The project has unsaved changes. Discard them?", {
      title: "Unsaved changes",
      kind: "warning",
    });
  }

  async function closeWindow() {
    if (await confirmDiscard()) appWindow.close();
  }

  async function newProjectAction() {
    if (!(await confirmDiscard())) return;
    pause();
    project = newProject();
    projectPath = null;
    savedJson = JSON.stringify($state.snapshot(project));
    playheadSec = 0;
    selection = [];
    selectedTrack = null;
    history.clear();
  }

  async function openProject() {
    if (!IS_TAURI) {
      status = "Open/Save are only available in the desktop app (browser dev mode).";
      return;
    }
    if (!(await confirmDiscard())) return;
    const path = await open({ filters: [{ name: "RigPilot", extensions: ["rigpilot"] }] });
    if (typeof path !== "string") return;
    try {
      project = await invoke<Project>("load_project", { path });
      projectPath = path;
      savedJson = JSON.stringify($state.snapshot(project));
      playheadSec = 0;
      selection = [];
      selectedTrack = null;
      history.clear();
      status = `Opened ${path}`;
    } catch (e) {
      await message(String(e), { title: "Can't open this project", kind: "error" });
      status = String(e);
    }
  }

  async function saveProject(as = false) {
    if (!IS_TAURI) {
      savedJson = JSON.stringify($state.snapshot(project));
      status = "Saved (browser dev — no file written).";
      return;
    }
    let path = projectPath;
    if (as || !path) {
      const chosen = await save({
        defaultPath: `${project.name}.rigpilot`,
        filters: [{ name: "RigPilot", extensions: ["rigpilot"] }],
      });
      if (!chosen) return;
      path = chosen;
    }
    try {
      await invoke("save_project", { path, project: $state.snapshot(project) });
      projectPath = path;
      savedJson = JSON.stringify($state.snapshot(project));
      status = `Saved ${path}`;
    } catch (e) {
      status = String(e);
    }
  }

  // Pick a custom export folder and remember it for next time.
  async function chooseExportDir() {
    const dir = await open({ directory: true, defaultPath: exportDir ?? undefined });
    if (typeof dir !== "string") return;
    lastExportDir = dir;
    localStorage.setItem("rigpilot:lastExportDir", dir);
  }

  async function runExport() {
    if (!IS_TAURI) {
      exportOpen = false;
      status = "Export needs the desktop app (browser dev mode).";
      return;
    }
    // Default to the project dir; only prompt when there is no target yet
    // (unsaved project and no remembered folder).
    let dir = exportDir;
    if (!dir) {
      const chosen = await open({ directory: true });
      if (typeof chosen !== "string") return;
      dir = chosen;
      lastExportDir = dir;
      localStorage.setItem("rigpilot:lastExportDir", dir);
    }
    try {
      const files = await invoke<string[]>("export_midi", {
        project: $state.snapshot(project),
        outDir: dir,
        options: { resetBlock, songEndTicks },
      });
      status = files.length
        ? `Exported to ${dir}: ${files.join(", ")}`
        : "Nothing to export — add a MIDI track first.";
      exportOpen = false;
    } catch (e) {
      status = String(e);
    }
  }
</script>

<svelte:window
  onkeydown={onkeydown}
  onpointerdown={(e) => {
    const t = e.target as HTMLElement;
    if (!t?.closest?.(".menu") && !t?.closest?.(".snapgroup")) {
      menuOpen = false;
      gridMenuOpen = false;
    }
  }}
/>

<div class="app">
  <header
    class="topbar"
    onpointerdown={startWindowDrag}
    ondblclick={(e) => isDragSurface(e) && appWindow.toggleMaximize()}
  >
    <div class="tb-left">
      <span class="grip" title="Drag to move the window">⠿</span>
      <span class="brand">Rig<span class="accent">Pilot</span></span>
      <div class="menu">
        <button class="hamb ghost" class:active={menuOpen} onclick={() => (menuOpen = !menuOpen)}>☰</button>
        {#if menuOpen}
          <div class="dropdown glass" role="menu" tabindex="-1">
            <button role="menuitem" onclick={() => { menuOpen = false; newProjectAction(); }}>New</button>
            <button role="menuitem" onclick={() => { menuOpen = false; openProject(); }}>Open…</button>
            <hr />
            <button role="menuitem" onclick={() => { menuOpen = false; saveProject(false); }}>Save <span class="kbd">Ctrl+S</span></button>
            <button role="menuitem" onclick={() => { menuOpen = false; saveProject(true); }}>Save As…</button>
          </div>
        {/if}
      </div>
      <input class="song" type="text" bind:value={project.name} title="Song name" />
    </div>

    <div class="tb-center">
      <div class="cellgroup">
        <div class="cell wide" class:on={isPlaying} role="button" tabindex="0" title="Play / Pause (Space)"
          onclick={() => (isPlaying ? pause() : play())} onkeydown={() => {}}>{isPlaying ? "⏸" : "▶"}</div>
        <div class="cell" role="button" tabindex="0" title="Stop" onclick={stop} onkeydown={() => {}}>⏹</div>
      </div>
      <div class="lcd">
        <div class="lc pos"><span class="microlabel">POSITION</span><span class="lcd-main mono">{positionLabel}</span></div>
        <div class="lcd-div"></div>
        <div class="lc"><span class="microlabel">BPM</span><Num min={20} max={300} flat bind:value={project.bpm} /></div>
        <div class="lcd-div"></div>
        <div class="lc"><span class="microlabel">SIG</span><span class="sig"><Num min={1} max={32} flat bind:value={project.timeSignature[0]} /><span class="slash mono">/</span><Num min={1} max={32} flat bind:value={project.timeSignature[1]} /></span></div>
      </div>
    </div>

    <div class="tb-right">
      <button class="toggle" class:active={coloredWaves} title="Spectral waveform coloring"
        onclick={() => (coloredWaves = !coloredWaves)}>Color</button>
      <button class="toggle" class:active={showAccents} title="Mark musically significant hits and accents"
        onclick={() => (showAccents = !showAccents)}>Accents</button>
      {#if showAccents}
        <div class="lc"><span class="microlabel">SENS</span><Num min={0} max={100} flat
          value={Math.round(accentSensitivity * 100)}
          onchange={(v: number) => (accentSensitivity = v / 100)} /></div>
      {/if}
      <span class="vsep"></span>
      <div class="midi-live">
        <button class="toggle" class:active={midiLive}
          title="Send MIDI live to a hardware/virtual port during playback"
          onclick={() => setMidiLive(!midiLive)}>◉ MIDI</button>
        {#if midiLive}
          <select class="port" title="MIDI output port" value={midiPort ?? ""}
            onpointerdown={refreshMidiPorts}
            onchange={(e) => setMidiPort(e.currentTarget.value)}>
            <option value="" disabled>Select port…</option>
            {#each midiPorts as p}
              <option value={p}>{p}</option>
            {/each}
          </select>
          <button class="danger" title="Panic — all notes off" disabled={!midiPort}
            onclick={midiPanic}>⏻</button>
        {/if}
      </div>
      <span class="vsep"></span>
      <button class="primary" onclick={() => (exportOpen = true)} disabled={!project.tracks.some((t) => t.type === "midi")}>Export</button>
      <span class="vsep"></span>
      <div class="cellgroup flat">
        <div class="cell pt" class:on={paletteOpen} role="button" tabindex="0" title="Command Palette"
          onclick={() => (paletteOpen = !paletteOpen)} onkeydown={() => {}}>◧</div>
        <div class="cell pt" class:on={inspectorOpen} role="button" tabindex="0" title="Inspector"
          onclick={() => (inspectorOpen = !inspectorOpen)} onkeydown={() => {}}>◨</div>
      </div>
      <div class="winbtns">
        <button class="ghost win" title="Minimize" onclick={() => appWindow.minimize()}>–</button>
        <button class="ghost win" title="Maximize" onclick={() => appWindow.toggleMaximize()}>□</button>
        <button class="ghost win close" title="Close" onclick={closeWindow}>✕</button>
      </div>
    </div>
  </header>

  <section class="main">
    {#if paletteOpen}
      <Palette
        definition={selectedMidiDef}
        inUse={paletteInUse}
        onshowautomation={(id) => paletteTrack !== null && showAutomation(paletteTrack, id)}
      />
    {/if}
    <div class="center">
    <div class="tlbar">
      <div class="snapgroup">
        <div class="cellgroup flat">
          <div class="cell snap-main" class:on={snapOn} role="button" tabindex="0" title="Toggle snap"
            onclick={() => (snapOn = !snapOn)} onkeydown={() => {}}>
            <span class="microlabel">SNAP {gridMode === "musical"
              ? snapLabel(snapDivision, snapFlavor)
              : `${snapSeconds}s`}</span>
          </div>
          <div class="cell snap-caret" class:open={gridMenuOpen} role="button" tabindex="0" title="Snap grid size"
            onclick={() => (gridMenuOpen = !gridMenuOpen)} onkeydown={() => {}}>▾</div>
        </div>
        {#if gridMenuOpen}
          <div class="grid-menu glass" role="listbox">
            {#if gridMode === "musical"}
              <div class="flavors">
                {#each SNAP_FLAVORS as [f, label, hint]}
                  <button class:on={snapFlavor === f} title={hint} onclick={() => (snapFlavor = f)}>{label}</button>
                {/each}
              </div>
              {#each SNAP_DIVISIONS as d}
                <button class:on={snapDivision === d} onclick={() => { snapDivision = d; snapOn = true; gridMenuOpen = false; }}>{snapLabel(d, snapFlavor)}</button>
              {/each}
            {:else}
              {#each [5, 1, 0.5, 0.1] as s}
                <button class:on={snapSeconds === s} onclick={() => { snapSeconds = s; snapOn = true; gridMenuOpen = false; }}>{s} s</button>
              {/each}
            {/if}
          </div>
        {/if}
      </div>
      <div class="seg small">
        <button class:active={gridMode === "musical"} onclick={() => (gridMode = "musical")}>Bars</button>
        <button class:active={gridMode === "time"} onclick={() => (gridMode = "time")}>Time</button>
      </div>
      <span class="vsep"></span>
      <div class="cellgroup flat zoom">
        <div class="cell" role="button" tabindex="0" title="Zoom out" onclick={() => zoomBy(1 / 1.2)} onkeydown={() => {}}>−</div>
        <div class="cell" role="button" tabindex="0" title="Zoom in" onclick={() => zoomBy(1.2)} onkeydown={() => {}}>+</div>
        <div class="cell wide" role="button" tabindex="0" title="Fit the whole song to view" onclick={() => timelineRef?.fitToSong()} onkeydown={() => {}}>Fit</div>
      </div>
      <button class="toggle" class:active={followPlayhead} title="Keep the playhead in view during playback"
        onclick={() => (followPlayhead = !followPlayhead)}>Follow</button>
      <button class="ghost" disabled title="Select a section to loop it">Loop</button>
      <span class="sp"></span>
    </div>
    <div class="arrangement">
      <div class="headers">
        <div class="ruler-spacer" style:height="{RULER_H}px"></div>
        <div class="sec-pad" style:height="{SECTIONS_H}px"><span class="microlabel">Sections</span></div>
        {#each project.tracks as track, ti}
          {@const lay = layout[ti]}
          <div
            class="track-block"
            class:drop-before={dropIndex === ti}
            style="--tc:{trackColor(track.name, track.type)}"
            role="presentation"
            ondragover={(e) => {
              if (dragTrack === null) return;
              e.preventDefault();
              const r = e.currentTarget.getBoundingClientRect();
              dropIndex = e.clientY < r.top + r.height / 2 ? ti : ti + 1;
            }}
            ondrop={(e) => {
              e.preventDefault();
              if (dragTrack !== null && dropIndex !== null) reorderTrack(dragTrack, dropIndex);
              dragTrack = null;
              dropIndex = null;
            }}
          >
            <div
              class="track-header"
              class:selected={selectedTrack === ti}
              style="height:{lay.height - lay.autoHeight}px"
              role="button"
              tabindex="0"
              draggable="true"
              ondragstart={(e) => {
                dragTrack = ti;
                e.dataTransfer!.effectAllowed = "move";
                e.dataTransfer!.setData("text/plain", String(ti));
              }}
              ondragend={() => { dragTrack = null; dropIndex = null; }}
              onclick={() => selectTrack(ti)}
              onkeydown={(e) => e.key === "Enter" && selectTrack(ti)}
            >
              <div class="tcolor"></div>
              <div class="tmeta">
                <div class="name">{track.name}</div>
                <div class="tsub microlabel mono">
                  {#if track.type === "midi"}
                    ch {track.midiChannel} · {definitions.find((d) => d.id === track.definitionId)?.model ?? "?"}
                  {:else}
                    audio reference
                  {/if}
                </div>
              </div>
              <div class="trow">
                {#if track.type === "audio"}
                  <button class="ms" class:on={track.mute} title="Mute"
                    onclick={(e) => { e.stopPropagation(); track.mute = !track.mute; }}>M</button>
                  <button class="ms solo" class:on={track.solo} title="Solo"
                    onclick={(e) => { e.stopPropagation(); track.solo = !track.solo; }}>S</button>
                {:else}
                  <button class="gear ghost" title="Device settings"
                    onclick={(e) => { e.stopPropagation(); settingsTrack = ti; }}>⚙</button>
                {/if}
              </div>
            </div>

            {#if track.type === "midi" && lay.autoHeight > 0}
              {@const shown = track.automationView.command}
              {@const inUseIds = new Set(curvesInUse(track).map((c) => c.commandId))}
              {@const offIds = new Set((track.automation ?? []).filter((c) => !c.enabled).map((c) => c.commandId))}
              {@const shownOn = shown ? (findCurve(track, shown)?.enabled ?? true) : true}
              <div class="auto-header" style="height:{lay.autoHeight}px">
                <div class="auto-row">
                  <AutoSelector
                    commands={automationCommands(defsById.get(track.definitionId))}
                    selected={shown}
                    inUse={inUseIds}
                    disabled={offIds}
                    inUseCount={inUseIds.size}
                    onselect={(id) => setLaneCommand(ti, id)}
                  />
                  {#if shown}
                    <button
                      class="auto-on"
                      class:on={shownOn}
                      title={shownOn
                        ? "Curve is on — click to switch it off"
                        : "Curve is off and will be skipped on export"}
                      onclick={(e) => { e.stopPropagation(); toggleCurve(ti, shown); }}
                    >{shownOn ? "ON" : "OFF"}</button>
                    <button
                      class="auto-dots ghost"
                      title="Curve actions"
                      onclick={(e) => openLaneMenu(e, ti, shown)}
                    >⋯</button>
                  {/if}
                </div>
                {#if shown}
                  <div
                    class="auto-grip"
                    role="separator"
                    title="Drag to resize the automation lane"
                    onpointerdown={(e) => startLaneResize(e, ti)}
                    onpointermove={dragLaneResize}
                    onpointerup={() => (laneResize = null)}
                  ></div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
        <div class="add-track">
          <button class="add" onclick={addAudioTrack}>＋ Audio</button>
          <button class="add" onclick={addMidiTrack} disabled={definitions.length === 0}>＋ MIDI</button>
        </div>
        <div
          class="header-fill"
          class:drop-before={dropIndex === project.tracks.length}
          role="presentation"
          ondragover={(e) => {
            if (dragTrack === null) return;
            e.preventDefault();
            dropIndex = project.tracks.length;
          }}
          ondrop={(e) => {
            e.preventDefault();
            if (dragTrack !== null && dropIndex !== null) reorderTrack(dragTrack, dropIndex);
            dragTrack = null;
            dropIndex = null;
          }}
        ></div>
      </div>
      <Timeline
        bind:this={timelineRef}
        {project}
        {definitions}
        {layout}
        audio={loadedAudio}
        playhead={playheadSec}
        {gridMode}
        {pxPerSecond}
        {snapTicks}
        {selection}
        {bpSelection}
        {valueClip}
        {coloredWaves}
        {accents}
        sections={project.sections ?? []}
        follow={followPlayhead}
        playing={isPlaying}
        onseek={seek}
        onzoom={(z) => (pxPerSecond = z)}
        {onselectionchange}
        onbpselect={selectBreakpoints}
        onlaneselect={selectLane}
        onlaneaction={laneAction}
        oneditvalue={editBreakpointValue}
        oncopyvalue={copyValue}
        oncommit={commit}
        oncreate={createEvent}
        onstatus={(m) => (status = m)}
      />
    </div>
    </div>
    {#if inspectorOpen}
      <Inspector
        {project}
        {definitions}
        {selection}
        {bpSelection}
        {laneSelection}
        {selectedTrack}
        oncommit={commit}
        ondeleteevents={deleteSelection}
        ondeletebreakpoints={deleteBreakpoints}
        onlaneaction={laneAction}
        onopentracksettings={(ti) => (settingsTrack = ti)}
        onremovetrack={removeTrack}
      />
    {/if}
  </section>

  <footer class="mono">{status}</footer>
</div>

{#if settingsTrack !== null && project.tracks[settingsTrack]?.type === "midi"}
  {@const track = project.tracks[settingsTrack]}
  {#if track.type === "midi"}
    <Modal title="Device settings — {track.name}" onclose={() => closeTrackSettings(false)}>
      <label class="modal-row">
        <span class="microlabel">Track name</span>
        <input type="text" bind:value={track.name} />
      </label>
      {@const locked = !trackIsEmpty(track)}
      <label class="modal-row">
        <span class="microlabel">Device definition</span>
        <select
          value={track.definitionId}
          disabled={locked}
          onchange={(e) => changeDefinition(settingsTrack!, e.currentTarget.value)}
        >
          {#each definitions as d}
            <option value={d.id}>{d.manufacturer} {d.model}</option>
          {/each}
        </select>
      </label>
      {#if locked}
        <p class="modal-desc">
          The device can't be changed once the track has events or automation — its commands are
          what they point at. Create a new track instead.
        </p>
      {/if}
      {#each defsById.get(track.definitionId)?.warnings ?? [] as w}
        <p class="modal-desc warn-line">⚠ {w}</p>
      {/each}
      <label class="modal-row">
        <span class="microlabel">MIDI channel</span>
        <Num min={1} max={16} bind:value={track.midiChannel} />
      </label>
      <label class="modal-row" title="Messages are sent this much earlier at export, so slow devices switch on time. Commands may add their own latency on top (e.g. preset loads).">
        <span class="microlabel">Latency comp. (ms)</span>
        <Num min={0} max={2000} step={5} bind:value={track.latencyMs} />
      </label>
      {#if definitions.find((d) => d.id === track.definitionId)?.description}
        <p class="modal-desc">{definitions.find((d) => d.id === track.definitionId)?.description}</p>
      {/if}
      <div class="modal-actions">
        {#if settingsIsNew}
          <button onclick={() => closeTrackSettings(false)}>Cancel</button>
        {/if}
        <button class="primary" onclick={() => closeTrackSettings(true)}>OK</button>
      </div>
    </Modal>
  {/if}
{/if}

{#if valuePrompt}
  <Modal title={valuePrompt.title} onclose={() => (valuePrompt = null)}>
    <div class="modal-row">
      <span class="microlabel">Value</span>
      {#if valuePrompt.cmd.steps.length > 0}
        <select
          value={valuePrompt.value}
          onchange={(e) => (valuePrompt!.value = Number(e.currentTarget.value))}
        >
          {#each valuePrompt.cmd.steps as st}
            <option value={st.value}>{st.text}</option>
          {/each}
        </select>
      {:else}
        <Num
          min={rangeOf(valuePrompt.cmd)[0]}
          max={rangeOf(valuePrompt.cmd)[1]}
          value={valuePrompt.value}
          onchange={(v) => (valuePrompt!.value = v)}
        />
      {/if}
    </div>
    <div class="modal-actions">
      <button onclick={() => (valuePrompt = null)}>Cancel</button>
      <button class="primary" onclick={confirmValuePrompt}>OK</button>
    </div>
  </Modal>
{/if}

{#if headerMenu}
  <ContextMenu
    x={headerMenu.x}
    y={headerMenu.y}
    items={headerMenu.items}
    onclose={() => (headerMenu = null)}
  />
{/if}

{#if exportOpen}
  <Modal title="Export MIDI" onclose={() => (exportOpen = false)}>
    <label class="modal-check">
      <input type="checkbox" bind:checked={resetBlock} />
      <span>
        <strong>Reset Block</strong><br />
        <span class="modal-desc">Prepends init + all used Hold disengage messages at the song
        start, so a restart always begins from a clean device state.</span>
      </span>
    </label>
    {#if skippedCurves > 0}
      <p class="modal-desc">
        {skippedCurves} automation {skippedCurves === 1 ? "curve is" : "curves are"} switched off
        and will be skipped.
      </p>
    {/if}
    <div class="modal-row export-target">
      <span class="microlabel">Folder</span>
      <span class="path" title={exportDir ?? ""}>{exportDir ?? "Not chosen yet — you'll be asked"}</span>
      <button onclick={chooseExportDir}>Change…</button>
    </div>
    <div class="modal-actions">
      <button onclick={() => (exportOpen = false)}>Cancel</button>
      <button class="primary" onclick={runExport}>Export</button>
    </div>
  </Modal>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }
  .topbar {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 50px;
    padding: 0 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    z-index: 5;
  }
  .tb-left { display: flex; align-items: center; gap: 10px; }
  .tb-center { flex: 1; display: flex; align-items: center; justify-content: center; gap: 12px; min-width: 0; }
  .tb-right { display: flex; align-items: center; gap: 8px; }
  /* Window move handle — the visible hint that the whole bar drags the frameless window. */
  .grip {
    padding: 0 2px 0 4px; font-size: 15px; line-height: 1; color: var(--fg-3);
    cursor: grab; user-select: none;
  }
  .grip:hover { color: var(--fg-2); }
  .grip:active { cursor: grabbing; }
  .brand { font-weight: 700; font-size: 15px; letter-spacing: 0.02em; }
  .accent { color: var(--accent); }
  .menu { position: relative; }
  .hamb { width: 30px; justify-content: center; font-size: 14px; }
  .hamb.active { color: var(--accent); border-color: transparent; background: var(--accent-soft); }
  .dropdown {
    position: absolute; top: calc(100% + 6px); left: 0; min-width: 180px;
    display: flex; flex-direction: column; padding: 5px; z-index: 60;
  }
  .dropdown button {
    border: none; border-radius: 5px; background: transparent;
    justify-content: flex-start; height: 28px; width: 100%; color: var(--fg);
  }
  .dropdown button:hover { background: var(--accent); color: var(--accent-fg); }
  .dropdown button:hover .kbd { color: color-mix(in srgb, var(--accent-fg) 80%, transparent); }
  .dropdown hr { margin: 5px 6px; border-top: 1px solid var(--line); }
  .kbd { margin-left: auto; color: var(--fg-3); font-size: 10px; font-family: var(--font-mono); }
  .song {
    width: 150px; height: 26px; font-size: 12px; background: transparent;
    border-color: transparent; box-shadow: none; color: var(--fg-2);
  }
  .song:hover { border-color: var(--line); background: var(--raised); }
  .song:focus-visible { border-color: var(--accent); background: var(--raised); color: var(--fg); }
  .winbtns { display: flex; gap: 2px; margin-left: 4px; }
  .win { width: 28px; justify-content: center; font-size: 12px; }
  .win.close:hover { background: var(--danger); color: var(--fg-on-warm); border-color: var(--danger); }

  .center { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .cell.wide { min-width: 50px; font-size: 14px; }
  .cell.wide.on { color: var(--accent); }
  /* Position/BPM/time-sig readout — no fake LCD frame, just borderless mono type */
  .lcd {
    display: flex; align-items: center; gap: 14px; height: 34px; padding: 0 14px;
  }
  .lc { display: flex; flex-direction: column; line-height: 1.15; align-items: flex-start; }
  .lcd-main { font-size: 13px; color: var(--fg); }
  .lc.pos .lcd-main { display: inline-block; min-width: 96px; } /* fixed width → no layout shift as it counts */
  .lcd-div { width: 1px; align-self: stretch; background: var(--line); margin: 7px 0; }
  .sig { display: inline-flex; align-items: center; gap: 1px; }
  .sig :global(.num.flat input) { width: 16px; text-align: center; }
  .slash { color: var(--fg-3); font-size: 12px; }
  /* snap split control */
  .snapgroup { position: relative; }
  .snap-main { padding: 0 9px 0 12px; }
  .snap-caret { min-width: 0; padding: 0 8px; font-size: 9px; color: var(--fg-3); }
  .snap-caret.open { color: var(--fg); background: var(--accent-soft); }
  .grid-menu {
    position: absolute; top: calc(100% + 6px); right: 0; z-index: 40; min-width: 84px; padding: 4px;
    display: flex; flex-direction: column; gap: 1px;
  }
  .grid-menu button {
    height: 24px; justify-content: flex-start; background: transparent; border: none; border-radius: 3px;
    color: var(--fg-2); font-size: 12.5px; font-family: var(--font-mono);
  }
  .grid-menu button:hover { background: var(--accent); color: var(--accent-fg); }
  .grid-menu button.on { color: var(--accent); }
  .grid-menu button.on:hover { color: var(--accent-fg); }
  /* dotted/triplet modify every division, so the menu stays one short list plus this row */
  .grid-menu .flavors {
    display: flex; gap: 1px; margin-bottom: 3px; padding-bottom: 3px; border-bottom: 1px solid var(--line);
  }
  .grid-menu .flavors button { flex: 1; height: 20px; justify-content: center; font-size: 11px; }
  .grid-menu .flavors button.on { background: var(--accent-soft); }
  .seg.small button { height: 19px; padding: 0 9px; font-size: 11px; }
  .pt { min-width: 34px; font-size: 13px; }
  .vsep { width: 1px; align-self: stretch; background: var(--line); margin: 9px 2px; }
  .midi-live { display: flex; align-items: center; gap: 6px; }
  .midi-live .port { max-width: 190px; text-overflow: ellipsis; }
  .drop-before { box-shadow: inset 0 2px 0 var(--accent); }

  /* timeline toolbar — snap/grid-mode/zoom are timeline-scoped, not app-scoped */
  .tlbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
    height: 34px;
    padding: 0 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .tlbar .zoom .cell.wide { min-width: 32px; font-size: 11px; }
  .sp { flex: 1; }

  .main { display: flex; flex: 1; min-height: 0; }
  .arrangement {
    display: flex;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
  .headers {
    width: 210px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  .add-track {
    display: flex;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .add-track .add {
    flex: 1;
    justify-content: center;
    height: 30px;
    background: transparent;
    border: 1px dashed var(--line-2);
    color: var(--fg-2);
    font-size: 12px;
  }
  .add-track .add:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .header-fill {
    flex: 1;
    background: var(--panel);
  }
  .ruler-spacer {
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .sec-pad {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    padding: 0 11px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .track-block {
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .track-block > :last-child {
    border-bottom: 1px solid var(--line);
  }
  .track-header {
    box-sizing: border-box;
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    background: var(--panel);
    cursor: pointer;
  }
  /* Automation Lane header: the Command selector lives in the header column,
     the curve itself is drawn in the Timeline to the right. */
  .auto-header {
    box-sizing: border-box;
    position: relative;
    display: flex;
    flex-direction: column;
    padding: 3px 10px 0;
    background: var(--panel);
    border-top: 1px solid var(--line);
  }
  .auto-row {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    flex-shrink: 0;
  }
  .auto-on {
    flex-shrink: 0;
    width: 30px;
    height: 20px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--fg-3);
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .auto-on:hover {
    color: var(--fg);
  }
  .auto-on.on {
    color: var(--auto);
    border-color: color-mix(in srgb, var(--auto) 45%, var(--line));
  }
  .auto-dots {
    flex-shrink: 0;
    width: 18px;
    height: 20px;
    padding: 0;
    justify-content: center;
    font-size: 13px;
    line-height: 1;
  }
  .auto-grip {
    margin-top: auto;
    height: 6px;
    flex-shrink: 0;
    cursor: ns-resize;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .auto-grip::before {
    content: "";
    width: 26px;
    height: 2px;
    border-radius: 1px;
    background: var(--line-2);
  }
  .auto-grip:hover::before {
    background: var(--accent);
  }
  .track-header:hover,
  .track-header.selected {
    background: var(--accent-soft);
  }
  .tcolor {
    width: 4px;
    height: 30px;
    flex-shrink: 0;
    border-radius: 3px;
    background: var(--tc);
  }
  .tmeta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .track-header .name {
    font-weight: 600;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tsub {
    font-size: 9px;
  }
  .trow {
    display: flex;
    gap: 3px;
  }
  .ms {
    width: 20px;
    height: 20px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    color: var(--fg-3);
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .ms:hover {
    color: var(--fg);
  }
  .ms.on {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--fg-on-warm);
  }
  .ms.solo.on {
    background: var(--warn);
    border-color: var(--warn);
    color: var(--fg-on-warm);
  }
  .gear {
    width: 22px;
    height: 22px;
    padding: 0;
    justify-content: center;
    font-size: 12px;
  }

  footer {
    color: var(--fg-2);
    font-size: 11px;
    min-height: 14px;
    padding: 0 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .modal-row {
    display: grid;
    grid-template-columns: 110px 1fr;
    align-items: center;
    gap: 10px;
  }
  .warn-line {
    color: var(--warn);
  }
  .modal-desc {
    color: var(--fg-2);
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
  }
  .modal-check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .modal-check input {
    margin-top: 3px;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .export-target {
    grid-template-columns: 110px 1fr auto;
    margin: 12px 0;
  }
  .export-target .path {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--fg-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
</style>
