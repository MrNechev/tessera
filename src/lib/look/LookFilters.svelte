<script lang="ts">
  // The SVG filters behind every photo look on screen (see looks.svelte.ts).
  import { lookDefs, vignetteImage } from './looks.svelte';
</script>

<svg class="look-defs" aria-hidden="true" width="0" height="0">
  <defs>
    {#each lookDefs.list as [id, d] (id)}
      <filter {id} x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB">
        {#if d.matrix}
          <feColorMatrix type="matrix" values={d.matrix} result="col" />
        {/if}
        {#if d.vignette > 0}
          <feImage href={vignetteImage(d.vignette)} x="0%" y="0%" width="100%" height="100%" preserveAspectRatio="none" result="vig" />
          <feBlend in={d.matrix ? 'col' : 'SourceGraphic'} in2="vig" mode="multiply" result="mul" />
          <feComposite in="mul" in2="SourceGraphic" operator="in" />
        {/if}
      </filter>
    {/each}
  </defs>
</svg>

<style>
  .look-defs {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
    pointer-events: none;
  }
</style>
