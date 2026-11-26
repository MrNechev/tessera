<script lang="ts">
  import Modal from '../../components/ui/Modal.svelte';
  import { SHORTCUTS } from '../actions.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();
</script>

<Modal title="Paint shortcuts" width={620} {onclose}>
  <div class="grid">
    {#each SHORTCUTS as [k, what] (k)}
      <span class="k">{#each k.split(', ') as part, i (i)}{#if i}<span class="sep">, </span>{/if}<span class="kbd">{part}</span>{/each}</span>
      <span>{what}</span>
    {/each}
  </div>
</Modal>

<style>
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 7px 16px;
    align-items: center;
  }
  .k {
    white-space: nowrap;
    text-align: right;
  }
  .sep {
    color: var(--text-3);
  }
</style>
