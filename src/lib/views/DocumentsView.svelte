<script lang="ts">
  /** Documents (from Letopis): the document list, or one document's editor/canvas. */
  import DocList from '../documents/components/DocList.svelte';
  import Editor from '../documents/components/Editor.svelte';
  import { router } from '../documents/lib/router.svelte';

</script>

<div class="lt documents-view">
  {#key router.route.name + ('id' in router.route ? router.route.id : '')}
    {#if router.route.name === 'doc'}
      <Editor id={router.route.id} />
    {:else}
      <DocList />
    {/if}
  {/key}
</div>

<style>
  .documents-view {
    height: 100%;
    overflow: auto;
    background: var(--bg);
  }
</style>
