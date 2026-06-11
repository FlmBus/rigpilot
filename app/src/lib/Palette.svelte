<script lang="ts" module>
  // The HTML5 drag API hides payload data during dragover, so the Timeline reads
  // the currently dragged command from here to validate/preview the drop target.
  export const dragPayload: {
    current: { definitionId: string; commandId: string } | null;
  } = { current: null };
</script>

<script lang="ts">
  import { COMMAND_TYPE_COLORS, type CommandInfo, type DefinitionInfo } from "./types";

  let { definition }: { definition: DefinitionInfo | undefined } = $props();

  let query = $state("");
  let collapsed = $state<Record<string, boolean>>({});

  const groups = $derived.by(() => {
    if (!definition) return [];
    const q = query.toLowerCase();
    const byGroup = new Map<string, CommandInfo[]>();
    for (const c of definition.commands) {
      if (q && !c.name.toLowerCase().includes(q)) continue;
      const g = c.group ?? "Ungrouped";
      if (!byGroup.has(g)) byGroup.set(g, []);
      byGroup.get(g)!.push(c);
    }
    return [...byGroup.entries()]; // insertion order = order of first appearance
  });

  function ondragstart(e: DragEvent, cmd: CommandInfo) {
    if (!definition) return;
    dragPayload.current = { definitionId: definition.id, commandId: cmd.id };
    e.dataTransfer?.setData(
      "application/x-rigpilot-command",
      JSON.stringify(dragPayload.current),
    );
    e.dataTransfer!.effectAllowed = "copy";
  }
</script>

<div class="palette panel">
  <div class="head microlabel">Command Palette</div>
  {#if !definition}
    <p class="empty">Select a MIDI track to see its commands.</p>
  {:else}
    <div class="sub-head">
      <span class="microlabel device">{definition.manufacturer} {definition.model}</span>
      <input class="search" type="text" placeholder="Filter…" bind:value={query} />
    </div>
    <div class="groups">
      {#each groups as [group, commands]}
        <div class="group">
          <button class="ghost group-toggle" onclick={() => (collapsed[group] = !collapsed[group])}>
            <span class="caret">{collapsed[group] ? "▸" : "▾"}</span>
            <span class="microlabel">{group}</span>
          </button>
          {#if !collapsed[group]}
            <div class="items">
              {#each commands as cmd}
                <div
                  class="item"
                  role="button"
                  tabindex="0"
                  draggable="true"
                  ondragstart={(e) => ondragstart(e, cmd)}
                  ondragend={() => (dragPayload.current = null)}
                  title={(cmd.description ?? "") +
                    (cmd.deterministic ? "" : "\n⚠ Result depends on the device's current state.")}
                >
                  <span class="stripe" style:background={COMMAND_TYPE_COLORS[cmd.commandType]}></span>
                  <span class="name">{cmd.name}</span>
                  {#if !cmd.deterministic}<span class="warn">⚠</span>{/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .palette {
    display: flex;
    flex-direction: column;
    width: 230px;
    flex-shrink: 0;
    overflow: hidden;
  }
  .empty {
    color: var(--fg-dim);
    font-size: 12px;
    margin: 0;
    padding: 10px;
  }
  .head {
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
    flex-shrink: 0;
  }
  .sub-head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .device {
    color: var(--accent);
  }
  .search {
    width: 100%;
    height: 24px;
  }
  .groups {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    justify-content: flex-start;
    gap: 10px;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 8px;
  }
  .group {
    min-width: 0;
  }
  .group-toggle {
    height: 20px;
    padding: 0 2px;
  }
  .caret {
    font-size: 9px;
    color: var(--fg-faint);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px 0 0;
    border: 1px solid var(--line);
    border-radius: 2px;
    background: var(--bg2);
    cursor: grab;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }
  .item:hover {
    border-color: var(--accent);
  }
  .item:active {
    cursor: grabbing;
  }
  .stripe {
    width: 3px;
    align-self: stretch;
    flex-shrink: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .warn {
    color: var(--warn);
    font-size: 10px;
  }
</style>
