<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    children,
  }: { title: string; onclose: () => void; children: Snippet } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog panel" role="dialog" aria-label={title}>
    <header>
      <span class="microlabel">{title}</span>
      <button class="ghost" onclick={onclose} aria-label="Close">✕</button>
    </header>
    <div class="body">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--canvas) 60%, transparent);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .dialog {
    min-width: 380px;
    max-width: 520px;
    background: var(--panel);
    border: 1px solid var(--line-2);
    border-radius: var(--r);
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 30px;
    padding: 0 6px 0 14px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
