<script lang="ts">
  import Num from "./Num.svelte";
  import {
    PPQN,
    barTicks,
    type DefinitionInfo,
    type EventRef,
    type Project,
  } from "./types";

  let {
    project,
    definitions,
    selection,
    selectedTrack,
    oncommit,
    ondeleteevents,
    onopentracksettings,
    onremovetrack,
  }: {
    project: Project;
    definitions: DefinitionInfo[];
    selection: EventRef[];
    selectedTrack: number | null;
    oncommit: () => void;
    ondeleteevents: () => void;
    onopentracksettings: (ti: number) => void;
    onremovetrack: (ti: number) => void;
  } = $props();

  const single = $derived(selection.length === 1 ? selection[0] : null);
  const event = $derived.by(() => {
    if (!single) return null;
    const t = project.tracks[single.ti];
    return t?.type === "midi" ? (t.events[single.ei] ?? null) : null;
  });
  const command = $derived.by(() => {
    if (!single || !event) return null;
    const t = project.tracks[single.ti];
    if (t?.type !== "midi") return null;
    return (
      definitions.find((d) => d.id === t.definitionId)?.commands.find((c) => c.id === event.commandId) ?? null
    );
  });

  const track = $derived(selectedTrack !== null ? (project.tracks[selectedTrack] ?? null) : null);

  const bpb = $derived(barTicks(project.timeSignature));
  const posBar = $derived(event ? Math.floor(event.tick / bpb) + 1 : 1);
  const posBeat = $derived(event ? (event.tick % bpb) / PPQN + 1 : 1);

  function setPos(bar: number, beat: number) {
    if (!event) return;
    oncommit();
    event.tick = Math.max(0, Math.round((bar - 1) * bpb + (beat - 1) * PPQN));
  }
  function setLength(beats: number) {
    if (!event || event.kind === "one-shot") return;
    oncommit();
    const next = Math.max(PPQN / 8, Math.round(beats * PPQN));
    if (event.kind === "automation" && event.breakpoints && event.length) {
      const f = next / event.length;
      for (const bp of event.breakpoints) bp[0] = Math.round(bp[0] * f);
    }
    event.length = next;
  }
</script>

