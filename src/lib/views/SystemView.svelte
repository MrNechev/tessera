<script lang="ts">
  /** System status (from Letopis): queues, cache, GPU, connection, stress test. */
  import System from '../documents/components/System.svelte';
  import DebugOverlay from '../documents/components/DebugOverlay.svelte';

  let debug = $state(false);
  try {
    debug = localStorage.getItem('letopis.debug') === '1';
  } catch {
    /* storage unavailable */
  }
  function toggle() {
    debug = !debug;
    try {
      localStorage.setItem('letopis.debug', debug ? '1' : '0');
    } catch {
      /* ignore */
    }
  }
</script>

<div class="lt system-view">
  <System {debug} ontoggledebug={toggle} />
</div>
{#if debug}<div class="lt"><DebugOverlay onclose={toggle} /></div>{/if}

<style>
  .system-view {
    height: 100%;
    overflow: auto;
    background: var(--bg);
  }
</style>
