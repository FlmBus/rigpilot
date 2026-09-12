<script lang="ts" module>
  // The HTML5 drag API hides payload data during dragover, so the Timeline reads
  // the currently dragged command from here to validate/preview the drop target.
  export const dragPayload: {
    current: { definitionId: string; commandId: string } | null;
  } = { current: null };
</script>

<script lang="ts">
  import { COMMAND_TYPE_COLORS, type CommandInfo, type DefinitionInfo } from "./types";

  let {
    definition,
    inUse,
    onshowautomation,
  }: {
    definition: DefinitionInfo | undefined;
    /** Automation Commands whose curve already has breakpoints on the shown track. */
    inUse: Set<string>;
    onshowautomation: (commandId: string) => void;
  } = $props();

  let query = $state("");
  let collapsed = $state<Record<string, boolean>>({});

  function bucket(commands: CommandInfo[]) {
    const byGroup = new Map<string, CommandInfo[]>();
    for (const c of commands) {
      const g = c.group ?? "Ungrouped";
      if (!byGroup.has(g)) byGroup.set(g, []);
      byGroup.get(g)!.push(c);
    }
    return [...byGroup.entries()]; // insertion order = order of first appearance
  }

  // One-Shot/Hold are dragged onto the timeline; Automation is clicked to open
  // its curve — two different verbs, so they render as two labelled sections.
  const filtered = $derived.by(() => {
    if (!definition) return [];
    const q = query.toLowerCase();
    return definition.commands.filter((c) => !q || c.name.toLowerCase().includes(q));
  });
  const dragGroups = $derived(bucket(filtered.filter((c) => c.commandType !== "automation")));
  const autoGroups = $derived(bucket(filtered.filter((c) => c.commandType === "automation")));

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
    {#snippet groupList(list: [string, CommandInfo[]][])}
      {#each list as [group, commands]}
        <div class="group">
          <button class="ghost group-toggle" onclick={() => (collapsed[group] = !collapsed[group])}>
            <span class="caret">{collapsed[group] ? "▸" : "▾"}</span>
            <span class="microlabel">{group}</span>
          </button>
          {#if !collapsed[group]}
            <div class="items">
              {#each commands as cmd}
                {@const auto = cmd.commandType === "automation"}
                <div
                  class="item"
                  class:auto
                  role="button"
                  tabindex="0"
                  draggable={!auto}
                  ondragstart={(e) => ondragstart(e, cmd)}
                  ondragend={() => (dragPayload.current = null)}
                  onclick={() => auto && onshowautomation(cmd.id)}
                  onkeydown={(e) => e.key === "Enter" && auto && onshowautomation(cmd.id)}
                  title={auto
                    ? `${cmd.description ?? cmd.name}\nClick to show this curve in the track's automation lane.`
                    : (cmd.description ?? "") +
                      (cmd.deterministic ? "" : "\n⚠ Result depends on the device's current state.")}
                >
                  <span
                    class="tdot {cmd.commandType === 'hold' ? 'square' : 'circle'}"
                    style:background={COMMAND_TYPE_COLORS[cmd.commandType]}
                  ></span>
                  <span class="name">{cmd.name}</span>
                  {#if auto && inUse.has(cmd.id)}<span class="used" title="This curve has breakpoints">●</span>{/if}
                  {#if !cmd.deterministic}<span class="warn">⚠</span>{/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {/snippet}
    <div class="groups">
      {#if dragGroups.length}
        <div class="pal-sect"><span class="microlabel">Drag onto timeline</span></div>
        {@render groupList(dragGroups)}
      {/if}
      {#if autoGroups.length}
        <div class="pal-sect"><span class="microlabel">Automate — click to open</span></div>
        {@render groupList(autoGroups)}
      {/if}
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
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  .empty {
    color: var(--fg-3);
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
    border-bottom: 1px solid var(--line);
    flex-shrink: 0;
  }
  .pal-sect {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 4px 2px;
  }
  .pal-sect::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--line);
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
    color: var(--fg-3);
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
    color: var(--fg-2);
    cursor: grab;
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    transition: background 90ms ease, color 90ms ease;
  }
  .item:hover {
    background: var(--accent-soft);
    color: var(--fg);
  }
  .item:active {
    cursor: grabbing;
    background: var(--line);
  }
  /* Automation has a lane of its own — there is nothing to drag onto the timeline. */
  .item.auto {
    cursor: pointer;
  }
  .item.auto:active {
    cursor: pointer;
  }
  .used {
    margin-left: auto;
    font-size: 8px;
    color: var(--auto);
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
