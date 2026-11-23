<script lang="ts">
  import Sortable from 'sortablejs';
  import { onMount, tick } from 'svelte';
  import { DocState } from '../lib/docstate.svelte';
  import { dnd, hasMedia, readDrop } from '../lib/dnd.svelte';
  import { describe as describeExternal, hasFiles, matchFiles, matchText, type ExternalResult } from '../lib/external';
  import { palette, type Command } from '../lib/palette.svelte';
  import { isEmptyText, toHTML } from '../lib/tiptap';
  import { CLIP_PREFIX, parseClip, remapClip } from '../lib/canvas';
  import { newSection } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { FileItem, MediaRef } from '../lib/types';
  import Canvas from './Canvas.svelte';
  import DocSettings from './DocSettings.svelte';
  import FindReplace from './FindReplace.svelte';
  import { exportMarkdown, exportOffline, exportPptx } from '../lib/exporters';
  import HistoryDialog from './HistoryDialog.svelte';
  import Icon from './Icon.svelte';
  import LibraryPanel from './LibraryPanel.svelte';
  import Outline from './Outline.svelte';
  import SectionView from './SectionView.svelte';
  import ThemeToggle from './ThemeToggle.svelte';

  let { id }: { id: string } = $props();
  const ds = new DocState();
  const view = $derived(router.route.name === 'doc' ? router.route.view : 'doc');
  let canvas = $state<ReturnType<typeof Canvas>>();

  function pref(k: string, d: string) {
    try {
      return localStorage.getItem('letopis.ed.' + k) ?? d;
    } catch {
      return d;
    }
  }
  function setPref(k: string, v: string) {
    try {
      localStorage.setItem('letopis.ed.' + k, v);
    } catch {}
  }

  let showLibrary = $state(pref('library', '1') === '1');
  let showOutline = $state(pref('outline', '1') === '1');
  let showCanvasPanel = $state(pref('cpanel', '1') === '1');
  let libWidth = $state(Number(pref('libWidth', '380')));
  let relink = $state<{ sectionId: string; index: number } | null>(null);
  let history = $state(false);
  let sectionsEl = $state<HTMLDivElement>();
  let scrollEl = $state<HTMLDivElement>();
  let dropAt = $state<number | null>(null);

  // ---------- remembered position (per document, this browser) ----------
  const posKey = $derived('letopis.pos.' + id);
  let posTimer: ReturnType<typeof setTimeout> | undefined;
  function savePos() {
    clearTimeout(posTimer);
    posTimer = setTimeout(() => {
      const el = scrollEl;
      if (!el || view !== 'doc') return;
      const secs = el.querySelectorAll<HTMLElement>('[data-section-id]');
      let anchor: HTMLElement | null = null;
      for (const s of secs) {
        if (s.offsetTop + s.offsetHeight > el.scrollTop) {
          anchor = s;
          break;
        }
      }
      try {
        localStorage.setItem(posKey, JSON.stringify({ id: anchor?.dataset.sectionId, off: anchor ? el.scrollTop - anchor.offsetTop : 0, active: ds.activeSection }));
      } catch {}
    }, 300);
  }
  async function restorePos() {
    let p: { id?: string; off: number; active?: string } | null = null;
    try {
      p = JSON.parse(localStorage.getItem(posKey) || 'null');
    } catch {}
    if (!p) return;
    if (p.active && ds.doc?.sections.some((s) => s.id === p!.active)) ds.activeSection = p.active;
    if (view !== 'doc' || !p.id) return;
    await tick();
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const el = scrollEl?.querySelector<HTMLElement>(`[data-section-id="${p.id}"]`);
    if (el && scrollEl) scrollEl.scrollTop = el.offsetTop + p.off;
  }
  $effect(() => {
    void ds.activeSection;
    savePos();
  });

  // ---------- media from outside (files from the computer, pasted paths) ----------
  async function insertExternal(r: ExternalResult, sectionId: string | null) {
    const msg = describeExternal(r);
    if (msg) toasts.show(msg, r.refs.length ? 'info' : 'error', 7000);
    if (!r.refs.length || !ds.doc) return;
    resolver.prefetch(r.refs);
    if (view === 'canvas' && canvas && !sectionId) canvas.addMediaCard(r.refs);
    else if (sectionId) ds.addMedia(sectionId, r.refs);
    else {
      const at = ds.activeSection ? ds.sectionIndex(ds.activeSection) + 1 : null;
      ds.addMedia(null, r.refs, at);
    }
    toasts.show(`Added ${r.refs.length} file${r.refs.length === 1 ? '' : 's'} from the library`, 'ok');
  }

  function onExternalDragOver(e: DragEvent) {
    if (hasFiles(e) && !hasMedia(e)) {
      e.preventDefault();
      e.dataTransfer!.dropEffect = 'copy';
    }
  }

  async function onExternalDrop(e: DragEvent) {
    if (!hasFiles(e) || hasMedia(e) || !e.dataTransfer?.files.length) return;
    e.preventDefault();
    const target = (e.target as HTMLElement).closest('[data-section-id]') as HTMLElement | null;
    try {
      await insertExternal(await matchFiles(e.dataTransfer.files), view === 'doc' ? (target?.dataset.sectionId ?? null) : null);
    } catch (err) {
      toasts.error((err as Error).message);
    }
  }

  async function onPaste(e: ClipboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t?.closest?.('input, textarea, [contenteditable="true"], .ProseMirror')) return;
    const text = e.clipboardData?.getData('text/plain');
    if (!text) return;
    if (view === 'doc' && ds.doc) {
      const clip = parseClip(text);
      if (clip) {
        // cards and notes copied on a canvas (or sections copied here) become sections
        e.preventDefault();
        const p = remapClip(clip);
        const secs = [
          ...p.sections.map((s) => ({ ...s, canvas: undefined })),
          ...p.items
            .filter((i) => i.type === 'note' && i.text.trim())
            .map((i) => {
              const s = newSection('text-only');
              s.text = { type: 'doc', content: (i as { text: string }).text.split('\n').map((l) => (l ? { type: 'paragraph', content: [{ type: 'text', text: l }] } : { type: 'paragraph' })) };
              return s;
            }),
        ];
        let at = ds.activeSection ? ds.sectionIndex(ds.activeSection) + 1 : ds.doc.sections.length;
        for (const s of secs) ds.addSection(at++, s);
        resolver.prefetch(secs.flatMap((s) => s.media));
        if (secs.length) {
          toasts.show(`Pasted ${secs.length} section${secs.length === 1 ? '' : 's'}`, 'ok');
          scrollToSection(secs[0].id);
        }
        return;
      }
    }
    const r = await matchText(text).catch(() => null);
    if (!r) return;
    e.preventDefault();
    await insertExternal(r, view === 'doc' ? ds.activeSection : null);
  }

  // ---------- command palette ----------
  function label(sec: { title?: string; text?: unknown; media: { path: string }[] }, i: number) {
    if (sec.title) return sec.title;
    const t = isEmptyText(sec.text) ? '' : toHTML(sec.text).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    return t ? t.slice(0, 70) : sec.media[0]?.path.split('/').pop() || `Section ${i + 1}`;
  }
  const unregister = palette.register('editor', (): Command[] => {
    if (!ds.doc) return [];
    const cmds: Command[] = [
      { id: 'ed:view', group: 'This document', icon: view === 'doc' ? 'canvas' : 'notes', title: view === 'doc' ? 'Switch to canvas' : 'Switch to document', run: () => router.go(view === 'doc' ? `/doc/${id}/canvas` : `/doc/${id}`) },
      { id: 'ed:present', group: 'This document', icon: 'present', title: 'Present', shortcut: 'F5', run: () => present() },
      { id: 'ed:add', group: 'This document', icon: 'plus', title: 'Add section', run: () => { const s = ds.addSection(ds.activeSection ? ds.sectionIndex(ds.activeSection) + 1 : null); scrollToSection(s.id); } },
      { id: 'ed:find', group: 'This document', icon: 'replace', title: 'Find and replace', shortcut: 'Ctrl+H', run: () => { findOpen = true; } },
      { id: 'ed:history', group: 'This document', icon: 'history', title: 'Version history', run: () => { history = true; } },
      { id: 'ed:settings', group: 'This document', icon: 'settings', title: 'Document settings (tags, cover, music, slideshow)', run: () => { settingsOpen = true; } },
      { id: 'ed:library', group: 'This document', icon: 'panel-left', title: showLibrary ? 'Hide library' : 'Show library', run: () => { showLibrary = !showLibrary; setPref('library', showLibrary ? '1' : '0'); } },
    ];
    ds.doc.sections.forEach((sec, i) =>
      cmds.push({
        id: 'sec:' + sec.id,
        group: 'Sections',
        icon: sec.chapter ? 'chapter' : 'layout',
        title: `${i + 1}. ${label(sec, i)}`,
        run: () => (view === 'canvas' && canvas ? canvas.focusCard(sec.id) : scrollToSection(sec.id)),
      }),
    );
    (ds.doc.canvas?.items ?? []).forEach((it, i) => {
      if (it.type === 'frame')
        cmds.push({ id: 'fr:' + it.id, group: 'Frames', icon: 'frame', title: it.title || `Frame ${i + 1}`, run: async () => { if (view !== 'canvas') router.go(`/doc/${id}/canvas`); await tick(); canvas?.focusCard(it.id); } });
    });
    return cmds;
  });

  onMount(() => {
    ds.load(id).then(async () => {
      if (ds.doc) {
        document.title = `${ds.doc.title} · Tessera`;
        resolver.prefetch(ds.doc.sections.flatMap((s) => s.media));
        const jump = router.route.name === 'doc' ? router.route.section : undefined;
        if (jump && ds.doc.sections.some((s) => s.id === jump)) {
          if (view === 'canvas') {
            await tick();
            canvas?.focusCard(jump);
          } else scrollToSection(jump, false);
          history_replace();
        } else restorePos();
      }
    });
    const beforeUnload = (e: BeforeUnloadEvent) => {
      if (ds.pending) {
        ds.flush();
        e.preventDefault();
      }
    };
    const onHide = () => {
      if (document.visibilityState === 'hidden') ds.flush();
    };
    addEventListener('beforeunload', beforeUnload);
    document.addEventListener('visibilitychange', onHide);
    return () => {
      unregister();
      clearTimeout(posTimer);
      ds.flush();
      removeEventListener('beforeunload', beforeUnload);
      document.removeEventListener('visibilitychange', onHide);
      document.title = 'Tessera';
    };
  });

  // drag-reorder of sections in the document column (handle = grip in the section header)
  $effect(() => {
    const el = sectionsEl;
    if (!el) return;
    let next: Node | null = null;
    const s = Sortable.create(el, {
      handle: '.drag-handle',
      draggable: '.sec-wrap',
      animation: 160,
      ghostClass: 'sec-ghost',
      scroll: true,
      scrollSensitivity: 120,
      scrollSpeed: 18,
      onStart: (e) => (next = e.item.nextSibling),
      onEnd: (e) => {
        e.from.insertBefore(e.item, next);
        if (e.oldDraggableIndex !== undefined && e.newDraggableIndex !== undefined)
          ds.moveSection(e.oldDraggableIndex, e.newDraggableIndex);
      },
    });
    return () => s.destroy();
  });

  /** drop ?s=<section> from the address after jumping, so reloads keep the remembered spot */
  function history_replace() {
    window.history.replaceState(null, '', `#/doc/${id}${view === 'canvas' ? '/canvas' : ''}`);
  }

  let findOpen = $state(false);
  let settingsOpen = $state(false);
  let docMenu = $state(false);

  async function jumpTo(sectionId?: string, itemId?: string) {
    if (itemId) {
      if (view !== 'canvas') router.go(`/doc/${id}/canvas`);
      await tick();
      canvas?.focusCard(itemId);
    } else if (sectionId) {
      if (view === 'canvas') canvas?.focusCard(sectionId);
      else scrollToSection(sectionId);
    }
  }

  async function runExport(kind: string) {
    docMenu = false;
    if (!ds.doc) return;
    await ds.save();
    try {
      if (kind === 'md') await exportMarkdown(ds.doc);
      else if (kind === 'pptx') await exportPptx(ds.doc);
      else if (kind === 'print') window.open(`#/print/${id}`, '_blank');
      else if (kind === 'slides') window.open(`#/print/${id}?mode=slides`, '_blank');
      else if (kind === 'canvas') window.open(`#/print/${id}?mode=canvas`, '_blank');
      else if (kind === 'offline') {
        exportOffline(ds.doc, confirm('Include original videos at full quality?\n\nOK = originals (bigger), Cancel = lighter copies where available (smaller, faster).') ? 'original' : 'light');
        toasts.show('Preparing the package — videos that still need converting are converted first; the download starts when ready', 'info', 7000);
      }
    } catch (e) {
      toasts.error('Export failed: ' + (e as Error).message);
    }
  }

  function refsOf(items: { libraryId: string; path: string }[]): MediaRef[] {
    return items.map((x) => ({ libraryId: x.libraryId, path: x.path }));
  }

  async function scrollToSection(sid: string, smooth = true) {
    await tick();
    const el = scrollEl?.querySelector(`[data-section-id="${sid}"]`);
    el?.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto', block: 'start' });
    ds.activeSection = sid;
  }

  function onPick(items: FileItem[], newSection: boolean) {
    if (!ds.doc) return;
    if (relink) {
      const sec = ds.doc.sections.find((s) => s.id === relink!.sectionId);
      const m = sec?.media[relink.index];
      if (m) {
        m.libraryId = items[0].libraryId;
        m.path = items[0].path;
        ds.touch();
        toasts.show('Relinked', 'ok');
      }
      relink = null;
      return;
    }
    const refs = refsOf(items);
    const active = ds.activeSection ? ds.doc.sections.find((s) => s.id === ds.activeSection) : null;
    if (view === 'canvas' && canvas) {
      if (!newSection && active) ds.addMedia(active.id, refs);
      else canvas.addMediaCard(refs);
      return;
    }
    if (!newSection && active) {
      ds.addMedia(active.id, refs);
    } else {
      const at = active ? ds.sectionIndex(active.id) + 1 : null;
      const before = new Set(ds.doc.sections.map((s) => s.id));
      ds.addMedia(null, refs, at);
      const added = ds.doc.sections.find((s) => !before.has(s.id));
      if (added) scrollToSection(added.id);
    }
  }

  function onZoneOver(e: DragEvent, i: number) {
    if (!hasMedia(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'copy';
    dropAt = i;
  }

  function onZoneDrop(e: DragEvent, i: number) {
    const items = readDrop(e);
    dropAt = null;
    dnd.dragging = false;
    if (!items) return;
    e.preventDefault();
    ds.addMedia(null, refsOf(items), i);
  }

  async function present(fromSection = -1) {
    if (!ds.doc) return;
    if (view === 'canvas' && canvas && fromSection === -1) return canvas.present(false);
    await ds.save();
    let start = 0;
    if (fromSection >= 0) start = ds.doc.sections.slice(0, fromSection).filter((s) => s.slide).length;
    try {
      await document.documentElement.requestFullscreen();
    } catch {
      /* fullscreen not allowed: present in the window */
    }
    router.go(`/present/${ds.doc.id}?s=${start}`);
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      ds.save();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && (e.key === 'h' || e.key === 'H' || (e.key === 'f' && e.shiftKey) || (e.key === 'F' && e.shiftKey))) {
      e.preventDefault();
      findOpen = true;
      return;
    }
    if (e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key === 'Enter')) {
      e.preventDefault();
      if (view === 'canvas' && canvas) canvas.present(e.shiftKey);
      else present(e.shiftKey && ds.activeSection ? ds.sectionIndex(ds.activeSection) : -1);
      return;
    }
    if (view === 'canvas') {
      if (e.key === 'Escape' && relink) relink = null;
      return;
    }
    // copy the selected section (when no text is selected) — paste it on a canvas or here
    if ((e.ctrlKey || e.metaKey) && (e.key === 'c' || e.key === 'C') && ds.activeSection && ds.doc) {
      const t = e.target as HTMLElement;
      const sel = window.getSelection();
      if (!t.closest('input, textarea, [contenteditable="true"]') && (!sel || sel.isCollapsed)) {
        const sec = ds.doc.sections.find((s) => s.id === ds.activeSection);
        if (sec) {
          e.preventDefault();
          navigator.clipboard?.writeText(CLIP_PREFIX + JSON.stringify({ v: 1, items: [], sections: [$state.snapshot(sec)] })).then(() => toasts.show('Section copied — paste it here or on a canvas', 'ok'));
        }
        return;
      }
    }
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && ds.activeSection) {
      e.preventDefault();
      const i = ds.sectionIndex(ds.activeSection);
      const j = e.key === 'ArrowUp' ? i - 1 : i + 1;
      ds.moveSection(i, j);
      scrollToSection(ds.activeSection, false);
    }
    if (e.key === 'Escape' && relink) relink = null;
  }

  // resizable library panel
  function startResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = libWidth;
    const move = (ev: PointerEvent) => (libWidth = Math.max(260, Math.min(900, startW + ev.clientX - startX)));
    const up = () => {
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
      setPref('libWidth', String(libWidth));
    };
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
  }

  // sections inside a collapsed chapter are hidden (they stay in the DOM for drag & drop)
  const inCollapsedChapter = $derived.by(() => {
    const out: boolean[] = [];
    let collapsed = false;
    for (const s of ds.doc?.sections ?? []) {
      if (s.chapter) {
        collapsed = !!s.collapsed;
        out.push(false);
      } else out.push(collapsed);
    }
    return out;
  });

  const statusText = $derived(
    {
      loading: 'Loading…',
      saved: 'Saved',
      dirty: 'Unsaved changes',
      saving: 'Saving…',
      error: 'Save failed — retrying',
      conflict: 'Conflict',
    }[ds.status],
  );
