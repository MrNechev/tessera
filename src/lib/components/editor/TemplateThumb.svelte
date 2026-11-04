<script lang="ts" module>
  import { AdjustCache } from '../../render/adjust';
  import type { RenderResources } from '../../render/renderer';
  const cache = new Map<string, string>();
  const thumbResources: RenderResources = {
    image: () => null,
    sticker: (id) => stickers.image(id),
    adjustCache: new AdjustCache(1),
  };
</script>

<script lang="ts">
  import { onFontsLoaded } from '../../assets/fonts';
  import { renderDocument } from '../../render/renderer';
  import { stickers } from '../../stores/stickers.svelte';
  import { previewDoc } from '../../templates/instantiate';
  import type { TemplateDef } from '../../templates/types';
  import { onMount } from 'svelte';

  interface Props {
    template: TemplateDef;
    width?: number;
  }
  let { template, width = 240 }: Props = $props();
  let url = $state('');
  let fontTick = $state(0);

  function render() {
    const key = `${template.id}|${width}|${stickers.version}|${fontTick}`;
    const hit = cache.get(key);
    if (hit) {
      url = hit;
      return;
    }
    const doc = previewDoc(template, (id) => stickers.aspect(id));
    const s = (width * 2) / doc.page.width;
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(doc.page.width * s);
    canvas.height = Math.round(doc.page.height * s);
    const c = canvas.getContext('2d')!;
    renderDocument(c, doc, { scale: s, resources: thumbResources, placeholders: true });
    url = canvas.toDataURL('image/jpeg', 0.85);
    cache.set(key, url);
  }

  $effect(() => {
    void stickers.version;
    void stickers.loaded;
    void fontTick;
    if (stickers.loaded) render();
  });

  onMount(() => {
    let t: ReturnType<typeof setTimeout> | null = null;
    return onFontsLoaded(() => {
      if (t) clearTimeout(t);
      t = setTimeout(() => fontTick++, 150);
    });
  });
</script>

<div class="thumb" style:aspect-ratio="{template.page.width} / {template.page.height}">
  {#if url}<img src={url} alt={template.name} draggable="false" />{/if}
</div>

<style>
  .thumb {
    width: 100%;
    border-radius: 8px;
    overflow: hidden;
    background: var(--panel-3);
    box-shadow: var(--shadow-sm);
  }
  img {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
