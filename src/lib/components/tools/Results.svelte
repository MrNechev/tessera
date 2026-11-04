<script lang="ts">
  import { CircleAlert, CircleCheck, FolderOpen, MinusCircle } from '@lucide/svelte';
  import { api } from '../../api/backend';
  import type { JobResult } from '../../api/types';
  import { formatBytes } from '../../stores/batch.svelte';

  interface Props {
    results: JobResult[];
  }
  let { results }: Props = $props();

  const done = $derived(results.filter((r) => r.status === 'done'));
  const before = $derived(done.reduce((s, r) => s + r.inputBytes, 0));
  const after = $derived(done.reduce((s, r) => s + r.outputBytes, 0));
  const saved = $derived(before > 0 ? 1 - after / before : 0);
  const firstOut = $derived(done.find((r) => r.output)?.output ?? null);
</script>

{#if results.length}
  <div class="results card">
    <header>
      <div>
        <h3>Results</h3>
        <span class="hint">
          {done.length} done{results.length - done.length ? ` · ${results.length - done.length} skipped/failed` : ''} ·
          {formatBytes(before)} → {formatBytes(after)}
          {#if before > 0}<b class:good={saved > 0} class:bad={saved < 0}>({saved >= 0 ? '−' : '+'}{Math.abs(saved * 100).toFixed(1)}%)</b>{/if}
        </span>
      </div>
      <span class="spacer"></span>
      {#if firstOut}
        <button class="btn small" onclick={() => void api.revealPath(firstOut)}><FolderOpen size={14} /> Show files</button>
      {/if}
    </header>
    <div class="list">
      {#each results as r (r.input)}
        <div class="res">
          {#if r.status === 'done'}<span class="ok"><CircleCheck size={15} /></span>
          {:else if r.status === 'skipped'}<span class="skip"><MinusCircle size={15} /></span>
          {:else}<span class="fail"><CircleAlert size={15} /></span>{/if}
          <div class="info">
            <span class="truncate" title={r.output ?? r.input}>{(r.output ?? r.input).split('/').pop()}</span>
            {#if r.message || r.warnings.length}
              <span class="hint truncate" title={[r.message, ...r.warnings].filter(Boolean).join('\n')}>{r.message ?? r.warnings[0]}</span>
            {/if}
          </div>
          {#if r.status === 'done'}
            <span class="meta">{r.quality ? `q${r.quality}` : ''}{r.ssim ? ` · SSIM ${r.ssim.toFixed(3)}` : ''}</span>
            <span class="meta">{formatBytes(r.inputBytes)} → {formatBytes(r.outputBytes)}</span>
            {@const d = r.inputBytes ? 1 - r.outputBytes / r.inputBytes : 0}
            <span class="delta" class:good={d > 0} class:bad={d < 0}>{d >= 0 ? '−' : '+'}{Math.abs(d * 100).toFixed(0)}%</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .results {
    display: flex;
    flex-direction: column;
    max-height: 42%;
    min-height: 140px;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    font-size: 14px;
  }
  .list {
    overflow-y: auto;
    padding: 6px;
  }
  .res {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
    padding: 2px 10px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .meta {
    color: var(--text-3);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .delta {
    width: 52px;
    text-align: right;
    font-weight: 700;
    font-size: 12px;
  }
  .good {
    color: var(--success);
  }
  .bad {
    color: var(--warning);
  }
  .ok {
    color: var(--success);
  }
  .skip {
    color: var(--text-3);
  }
  .fail {
    color: var(--danger);
  }
</style>
