<script lang="ts">
  // "Aa" menu next to a section title: the classic serif or one of Tessera's text styles
  // (text/styles.ts TITLE_STYLES). Each entry is drawn in its own style.
  import { lookCss, titleStyle, TITLE_STYLES } from '../../text/styles';

  let { value, onpick }: { value: string | undefined; onpick: (id: string | undefined) => void } = $props();
  let open = $state(false);
  const current = $derived(titleStyle(value));

  function pick(id: string) {
    open = false;
    onpick(id === 'classic' ? undefined : id);
  }
</script>

<div class="tsm">
  <button class="icon-btn aa" class:on={!!current.look} title="Title style: {current.label}" onclick={() => (open = !open)}>Aa</button>
  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (open = false)}></div>
    <div class="pop" role="menu">
      <div class="head">Title style</div>
      {#each TITLE_STYLES as t (t.id)}
        <button class="opt" class:on={t.id === current.id} role="menuitemradio" aria-checked={t.id === current.id} onclick={() => pick(t.id)}>
          <span class="sample" class:classic={!t.look} style={t.look ? lookCss(t.look) : ''} style:font-size="{Math.round(17 * t.scale)}px">{t.label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tsm {
    position: relative;
  }
  .aa {
    font: 650 14px var(--serif);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 9;
  }
  .pop {
    position: absolute;
    left: 0;
    top: 32px;
    z-index: 10;
    min-width: 200px;
    padding: 4px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .head {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    padding: 4px 10px 2px;
  }
  .opt {
    display: flex;
    align-items: center;
    width: 100%;
    min-height: 36px;
    padding: 4px 10px;
    border: none;
    background: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    color: var(--text);
  }
  .opt:hover {
    background: var(--surface-3);
  }
  .opt.on {
    background: var(--accent-soft);
  }
  .sample {
    line-height: 1.2;
    white-space: nowrap;
  }
  .sample.classic {
    font-family: var(--serif);
    font-weight: 650;
  }
</style>
