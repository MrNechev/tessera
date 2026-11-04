<script lang="ts">
  interface Props {
    label?: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    /** Display transform for the number box (e.g. fraction → %). */
    scale?: number;
    unit?: string;
    decimals?: number;
    disabled?: boolean;
    onchange: (v: number) => void;
  }
  let { label, value, min = 0, max = 100, step = 1, scale = 1, unit = '', decimals = 0, disabled = false, onchange }: Props = $props();

  const fill = $derived(`${((value - min) / (max - min || 1)) * 100}%`);
  const shown = $derived((value * scale).toFixed(decimals));

  function fromBox(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!Number.isFinite(v)) return;
    onchange(Math.max(min, Math.min(max, v / scale)));
  }
</script>

<div class="slider" class:disabled>
  {#if label}
    <div class="top">
      <span class="label">{label}</span>
      <span class="num">
        <input type="number" value={shown} step={step * scale} onchange={fromBox} {disabled} />{unit}
      </span>
    </div>
  {/if}
  <input
    type="range"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    style:--fill={fill}
    oninput={(e) => onchange(parseFloat((e.target as HTMLInputElement).value))}
  />
</div>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .slider.disabled {
    opacity: 0.5;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .num {
    display: inline-flex;
    align-items: center;
    color: var(--text-3);
    font-size: 11.5px;
  }
  .num input {
    width: 46px;
    text-align: right;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 1px 3px;
    color: var(--text-2);
    font-size: 11.5px;
    -moz-appearance: textfield;
    appearance: textfield;
  }
  .num input::-webkit-inner-spin-button {
    -webkit-appearance: none;
  }
  .num input:hover,
  .num input:focus {
    border-color: var(--border-strong);
    background: var(--bg-2);
    outline: none;
  }
</style>
