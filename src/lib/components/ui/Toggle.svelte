<script lang="ts">
  interface Props {
    checked: boolean;
    label?: string;
    hint?: string;
    disabled?: boolean;
    onchange: (v: boolean) => void;
  }
  let { checked, label, hint, disabled = false, onchange }: Props = $props();
</script>

<label class="toggle" class:disabled>
  <span class="text">
    {#if label}<span class="lbl">{label}</span>{/if}
    {#if hint}<span class="hint">{hint}</span>{/if}
  </span>
  <input type="checkbox" {checked} {disabled} onchange={(e) => onchange((e.target as HTMLInputElement).checked)} />
  <span class="track"><span class="knob"></span></span>
</label>

<style>
  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    min-height: 26px;
  }
  .toggle.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .lbl {
    font-size: 12.5px;
    color: var(--text);
  }
  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .track {
    position: relative;
    width: 32px;
    height: 18px;
    border-radius: 18px;
    background: var(--border-strong);
    transition: background 0.15s;
    flex: none;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }
  input:checked + .track {
    background: var(--accent);
  }
  input:checked + .track .knob {
    transform: translateX(14px);
  }
  input:focus-visible + .track {
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
</style>