</script>

<svelte:window onkeydown={onKey} onpaste={onPaste} />

<div class="editor" style="--lib-w:{showLibrary ? libWidth : 0}px">
  <header class="top">
    <button class="icon-btn" title="All documents" onclick={() => router.go('/')}><Icon name="home" /></button>
    <button class="icon-btn" class:on={showLibrary} title="Library" onclick={() => { showLibrary = !showLibrary; setPref('library', showLibrary ? '1' : '0'); }}><Icon name="panel-left" /></button>
    {#if ds.doc}
      <input class="doc-title" bind:value={ds.doc.title} oninput={() => ds.touch()} placeholder="Untitled" />
    {:else}
      <div class="doc-title"></div>
    {/if}
    <span class="status {ds.status}" title={ds.error || (ds.lastSavedAt ? 'Last saved ' + new Date(ds.lastSavedAt).toLocaleString() : '')}>
      <span class="dot"></span>{statusText}
    </span>
    <div class="grow"></div>
    <div class="viewseg" role="tablist" aria-label="View">
      <button role="tab" aria-selected={view === 'doc'} class:on={view === 'doc'} title="Document: sections top to bottom" onclick={() => router.go(`/doc/${id}`)}><Icon name="notes" size={15} /> Document</button>
      <button role="tab" aria-selected={view === 'canvas'} class:on={view === 'canvas'} title="Canvas: the same sections as cards on a whiteboard" onclick={() => router.go(`/doc/${id}/canvas`)}><Icon name="canvas" size={15} /> Canvas</button>
    </div>
    {#if ds.doc?.tags?.length}
      <button class="tagchips" title="Tags — edit in document settings" onclick={() => (settingsOpen = true)}>
        {#each ds.doc.tags.slice(0, 3) as t (t)}<span class="chip">{t}</span>{/each}{#if ds.doc.tags.length > 3}<span class="chip">+{ds.doc.tags.length - 3}</span>{/if}
      </button>
    {/if}
    <button class="btn ghost" title="Version history" onclick={() => (history = true)} disabled={!ds.doc}><Icon name="history" /> History</button>
    <div class="menuwrap">
      <button class="icon-btn" title="Document menu" onclick={() => (docMenu = !docMenu)} disabled={!ds.doc}><Icon name="more" /></button>
      {#if docMenu}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="menu-backdrop" onclick={() => (docMenu = false)}></div>
        <div class="dmenu">
          <button onclick={() => { docMenu = false; findOpen = true; }}><Icon name="replace" size={15} /> Find and replace <span class="kbd">Ctrl H</span></button>
          <button onclick={() => { docMenu = false; settingsOpen = true; }}><Icon name="settings" size={15} /> Document settings <span class="muted">tags, cover, music</span></button>
          <button onclick={() => { docMenu = false; palette.open = true; }}><Icon name="command" size={15} /> Command palette <span class="kbd">Ctrl K</span></button>
          <div class="dsep">Export</div>
          <button onclick={() => runExport('print')}><Icon name="file-pdf" size={15} /> PDF / print (document)</button>
          <button onclick={() => runExport('slides')}><Icon name="file-pdf" size={15} /> PDF (slides)</button>
          <button onclick={() => runExport('canvas')}><Icon name="file-pdf" size={15} /> PDF (canvas frames)</button>
          <button onclick={() => runExport('pptx')}><Icon name="present" size={15} /> PowerPoint (.pptx)</button>
          <button onclick={() => runExport('md')}><Icon name="file" size={15} /> Markdown (.md)</button>
          <a href="/api/documents/{id}/export" onclick={() => (docMenu = false)}><Icon name="download" size={15} /> JSON (for import / backup)</a>
          <button onclick={() => runExport('offline')}><Icon name="package" size={15} /> Offline presentation (.zip with media)</button>
        </div>
      {/if}
    </div>
    {#if view === 'canvas'}
      <button class="icon-btn" class:on={showCanvasPanel} title="Frames and cards" onclick={() => { showCanvasPanel = !showCanvasPanel; setPref('cpanel', showCanvasPanel ? '1' : '0'); }}><Icon name="outline" /></button>
    {:else}
      <button class="icon-btn" class:on={showOutline} title="Outline" onclick={() => { showOutline = !showOutline; setPref('outline', showOutline ? '1' : '0'); }}><Icon name="outline" /></button>
    {/if}
    <ThemeToggle />
    <button
      class="btn primary"
      title={view === 'canvas' ? 'Present the canvas frames (F5) · Shift+F5 from the selected frame' : 'Present (F5) · Shift+F5 from the selected section'}
      onclick={() => present()}
      disabled={!ds.doc}><Icon name="present" /> Present</button
    >
  </header>

  {#if findOpen && ds.doc}
    <FindReplace {ds} onclose={() => (findOpen = false)} onjump={jumpTo} />
  {/if}

  {#if ds.recovery}
    <div class="conflict recovery">
      <Icon name="history" />
      <span>Edits from {new Date(ds.recovery.at).toLocaleString()} were never saved (the tab was closed, the browser crashed or the server was unreachable).</span>
      <button class="btn sm" onclick={() => ds.discardRecovery()}>Discard them</button>
      <button class="btn sm primary" onclick={() => ds.restoreRecovery()}>Restore them</button>
    </div>
  {/if}

  {#if ds.status === 'conflict'}
    <div class="conflict">
      <Icon name="alert" />
      <span>This document was changed in another tab or on another computer{ds.conflict ? ` (version ${ds.conflict.version}, ${new Date(ds.conflict.updatedAt).toLocaleString()})` : ''}. Your latest edits are not saved.</span>
      <button class="btn sm" onclick={() => ds.takeTheirs()}>Load their version</button>
      <button class="btn sm primary" onclick={() => ds.keepMine()}>Overwrite with mine</button>
    </div>
  {/if}

  <div class="main" class:with-outline={showOutline && view === 'doc'}>
    {#if showLibrary}
      <aside class="lib">
        <LibraryPanel onpick={onPick} relinking={!!relink} oncancelrelink={() => (relink = null)} />
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="resizer" onpointerdown={startResize}></div>
      </aside>
    {/if}

    {#if view === 'canvas'}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="canvaswrap" ondragover={onExternalDragOver} ondrop={onExternalDrop}>
        {#if ds.status === 'loading'}
          <p class="muted center">Loading…</p>
        {:else if !ds.doc}
          <div class="center">
            <p>{ds.error}</p>
            <button class="btn" onclick={() => router.go('/')}>Back to documents</button>
          </div>
        {:else}
          <Canvas
            bind:this={canvas}
            {ds}
            showPanel={showCanvasPanel}
            onrelink={(sid, mi) => { relink = { sectionId: sid, index: mi }; if (!showLibrary) showLibrary = true; }}
            onpresentdoc={(idx) => present(idx)}
          />
        {/if}
      </div>
    {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="docscroll" data-scroll-root bind:this={scrollEl} onscroll={savePos} ondragover={onExternalDragOver} ondrop={onExternalDrop}>
      {#if ds.status === 'loading'}
        <p class="muted center">Loading…</p>
      {:else if !ds.doc}
        <div class="center">
          <p>{ds.error}</p>
          <button class="btn" onclick={() => router.go('/')}>Back to documents</button>
        </div>
      {:else}
        {#key ds.revision}
          <div class="sections" bind:this={sectionsEl}>
            {#each ds.doc.sections as s, i (s.id)}
              <div class="sec-wrap" class:chapter-hidden={inCollapsedChapter[i]}>
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="zone"
                  class:show={dnd.dragging}
                  class:over={dropAt === i}
                  ondragover={(e) => onZoneOver(e, i)}
                  ondragleave={() => (dropAt = null)}
                  ondrop={(e) => onZoneDrop(e, i)}
                >
                  <span>New section here</span>
                </div>
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <div onclick={() => (ds.activeSection = s.id)}>
                  <SectionView
                    section={s}
                    index={i}
                    total={ds.doc.sections.length}
                    {ds}
                    onrelink={(sid, mi) => { relink = { sectionId: sid, index: mi }; if (!showLibrary) showLibrary = true; }}
                    onpresent={(idx) => present(idx)}
                  />
                </div>
              </div>
            {/each}
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="zone end"
            class:show={dnd.dragging}
            class:over={dropAt === ds.doc.sections.length}
            ondragover={(e) => onZoneOver(e, ds.doc!.sections.length)}
            ondragleave={() => (dropAt = null)}
            ondrop={(e) => onZoneDrop(e, ds.doc!.sections.length)}
          >
            <span>New section here</span>
          </div>
          <div class="add">
            <button class="btn" onclick={() => { const s = ds.addSection(null); scrollToSection(s.id); }}><Icon name="plus" /> Add section</button>
            <span class="muted">or drop media from the library</span>
          </div>
        {/key}
      {/if}
    </div>
    {/if}

    {#if showOutline && ds.doc && view === 'doc'}
      <aside class="outline"><Outline {ds} onjump={(sid) => scrollToSection(sid)} /></aside>
    {/if}
  </div>
</div>

{#if settingsOpen && ds.doc}
  <DocSettings {ds} onclose={() => (settingsOpen = false)} />
{/if}

{#if history && ds.doc}
  <HistoryDialog {ds} onclose={() => (history = false)} />
{/if}

<style>
  .editor {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
    z-index: 5;
  }
  .doc-title {
    flex: 0 1 460px;
    min-width: 120px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    outline: none;
    font: 650 18px var(--serif);
    padding: 4px 8px;
  }
  .doc-title:hover,
  .doc-title:focus {
    border-color: var(--line-strong);
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--green);
  }
  .status.dirty .dot,
  .status.saving .dot {
    background: var(--accent);
  }
  .status.error .dot,
  .status.conflict .dot {
    background: var(--red);
  }
  .status.error,
  .status.conflict {
    color: var(--red);
  }
  .grow {
    flex: 1;
  }
  .viewseg {
    display: inline-flex;
    padding: 2px;
    border-radius: 8px;
    background: var(--bg-2);
    border: 1px solid var(--line);
    margin-right: 6px;
  }
  .viewseg button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 13px;
  }
  .viewseg button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .tagchips {
    display: inline-flex;
    gap: 3px;
    border: none;
    background: none;
    cursor: pointer;
    padding: 0;
  }
  .menuwrap {
    position: relative;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
  .dmenu {
    position: absolute;
    right: 0;
    top: 36px;
    z-index: 51;
    min-width: 290px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .dmenu button,
  .dmenu a {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: none;
    background: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    color: var(--text);
    text-decoration: none;
    font-size: 13px;
  }
  .dmenu button:hover,
  .dmenu a:hover {
    background: var(--surface-2);
  }
  .dmenu .kbd,
  .dmenu .muted {
    margin-left: auto;
    font-size: 11px;
  }
  .dsep {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    padding: 8px 10px 3px;
    border-top: 1px solid var(--line);
    margin-top: 4px;
  }
  .conflict.recovery {
    background: var(--accent-soft);
    border-bottom-color: var(--accent);
  }
  .canvaswrap {
    min-height: 0;
    position: relative;
  }
  .conflict {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: var(--red-soft);
    color: var(--text);
    border-bottom: 1px solid var(--red);
  }
  .conflict span {
    flex: 1;
  }
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: var(--lib-w) minmax(0, 1fr);
  }
  .main.with-outline {
    grid-template-columns: var(--lib-w) minmax(0, 1fr) 240px;
  }
  .lib {
    position: relative;
    min-height: 0;
    border-right: 1px solid var(--line);
    overflow: hidden;
  }
  .resizer {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 3;
  }
  .resizer:hover {
    background: var(--accent-soft);
  }
  .outline {
    min-height: 0;
    border-left: 1px solid var(--line);
    overflow: hidden;
  }
  .docscroll {
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 24px 28px 120px;
    overscroll-behavior: contain;
  }
  .sections {
    max-width: 1000px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
  }
  .sec-wrap {
    display: flex;
    flex-direction: column;
  }
  .sec-wrap.chapter-hidden {
    display: none;
  }
  :global(.sec-ghost) {
    opacity: 0.35;
  }
  /* drop zones keep a fixed height so nothing shifts when a drag starts */
  .zone {
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    position: relative;
  }
  .zone span {
    display: none;
    font-size: 12px;
    color: var(--accent);
    background: var(--bg);
    padding: 0 8px;
    position: relative;
  }
  .zone.show::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    border-top: 1.5px dashed var(--line-strong);
  }
  .zone.show span {
    display: block;
    color: var(--text-3);
  }
  .zone.over {
    background: var(--accent-soft);
  }
  .zone.over::before {
    border-top-color: var(--accent);
  }
  .zone.over span {
    color: var(--accent);
  }
  .zone.end {
    max-width: 1000px;
    margin: 0 auto;
  }
  .add {
    max-width: 1000px;
    margin: 14px auto 0;
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .center {
    text-align: center;
    margin-top: 80px;
  }
</style>
