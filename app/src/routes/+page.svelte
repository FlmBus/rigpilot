<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import Timeline from "$lib/Timeline.svelte";
  import Palette from "$lib/Palette.svelte";
  import Inspector from "$lib/Inspector.svelte";
  import Modal from "$lib/Modal.svelte";
  import Num from "$lib/Num.svelte";
  import { loadAudio, Player, type LoadedAudio, type TrackPlayback } from "$lib/audio";
  import { History } from "$lib/undo";
  import {
    PPQN,
    RULER_H,
    barTicks,
    secondsPerBeat,
    secondsToTick,
    tickToSeconds,
    trackHeight,
    snapToSteps,
    type CommandInfo,
    type DefinitionInfo,
    type EventRef,
    type Project,
    type RpEvent,
  } from "$lib/types";
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
  let snapSeconds = $state(1); // raw-time grid step
  let gridMode = $state<"musical" | "time">("musical");
  let pxPerSecond = $state(40);
  let coloredWaves = $state(true);

  // selection
  let selection = $state<EventRef[]>([]);
  let selectedTrack = $state<number | null>(null);
  let clipboard: { rel: number; lane: number; ev: RpEvent }[] = [];

  // dialogs / menus
  let menuOpen = $state(false);
  let dragTrack = $state<number | null>(null);
  let dropIndex = $state<number | null>(null);
  let settingsTrack = $state<number | null>(null);
  let settingsIsNew = $state(false);
  let exportOpen = $state(false);
  let resetBlock = $state(true);
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
  const TRACK_COLORS = ["#ff2e88", "#2ee08a", "#ffb02e", "#39d3e6", "#c084fc", "#fb7185", "#38bdf8"];
  const trackColor = (ti: number, type: string) =>
    type === "audio" ? "#6b6b78" : TRACK_COLORS[ti % TRACK_COLORS.length];

  const appWindow = IS_TAURI
    ? getCurrentWindow()
    : ({ minimize() {}, toggleMaximize() {}, close() {} } as ReturnType<typeof getCurrentWindow>);
  const player = new Player();
  const audioCache = new Map<string, LoadedAudio>();
  let loadedAudio = $state<(LoadedAudio | null)[]>([]);
  const history = new History();

  function newProject(): Project {
    return { name: "Untitled Song", bpm: 120, timeSignature: [4, 4], tracks: [] };
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

  const snapTicks = $derived.by(() => {
    if (!snapOn) return null;
    if (gridMode === "musical") return Math.max(1, Math.round(barTicks(project.timeSignature) / snapDivision));
    return secondsToTick(snapSeconds, project.bpm);
  });

  const selectedMidiDef = $derived.by(() => {
    if (selectedTrack === null) return undefined;
    const t = project.tracks[selectedTrack];
    if (!t || t.type !== "midi") return undefined;
    return definitions.find((d) => d.id === t.definitionId);
  });

  // ---- undo ----
  function commit() {
    history.push($state.snapshot(project));
  }
  function undo() {
    const prev = history.undo($state.snapshot(project));
    if (prev) {
      project = prev as Project;
      selection = [];
      status = "Undo";
    }
  }
  function redo() {
    const next = history.redo($state.snapshot(project));
    if (next) {
      project = next as Project;
      selection = [];
      status = "Redo";
    }
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
        options: { resetBlock: true },
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
    if (i !== null) selection = [];
  }
  function onselectionchange(refs: EventRef[]) {
    selection = refs;
    if (refs.length > 0) selectedTrack = null;
  }

  function makeEvent(cmd: CommandInfo, tick: number, lane: number): RpEvent {
    const params: Record<string, number> = {};
    for (const p of cmd.params) params[p.id] = p.default ?? p.min;
    const base = { commandId: cmd.id, tick, params, lane };
    if (cmd.commandType === "hold") return { ...base, kind: "hold", length: PPQN * 4 };
    if (cmd.commandType === "automation") {
      const end = PPQN * 4;
      const lo = snapToSteps(cmd.steps, 0);
      const hi = snapToSteps(cmd.steps, 127);
      return { ...base, kind: "automation", length: end, breakpoints: [[0, lo], [end, hi]] };
    }
    return { ...base, kind: "one-shot" };
  }

  function createEvent(ti: number, commandId: string, tick: number, lane: number) {
    const track = project.tracks[ti];
    if (track.type !== "midi") return;
    const cmd = definitions
      .find((d) => d.id === track.definitionId)
      ?.commands.find((c) => c.id === commandId);
    if (!cmd) return;
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
      deleteSelection();
    } else if (mod && e.key === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (mod && (e.key === "y" || (e.key === "z" && e.shiftKey) || e.key === "Z")) {
      e.preventDefault();
      redo();
    } else if (mod && e.key === "c") {
      copySelection();
    } else if (mod && e.key === "v") {
      paste();
    } else if (mod && e.key === "s") {
      e.preventDefault();
      saveProject(false);
    } else if (e.key === "Escape") {
      selection = [];
      selectedTrack = null;
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

  async function changeDefinition(ti: number, definitionId: string) {
    const t = project.tracks[ti];
    if (t.type !== "midi" || t.definitionId === definitionId) return;
    if (t.events.length > 0) {
      const ok = await ask(
        "Changing the Device Definition removes all events on this track. Continue?",
        { title: "Change Device Definition", kind: "warning" },
      );
      if (!ok) return;
    }
    commit();
    t.definitionId = definitionId;
    t.latencyMs = definitions.find((d) => d.id === definitionId)?.defaultLatency ?? 0;
    t.events = [];
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

  async function runExport() {
    if (!IS_TAURI) {
      exportOpen = false;
      status = "Export needs the desktop app (browser dev mode).";
      return;
    }
    const dir = await open({ directory: true });
    if (typeof dir !== "string") return;
    try {
      const files = await invoke<string[]>("export_midi", {
        project: $state.snapshot(project),
        outDir: dir,
        options: { resetBlock },
      });
      status = files.length
        ? `Exported: ${files.join(", ")}`
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
  <header class="topbar" data-tauri-drag-region>
    <div class="tb-left">
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
      <div class="snapgroup">
        <div class="cellgroup flat">
          <div class="cell snap-main" class:on={snapOn} role="button" tabindex="0" title="Toggle snap"
            onclick={() => (snapOn = !snapOn)} onkeydown={() => {}}>
            <span class="microlabel">SNAP {gridMode === "musical" ? `1/${snapDivision}` : `${snapSeconds}s`}</span>
          </div>
          <div class="cell snap-caret" class:open={gridMenuOpen} role="button" tabindex="0" title="Snap grid size"
            onclick={() => (gridMenuOpen = !gridMenuOpen)} onkeydown={() => {}}>▾</div>
        </div>
        {#if gridMenuOpen}
          <div class="grid-menu glass" role="listbox">
            {#if gridMode === "musical"}
              {#each [1, 2, 4, 8, 16, 32] as d}
                <button class:on={snapDivision === d} onclick={() => { snapDivision = d; snapOn = true; gridMenuOpen = false; }}>1/{d}</button>
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
      <button class="toggle" class:active={coloredWaves} title="Spectral waveform coloring"
        onclick={() => (coloredWaves = !coloredWaves)}>Color</button>
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
      <Palette definition={selectedMidiDef ?? (selection.length > 0 ? definitions.find((d) => {
        const t = project.tracks[selection[0].ti];
        return t?.type === "midi" && d.id === t.definitionId;
      }) : undefined)} />
    {/if}
    <div class="center">
    <div class="arrangement">
      <div class="headers">
        <div class="ruler-spacer" style:height="{RULER_H}px"></div>
        {#each project.tracks as track, ti}
          <div
            class="track-header"
            class:selected={selectedTrack === ti}
            class:drop-before={dropIndex === ti}
            style="height:{trackHeight(track)}px; --tc:{trackColor(ti, track.type)}"
            role="button"
            tabindex="0"
            draggable="true"
            ondragstart={(e) => {
              dragTrack = ti;
              e.dataTransfer!.effectAllowed = "move";
              e.dataTransfer!.setData("text/plain", String(ti));
            }}
            ondragover={(e) => {
              if (dragTrack === null) return;
              e.preventDefault();
              const r = e.currentTarget.getBoundingClientRect();
              dropIndex = e.clientY < r.top + r.height / 2 ? ti : ti + 1;
            }}
            ondragend={() => { dragTrack = null; dropIndex = null; }}
            ondrop={(e) => {
              e.preventDefault();
              if (dragTrack !== null && dropIndex !== null) reorderTrack(dragTrack, dropIndex);
              dragTrack = null;
              dropIndex = null;
            }}
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
        {project}
        {definitions}
        audio={loadedAudio}
        playhead={playheadSec}
        {gridMode}
        {pxPerSecond}
        {snapTicks}
        {selection}
        {coloredWaves}
        onseek={seek}
        onzoom={(z) => (pxPerSecond = z)}
        {onselectionchange}
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
        {selectedTrack}
        oncommit={commit}
        ondeleteevents={deleteSelection}
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
      <label class="modal-row">
        <span class="microlabel">Device definition</span>
        <select
          value={track.definitionId}
          onchange={(e) => changeDefinition(settingsTrack!, e.currentTarget.value)}
        >
          {#each definitions as d}
            <option value={d.id}>{d.manufacturer} {d.model}</option>
          {/each}
        </select>
      </label>
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
    <div class="modal-actions">
      <button onclick={() => (exportOpen = false)}>Cancel</button>
      <button class="primary" onclick={runExport}>Choose folder &amp; export</button>
    </div>
  </Modal>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg0);
    border: 1px solid #000;
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
    background: var(--bar-bg);
    border-bottom: 1px solid #000;
    box-shadow: var(--rim);
    z-index: 5;
  }
  .tb-left { display: flex; align-items: center; gap: 10px; }
  .tb-center { flex: 1; display: flex; align-items: center; justify-content: center; gap: 12px; min-width: 0; }
  .tb-right { display: flex; align-items: center; gap: 8px; }
  .brand { font-weight: 700; font-size: 15px; letter-spacing: 0.02em; padding-left: 6px; }
  .accent { color: var(--accent); }
  .menu { position: relative; }
  .hamb { width: 30px; justify-content: center; font-size: 14px; }
  .hamb.active { color: var(--accent); border-color: var(--line); background: var(--bg2); }
  .dropdown {
    position: absolute; top: calc(100% + 6px); left: 0; min-width: 180px;
    display: flex; flex-direction: column; padding: 5px; z-index: 60;
  }
  .dropdown button {
    border: none; border-radius: 5px; background: transparent;
    justify-content: flex-start; height: 28px; width: 100%; color: var(--fg);
  }
  .dropdown button:hover { background: var(--accent); color: #fff; }
  .dropdown button:hover .kbd { color: rgba(255, 255, 255, 0.8); }
  .dropdown hr { margin: 5px 6px; border-top: 1px solid #000; box-shadow: 0 1px 0 rgba(255, 255, 255, 0.03); }
  .kbd { margin-left: auto; color: var(--fg-faint); font-size: 10px; font-family: var(--font-mono); }
  .song {
    width: 150px; height: 26px; font-size: 12px; background: transparent;
    border-color: transparent; box-shadow: none; color: var(--fg-dim);
  }
  .song:hover { border-color: var(--line); background: #07070a; box-shadow: var(--well-in); }
  .song:focus-visible { border-color: var(--accent); background: #07070a; box-shadow: var(--well-in); color: var(--fg); }
  .winbtns { display: flex; gap: 2px; margin-left: 4px; }
  .win { width: 28px; justify-content: center; font-size: 12px; }
  .win.close:hover { background: #c2304a; color: #fff; border-color: #c2304a; }

  .center { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .cell.wide { min-width: 50px; font-size: 14px; }
  .cell.wide.on { color: var(--accent); }
  /* LCD display in the topbar */
  .lcd {
    display: flex; align-items: center; gap: 14px; height: 34px; padding: 0 14px;
    background: #050507; border: 1px solid #000; border-radius: var(--radius-sm); box-shadow: var(--well-in);
  }
  .lc { display: flex; flex-direction: column; line-height: 1.15; align-items: flex-start; }
  .lcd-main { font-size: 13px; color: var(--green); }
  .lc.pos .lcd-main { display: inline-block; min-width: 96px; } /* fixed width → no layout shift as it counts */
  .lcd-div { width: 1px; align-self: stretch; background: #000; box-shadow: 1px 0 0 rgba(255, 255, 255, 0.03); margin: 7px 0; }
  .sig { display: inline-flex; align-items: center; gap: 1px; }
  .sig :global(.num.flat input) { width: 16px; text-align: center; }
  .slash { color: var(--fg-faint); font-size: 12px; }
  /* snap split control */
  .snapgroup { position: relative; }
  .snap-main { padding: 0 9px 0 12px; }
  .snap-caret { min-width: 0; padding: 0 8px; font-size: 9px; color: var(--fg-faint); }
  .snap-caret.open { color: var(--fg); background: rgba(255, 255, 255, 0.05); }
  .grid-menu {
    position: absolute; top: calc(100% + 6px); right: 0; z-index: 40; min-width: 84px; padding: 4px;
    display: flex; flex-direction: column; gap: 1px;
  }
  .grid-menu button {
    height: 24px; justify-content: flex-start; background: transparent; border: none; border-radius: 3px;
    color: var(--fg-dim); font-size: 12.5px; font-family: var(--font-mono);
  }
  .grid-menu button:hover { background: var(--accent); color: #fff; }
  .grid-menu button.on { color: var(--accent); }
  .grid-menu button.on:hover { color: #fff; }
  .seg.small button { height: 19px; padding: 0 9px; font-size: 11px; }
  .pt { min-width: 34px; font-size: 13px; }
  .vsep { width: 1px; align-self: stretch; background: #000; box-shadow: 1px 0 0 rgba(255, 255, 255, 0.03); margin: 9px 2px; }
  .midi-live { display: flex; align-items: center; gap: 6px; }
  .midi-live .port { max-width: 190px; text-overflow: ellipsis; }
  .drop-before { box-shadow: inset 0 2px 0 var(--accent); }

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
    background: var(--bg2);
    border-right: 1px solid #000;
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
    border: 1px dashed var(--line-strong);
    color: var(--fg-dim);
    font-size: 12px;
  }
  .add-track .add:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .header-fill {
    flex: 1;
    background: var(--bg2);
  }
  .ruler-spacer {
    background: var(--bg1);
    border-bottom: 1px solid #000;
    box-shadow: var(--rim);
  }
  .track-header {
    box-sizing: border-box;
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    background: var(--bg2);
    border-bottom: 1px solid var(--line);
    cursor: pointer;
  }
  .track-header:hover,
  .track-header.selected {
    background: var(--bg3);
  }
  .tcolor {
    width: 4px;
    height: 30px;
    flex-shrink: 0;
    border-radius: 3px;
    background: var(--tc);
    box-shadow: 0 0 8px -1px var(--tc);
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
    color: var(--fg-faint);
    background: #07070a;
    border: 1px solid var(--line);
    border-radius: 4px;
    box-shadow: var(--well-in);
  }
  .ms:hover {
    color: var(--fg);
  }
  .ms.on {
    background: #ff4d4d;
    border-color: #ff4d4d;
    color: #2a0000;
    box-shadow: none;
  }
  .ms.solo.on {
    background: var(--warn);
    border-color: var(--warn);
    color: #2a1c00;
  }
  .gear {
    width: 22px;
    height: 22px;
    padding: 0;
    justify-content: center;
    font-size: 12px;
  }

  footer {
    color: var(--fg-dim);
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
  .modal-desc {
    color: var(--fg-dim);
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
</style>
