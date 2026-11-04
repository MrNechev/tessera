<script lang="ts">
  import { FolderOpen } from '@lucide/svelte';
  import { pickFolder } from '../../api/backend';
  import type { ConflictPolicy, OutputSpec } from '../../api/types';
  import { app } from '../../stores/app.svelte';
  import Segmented from '../ui/Segmented.svelte';

  interface Props {
    value: OutputSpec;
    onchange: (v: OutputSpec) => void;
  }
  let { value, onchange }: Props = $props();

  type Where = 'same' | 'save' | 'custom';
  const where = $derived<Where>(value.directory === null ? 'same' : value.directory === app.settings.saveDirectory ? 'save' : 'custom');

  async function choose() {
    const dir = await pickFolder('Choose output folder', value.directory ?? app.settings.saveDirectory);
    if (dir) onchange({ ...value, directory: dir });
  }

  function setWhere(w: Where) {
    if (w === 'same') onchange({ ...value, directory: null });
    else if (w === 'save') onchange({ ...value, directory: app.settings.saveDirectory || null });
    else void choose();
  }
</script>

<div class="col">
  <Segmented
    options={[
      { value: 'same', label: 'Next to originals' },
      { value: 'save', label: 'Save folder' },
      { value: 'custom', label: 'Choose…' },
    ]}
    value={where}
    onchange={setWhere}
  />
  {#if value.directory}
    <button class="dir" onclick={choose} title="Change folder"><FolderOpen size={14} /><span class="truncate">{value.directory}</span></button>
  {/if}
  <div class="row">
    <span class="label">Add to file names</span>
    <span class="spacer"></span>
    <input class="input suffix" value={value.suffix} placeholder="(nothing)" oninput={(e) => onchange({ ...value, suffix: (e.target as HTMLInputElement).value })} />
  </div>
  <div class="row">
    <span class="label">If a file exists</span>
    <span class="spacer"></span>
    <select class="select" value={value.conflict} onchange={(e) => onchange({ ...value, conflict: (e.target as HTMLSelectElement).value as ConflictPolicy })}>
      <option value="rename">Keep both</option>
      <option value="skip">Skip</option>
      <option value="overwrite">Replace</option>
    </select>
  </div>
  {#if value.directory === null && !value.suffix && value.conflict === 'overwrite'}
    <p class="warn">Originals with the same format will be replaced.</p>
  {/if}
</div>

<style>
  .dir {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .suffix {
    width: 130px;
  }
  .warn {
    color: var(--warning);
    font-size: 12px;
    margin: 0;
  }
</style>
