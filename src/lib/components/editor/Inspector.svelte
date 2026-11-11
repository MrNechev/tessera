<script lang="ts">
  import {
    AlignCenterHorizontal,
    AlignCenterVertical,
    AlignEndHorizontal,
    AlignEndVertical,
    AlignStartHorizontal,
    AlignStartVertical,
    Bold,
    CaseUpper,
    Eye,
    EyeOff,
    FlipHorizontal,
    FlipVertical,
    Italic,
    Lock,
    LockOpen,
    TextAlignCenter,
    TextAlignEnd,
    TextAlignStart,
  } from '@lucide/svelte';
  import { FONTS, FONT_CATEGORIES, ensureFont, fontInfo } from '../../assets/fonts';
  import { editor } from '../../editor/editor.svelte';
  import { neutralAdjust } from '../../model/factory';
  import type { CalendarElement, CollageElement, PhotoAdjust, PhotoElement, PhotoFrameStyle, ShapeElement, TextElement } from '../../model/types';
  import { MONTH_NAMES } from '../../render/calendar';
  import { FILTER_PRESETS, presetAdjust } from '../../render/adjust';
  import { MASK_SHAPES } from '../../render/shapes';
  import { stickers } from '../../stores/stickers.svelte';
  import ColorInput from '../ui/ColorInput.svelte';
  import Section from '../ui/Section.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import Layers from './Layers.svelte';

  let tab = $state<'props' | 'layers'>('props');
  const el = $derived(editor.single);
  const S = $derived(editor.pageShort);

  function patch(p: Partial<CollageElement>, key?: string) {
    if (el) editor.patch(el.id, p, { key: key ? `${key}:${el.id}` : undefined });
  }

  const DEFAULT_SHADOW = { color: 'rgba(20,15,30,0.35)', blur: 0.012, x: 0, y: 0.005 };

  const FRAMES: { id: PhotoFrameStyle; name: string }[] = [
    { id: 'none', name: 'None' },
    { id: 'polaroid', name: 'Polaroid' },
    { id: 'instant', name: 'Instant' },
    { id: 'mat', name: 'Mat' },
    { id: 'film', name: 'Film' },
    { id: 'tape', name: 'Tape' },
  ];

  function setFrame(p: PhotoElement, style: PhotoFrameStyle) {
    const color = style === 'film' ? '#141218' : style === 'tape' ? '#f7a8c4' : p.frame?.color && p.frame.style !== 'film' && p.frame.style !== 'tape' ? p.frame.color : '#ffffff';
    patch({ frame: style === 'none' ? null : { ...(p.frame ?? {}), style, color } } as Partial<PhotoElement>);
  }

  function setAdjust(p: PhotoElement, key: keyof PhotoAdjust, v: number) {
    patch({ adjust: { ...(p.adjust ?? neutralAdjust()), [key]: v }, filter: null } as Partial<PhotoElement>, `adj-${key}`);
  }

  const fontsByCat = FONT_CATEGORIES.map((c) => ({ ...c, fonts: FONTS.filter((f) => f.category === c.id) }));
</script>

<div class="tabs">
  <button class:on={tab === 'props'} onclick={() => (tab = 'props')}>Properties</button>
  <button class:on={tab === 'layers'} onclick={() => (tab = 'layers')}>Layers</button>
</div>

