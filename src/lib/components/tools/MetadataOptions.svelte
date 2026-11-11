<script lang="ts">
  import type { MetadataPolicy } from '../../api/types';
  import Segmented from '../ui/Segmented.svelte';
  import Toggle from '../ui/Toggle.svelte';

  interface Props {
    value: MetadataPolicy;
    onchange: (v: MetadataPolicy) => void;
  }
  let { value, onchange }: Props = $props();

  const set = (patch: Partial<MetadataPolicy>) => onchange({ ...value, ...patch });
</script>

<div class="col">
  <Segmented
    options={[
      { value: 'keepAll', label: 'Keep all' },
      { value: 'custom', label: 'Choose' },
      { value: 'stripAll', label: 'Remove all' },
    ]}
    value={value.mode}
    onchange={(m) => set({ mode: m })}
  />
  {#if value.mode === 'keepAll'}
    <p class="hint">Every EXIF/XMP field and the colour profile are carried over (camera, phone model, dates, location…).</p>
  {:else if value.mode === 'stripAll'}
    <p class="hint">No EXIF, XMP or GPS data is written. Photos are rotated upright so they still display correctly.</p>
  {:else}
    <Toggle checked={value.keepDateTime} label="Date & time taken" onchange={(v) => set({ keepDateTime: v })} />
    <Toggle checked={value.keepCamera} label="Camera / phone & lens" onchange={(v) => set({ keepCamera: v })} />
    <Toggle checked={value.keepExposure} label="Camera settings" hint="Exposure, ISO, aperture, focal length" onchange={(v) => set({ keepExposure: v })} />
    <Toggle checked={value.keepLocation} label="Location (GPS)" onchange={(v) => set({ keepLocation: v })} />
    <Toggle checked={value.keepAuthor} label="Author, copyright & description" onchange={(v) => set({ keepAuthor: v })} />
    <!-- Only honoured here: "Keep all" always keeps the profile, "Remove all" always drops it. -->
    <Toggle checked={value.keepIcc} label="Colour profile (ICC)" hint="Recommended — keeps colours accurate" onchange={(v) => set({ keepIcc: v })} />
  {/if}
  {#if value.mode !== 'stripAll'}
    <!-- "Remove all" never copies the file dates. -->
    <Toggle checked={value.keepFileTimes} label="Keep file modified date" onchange={(v) => set({ keepFileTimes: v })} />
  {/if}
</div>
