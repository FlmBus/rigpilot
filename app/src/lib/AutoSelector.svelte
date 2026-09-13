<script lang="ts">
  // Picks which Automation Command's curve a track's Automation Lane shows.
  // One lane per track, one curve at a time — the selector is the only way in,
  // so curves that already have points are grouped at the top and marked.
  import { COMMAND_TYPE_COLORS, type CommandInfo } from "./types";

  let {
    commands,
    selected,
    inUse,
    disabled,
    inUseCount,
    onselect,
  }: {
    commands: CommandInfo[];
    selected: string | null;
    /** Commands whose curve has at least one breakpoint. */
    inUse: Set<string>;
    /** Commands whose curve is switched off. */
    disabled: Set<string>;
    inUseCount: number;
    onselect: (commandId: string | null) => void;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let root = $state<HTMLDivElement>();

  const current = $derived(commands.find((c) => c.id === selected) ?? null);

  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? commands.filter((c) => c.name.toLowerCase().includes(q)) : commands;
  });

  const used = $derived(matches.filter((c) => inUse.has(c.id)));

  /** Everything else, in palette order, under its Group. */
  const groups = $derived.by(() => {
    const byGroup = new Map<string, CommandInfo[]>();
    for (const c of matches) {
      if (inUse.has(c.id)) continue;
      const g = c.group ?? "Ungrouped";
      if (!byGroup.has(g)) byGroup.set(g, []);
      byGroup.get(g)!.push(c);
    }
    return [...byGroup.entries()];
  });

  function choose(id: string | null) {
    open = false;
    query = "";
    onselect(id);
  }

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    query = "";
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }}
  onkeydown={(e) => {
    if (open && e.key === "Escape") open = false;
  }}
/>

<div class="autosel" bind:this={root}>
  <button class="trigger" class:open onclick={toggle} title="Which curve this lane shows">
    {#if current}
      <span class="tdot circle" style:background={COMMAND_TYPE_COLORS.automation}></span>
      <span class="label">{current.name}</span>
      {#if inUse.has(current.id)}
        <span class="mark" class:off={disabled.has(current.id)}>●</span>
      {/if}
    {:else}
      <span class="label none">No automation</span>
      {#if inUseCount > 0}<span class="badge microlabel">{inUseCount}</span>{/if}
    {/if}
    <span class="caret">▾</span>
  </button>

  {#if open}
    <div class="pop glass" role="listbox" tabindex="-1">
      {#if commands.length > 6}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="filter" type="text" placeholder="Filter…" bind:value={query} autofocus />
      {/if}
      <div class="items">
        <button class="item none-item" class:sel={!selected} onclick={() => choose(null)}>
          <span class="label">No automation</span>
          <span class="hint microlabel">collapse</span>
        </button>

        {#if used.length > 0}
          <div class="group-label microlabel">In use</div>
          {#each used as c}
            <button class="item" class:sel={c.id === selected} onclick={() => choose(c.id)}>
              <span class="tdot circle" style:background={COMMAND_TYPE_COLORS.automation}></span>
              <span class="label">{c.name}</span>
              <span class="mark" class:off={disabled.has(c.id)}>●</span>
            </button>
          {/each}
        {/if}

        {#each groups as [group, items]}
          <div class="group-label microlabel">{group}</div>
          {#each items as c}
            <button class="item" class:sel={c.id === selected} onclick={() => choose(c.id)}>
              <span class="tdot circle dim" style:background={COMMAND_TYPE_COLORS.automation}></span>
              <span class="label">{c.name}</span>
            </button>
          {/each}
        {/each}

        {#if matches.length === 0}
          <div class="empty microlabel">No matching command</div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .autosel {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 20px;
    padding: 0 5px;
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--fg-2);
    font-size: 11px;
  }
  .trigger:hover,
  .trigger.open {
    color: var(--fg);
    border-color: var(--line-2);
  }
  .label {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    color: var(--fg-3);
  }
  .caret {
    font-size: 8px;
    color: var(--fg-3);
    flex-shrink: 0;
  }
  .badge {
    flex-shrink: 0;
    padding: 0 4px;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 9px;
  }
  .mark {
    flex-shrink: 0;
    font-size: 8px;
    color: var(--auto);
  }
  .mark.off {
    color: var(--fg-3);
  }
  .tdot.dim {
    opacity: 0.4;
  }
  .pop {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 50;
    width: max(190px, 100%);
    max-height: 320px;
    display: flex;
    flex-direction: column;
    padding: 4px;
  }
  .filter {
    height: 22px;
    margin-bottom: 4px;
    font-size: 11px;
  }
  .items {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 24px;
    padding: 0 6px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--fg-2);
    font-size: 12px;
  }
  .item:hover {
    background: var(--accent);
    color: var(--accent-fg);
  }
  .item:hover .mark {
    color: var(--accent-fg);
  }
  .item.sel {
    color: var(--fg);
  }
  .none-item .hint {
    color: var(--fg-3);
    font-size: 9px;
  }
  .none-item:hover .hint {
    color: color-mix(in srgb, var(--accent-fg) 75%, transparent);
  }
  .group-label {
    padding: 6px 6px 2px;
    color: var(--fg-3);
  }
  .empty {
    padding: 8px 6px;
    color: var(--fg-3);
  }
</style>
