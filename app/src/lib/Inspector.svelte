<script lang="ts">
  import Num from "./Num.svelte";
  import {
    PPQN,
    MIN_EVENT_TICKS,
    DEFAULT_AUTOMATION_RESOLUTION_MS,
    barTicks,
    stepLabel,
    type BpSelection,
    type DefinitionInfo,
    type EventRef,
    type Project,
    type Shape,
  } from "./types";
  import { allowedShapes, clampValue, findCurve, forcedShape, rangeOf } from "./automation";

  let {
    project,
    definitions,
    selection,
    bpSelection,
    laneSelection,
    selectedTrack,
    oncommit,
    ondeleteevents,
    ondeletebreakpoints,
    onlaneaction,
    onopentracksettings,
    onremovetrack,
  }: {
    project: Project;
    definitions: DefinitionInfo[];
    selection: EventRef[];
    bpSelection: BpSelection | null;
    laneSelection: number | null;
    selectedTrack: number | null;
    oncommit: () => void;
    ondeleteevents: () => void;
    ondeletebreakpoints: () => void;
    onlaneaction: (action: "constant" | "clear" | "toggle", ti: number, commandId: string) => void;
    onopentracksettings: (ti: number) => void;
    onremovetrack: (ti: number) => void;
  } = $props();

  // ---- automation selections ----
  function commandOf(ti: number, commandId: string) {
    const t = project.tracks[ti];
    if (t?.type !== "midi") return null;
    return (
      definitions.find((d) => d.id === t.definitionId)?.commands.find((c) => c.id === commandId) ??
      null
    );
  }

  const bpCurve = $derived.by(() => {
    if (!bpSelection) return null;
    const t = project.tracks[bpSelection.ti];
    return t?.type === "midi" ? (findCurve(t, bpSelection.commandId) ?? null) : null;
  });
  const bpCmd = $derived(bpSelection ? commandOf(bpSelection.ti, bpSelection.commandId) : null);
  const bpIndex = $derived(bpSelection?.idx.length === 1 ? bpSelection.idx[0] : null);
  const point = $derived(bpIndex !== null ? (bpCurve?.breakpoints[bpIndex] ?? null) : null);
  /** The shape describes the segment arriving at the point, so the first has none. */
  const isFirstPoint = $derived(bpIndex === 0);
  const bpSpan = $derived.by(() => {
    if (!bpSelection || !bpCurve || bpSelection.idx.length < 2) return null;
    const ticks = bpSelection.idx.map((i) => bpCurve.breakpoints[i]?.tick ?? 0);
    return { from: Math.min(...ticks), to: Math.max(...ticks) };
  });

  const laneTrack = $derived(laneSelection !== null ? project.tracks[laneSelection] : null);
  const laneCmd = $derived.by(() => {
    if (laneSelection === null || laneTrack?.type !== "midi") return null;
    const id = laneTrack.automationView.command;
    return id ? commandOf(laneSelection, id) : null;
  });
  const laneCurve = $derived.by(() => {
    if (laneTrack?.type !== "midi" || !laneCmd) return null;
    return findCurve(laneTrack, laneCmd.id) ?? null;
  });

  const posOf = (tick: number) => ({
    bar: Math.floor(tick / bpb) + 1,
    beat: (tick % bpb) / PPQN + 1,
  });

  function setPointTick(bar: number, beat: number) {
    if (!point || !bpCurve || bpIndex === null) return;
    const prev = bpCurve.breakpoints[bpIndex - 1];
    const next = bpCurve.breakpoints[bpIndex + 1];
    const want = Math.max(0, Math.round((bar - 1) * bpb + (beat - 1) * PPQN));
    oncommit();
    point.tick = Math.max(
      prev ? prev.tick + 1 : 0,
      Math.min(next ? next.tick - 1 : Number.MAX_SAFE_INTEGER, want),
    );
  }

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
    event.length = Math.max(MIN_EVENT_TICKS, Math.round(beats * PPQN));
  }
</script>

