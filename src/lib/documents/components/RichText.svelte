<script lang="ts">
  import { Editor } from '@tiptap/core';
  import { onDestroy } from 'svelte';
  import { extensions, toHTML } from '../lib/tiptap';
  import { viewport } from '../lib/viewport';
  import Icon from './Icon.svelte';

  let {
    content,
    onchange,
    onfocus,
    placeholder = 'Write something…',
    gps,
  }: {
    content: any;
    onchange: (json: any) => void;
    onfocus?: () => void;
    placeholder?: string;
    /** GPS of the section's media, offered when inserting a map */
    gps?: () => { lat: number; lon: number; label: string } | null;
  } = $props();

  let near = $state(false);
  let host = $state<HTMLDivElement>();
  let editor = $state<Editor | null>(null);
  let focused = $state(false);
  let tick = $state(0);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pendingFocus: 'end' | null = null;
  let changed = false; // only real edits are reported (mount/unmount/blur must not dirty the doc)

  function emit() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (editor && changed) {
      changed = false;
      onchange(editor.getJSON());
    }
  }

  function create() {
    if (!host || editor) return;
    const ed = new Editor({
      element: host,
      extensions: extensions(placeholder),
      content: $state.snapshot(content) ?? '',
      editorProps: { attributes: { class: 'prose', spellcheck: 'true' } },
      onUpdate: () => {
        changed = true;
        if (timer) clearTimeout(timer);
        timer = setTimeout(emit, 300);
      },
      onTransaction: () => tick++,
      onFocus: () => {
        focused = true;
        onfocus?.();
      },
      onBlur: () => {
        focused = false;
        emit();
      },
    });
    editor = ed;
    if (pendingFocus) {
      ed.commands.focus(pendingFocus);
      pendingFocus = null;
    }
  }

  function destroy() {
    if (!editor) return;
    emit();
    editor.destroy();
    editor = null;
  }

  $effect(() => {
    if (near && host && !editor) create();
    else if (!near && editor && !focused) destroy();
  });

  onDestroy(destroy);

  function activateStatic() {
    pendingFocus = 'end';
    near = true;
  }

  const is = (name: string, attrs?: Record<string, unknown>) => {
    void tick;
    return !!editor?.isActive(name, attrs);
  };
  const isAlign = (a: string) => {
    void tick;
    return !!editor?.isActive({ textAlign: a });
  };

  function setLink() {
    if (!editor) return;
    const prev = editor.getAttributes('link').href as string | undefined;
    const url = prompt('Link URL (empty to remove)', prev ?? 'https://');
    if (url === null) return;
    if (!url.trim()) editor.chain().focus().extendMarkRange('link').unsetLink().run();
    else editor.chain().focus().extendMarkRange('link').setLink({ href: url.trim() }).run();
  }

  const c = () => editor!.chain().focus();

  function addYoutube() {
    const url = prompt('YouTube link');
    if (!url?.trim()) return;
    if (!c().setYoutubeVideo({ src: url.trim() }).run()) alert('That does not look like a YouTube link.');
  }

  function addMap() {
    const g = gps?.();
    const def = g ? `${g.lat.toFixed(5)}, ${g.lon.toFixed(5)}` : '';
    const v = prompt(g ? `Map at the photo's location (${g.label || 'from GPS'}) — or type "lat, lon"` : 'Coordinates for the map ("lat, lon", e.g. 64.1466, -21.9426)', def);
    if (!v) return;
    const m = v.match(/(-?\d+(?:\.\d+)?)\s*[,; ]\s*(-?\d+(?:\.\d+)?)/);
    if (!m) return alert('Use "latitude, longitude", e.g. 64.1466, -21.9426');
    (c() as any).setMap({ lat: Number(m[1]), lon: Number(m[2]), zoom: 13, label: g && v === def ? g.label : '' }).run();
  }
</script>