{#if tab === 'layers'}
  <Layers />
{:else if editor.selection.length > 1}
  <Section title="{editor.selection.length} items selected">
    <div class="align">
      <button class="icon-btn" title="Align left" onclick={() => editor.align('left')}><AlignStartVertical size={16} /></button>
      <button class="icon-btn" title="Align centre" onclick={() => editor.align('hcenter')}><AlignCenterVertical size={16} /></button>
      <button class="icon-btn" title="Align right" onclick={() => editor.align('right')}><AlignEndVertical size={16} /></button>
      <button class="icon-btn" title="Align top" onclick={() => editor.align('top')}><AlignStartHorizontal size={16} /></button>
      <button class="icon-btn" title="Align middle" onclick={() => editor.align('vcenter')}><AlignCenterHorizontal size={16} /></button>
      <button class="icon-btn" title="Align bottom" onclick={() => editor.align('bottom')}><AlignEndHorizontal size={16} /></button>
    </div>
    <Slider label="Opacity" value={editor.selected[0]?.opacity ?? 1} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => editor.patchSelected({ opacity: v }, { key: 'multi-opacity' })} />
    <div class="row">
      <button class="btn grow" onclick={() => editor.duplicateSelected()}>Duplicate</button>
      <button class="btn grow danger" onclick={() => editor.deleteSelected()}>Delete</button>
    </div>
  </Section>
{:else if !el}
  <div class="empty">
    <h4>{editor.doc.name}</h4>
    <p class="hint">Select something on the page to edit it. Tips:</p>
    <ul class="hint">
      <li>Drag a laid-out photo inside its frame to re-centre it; drop it on another photo to swap.</li>
      <li>Double-click a photo to reposition & zoom; double-click text to edit it.</li>
      <li><span class="kbd">Ctrl+S</span> saves a copy to your save folder, <span class="kbd">Ctrl+⇧+S</span> lets you choose.</li>
      <li><span class="kbd">Space</span>+drag or the mouse wheel pans, <span class="kbd">Ctrl</span>+wheel zooms.</li>
    </ul>
  </div>
{:else}
  {#if el.kind === 'photo'}
    {@const p = el}
    <Section title="Photo">
      {#if p.asset}
        <Slider label="Zoom" value={p.crop.zoom} min={1} max={5} step={0.01} scale={100} unit="%" onchange={(v) => patch({ crop: { ...p.crop, zoom: v } } as Partial<PhotoElement>, 'zoom')} />
        <div class="row">
          <button class="btn small grow" onclick={() => patch({ crop: { x: 0.5, y: 0.5, zoom: 1 } } as Partial<PhotoElement>)}>Reset crop</button>
          <button class="btn small grow" onclick={() => (editor.cropTarget = editor.cropTarget === p.id ? null : p.id)}>{editor.cropTarget === p.id ? 'Done' : 'Reposition'}</button>
          <button class="icon-btn" title="Flip horizontally" onclick={() => patch({ flipX: !p.flipX })}><FlipHorizontal size={16} /></button>
          <button class="icon-btn" title="Flip vertically" onclick={() => patch({ flipY: !p.flipY })}><FlipVertical size={16} /></button>
        </div>
        <p class="hint">{editor.doc.assets[p.asset]?.name ?? ''}</p>
      {:else}
        <p class="hint">Empty frame — drag a photo from the Photos tab onto it, or double-click it.</p>
      {/if}
      {#if p.slot}
        <button class="btn small" onclick={() => editor.detach(p.id)}>Detach from layout</button>
      {/if}
    </Section>

    <Section title="Shape">
      <div class="masks">
        {#each MASK_SHAPES as m (m.id)}
          <button class="btn small" class:active={p.mask.shape === m.id} onclick={() => patch({ mask: { shape: m.id, radius: p.mask.radius } } as Partial<PhotoElement>)}>{m.name}</button>
        {/each}
      </div>
      {#if p.mask.shape === 'rect'}
        <Slider label="Rounded corners" value={p.mask.radius} min={0} max={0.5} step={0.005} scale={100} unit="%" onchange={(v) => patch({ mask: { shape: 'rect', radius: v } } as Partial<PhotoElement>, 'radius')} />
      {/if}
    </Section>

    <Section title="Frame & border">
      <div class="masks">
        {#each FRAMES as f (f.id)}
          <button class="btn small" class:active={(p.frame?.style ?? 'none') === f.id} onclick={() => setFrame(p, f.id)}>{f.name}</button>
        {/each}
      </div>
      {#if p.frame}
        <ColorInput label={p.frame.style === 'tape' ? 'Tape colour' : 'Frame colour'} value={p.frame.color} onchange={(c) => patch({ frame: { ...p.frame!, color: c } } as Partial<PhotoElement>, 'frame-color')} />
        {#if p.frame.style === 'polaroid' || p.frame.style === 'instant'}
          <input class="input" placeholder="Caption (e.g. us ♥)" value={p.frame.caption ?? ''} oninput={(e) => patch({ frame: { ...p.frame!, caption: (e.target as HTMLInputElement).value } } as Partial<PhotoElement>, 'caption')} />
          <select class="select" value={p.frame.captionFont ?? 'caveat'} onchange={(e) => patch({ frame: { ...p.frame!, captionFont: (e.target as HTMLSelectElement).value } } as Partial<PhotoElement>)}>
            {#each FONTS.filter((f) => ['handwriting', 'script'].includes(f.category)) as f (f.id)}<option value={f.id}>{f.family}</option>{/each}
          </select>
        {/if}
      {/if}
      <Slider label="Border" value={p.border ? p.border.width / S : 0} min={0} max={0.03} step={0.0005} scale={100} decimals={1} unit="%" onchange={(v) => patch({ border: v > 0 ? { width: v * S, color: p.border?.color ?? '#ffffff' } : null } as Partial<PhotoElement>, 'border')} />
      {#if p.border}
        <ColorInput label="Border colour" value={p.border.color} onchange={(c) => patch({ border: { ...p.border!, color: c } } as Partial<PhotoElement>, 'border-color')} />
      {/if}
    </Section>

    <Section title="Filters" open={false}>
      <div class="filters">
        {#each FILTER_PRESETS as f (f.id)}
          <button class="btn small" class:active={(p.filter ?? 'none') === f.id} onclick={() => patch({ filter: f.id, adjust: f.id === 'none' ? null : presetAdjust(f.id) } as Partial<PhotoElement>)}>{f.name}</button>
        {/each}
      </div>
      {#each [['brightness', 'Brightness', -1, 1], ['contrast', 'Contrast', -1, 1], ['saturation', 'Saturation', -1, 1], ['warmth', 'Warmth', -1, 1], ['fade', 'Fade', 0, 1], ['vignette', 'Vignette', 0, 1], ['grayscale', 'Black & white', 0, 1], ['sepia', 'Sepia', 0, 1]] as const as [k, label, min, max] (k)}
        <Slider {label} value={p.adjust?.[k] ?? 0} {min} {max} step={0.01} scale={100} unit="%" onchange={(v) => setAdjust(p, k, v)} />
      {/each}
    </Section>
  {:else if el.kind === 'text'}
    {@const t = el}
    <Section title="Text">
      <textarea class="input" rows="3" value={t.text} oninput={(e) => patch({ text: (e.target as HTMLTextAreaElement).value } as Partial<TextElement>, 'text')}></textarea>
      <select
        class="select font-select"
        value={t.font}
        style:font-family="'{fontInfo(t.font).family}'"
        onchange={(e) => {
          const id = (e.target as HTMLSelectElement).value;
          void ensureFont(id, t.weight).then(() => patch({ font: id } as Partial<TextElement>));
        }}
      >
        {#each fontsByCat as c (c.id)}
          <optgroup label={c.name}>
            {#each c.fonts as f (f.id)}<option value={f.id} style:font-family="'{f.family}'">{f.family}</option>{/each}
          </optgroup>
        {/each}
      </select>
      <Slider label="Size" value={t.size / S} min={0.01} max={0.4} step={0.001} scale={100} decimals={1} unit="%" onchange={(v) => patch({ size: v * S } as Partial<TextElement>, 'size')} />
      <div class="row">
        <Segmented
          full={false}
          options={[
            { value: 'left', icon: TextAlignStart, title: 'Align left' },
            { value: 'center', icon: TextAlignCenter, title: 'Centre' },
            { value: 'right', icon: TextAlignEnd, title: 'Align right' },
          ]}
          value={t.align}
          onchange={(v) => patch({ align: v } as Partial<TextElement>)}
        />
        <span class="spacer"></span>
        <button class="icon-btn" class:active={t.weight >= 600} title="Bold" onclick={() => {
          const w = t.weight >= 600 ? 400 : 700;
          void ensureFont(t.font, w).then(() => patch({ weight: w } as Partial<TextElement>));
        }}><Bold size={16} /></button>
        <button class="icon-btn" class:active={t.italic} title="Italic (slanted)" onclick={() => patch({ italic: !t.italic } as Partial<TextElement>)}><Italic size={16} /></button>
        <button class="icon-btn" class:active={!!t.uppercase} title="Uppercase" onclick={() => patch({ uppercase: !t.uppercase } as Partial<TextElement>)}><CaseUpper size={16} /></button>
      </div>
      <ColorInput label="Colour" value={t.fill} onchange={(c) => patch({ fill: c } as Partial<TextElement>, 'fill')} />
      <Slider label="Letter spacing" value={t.letterSpacing} min={-0.1} max={0.8} step={0.005} scale={100} unit="%" onchange={(v) => patch({ letterSpacing: v } as Partial<TextElement>, 'ls')} />
      <Slider label="Line height" value={t.lineHeight} min={0.7} max={2.5} step={0.01} scale={100} unit="%" onchange={(v) => patch({ lineHeight: v } as Partial<TextElement>, 'lh')} />
      <Slider label="Curve" value={t.curve} min={-1} max={1} step={0.01} scale={100} unit="%" onchange={(v) => patch({ curve: Math.abs(v) < 0.02 ? 0 : v } as Partial<TextElement>, 'curve')} />
    </Section>
    <Section title="Effects" open={!!(t.stroke || t.background)}>
      <Toggle checked={!!t.stroke} label="Outline" onchange={(v) => patch({ stroke: v ? { width: t.size * 0.06, color: '#ffffff' } : null } as Partial<TextElement>)} />
      {#if t.stroke}
        <Slider label="Outline width" value={t.stroke.width / t.size} min={0.01} max={0.25} step={0.005} scale={100} unit="%" onchange={(v) => patch({ stroke: { ...t.stroke!, width: v * t.size } } as Partial<TextElement>, 'stroke-w')} />
        <ColorInput label="Outline colour" value={t.stroke.color} onchange={(c) => patch({ stroke: { ...t.stroke!, color: c } } as Partial<TextElement>, 'stroke-c')} />
      {/if}
      <Toggle checked={!!t.background} label="Label background" onchange={(v) => patch({ background: v ? { color: '#ff5d8f', padding: 0.4, radius: 1 } : null } as Partial<TextElement>)} />
      {#if t.background}
        <ColorInput label="Label colour" value={t.background.color} alpha onchange={(c) => patch({ background: { ...t.background!, color: c } } as Partial<TextElement>, 'tbg-c')} />
        <Slider label="Padding" value={t.background.padding} min={0} max={1.5} step={0.01} scale={100} unit="%" onchange={(v) => patch({ background: { ...t.background!, padding: v } } as Partial<TextElement>, 'tbg-p')} />
        <Slider label="Roundness" value={t.background.radius} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => patch({ background: { ...t.background!, radius: v } } as Partial<TextElement>, 'tbg-r')} />
      {/if}
    </Section>
  {:else if el.kind === 'sticker'}
    <Section title="Sticker">
      <p class="hint">{stickers.info(el.sticker)?.name ?? el.sticker}</p>
      <div class="row">
        <button class="btn small grow" onclick={() => patch({ flipX: !el.flipX })}><FlipHorizontal size={14} /> Flip</button>
        <button class="btn small grow" onclick={() => patch({ flipY: !el.flipY })}><FlipVertical size={14} /> Flip vertical</button>
      </div>
    </Section>
  {:else if el.kind === 'shape'}
    {@const sh = el as ShapeElement}
    <Section title="Shape">
      {#if sh.shape !== 'ring'}
        <Toggle checked={!!sh.fill} label="Fill" onchange={(v) => patch({ fill: v ? { kind: 'solid', color: '#ff5d8f' } : null } as Partial<ShapeElement>)} />
        {#if sh.fill && sh.fill.kind === 'solid'}
          <ColorInput label="Fill colour" value={sh.fill.color} alpha onchange={(c) => patch({ fill: { kind: 'solid', color: c } } as Partial<ShapeElement>, 'shape-fill')} />
        {/if}
      {/if}
      <Slider label="Outline" value={sh.stroke ? sh.stroke.width / S : 0} min={0} max={0.03} step={0.0005} scale={100} decimals={1} unit="%" onchange={(v) => patch({ stroke: v > 0 ? { width: v * S, color: sh.stroke?.color ?? '#1d1b26' } : null } as Partial<ShapeElement>, 'shape-stroke')} />
      {#if sh.stroke}
        <ColorInput label="Outline colour" value={sh.stroke.color} onchange={(c) => patch({ stroke: { ...sh.stroke!, color: c } } as Partial<ShapeElement>, 'shape-stroke-c')} />
      {/if}
      {#if sh.shape === 'rect'}
        <Slider label="Rounded corners" value={sh.radius} min={0} max={0.5} step={0.005} scale={100} unit="%" onchange={(v) => patch({ radius: v } as Partial<ShapeElement>, 'shape-radius')} />
      {/if}
    </Section>
  {/if}

  {#if el.kind === 'calendar'}
    {@const cal = el}
    <Section title="Calendar">
      <div class="row">
        <select class="select grow" value={cal.month} onchange={(e) => patch({ month: +(e.target as HTMLSelectElement).value } as Partial<CalendarElement>)}>
          {#each MONTH_NAMES as n, i (n)}<option value={i + 1}>{n}</option>{/each}
        </select>
        <input class="input year" type="number" min="1900" max="2200" value={cal.year} onchange={(e) => patch({ year: Math.max(1900, Math.min(2200, +(e.target as HTMLInputElement).value || cal.year)) } as Partial<CalendarElement>)} />
      </div>
      <div class="row">
        <span class="label">Week starts on</span>
        <span class="spacer"></span>
        <Segmented full={false} options={[{ value: 1, label: 'Mon' }, { value: 0, label: 'Sun' }]} value={cal.firstDay} onchange={(v) => patch({ firstDay: v } as Partial<CalendarElement>)} />
      </div>
      <Segmented
        options={[
          { value: 'clean', label: 'Clean' },
          { value: 'lines', label: 'Lines' },
          { value: 'boxes', label: 'Boxes' },
          { value: 'dots', label: 'Dots' },
        ]}
        value={cal.style}
        onchange={(v) => patch({ style: v } as Partial<CalendarElement>)}
      />
      <input
        class="input"
        placeholder="Mark days, e.g. 3, 14, 25"
        value={cal.marked.join(', ')}
        onchange={(e) => {
          const days = [...new Set((e.target as HTMLInputElement).value.split(/[\s,;]+/).map((v) => parseInt(v)).filter((d) => d >= 1 && d <= 31))].sort((a, b) => a - b);
          patch({ marked: days } as Partial<CalendarElement>);
        }}
      />
      <Toggle checked={cal.showTitle} label="Show month name" onchange={(v) => patch({ showTitle: v } as Partial<CalendarElement>)} />
      {#if cal.showTitle}
        <Toggle checked={cal.showYear} label="Show year" onchange={(v) => patch({ showYear: v } as Partial<CalendarElement>)} />
      {/if}
    </Section>
    <Section title="Fonts & colours">
      <select class="select" value={cal.titleFont} onchange={(e) => { const id = (e.target as HTMLSelectElement).value; void ensureFont(id, 700).then(() => patch({ titleFont: id } as Partial<CalendarElement>)); }}>
        {#each fontsByCat as c (c.id)}
          <optgroup label="{c.name} — title">
            {#each c.fonts as f (f.id)}<option value={f.id} style:font-family="'{f.family}'">{f.family}</option>{/each}
          </optgroup>
        {/each}
      </select>
      <select class="select" value={cal.font} onchange={(e) => { const id = (e.target as HTMLSelectElement).value; void ensureFont(id, 400).then(() => patch({ font: id } as Partial<CalendarElement>)); }}>
        {#each fontsByCat.filter((c) => ['sans', 'rounded', 'serif', 'display'].includes(c.id)) as c (c.id)}
          <optgroup label="{c.name} — days">
            {#each c.fonts as f (f.id)}<option value={f.id} style:font-family="'{f.family}'">{f.family}</option>{/each}
          </optgroup>
        {/each}
      </select>
      <ColorInput label="Text" value={cal.color} onchange={(c) => patch({ color: c } as Partial<CalendarElement>, 'cal-color')} />
      <ColorInput label="Weekends" value={cal.weekendColor} onchange={(c) => patch({ weekendColor: c } as Partial<CalendarElement>, 'cal-weekend')} />
      <ColorInput label="Marked days" value={cal.accent} onchange={(c) => patch({ accent: c } as Partial<CalendarElement>, 'cal-accent')} />
      <Toggle checked={!!cal.background} label="Card background" onchange={(v) => patch({ background: v ? '#ffffff' : null } as Partial<CalendarElement>)} />
      {#if cal.background}
        <ColorInput label="Card colour" value={cal.background} alpha onchange={(c) => patch({ background: c } as Partial<CalendarElement>, 'cal-bg')} />
      {/if}
    </Section>
  {/if}

  <Section title="Appearance">
    <Slider label="Opacity" value={el.opacity} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => patch({ opacity: v }, 'opacity')} />
    <Toggle checked={!!el.shadow} label="Shadow" onchange={(v) => patch({ shadow: v ? { ...DEFAULT_SHADOW } : null })} />
    {#if el.shadow}
      {@const sd = el.shadow}
      <Slider label="Softness" value={sd.blur} min={0} max={0.06} step={0.0005} scale={100} decimals={1} unit="%" onchange={(v) => patch({ shadow: { ...sd, blur: v } }, 'sh-blur')} />
      <Slider label="Distance" value={sd.y} min={-0.03} max={0.04} step={0.0005} scale={100} decimals={1} unit="%" onchange={(v) => patch({ shadow: { ...sd, y: v } }, 'sh-y')} />
      <ColorInput label="Shadow colour" value={sd.color} alpha onchange={(c) => patch({ shadow: { ...sd, color: c } }, 'sh-c')} />
    {/if}
  </Section>

  <Section title="Position" open={false}>
    <div class="pos">
      <label><span class="hint">X</span><input class="input" type="number" value={Math.round(el.x)} onchange={(e) => patch({ x: +(e.target as HTMLInputElement).value })} disabled={el.kind === 'photo' && el.slot} /></label>
      <label><span class="hint">Y</span><input class="input" type="number" value={Math.round(el.y)} onchange={(e) => patch({ y: +(e.target as HTMLInputElement).value })} disabled={el.kind === 'photo' && el.slot} /></label>
      <label><span class="hint">W</span><input class="input" type="number" value={Math.round(el.w)} onchange={(e) => patch({ w: Math.max(4, +(e.target as HTMLInputElement).value) })} disabled={el.kind === 'photo' && el.slot} /></label>
      <label><span class="hint">H</span><input class="input" type="number" value={Math.round(el.h)} onchange={(e) => patch({ h: Math.max(4, +(e.target as HTMLInputElement).value) })} disabled={el.kind === 'text' || (el.kind === 'photo' && el.slot)} /></label>
    </div>
    <Slider label="Rotation" value={el.rotation} min={-180} max={180} step={1} unit="°" onchange={(v) => patch({ rotation: v }, 'rot')} />
    <div class="row">
      <button class="btn small grow" onclick={() => patch({ locked: !el.locked })}>{#if el.locked}<LockOpen size={14} /> Unlock{:else}<Lock size={14} /> Lock{/if}</button>
      <button class="btn small grow" onclick={() => patch({ hidden: !el.hidden })}>{#if el.hidden}<Eye size={14} /> Show{:else}<EyeOff size={14} /> Hide{/if}</button>
    </div>
  </Section>
{/if}

<style>
  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 10px 0;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--panel);
    z-index: 2;
  }
  .tabs button {
    flex: 1;
    height: 34px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--text-3);
    font-weight: 600;
    cursor: pointer;
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .empty {
    padding: 18px 16px;
  }
  .empty h4 {
    font-size: 14px;
    margin-bottom: 6px;
  }
  .empty ul {
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    line-height: 1.5;
  }
  .align {
    display: flex;
    justify-content: space-between;
  }
  .masks,
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .font-select {
    font-size: 15px;
  }
  .pos {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .year {
    width: 84px;
  }
  .pos label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pos .input {
    width: 100%;
  }
  .danger {
    color: var(--danger);
  }
</style>
