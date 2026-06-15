<script lang="ts">
  let {
    value = $bindable(0),
    min = Number.NEGATIVE_INFINITY,
    max = Number.POSITIVE_INFINITY,
    step = 1,
    flat = false,
    onchange,
  }: {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    flat?: boolean;
    onchange?: (v: number) => void;
  } = $props();

  function set(v: number) {
    if (Number.isNaN(v)) return;
    const clamped = Math.max(min, Math.min(max, v));
    value = clamped;
    onchange?.(clamped);
  }

  // vertical drag to scrub; a plain click (no drag) focuses the field for typing
  let inputEl: HTMLInputElement | undefined;
  let dragging = false;
  let moved = false;
  let startY = 0;
  let startVal = 0;
  function down(e: PointerEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".btns")) return;
    dragging = true;
    moved = false;
    startY = e.clientY;
    startVal = value;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    const dy = startY - e.clientY;
    if (Math.abs(dy) >= 3) moved = true;
    if (moved) set(startVal + Math.round(dy / 4) * step);
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    if (!moved) {
      inputEl?.focus();
      inputEl?.select();
    }
  }
</script>

<span class="num" class:flat onpointerdown={down} onpointermove={move} onpointerup={up}>
  <input
    bind:this={inputEl}
    type="number"
    {step}
    value={Number(Number(value).toFixed(4))}
    onchange={(e) => set(Number(e.currentTarget.value))}
  />
  <span class="btns">
    <button type="button" tabindex="-1" aria-label="Increment" onclick={() => set(value + step)}>▴</button>
    <button type="button" tabindex="-1" aria-label="Decrement" onclick={() => set(value - step)}>▾</button>
  </span>
</span>

<style>
  .num {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid #000;
    border-radius: var(--radius-sm);
    background: #07070a;
    box-shadow: var(--well-in);
    height: var(--ctl-h);
    overflow: hidden;
    cursor: ns-resize;
    touch-action: none;
  }
  .num:focus-within {
    border-color: var(--accent);
    cursor: text;
  }
  input {
    border: none;
    background: transparent;
    box-shadow: none;
    height: 100%;
    width: 52px;
    min-width: 0;
    font-family: var(--font-mono);
    cursor: inherit;
  }
  /* flat = embedded in an LCD / readout: no chrome, green mono, no steppers */
  .num.flat {
    border: none;
    background: transparent;
    box-shadow: none;
    height: auto;
  }
  .num.flat input {
    width: 46px;
    padding: 0;
    color: var(--green);
    font-size: 13px;
  }
  .num.flat .btns {
    display: none;
  }
  input:focus-visible {
    outline: none;
  }
  .btns {
    display: flex;
    flex-direction: column;
    width: 15px;
    border-left: 1px solid var(--line);
    flex-shrink: 0;
  }
  button {
    flex: 1;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--fg-faint);
    font-size: 7px;
    line-height: 1;
    padding: 0;
    height: auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  button:hover {
    color: var(--accent);
    background: var(--bg3);
  }
  button:first-child {
    border-bottom: 1px solid var(--line);
  }
</style>
