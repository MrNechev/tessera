<script lang="ts">
  // The SVG filters behind every photo look on screen (see looks.svelte.ts).
  import { lookDefs, vignetteImage } from './looks.svelte';
</script>

<svg class="look-defs" aria-hidden="true" width="0" height="0">
  <defs>
    {#each lookDefs.list as [id, d] (id)}
      <filter {id} x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB">
        <!-- each stage reads the previous one: SourceGraphic → col → crv → duo → vignette -->
        <feColorMatrix type="matrix" values={d.matrix ?? '1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0'} result="col" />
        {#if d.curve}
          <feComponentTransfer in="col" result="crv">
            <feFuncR type="table" tableValues={d.curve} />
            <feFuncG type="table" tableValues={d.curve} />
            <feFuncB type="table" tableValues={d.curve} />
          </feComponentTransfer>
        {:else}
          <feOffset in="col" dx="0" dy="0" result="crv" />
        {/if}
        {#if d.duotone}
          <feColorMatrix in="crv" type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0" result="lum" />
          <feComponentTransfer in="lum" result="map">
            <feFuncR type="table" tableValues={d.duotone.r} />
            <feFuncG type="table" tableValues={d.duotone.g} />
            <feFuncB type="table" tableValues={d.duotone.b} />
          </feComponentTransfer>
          <feComposite in="map" in2="crv" operator="arithmetic" k1="0" k2={d.duotone.amount} k3={1 - d.duotone.amount} k4="0" result="duo" />
        {:else}
          <feOffset in="crv" dx="0" dy="0" result="duo" />
        {/if}
        {#if d.vignette > 0}
          <feImage href={vignetteImage(d.vignette)} x="0%" y="0%" width="100%" height="100%" preserveAspectRatio="none" result="vig" />
          <feBlend in="duo" in2="vig" mode="multiply" result="mul" />
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
