<script lang="ts">
  import { ChevronLeft, ChevronRight, Hand, Shuffle } from '@lucide/svelte';
  import { ENGINE_INFO, managedPhotos } from '../../../collage/layout';
  import { LETTER_FONTS } from '../../../collage/engines/artistic';
  import { ensureFont, fontInfo } from '../../../assets/fonts';
  import { editor } from '../../../editor/editor.svelte';
  import type { LayoutEngine, LayoutShape } from '../../../model/types';
  import ColorInput from '../../ui/ColorInput.svelte';
  import Section from '../../ui/Section.svelte';
  import Segmented from '../../ui/Segmented.svelte';
  import Slider from '../../ui/Slider.svelte';
  import Toggle from '../../ui/Toggle.svelte';

  const L = $derived(editor.doc.layout);
  const photos = $derived(managedPhotos(editor.doc));
  const first = $derived(photos[0]);
  const S = $derived(editor.pageShort);
  const bgSolid = $derived(editor.doc.background.fill.kind === 'solid' ? editor.doc.background.fill.color : '#ffffff');

  // Tiny schematic previews for each engine (viewBox 0 0 40 30).
  const ICONS: Record<string, string> = {
    mosaic: 'M2 2h20v14H2zM24 2h14v8H24zM24 12h14v16H24zM2 18h9v10H2zM13 18h9v10h-9z',
    columns: 'M2 2h11v10H2zM15 2h23v12H15zM2 14h11v14H2zM15 16h11v12H15zM28 16h10v12H28z',
    rows: 'M2 2h14v12H2zM18 2h20v12H18zM2 16h9v12H2zM13 16h12v12H13zM27 16h11v12H27z',
    grid: 'M2 2h11v12H2zM15 2h10v12H15zM27 2h11v12H27zM2 16h11v12H2zM15 16h10v12H15zM27 16h11v12H27z',
    featured: 'M2 2h22v26H2zM26 2h12v8H26zM26 12h12v7H26zM26 21h12v7H26z',
    shape: 'M20 27 9 17c-5-5-2-13 5-13 3 0 5 2 6 4 1-2 3-4 6-4 7 0 10 8 5 13z',
    scatter: 'M4 6l12-3 2 11-12 3zM20 9l13 2-2 12-13-2zM8 18l11 1-1 10-11-1z',
    ring: 'M20 9a6 6 0 1 1 0 12 6 6 0 0 1 0-12zM20 1a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zM20 22a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zM8 11a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zM32 11a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7z',
    letters: 'M2 28L10 2h6l8 26h-6l-1.6-6H9.6L8 28zM11 17h4l-2-7zM26 2h6v20h6v6H26z',
    spiral: 'M2 2h22v26H2zM26 2h12v16H26zM26 20h7v8h-7zM35 20h3v4h-3zM35 26h3v2h-3z',
  };

  const letterFonts = LETTER_FONTS.map((id) => fontInfo(id));
  let textDraft = $state('');
  $effect.pre(() => {
    textDraft = L.text ?? 'LOVE';
  });
  let textTimer: ReturnType<typeof setTimeout> | null = null;
  function setText(v: string) {
    textDraft = v;
    if (textTimer) clearTimeout(textTimer);
    // Applies to the page being edited now, not whichever is active when the timer fires.
    const page = editor.activePage;
    const doc = editor.doc.id;
    textTimer = setTimeout(() => {
      if (editor.activePage === page && editor.doc.id === doc) editor.setLayout({ text: v }, 'layout-text');
    }, 350);
  }
  function setFont(id: string) {
    const weight = fontInfo(id).weights.includes(800) ? 800 : Math.max(...fontInfo(id).weights);
    const page = editor.activePage;
    const doc = editor.doc.id;
    void ensureFont(id, weight).then(() => {
      if (editor.activePage === page && editor.doc.id === doc) editor.setLayout({ font: id, fontWeight: weight });
    });
  }

  const shapes: { value: LayoutShape; label: string }[] = [
    { value: 'heart', label: '♥' },
    { value: 'circle', label: '●' },
    { value: 'star', label: '★' },
    { value: 'hexagon', label: '⬢' },
    { value: 'diamond', label: '◆' },
  ];

  function choose(id: LayoutEngine) {
    if (L.engine === id) return;
    editor.setEngine(id);
  }

  function setBgColor(c: string) {
    editor.update((d) => {
      d.background.fill = { kind: 'solid', color: c };
    }, { key: 'layout-bg' });
  }
</script>

<div class="head">
  <h3>Layouts</h3>
  <p class="hint">Automatic arrangements for any number of photos. Hit <b>Regenerate</b> until you love it.</p>
</div>

