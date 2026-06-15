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
    background: rgba(5, 5, 7, 0.6);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .dialog {
    min-width: 380px;
    max-width: 520px;
    background: linear-gradient(180deg, #141419, #0e0e12);
    border: 1px solid #000;
    border-radius: var(--radius);
    box-shadow: var(--rim), var(--float);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 30px;
    padding: 0 6px 0 14px;
    border-bottom: 1px solid #000;
    box-shadow: 0 1px 0 rgba(255, 255, 255, 0.04);
  }
  .body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
