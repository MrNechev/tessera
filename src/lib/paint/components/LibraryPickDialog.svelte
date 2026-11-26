<script lang="ts">
  /** Pick one photo from the library (open it in Paint, or add it as a layer). */
  import Modal from '../../components/ui/Modal.svelte';
  import LibraryPanel from '../../documents/components/LibraryPanel.svelte';
  import type { FileItem } from '../../documents/lib/types';
  import { toasts } from '../../stores/toasts.svelte';

  interface Props {
    title: string;
    onpick: (f: FileItem) => void;
    onclose: () => void;
  }
  let { title, onpick, onclose }: Props = $props();

  function pick(items: FileItem[]) {
    const f = items.find((i) => i.kind === 'image');
    if (!f) {
      toasts.push({ kind: 'info', title: 'Pick a photo', message: 'Paint works with images, not videos.' });
      return;
    }
    onclose();
    onpick(f);
  }
</script>

<Modal {title} width={1100} {onclose}>
  <div class="lt pick">
    <LibraryPanel paletteTarget={false} hint="Click a photo to open it · double-click: preview" onpick={(items) => pick(items)} />
  </div>
</Modal>

<style>
  .pick {
    height: min(70vh, 720px);
    display: flex;
    flex-direction: column;
  }
  .pick :global(.panel) {
    flex: 1;
    min-height: 0;
    background: transparent;
  }
</style>