<div class="rt" class:focused use:viewport={{ onNear: (v) => (near = v) }}>
  {#if editor && focused}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="toolbar" onmousedown={(e) => e.preventDefault()}>
      <button class="icon-btn" class:on={is('heading', { level: 1 })} title="Heading 1" onclick={() => c().toggleHeading({ level: 1 }).run()}><b>H1</b></button>
      <button class="icon-btn" class:on={is('heading', { level: 2 })} title="Heading 2" onclick={() => c().toggleHeading({ level: 2 }).run()}><b>H2</b></button>
      <button class="icon-btn" class:on={is('heading', { level: 3 })} title="Heading 3" onclick={() => c().toggleHeading({ level: 3 }).run()}><b>H3</b></button>
      <span class="sep"></span>
      <button class="icon-btn" class:on={is('bold')} title="Bold (Ctrl+B)" onclick={() => c().toggleBold().run()}><Icon name="bold" /></button>
      <button class="icon-btn" class:on={is('italic')} title="Italic (Ctrl+I)" onclick={() => c().toggleItalic().run()}><Icon name="italic" /></button>
      <button class="icon-btn" class:on={is('underline')} title="Underline (Ctrl+U)" onclick={() => c().toggleUnderline().run()}><Icon name="underline" /></button>
      <button class="icon-btn" class:on={is('strike')} title="Strikethrough" onclick={() => c().toggleStrike().run()}><Icon name="strike" /></button>
      <button class="icon-btn" class:on={is('highlight')} title="Highlight" onclick={() => c().toggleHighlight().run()}><Icon name="highlight" /></button>
      <button class="icon-btn" class:on={is('code')} title="Inline code" onclick={() => c().toggleCode().run()}><Icon name="code" /></button>
      <button class="icon-btn" class:on={is('link')} title="Link" onclick={setLink}><Icon name="link" /></button>
      <span class="sep"></span>
      <button class="icon-btn" class:on={is('bulletList')} title="Bullet list" onclick={() => c().toggleBulletList().run()}><Icon name="list" /></button>
      <button class="icon-btn" class:on={is('orderedList')} title="Numbered list" onclick={() => c().toggleOrderedList().run()}><Icon name="list-ol" /></button>
      <button class="icon-btn" class:on={is('blockquote')} title="Quote" onclick={() => c().toggleBlockquote().run()}><Icon name="quote" /></button>
      <button class="icon-btn" class:on={is('codeBlock')} title="Code block (highlighted; type ``` + language)" onclick={() => c().toggleCodeBlock().run()}><Icon name="code2" /></button>
      <span class="sep"></span>
      <button class="icon-btn" title="Embed a YouTube video" onclick={addYoutube}><Icon name="youtube" /></button>
      <button class="icon-btn" title="Insert a map (uses the photo's GPS location when there is one)" onclick={addMap}><Icon name="map" /></button>
      <span class="sep"></span>
      <button class="icon-btn" class:on={isAlign('left')} title="Align left" onclick={() => c().setTextAlign('left').run()}><Icon name="align-left" /></button>
      <button class="icon-btn" class:on={isAlign('center')} title="Center" onclick={() => c().setTextAlign('center').run()}><Icon name="align-center" /></button>
      <button class="icon-btn" class:on={isAlign('right')} title="Align right" onclick={() => c().setTextAlign('right').run()}><Icon name="align-right" /></button>
      <span class="sep"></span>
      <button class="icon-btn" title="Undo (Ctrl+Z)" onclick={() => c().undo().run()}><Icon name="undo" /></button>
      <button class="icon-btn" title="Redo (Ctrl+Shift+Z)" onclick={() => c().redo().run()}><Icon name="redo" /></button>
    </div>
  {/if}
  {#if !editor}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="prose static" onclick={activateStatic}>
      {@html toHTML(content)}
    </div>
  {/if}
  <div class="host" bind:this={host}></div>
</div>

<style>
  .rt {
    position: relative;
  }
  .toolbar {
    position: sticky;
    top: 6px;
    z-index: 4;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1px;
    padding: 3px;
    margin: 0 0 8px;
    border-radius: 9px;
    background: var(--surface-3);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    width: max-content;
    max-width: 100%;
  }
  .toolbar .icon-btn {
    width: 28px;
    height: 28px;
    font-size: 12px;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--line-strong);
    margin: 0 3px;
  }
  .static {
    cursor: text;
    min-height: 1.6em;
  }
  .static:empty::before {
    content: 'Write something…';
    color: var(--text-3);
  }
</style>
