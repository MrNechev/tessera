<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { DocState } from '../lib/docstate.svelte';
  import { findAll, replaceAll, replaceOne, type FindOpts } from '../lib/findreplace';
  import { toasts } from '../lib/toast.svelte';
  import Icon from './Icon.svelte';

  let { ds, onclose, onjump }: { ds: DocState; onclose: () => void; onjump: (sectionId?: string, itemId?: string) => void } = $props();

  let q = $state('');
  let repl = $state('');
  let opts = $state<FindOpts>({ caseSensitive: false, wholeWord: false });
  let cur = $state(0);
  let input = $state<HTMLInputElement>();
  let tickN = $state(0); // recompute after replacing

  const matches = $derived.by(() => {
    void tickN;
    return ds.doc ? findAll(ds.doc, q, opts) : [];
  });

  $effect(() => {
    void q;
    void opts.caseSensitive;
    void opts.wholeWord;
    cur = 0;
  });

  function go(d: number) {
    if (!matches.length) return;
    cur = (cur + d + matches.length) % matches.length;
    const m = matches[cur];
    onjump(m.sectionId, m.itemId);
  }

  function changed() {
    tickN++;
    ds.revision++; // re-mount text editors with the new content
    ds.touch();
  }

  function one() {
    if (!ds.doc || !matches.length) return;
    if (replaceOne(ds.doc, q, repl, opts, cur)) {
      changed();
      cur = Math.min(cur, Math.max(0, matches.length - 1));
    }
  }

  function all() {
    if (!ds.doc) return;
    const n = replaceAll(ds.doc, q, repl, opts);
    if (n) {
      changed();
      toasts.show(`Replaced ${n} occurrence${n === 1 ? '' : 's'} — undo with Version history if needed`, 'ok');
    }
  }

  onMount(() => {
    tick().then(() => input?.focus());
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      go(e.shiftKey ? -1 : 1);
    }
  }
</script>

<div class="fr">
  <Icon name="search" size={15} />
  <input class="input" bind:this={input} bind:value={q} onkeydown={onKey} placeholder="Find in this document" />
  <button class="seg" class:on={opts.caseSensitive} title="Match case" onclick={() => (opts.caseSensitive = !opts.caseSensitive)}>Aa</button>
  <button class="seg" class:on={opts.wholeWord} title="Whole words" onclick={() => (opts.wholeWord = !opts.wholeWord)}>W</button>
  <span class="count muted">{q ? (matches.length ? `${cur + 1} / ${matches.length}` : 'no matches') : ''}</span>
  <button class="icon-btn" title="Previous (Shift+Enter)" onclick={() => go(-1)} disabled={!matches.length}><Icon name="chev-up" /></button>
  <button class="icon-btn" title="Next (Enter)" onclick={() => go(1)} disabled={!matches.length}><Icon name="chev-down" /></button>
  <span class="sep"></span>
  <input class="input" bind:value={repl} placeholder="Replace with" onkeydown={(e) => e.key === 'Escape' && onclose()} />
  <button class="btn sm" onclick={one} disabled={!matches.length}>Replace</button>
  <button class="btn sm" onclick={all} disabled={!matches.length}>All</button>
  <button class="icon-btn" title="Close (Esc)" onclick={onclose}><Icon name="x" /></button>
  {#if matches[cur]}
    {@const m = matches[cur]}
    <div class="ctx muted"><span class="w">{m.where}</span> …{m.before}<mark>{m.text}</mark>{m.after}…</div>
  {/if}
</div>

<style>
  .fr {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg-2);
    color: var(--text-3);
  }
  .fr .input {
    height: 28px;
    width: 220px;
  }
  .seg {
    height: 26px;
    min-width: 28px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface-2);
    font-size: 12px;
    cursor: pointer;
  }
  .seg.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .count {
    min-width: 70px;
    font-size: 12px;
    text-align: center;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--line-strong);
    margin: 0 4px;
  }
  .ctx {
    flex-basis: 100%;
    font-size: 12px;
    padding: 2px 22px 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ctx .w {
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.05em;
    margin-right: 6px;
  }
  mark {
    background: #e8b04b66;
    color: var(--text);
  }
</style>
