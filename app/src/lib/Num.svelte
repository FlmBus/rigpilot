<script lang="ts">
  let {
    value = $bindable(0),
    min = Number.NEGATIVE_INFINITY,
    max = Number.POSITIVE_INFINITY,
    step = 1,
    onchange,
  }: {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    onchange?: (v: number) => void;
  } = $props();

  function set(v: number) {
    if (Number.isNaN(v)) return;
    const clamped = Math.max(min, Math.min(max, v));
    value = clamped;
    onchange?.(clamped);
  }
</script>

<span class="num">
  <input
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
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg2);
    height: var(--ctl-h);
    overflow: hidden;
  }
  .num:focus-within {
    border-color: var(--accent);
  }
  input {
    border: none;
    background: transparent;
    height: 100%;
    width: 52px;
    min-width: 0;
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
