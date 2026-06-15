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
                  <span
                    class="tdot {cmd.commandType === 'hold' ? 'square' : cmd.commandType === 'automation' ? 'circle' : 'diamond'}"
                    style:background={COMMAND_TYPE_COLORS[cmd.commandType]}
                  ></span>
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
    background: linear-gradient(180deg, #141419, #0e0e12);
    border-right: 1px solid #000;
    box-shadow: var(--rim);
  }
  .empty {
    color: var(--fg-faint);
    font-size: 12px;
    margin: 0;
    padding: 14px;
    line-height: 1.5;
  }
  .head {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 12px;
    border-bottom: 1px solid #000;
    box-shadow: 0 1px 0 rgba(255, 255, 255, 0.04);
    flex-shrink: 0;
  }
  .sub-head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px 2px;
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
    gap: 14px;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 6px 8px 12px;
  }
  .group {
    min-width: 0;
  }
  .group-toggle {
    height: 18px;
    padding: 0 4px;
    gap: 5px;
  }
  .caret {
    font-size: 9px;
    color: var(--fg-faint);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-top: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 27px;
    padding: 0 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--fg-dim);
    cursor: grab;
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    transition: background 90ms ease, color 90ms ease;
  }
  .item:hover {
    background: var(--bg3);
    color: var(--fg);
  }
  .item:active {
    cursor: grabbing;
    background: var(--bg2);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .warn {
    color: var(--warn);
    font-size: 10px;
    margin-left: auto;
  }
</style>
