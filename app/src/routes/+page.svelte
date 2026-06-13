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
    type CommandInfo,
    type DefinitionInfo,
    type EventRef,
    type Project,
    type RpEvent,
  } from "$lib/types";

  let definitions = $state<DefinitionInfo[]>([]);
  let project = $state<Project>(newProject());
  let projectPath = $state<string | null>(null);
  let savedJson = $state(JSON.stringify(newProject()));
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

  const appWindow = getCurrentWindow();
  const player = new Player();
  const audioCache = new Map<string, LoadedAudio>();
  let loadedAudio = $state<(LoadedAudio | null)[]>([]);
  const history = new History();

  function newProject(): Project {
    return { name: "Untitled Song", bpm: 120, timeSignature: [4, 4], tracks: [] };
  }

  $effect(() => {
    invoke<DefinitionInfo[]>("list_definitions")
      .then((d) => (definitions = d))
      .catch((e) => (status = String(e)));
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
    await player.play(playbackTracks(), playheadSec);
    isPlaying = true;
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
    const m = Math.floor(playheadSec / 60);
    const s = (playheadSec % 60).toFixed(1).padStart(4, "0");
    const beats = playheadSec / secondsPerBeat(project.bpm);
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
    if (cmd.commandType === "automation")
      return { ...base, kind: "automation", length: PPQN * 4, breakpoints: [[0, 0], [PPQN * 4, 127]] };
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

<svelte:window onkeydown={onkeydown} onpointerdown={() => (menuOpen = false)} />

<div class="app">
  <header class="titlebar" data-tauri-drag-region>
    <span class="brand" data-tauri-drag-region>Rig<span class="accent">Pilot</span></span>
    <div class="toolbar">
      <div class="menu">
        <button class:active={menuOpen} onclick={(e) => { e.stopPropagation(); menuOpen = !menuOpen; }}>☰</button>
        {#if menuOpen}
          <div class="dropdown" role="menu" tabindex="-1" onpointerdown={(e) => e.stopPropagation()}>
            <button role="menuitem" onclick={() => { menuOpen = false; newProjectAction(); }}>New</button>
            <button role="menuitem" onclick={() => { menuOpen = false; openProject(); }}>Open…</button>
            <hr />
            <button role="menuitem" onclick={() => { menuOpen = false; saveProject(false); }}>Save <span class="kbd">Ctrl+S</span></button>
            <button role="menuitem" onclick={() => { menuOpen = false; saveProject(true); }}>Save As…</button>
          </div>
        {/if}
      </div>
      <button class="primary" onclick={() => (exportOpen = true)} disabled={!project.tracks.some((t) => t.type === "midi")}>
        Export MIDI…
      </button>
    </div>
    <span class="title mono" data-tauri-drag-region>{project.name}{projectPath ? "" : " *"}</span>
    <div class="winbtns">
      <button class="ghost win" title="Minimize" onclick={() => appWindow.minimize()}>–</button>
      <button class="ghost win" title="Maximize" onclick={() => appWindow.toggleMaximize()}>□</button>
      <button class="ghost win close" title="Close" onclick={closeWindow}>✕</button>
    </div>
  </header>

  <section class="editbar panel">
    <button class="toggle" class:active={snapOn} onclick={() => (snapOn = !snapOn)}>Snap</button>
    {#if gridMode === "musical"}
      <select bind:value={snapDivision} disabled={!snapOn} title="Snap grid (fraction of a bar)">
        <option value={1}>1/1</option>
        <option value={2}>1/2</option>
        <option value={4}>1/4</option>
        <option value={8}>1/8</option>
        <option value={16}>1/16</option>
        <option value={32}>1/32</option>
      </select>
    {:else}
      <select bind:value={snapSeconds} disabled={!snapOn} title="Snap grid (seconds)">
        <option value={5}>5 s</option>
        <option value={1}>1 s</option>
        <option value={0.5}>0.5 s</option>
        <option value={0.1}>0.1 s</option>
      </select>
    {/if}
    <div class="grid-toggle" role="group" aria-label="Grid mode">
      <button class="toggle" class:active={gridMode === "musical"} onclick={() => (gridMode = "musical")}>Bars</button>
      <button class="toggle" class:active={gridMode === "time"} onclick={() => (gridMode = "time")}>Time</button>
    </div>
    <button class="toggle" class:active={coloredWaves} title="Spectral waveform coloring (low=red, mid=green, high=blue)"
      onclick={() => (coloredWaves = !coloredWaves)}>Color</button>
    <span class="sep"></span>
    <label><span class="microlabel">Song</span><input class="song" type="text" bind:value={project.name} /></label>
    <span class="spacer"></span>
    <button onclick={addAudioTrack}>+ Audio</button>
    <button onclick={addMidiTrack} disabled={definitions.length === 0}>+ MIDI</button>
  </section>

  <section class="main">
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
    <div class="center">
    <div class="arrangement">
      <div class="headers">
        <div class="ruler-spacer" style:height="{RULER_H}px"></div>
        {#each project.tracks as track, ti}
          <div
            class="track-header"
            class:selected={selectedTrack === ti}
            class:drop-before={dropIndex === ti}
            style:height="{trackHeight(track)}px"
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
            <div class="name">{track.name}</div>
            <div class="row">
              <button class="mini toggle" class:active={track.mute}
                onclick={(e) => { e.stopPropagation(); track.mute = !track.mute; }}>M</button>
              <button class="mini toggle" class:active={track.solo}
                onclick={(e) => { e.stopPropagation(); track.solo = !track.solo; }}>S</button>
              {#if track.type === "midi"}
                <button class="mini ghost" title="Device settings"
                  onclick={(e) => { e.stopPropagation(); settingsTrack = ti; }}>⚙</button>
              {:else}
                <input class="fader" type="range" min="0" max="1.5" step="0.01" title="Volume"
                  style="--fill: {Math.round((track.volume / 1.5) * 100)}%"
                  onclick={(e) => e.stopPropagation()} bind:value={track.volume} />
              {/if}
            </div>
            {#if track.type === "midi"}
              <div class="microlabel mono meta">
                ch {track.midiChannel} · {definitions.find((d) => d.id === track.definitionId)?.model ?? "?"}
              </div>
            {/if}
          </div>
        {/each}
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
    <section class="transport panel">
      <span class="t-zone"></span>
      <div class="t-center">
        <button class="play" onclick={() => (isPlaying ? pause() : play())} title="Space">
          {isPlaying ? "⏸" : "⏵"}
        </button>
        <button onclick={stop}>⏹</button>
        <span class="position mono">{positionLabel}</span>
      </div>
      <div class="t-zone t-right">
        <label><span class="microlabel">BPM</span>
          <Num min={20} max={300} bind:value={project.bpm} />
        </label>
        <label>
          <Num min={1} max={32} bind:value={project.timeSignature[0]} />
          <span class="microlabel">/</span>
          <Num min={1} max={32} bind:value={project.timeSignature[1]} />
        </label>
      </div>
    </section>
    </div>
    <Palette definition={selectedMidiDef ?? (selection.length > 0 ? definitions.find((d) => {
      const t = project.tracks[selection[0].ti];
      return t?.type === "midi" && d.id === t.definitionId;
    }) : undefined)} />
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
    padding: 8px;
    gap: 8px;
    background: var(--bg0);
    border: 1px solid var(--line-strong);
    border-radius: 20px 20px 8px 8px;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 6px 2px;
  }
  .titlebar .title {
    margin-left: auto;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .winbtns {
    display: flex;
    gap: 2px;
  }
  .win {
    width: 30px;
    justify-content: center;
    font-size: 12px;
  }
  .win.close:hover {
    background: #c2304a;
    color: #fff;
    border-color: #c2304a;
  }
  .brand {
    font-weight: 700;
    font-size: 16px;
    letter-spacing: 0.02em;
    padding-left: 10px;
    margin-right: 4px;
  }
  .menu {
    position: relative;
  }
  .menu .active {
    border-color: var(--accent);
    color: var(--accent);
  }
  .dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 170px;
    background: var(--bg2);
    border: 1px solid var(--line-strong);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    padding: 4px;
    z-index: 60;
  }
  .dropdown button {
    border: none;
    border-radius: 0;
    background: transparent;
    justify-content: flex-start;
    height: 28px;
    width: 100%;
  }
  .dropdown button:hover {
    background: var(--bg3);
  }
  .dropdown hr {
    margin: 4px 0;
  }
  .kbd {
    margin-left: auto;
    color: var(--fg-faint);
    font-size: 10px;
    font-family: var(--font-mono);
  }
  .accent {
    color: var(--accent);
  }
  .toolbar {
    display: flex;
    gap: 5px;
  }

  .editbar,
  .transport {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 6px 10px;
    flex-wrap: wrap;
    background: var(--bg2);
    border: none;
    border-radius: 0;
  }
  .center {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 8px;
  }
  .t-zone {
    flex: 1;
  }
  .t-center {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .t-right {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: flex-end;
  }
  .play {
    font-size: 14px;
    min-width: 38px;
    justify-content: center;
    background: var(--green);
    border-color: var(--green);
    color: #0b0b0d;
    font-weight: 700;
  }
  .play:hover {
    background: #25c378;
    border-color: #25c378;
  }
  .drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .position {
    color: var(--fg);
    font-size: 13px;
    min-width: 110px;
  }
  .sep {
    width: 1px;
    align-self: stretch;
    background: var(--line);
    margin: 2px 2px;
  }
  .song {
    width: 180px;
  }
  .spacer {
    flex: 1;
  }
  .grid-toggle {
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .grid-toggle button {
    border: none;
    border-radius: 0;
    height: 24px;
  }
  .grid-toggle button.active {
    background: var(--accent);
    color: #fff;
  }

  .main {
    display: flex;
    gap: 8px;
    flex: 1;
    min-height: 0;
  }
  .arrangement {
    display: flex;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
  .headers {
    width: 200px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
  }
  .header-fill {
    flex: 1;
    background: var(--bg1);
    border: 1px solid var(--line);
    border-top: none;
    border-right: none;
  }
  .ruler-spacer {
    background: var(--bg1);
    border: 1px solid var(--line);
    border-right: none;
  }
  .track-header {
    box-sizing: border-box;
    border: 1px solid var(--line);
    border-top: none;
    border-right: none;
    padding: 5px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--bg1);
    cursor: pointer;
  }
  .track-header.selected {
    border-left: 2px solid var(--accent);
    background: var(--bg2);
  }
  .track-header .name {
    font-weight: 600;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-header .row {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .mini {
    height: 18px;
    padding: 0 6px;
    font-size: 10px;
    font-weight: 700;
  }
  .fader {
    -webkit-appearance: none;
    appearance: none;
    width: 90px;
    height: 9px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--fill, 67%), var(--bg2) var(--fill, 67%));
  }
  .fader::-webkit-slider-runnable-track {
    height: 100%;
    background: transparent;
  }
  .fader::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 2px;
    height: 7px;
    margin-top: 0;
    background: #fff;
    border-radius: 0;
  }
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 9px;
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