<aside class="inspector panel">
  <div class="head microlabel">Inspector</div>
  <div class="body">
    {#if event && single && command}
      <div class="section">
        <div class="title">{command.name}</div>
        <div class="microlabel kind">{command.commandType}{command.deterministic ? "" : " ⚠ state-dependent"}</div>
        {#if command.description}<p class="desc">{command.description}</p>{/if}
      </div>
      <hr />
      <div class="grid">
        <span class="microlabel">Bar</span>
        <Num min={1} value={posBar} onchange={(v) => setPos(v, posBeat)} />
        <span class="microlabel">Beat</span>
        <Num min={1} step={0.25} value={posBeat} onchange={(v) => setPos(posBar, v)} />
        {#if event.kind !== "one-shot"}
          <span class="microlabel">Length ♩</span>
          <Num min={0.125} step={0.25} value={(event.length ?? 0) / PPQN} onchange={setLength} />
        {/if}
        <span class="microlabel">Lane</span>
        <Num min={0} value={event.lane} onchange={(v) => { oncommit(); event!.lane = v; }} />
      </div>
      {#if event.kind === "automation" && event.breakpoints}
        <hr />
        <div class="microlabel">Breakpoints ({event.breakpoints.length})</div>
        <div class="bp-list">
          {#each event.breakpoints as bp, i}
            <div class="bp-row">
              <span class="mono bp-t">{(bp[0] / PPQN).toFixed(2)}♩</span>
              <Num min={0} max={127} value={bp[1]} onchange={(v) => { oncommit(); bp[1] = v; }} />
              <button class="ghost bp-del" title="Delete breakpoint"
                disabled={i === 0 || i === event.breakpoints.length - 1}
                onclick={() => { oncommit(); event!.breakpoints!.splice(i, 1); }}>✕</button>
            </div>
          {/each}
        </div>
        <p class="desc">Double-click the curve to add a breakpoint, double-click a point to delete it.</p>
      {:else if command.params.length > 0 && event.params}
        <hr />
        <div class="grid">
          {#each command.params as p}
            <span class="microlabel" title={p.name}>{p.name}</span>
            {#if p.labels.length > 0}
              <select value={event.params[p.id]}
                onchange={(e) => { oncommit(); event!.params![p.id] = Number(e.currentTarget.value); }}>
                {#each p.labels as l}
                  <option value={l.value}>{l.text}</option>
                {/each}
              </select>
            {:else}
              <Num min={p.min} max={p.max} value={event.params[p.id]}
                onchange={(v) => { oncommit(); event!.params![p.id] = v; }} />
            {/if}
          {/each}
        </div>
      {/if}
      <hr />
      <button class="danger" onclick={ondeleteevents}>Delete event</button>
    {:else if selection.length > 1}
      <div class="section">
        <div class="title">{selection.length} events selected</div>
        <p class="desc">Drag to move, drag edges to resize, Del to delete, Ctrl+C/V to copy & paste.</p>
      </div>
      <hr />
      <button class="danger" onclick={ondeleteevents}>Delete {selection.length} events</button>
    {:else if track && selectedTrack !== null}
      <div class="section">
        <div class="title">{track.name}</div>
        <div class="microlabel kind">{track.type} track</div>
      </div>
      <hr />
      <div class="grid">
        <span class="microlabel">Name</span>
        <input type="text" bind:value={track.name} />
        {#if track.type === "audio"}
          <span class="microlabel">Volume</span>
          <input type="range" min="0" max="1.5" step="0.01" bind:value={track.volume} />
          <span class="microlabel">Pan</span>
          <input type="range" min="-1" max="1" step="0.01" bind:value={track.pan} />
          <span class="microlabel">Wave zoom</span>
          <Num min={0.25} max={16} step={0.25} value={track.waveformGain || 1}
            onchange={(v) => ((track as any).waveformGain = v)} />
          <span class="microlabel">Offset ♩</span>
          <Num step={0.25} value={track.offsetTicks / PPQN}
            onchange={(v) => { oncommit(); (track as any).offsetTicks = Math.round(v * PPQN); }} />
          <span class="microlabel">File</span>
          <span class="mono file">{track.file}</span>
        {:else}
          <span class="microlabel">Channel</span>
          <span class="mono">{track.midiChannel}</span>
          <span class="microlabel">Device</span>
          <span class="mono device">{definitions.find((d) => d.id === track.definitionId)?.model ?? track.definitionId}</span>
        {/if}
      </div>
      <hr />
      {#if track.type === "midi"}
        <button onclick={() => onopentracksettings(selectedTrack!)}>⚙ Device settings…</button>
      {/if}
      <button class="danger" onclick={() => onremovetrack(selectedTrack!)}>Remove track</button>
    {:else}
      <p class="desc empty">
        Nothing selected.<br /><br />
        Click an event or a track header. Ctrl-click toggles, Shift-click adds, drag on empty
        space draws a selection box.
      </p>
    {/if}
  </div>
</aside>

<style>
  .inspector {
    width: 240px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .head {
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .kind {
    color: var(--accent);
    margin-top: 2px;
  }
  .desc {
    color: var(--fg-dim);
    font-size: 12px;
    margin: 6px 0 0;
    line-height: 1.45;
    user-select: text;
  }
  .empty {
    padding: 6px;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    align-items: center;
  }
  .bp-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bp-row {
    display: grid;
    grid-template-columns: 52px 1fr 24px;
    gap: 6px;
    align-items: center;
  }
  .bp-t {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .bp-del {
    height: 22px;
    padding: 0 4px;
    justify-content: center;
  }
  .file,
  .device {
    font-size: 11px;
    color: var(--fg-dim);
    word-break: break-all;
    user-select: text;
  }
</style>