<aside class="inspector panel">
  <div class="head microlabel">Inspector</div>
  <div class="body">
    {#if point && bpCmd && bpSelection && bpIndex !== null}
      {@const pos = posOf(point.tick)}
      <div class="section">
        <div class="title">{bpCmd.name}</div>
        <div class="microlabel kind">breakpoint</div>
      </div>
      <hr />
      <div class="grid">
        <span class="microlabel">Bar</span>
        <Num min={1} value={pos.bar} onchange={(v) => setPointTick(v, pos.beat)} />
        <span class="microlabel">Beat</span>
        <Num min={1} step={0.25} value={pos.beat} onchange={(v) => setPointTick(pos.bar, v)} />
        <span class="microlabel">Value</span>
        {#if bpCmd.steps.length > 0}
          <select
            value={point.value}
            onchange={(e) => { oncommit(); point!.value = Number(e.currentTarget.value); }}
          >
            {#each bpCmd.steps as st}
              <option value={st.value}>{st.text}</option>
            {/each}
          </select>
        {:else}
          <Num
            min={rangeOf(bpCmd)[0]}
            max={rangeOf(bpCmd)[1]}
            value={point.value}
            onchange={(v) => { oncommit(); point!.value = clampValue(bpCmd, v); }}
          />
        {/if}
        {#if !isFirstPoint && !forcedShape(bpCmd)}
          <span class="microlabel" title="Shape of the line arriving at this point">Curve type</span>
          <select
            value={point.shape}
            onchange={(e) => {
              oncommit();
              point!.shape = e.currentTarget.value as Shape;
              if (point!.shape !== "curve") point!.tension = 0;
            }}
          >
            {#each allowedShapes(bpCmd) as shape}
              <option value={shape}>
                {shape === "linear" ? "Linear" : shape === "curve" ? "Curve" : "Hold"}
              </option>
            {/each}
          </select>
          {#if point.shape === "curve"}
            <span class="microlabel" title="Negative bends the other way">Bend</span>
            <Num
              min={-1}
              max={1}
              step={0.05}
              value={point.tension}
              onchange={(v) => { oncommit(); point!.tension = v; }}
            />
          {/if}
        {/if}
      </div>
      {#if bpCmd.steps.length > 0}
        <p class="desc">
          {bpCmd.name} only understands fixed steps{stepLabel(bpCmd.steps, point.value)
            ? ` — this point is “${stepLabel(bpCmd.steps, point.value)}”`
            : ""}, so the curve always jumps instead of ramping.
        </p>
      {:else if forcedShape(bpCmd) === "hold"}
        <p class="desc">{bpCmd.name} can only jump between values, so every segment holds.</p>
      {/if}
      <hr />
      <button class="danger" onclick={ondeletebreakpoints}>Delete point</button>
    {:else if bpSelection && bpSelection.idx.length > 1 && bpCmd}
      <div class="section">
        <div class="title">{bpSelection.idx.length} points selected</div>
        <div class="microlabel kind">{bpCmd.name}</div>
        {#if bpSpan}
          {@const from = posOf(bpSpan.from)}
          {@const to = posOf(bpSpan.to)}
          <p class="desc mono">
            bar {from.bar}.{from.beat.toFixed(2)} → {to.bar}.{to.beat.toFixed(2)}
          </p>
        {/if}
        <p class="desc">Drag up or down to move them together. ↑/↓ nudges by one.</p>
      </div>
      <hr />
      <button class="danger" onclick={ondeletebreakpoints}>
        Delete {bpSelection.idx.length} points
      </button>
    {:else if laneCmd && laneSelection !== null}
      <div class="section">
        <div class="title">{laneCmd.name}</div>
        <div class="microlabel kind">automation lane</div>
        {#if laneCmd.description}<p class="desc">{laneCmd.description}</p>{/if}
      </div>
      <hr />
      <div class="grid">
        <span class="microlabel">Curve</span>
        <button
          class="lane-toggle"
          class:on={laneCurve?.enabled ?? true}
          onclick={() => onlaneaction("toggle", laneSelection!, laneCmd!.id)}
        >
          {(laneCurve?.enabled ?? true) ? "On" : "Off — skipped on export"}
        </button>
        <span class="microlabel">Points</span>
        <span class="mono">{laneCurve?.breakpoints.length ?? 0}</span>
        <span
          class="microlabel"
          title="Minimum spacing between exported CC steps for this curve"
        >Resolution ms</span>
        <Num
          min={1}
          max={1000}
          value={laneCurve?.resolutionMs ?? DEFAULT_AUTOMATION_RESOLUTION_MS}
          onchange={(v) => {
            if (laneTrack?.type !== "midi" || !laneCmd) return;
            oncommit();
            const c = findCurve(laneTrack, laneCmd.id);
            if (c) c.resolutionMs = v;
          }}
        />
      </div>
      <p class="desc">
        ≈ {Math.round(1000 / (laneCurve?.resolutionMs ?? DEFAULT_AUTOMATION_RESOLUTION_MS))} CC
        updates/s · higher ms = fewer messages on a busy MIDI bus.
      </p>
      <div class="grid">
        <span
          class="microlabel"
          title="Re-send the standing value this often, even when the curve isn't moving"
        >Re-send ms</span>
        <Num
          min={0}
          max={10000}
          step={50}
          value={laneCurve?.resendMs ?? 0}
          onchange={(v) => {
            if (laneTrack?.type !== "midi" || !laneCmd) return;
            oncommit();
            const c = findCurve(laneTrack, laneCmd.id);
            if (c) c.resendMs = v;
          }}
        />
      </div>
      <p class="desc">
        {#if (laneCurve?.resendMs ?? 0) > 0}
          Repeats the current value every {laneCurve!.resendMs} ms so one swallowed message can't
          leave the device on the wrong value.
        {:else}
          0 = off. Set it if a dropped MIDI message would strand this control at the wrong value.
        {/if}
      </p>
      {#if !laneCurve?.breakpoints.length}
        <p class="desc">
          No points: this curve sends nothing, so the knob keeps whatever the player set on the
          device.
        </p>
      {/if}
      <hr />
      <button onclick={() => onlaneaction("constant", laneSelection!, laneCmd!.id)}>
        Set to constant…
      </button>
      <button
        class="danger"
        disabled={!laneCurve?.breakpoints.length}
        onclick={() => onlaneaction("clear", laneSelection!, laneCmd!.id)}
      >
        Clear curve
      </button>
    {:else if event && single && command}
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
          <Num
            min={MIN_EVENT_TICKS / PPQN}
            step={0.25}
            value={(event.length ?? 0) / PPQN}
            onchange={setLength}
          />
        {/if}
        <span class="microlabel">Lane</span>
        <Num min={0} value={event.lane} onchange={(v) => { oncommit(); event!.lane = v; }} />
      </div>
      {#if command.params.length > 0 && event.params}
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
          <input type="range" min="0" max="1.5" step="0.01" bind:value={track.volume}
            style="--fill:{(track.volume / 1.5) * 100}%" />
          <span class="microlabel">Pan</span>
          <input type="range" min="-1" max="1" step="0.01" bind:value={track.pan}
            style="--fill:{((track.pan + 1) / 2) * 100}%" />
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
    width: 250px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--panel);
    border-left: 1px solid var(--line);
  }
  .head {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    flex-shrink: 0;
    position: sticky;
    top: 0;
    background: var(--panel);
    z-index: 1;
  }
  .body {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .title {
    font-weight: 700;
    font-size: 15px;
  }
  .kind {
    color: var(--accent);
    margin-top: 3px;
  }
  .desc {
    color: var(--fg-2);
    font-size: 12px;
    margin: 6px 0 0;
    line-height: 1.45;
    -webkit-user-select: text;
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
  .lane-toggle {
    justify-content: flex-start;
    height: 24px;
    font-size: 12px;
    color: var(--fg-2);
  }
  .lane-toggle.on {
    color: var(--auto);
    border-color: color-mix(in srgb, var(--auto) 40%, var(--line));
  }
  .file,
  .device {
    font-size: 11px;
    color: var(--fg-2);
    word-break: break-all;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
