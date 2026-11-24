<script lang="ts">
  /**
   * A story image with its whole look baked in (crop, turns, pixels, vignette),
   * for places that show plain images: print layouts, slide previews, canvas
   * cards. Falls back to the untouched image while rendering or without a look.
   */
  import { bakeLook, hasLook, type Crop, type Look } from './looks.svelte';

  let { src, look, maxW = 1600, alt = '', style = '' }: { src: string; look: Look & { crop?: Crop }; maxW?: number; alt?: string; style?: string } = $props();

  let baked = $state('');
  const active = $derived(hasLook(look) || !!look.crop);
  $effect(() => {
    const s = src;
    const l = $state.snapshot(look) as Look & { crop?: Crop };
    baked = '';
    if (!active) return;
    let live = true;
    const img = new Image();
    img.decoding = 'async';
    img.src = s;
    void img
      .decode()
      .then(() => {
        if (live) baked = bakeLook(img, l.crop, l, maxW).toDataURL('image/jpeg', 0.9);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  });
</script>

<img src={baked || src} {alt} draggable="false" style={style || undefined} />
