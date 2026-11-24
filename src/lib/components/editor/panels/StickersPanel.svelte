<script lang="ts">
  import { editor } from '../../../editor/editor.svelte';
  import { createSticker } from '../../../model/factory';
  import type { StickerInfo } from '../../../stores/stickers.svelte';
  import StickerGrid from '../../StickerGrid.svelte';

  function add(s: StickerInfo) {
    const size = editor.pageShort * 0.18;
    const aspect = s.width / s.height;
    const w = aspect >= 1 ? size : size * aspect;
    const h = aspect >= 1 ? size / aspect : size;
    // Cascade new stickers a little so repeated clicks don't stack exactly.
    const n = editor.doc.elements.filter((e) => e.kind === 'sticker').length % 6;
    const off = n * editor.pageShort * 0.02;
    editor.place(createSticker({ sticker: s.id, x: 0, y: 0, w, h }), {
      x: editor.doc.page.width / 2 + off,
      y: editor.doc.page.height / 2 + off,
    });
  }
</script>

<div class="head">
  <h3>Stickers</h3>
</div>
<!-- the catalogue grid is shared with the story canvas' sticker picker -->
<div class="body">
  <StickerGrid onpick={add} />
</div>

<style>
  .head {
    padding: 16px 14px 10px;
  }
  h3 {
    font-size: 15px;
  }
  .body {
    padding: 0 14px 20px;
  }
</style>
