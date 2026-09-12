<script lang="ts" module>
  export type MenuItem =
    | { separator: true }
    | {
        separator?: false;
        label: string;
        shortcut?: string;
        /** Shown as a radio/check mark. */
        checked?: boolean;
        disabled?: boolean;
        danger?: boolean;
        onselect: () => void;
      };
</script>

<script lang="ts">
  // Right-click menu for the Timeline (points, segments, automation lanes).
  let {
    x,
    y,
    items,
    onclose,
  }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

  let el = $state<HTMLDivElement>();
  let size = $state({ w: 0, h: 0 });

  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width !== size.w || r.height !== size.h) size = { w: r.width, h: r.height };
  });

  // Keep the menu inside the window.
  const pos = $derived({
    x: Math.max(4, Math.min(x, window.innerWidth - size.w - 4)),
    y: Math.max(4, Math.min(y, window.innerHeight - size.h - 4)),
  });

  function pick(item: MenuItem) {
    if (item.separator || item.disabled) return;
    onclose();
    item.onselect();
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (el && !el.contains(e.target as Node)) onclose();
  }}
  onkeydown={(e) => e.key === "Escape" && onclose()}
/>

<div
  class="ctx glass"
  bind:this={el}
  style="left:{pos.x}px;top:{pos.y}px"
  role="menu"
  tabindex="-1"
>
  {#each items as item}
    {#if item.separator}
      <hr />
    {:else}
      <button
        role="menuitem"
        class:checked={item.checked}
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => pick(item)}
      >
        <span class="tick">{item.checked ? "•" : ""}</span>
        <span class="label">{item.label}</span>
        {#if item.shortcut}<span class="kbd">{item.shortcut}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .ctx {
    position: fixed;
    z-index: 200;
    min-width: 178px;
    padding: 5px;
    display: flex;
    flex-direction: column;
  }
  button {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 26px;
    padding: 0 8px 0 2px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--fg);
    font-size: 12.5px;
    justify-content: flex-start;
  }
  button:hover:not(:disabled) {
    background: var(--accent);
    color: var(--accent-fg);
  }
  button:disabled {
    color: var(--fg-3);
  }
  button.danger:hover:not(:disabled) {
    background: var(--danger);
    color: var(--fg-on-warm);
  }
  .tick {
    width: 12px;
    flex-shrink: 0;
    text-align: center;
    color: var(--accent);
    font-size: 13px;
  }
  button:hover:not(:disabled) .tick {
    color: var(--accent-fg);
  }
  .label {
    flex: 1;
    text-align: left;
    white-space: nowrap;
  }
  .kbd {
    margin-left: auto;
    padding-left: 12px;
    color: var(--fg-3);
    font-size: 10px;
    font-family: var(--font-mono);
  }
  button:hover:not(:disabled) .kbd {
    color: color-mix(in srgb, var(--accent-fg) 80%, transparent);
  }
  hr {
    margin: 4px 6px;
    border: none;
    border-top: 1px solid var(--line);
  }
</style>
