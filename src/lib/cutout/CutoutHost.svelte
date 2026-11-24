<script lang="ts">
  /**
   * Mounted once by the app: shows the Cut out dialog when someone calls
   * `openCutout()`. The dialog (and with it the worker and onnxruntime) is
   * a separate chunk, fetched on first use.
   */
  import { cutoutDialog } from './open.svelte';

  const load = () => import('./CutoutDialog.svelte');
</script>

{#if cutoutDialog.request}
  {@const request = cutoutDialog.request}
  {#await load() then m}
    <m.default {request} onclose={() => (cutoutDialog.request = null)} />
  {/await}
{/if}
