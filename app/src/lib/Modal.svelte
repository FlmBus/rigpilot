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
    background: rgba(5, 5, 7, 0.7);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .dialog {
    min-width: 380px;
    max-width: 520px;
    border-color: var(--line-strong);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