<div class="engines">
  {#each ENGINE_INFO as e (e.id)}
    <button class="engine" class:on={L.engine === e.id} title={e.description} onclick={() => choose(e.id)}>
      <svg viewBox="0 0 40 30" aria-hidden="true"><path d={ICONS[e.id]} /></svg>
      <span>{e.name}</span>
    </button>
  {/each}
  <button class="engine" class:on={L.engine === 'none'} title="Place and rotate photos freely by hand" onclick={() => choose('none')}>
    <Hand size={22} />
    <span>Freestyle</span>
  </button>
</div>

{#if L.engine !== 'none'}
  <div class="regen">
    <button class="icon-btn" title="Previous layout" disabled={editor.layoutHistoryPos <= 0} onclick={() => editor.layoutBack()}><ChevronLeft size={18} /></button>
    <button class="btn primary grow" onclick={() => editor.regenerate()} disabled={!photos.length} title="New random arrangement (Ctrl+R)"><Shuffle size={16} /> Regenerate</button>
    <button class="icon-btn" title="Next layout" disabled={editor.layoutHistoryPos >= editor.layoutHistoryLen - 1} onclick={() => editor.layoutForward()}><ChevronRight size={18} /></button>
  </div>
  {#if !photos.length}
    <p class="hint pad">Add photos from the <b>Photos</b> tab to fill the layout.</p>
  {:else}
    <p class="hint pad">Drag a photo inside its frame to re-centre it, or onto another photo to swap them.</p>
  {/if}

  {#if L.engine === 'letters'}
    <Section title="Letters">
      <input class="input" value={textDraft} placeholder="A word, a name, a number…" maxlength="24" oninput={(e) => setText((e.target as HTMLInputElement).value)} />
      <p class="hint">Short and bold works best: a name, LOVE, MOM, 2027, a birthday number. Use several lines for longer words.</p>
      <select class="select" value={L.font ?? 'montserrat'} onchange={(e) => setFont((e.target as HTMLSelectElement).value)}>
        {#each letterFonts as f (f.id)}<option value={f.id} style:font-family="'{f.family}'">{f.family}</option>{/each}
      </select>
    </Section>
  {/if}

  {#if L.engine === 'shape'}
    <Section title="Shape">
      <Segmented options={shapes} value={L.shape ?? 'heart'} onchange={(v) => editor.setLayout({ shape: v })} />
      <Toggle checked={L.repeat ?? true} label="Repeat photos to fill the shape" onchange={(v) => editor.setLayout({ repeat: v })} />
    </Section>
  {/if}

  <Section title="Spacing">
    <Slider label="Gap between photos" value={L.spacing} min={0} max={0.06} step={0.001} scale={100} decimals={1} unit="%" onchange={(v) => editor.setLayout({ spacing: v }, 'layout-spacing')} />
    <Slider label="Outer margin" value={L.margin} min={0} max={0.15} step={0.001} scale={100} decimals={1} unit="%" onchange={(v) => editor.setLayout({ margin: v }, 'layout-margin')} />
    {#if L.engine === 'columns' || L.engine === 'grid'}
      <Slider label="Columns (0 = automatic)" value={L.columns ?? 0} min={0} max={12} step={1} onchange={(v) => editor.setLayout({ columns: v }, 'layout-cols')} />
    {/if}
    <ColorInput label="Background / border colour" value={bgSolid} onchange={setBgColor} />
  </Section>

  {#if first && L.engine !== 'scatter'}
    <Section title="Photo style">
      {#if L.engine !== 'ring'}
        <Slider
          label="Rounded corners"
          value={first.mask.shape === 'rect' ? first.mask.radius : 0}
          min={0}
          max={0.5}
          step={0.005}
          scale={100}
          unit="%"
          onchange={(v) => editor.styleLayoutPhotos((p) => {
            p.mask = { shape: 'rect', radius: v };
          }, 'layout-radius')}
        />
      {/if}
      <Slider
        label="Photo border"
        value={first.border ? first.border.width / S : 0}
        min={0}
        max={0.03}
        step={0.0005}
        scale={100}
        decimals={1}
        unit="%"
        onchange={(v) => editor.styleLayoutPhotos((p) => {
          p.border = v > 0 ? { width: v * S, color: p.border?.color ?? '#ffffff' } : null;
        }, 'layout-border')}
      />
      {#if first.border}
        <ColorInput label="Border colour" value={first.border.color} onchange={(c) => editor.styleLayoutPhotos((p) => {
          if (p.border) p.border.color = c;
        }, 'layout-border-color')} />
      {/if}
      <Toggle
        checked={!!first.shadow}
        label="Drop shadow"
        onchange={(v) => editor.styleLayoutPhotos((p) => {
          p.shadow = v ? { color: 'rgba(20,15,30,0.35)', blur: 0.012, x: 0, y: 0.004 } : null;
        })}
      />
    </Section>
  {/if}

  <div class="pad">
    <button class="btn block" onclick={() => editor.unlockLayout()} title="Keep the current arrangement but move every photo by hand">
      <Hand size={15} /> Unlock for freestyle editing
    </button>
  </div>
{:else}
  <p class="hint pad">
    Freestyle: drag, resize and rotate photos by hand. Use the handles around a selected item; hold <span class="kbd">Shift</span> to snap
    rotation, <span class="kbd">Alt</span> while dragging a photo onto another to swap them.
  </p>
{/if}

<style>
  .head {
    padding: 16px 14px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    font-size: 15px;
  }
  .pad {
    padding: 4px 14px 12px;
    margin: 0;
  }
  .engines {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
    padding: 6px 14px 12px;
  }
  .engine {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 10px 4px 8px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--panel-2);
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .engine svg {
    width: 40px;
    height: 30px;
    fill: currentColor;
    opacity: 0.85;
  }
  .engine:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }
  .engine.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .regen {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 0 14px 8px;
  }
</style>
