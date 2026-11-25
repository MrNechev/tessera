<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import { api, fmtBytes, fmtDuration, spriteLayout } from '../lib/api';
  import { palette } from '../lib/palette.svelte';
  import { prefs } from '../lib/prefs.svelte';
  import { uiPrefs } from '../lib/uiprefs.svelte';
  import DuplicatesDialog from './DuplicatesDialog.svelte';
  import { dnd, MEDIA_MIME } from '../lib/dnd.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { DirNode, FileItem, Library } from '../lib/types';
  import FolderTree from './FolderTree.svelte';
  import Icon from './Icon.svelte';
  import Lightbox from './Lightbox.svelte';

  let {
    onpick,
    relinking = false,
    oncancelrelink,
    paletteTarget = true,
    onselect,
    hint = 'Click: insert · drag into the story · Alt+click: new section · tick ✓ (or Ctrl+click) to pick several, Shift+click for a range · double-click: preview',
  }: {
    onpick: (items: FileItem[], newSection: boolean) => void;
    relinking?: boolean;
    oncancelrelink?: () => void;
    /** Ctrl+K "open folder" opens folders here (off for panels that may be hidden). */
    paletteTarget?: boolean;
    /** The how-to line at the bottom (defaults to the document editor's). */
    hint?: string;
    /** Called whenever the Ctrl/Shift selection changes; the host then offers its own actions instead of "Insert". */
    onselect?: (items: FileItem[]) => void;
  } = $props();

  const PAGE = 240;
  const GAP = 6;

  function stored(k: string, d: string) {
    try {
      return localStorage.getItem('letopis.lib.' + k) ?? d;
    } catch {
      return d;
    }
  }
  function store(k: string, v: string) {
    try {
      localStorage.setItem('letopis.lib.' + k, v);
    } catch {}
  }

  let libraries = $state<Library[]>([]);
  let libId = $state(stored('lib', ''));
  let tree = $state<DirNode | null>(null);
  let dir = $state(stored('dir', ''));
  let recursive = $state(stored('recursive', '1') === '1');
  let kind = $state(stored('kind', 'all'));
  let sort = $state(stored('sort', 'mtime'));
  let desc = $state(stored('desc', '1') === '1');
  const tileMin = $derived(uiPrefs.tile);
  let search = $state('');
  let q = $state('');
  let showTree = $state(stored('tree', '1') === '1');
  /** folders, search and filters folded away: more room for the photos */
  let collapsed = $state(stored('collapsed', '0') === '1');
  function setCollapsed(v: boolean) {
    collapsed = v;
    store('collapsed', v ? '1' : '0');
  }
  /** wide (the Library page): folders on the left, photos on the right */
  let panelW = $state(0);
  const wide = $derived(panelW >= 820);
  // browse the folders, or the favourite / recently inserted files
  let source = $state<'browse' | 'fav' | 'recent'>((stored('source', 'browse') as 'browse') || 'browse');
  type Filters = { from: string; to: string; minShort: number; orientation: string; minDur: number; maxDur: number; minSizeMB: number; camera: string; gps: boolean; motion: boolean };
  const noFilters: Filters = { from: '', to: '', minShort: 0, orientation: '', minDur: 0, maxDur: 0, minSizeMB: 0, camera: '', gps: false, motion: false };
  let filters = $state<Filters>({ ...noFilters, ...(JSON.parse(stored('filters', '{}') || '{}') as Partial<Filters>) });
  let showFilters = $state(false);
  let cameras = $state<{ name: string; count: number }[]>([]);
  const activeFilters = $derived(Object.entries(filters).filter(([k, v]) => v !== (noFilters as any)[k]).length);
  const blur = $derived(uiPrefs.blur);
  let menu = $state(false);
  let dupes = $state(false);
  let hiddenOpen = $state(false);
  let folderMenu = $state<{ x: number; y: number; path: string } | null>(null);
  // hover previews: sprite scrubbing for videos, the clip for live photos
  let hover = $state<{ key: string; kind: 'sprite' | 'motion'; ready: boolean; frac: number } | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  const expanded = new SvelteSet<string>();

  let total = $state(0);
  const pages = new SvelteMap<number, FileItem[]>();
  const loading = new Set<number>();
  let gen = 0;

  let scroller = $state<HTMLDivElement>();
  let width = $state(300);
  let height = $state(400);
  let scrollTop = $state(0);
  const selected = new SvelteSet<string>();
  let lastClicked = -1;
  /** "Select" pressed: a click ticks a tile instead of inserting/selecting just it */
  let selectMode = $state(false);
  const selecting = $derived(selectMode || selected.size > 0);
  let clickTimer: ReturnType<typeof setTimeout> | undefined;
  let preview = $state<FileItem | null>(null);
  let scanning = $state(false);

  const cols = $derived(Math.max(2, Math.floor((width + GAP) / (tileMin + GAP))));
  const tile = $derived((width - GAP * (cols - 1)) / cols);
  const rowH = $derived(tile + GAP);
  const rows = $derived(Math.ceil(total / cols));
  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / rowH) - 3));
  const lastRow = $derived(Math.min(rows, Math.ceil((scrollTop + height) / rowH) + 3));
  const visible = $derived.by(() => {
    const out: number[] = [];
    for (let r = firstRow; r < lastRow; r++)
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        if (i < total) out.push(i);
      }
    return out;
  });

  function itemAt(i: number): FileItem | undefined {
    return pages.get(Math.floor(i / PAGE))?.[i % PAGE];
  }

  async function loadPage(p: number, g: number) {
    if (loading.has(p) || pages.has(p)) return;
    loading.add(p);
    try {
      if (source !== 'browse') {
        // favourites / recent: a fixed list of references, resolved to file info
        const list = (source === 'fav' ? prefs.favorites.filter((f) => !f.dir) : prefs.recent).filter(
          (x) => !q || x.path.toLowerCase().includes(q.toLowerCase()),
        );
        const res = await api.resolve(list.map((x) => ({ libraryId: x.libraryId, path: x.path })));
        if (g !== gen) return;
        let items = res.filter((r) => r.found && r.file).map((r) => r.file!) as FileItem[];
        if (kind !== 'all') items = items.filter((f) => f.kind === kind);
        total = items.length;
        pages.set(0, items);
        return;
      }
      const f = filters;
      const day = (s: string, end = false) => (s ? new Date(s + (end ? 'T23:59:59.999' : 'T00:00:00')).getTime() + (end ? 1 : 0) : undefined);
      const res = await api.files({
        library: libId, dir, recursive, q, kind, sort, desc, offset: p * PAGE, limit: PAGE,
        from: day(f.from), to: day(f.to, true), minShort: f.minShort || undefined, orientation: f.orientation || undefined,
        minDur: f.minDur || undefined, maxDur: f.maxDur || undefined, minSize: f.minSizeMB ? f.minSizeMB * 1e6 : undefined,
        camera: f.camera || undefined, gps: f.gps, motion: f.motion,
      });
      if (g !== gen) return;
      total = res.total;
      pages.set(p, res.items);
    } catch (e) {
      if (g === gen) toasts.error('Library: ' + (e as Error).message);
    } finally {
      loading.delete(p);
    }
  }

  function reset() {
    gen++;
    pages.clear();
    loading.clear();
    total = 0;
    selected.clear();
    lastClicked = -1;
    if (scroller) scroller.scrollTop = 0;
    scrollTop = 0;
    loadPage(0, gen);
  }

  // query changes → reload
  $effect(() => {
    void [libId, dir, recursive, q, kind, sort, desc, source, JSON.stringify(filters)];
    if (source === 'fav') void prefs.favorites.length;
    untrack(() => {
      store('source', source);
      store('filters', JSON.stringify(filters));
      store('lib', libId);
      store('dir', dir);
      store('recursive', recursive ? '1' : '0');
      store('kind', kind);
      store('sort', sort);
      store('desc', desc ? '1' : '0');
      if (libId) reset();
    });
  });

  // load the pages that are on screen
  $effect(() => {
    if (!libId || !total) return;
    const a = Math.floor((firstRow * cols) / PAGE);
    const b = Math.floor((Math.max(0, lastRow * cols - 1)) / PAGE);
    const g = gen;
    untrack(() => {
      for (let p = a; p <= b; p++) loadPage(p, g);
    });
  });

  let searchTimer: ReturnType<typeof setTimeout>;
  $effect(() => {
    const s = search;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => (q = s.trim()), 220);
  });

  async function loadLibraries() {
    libraries = await api.libraries();
    if (!libraries.find((l) => l.id === libId)) libId = libraries[0]?.id ?? '';
  }

  async function loadTree() {
    if (!libId) return;
    tree = await api.tree(libId);
    // expand the path to the current folder
    const parts = dir.split('/').filter(Boolean);
    for (let i = 1; i < parts.length; i++) expanded.add(parts.slice(0, i).join('/'));
  }

  $effect(() => {
    if (libId) untrack(loadTree);
  });

  onMount(() => {
    loadLibraries().catch((e) => toasts.error((e as Error).message));
    prefs.load();
    const open = (l: string, p: string, find?: string) => {
      libId = l;
      source = 'browse';
      showTree = true;
      selectDir(p);
      if (find) search = q = find;
      const parts = p.split('/');
      for (let i = 1; i < parts.length; i++) expanded.add(parts.slice(0, i).join('/'));
    };
    if (palette.pendingFolder) {
      const f = palette.pendingFolder;
      palette.pendingFolder = null;
      open(f.libraryId, f.path, f.search);
      // "show this day in the Library" (stories calendar, On this day): the whole library, filtered by capture date
      if (f.days) {
        filters = { ...noFilters, from: f.days.from, to: f.days.to };
        recursive = true;
        sort = 'taken';
        desc = false;
      }
    }
    if (!paletteTarget) return;
    palette.openFolder = open;
    return () => {
      if (palette.openFolder === open) palette.openFolder = null;
    };
  });

  $effect(() => {
    if (showFilters && !cameras.length) untrack(() => api.cameras().then((c) => (cameras = c)).catch(() => {}));
  });

  function setBlur(v: boolean) {
    uiPrefs.blur = v;
  }

  function hoverStart(f: FileItem) {
    clearTimeout(hoverTimer);
    const kind = f.kind === 'video' && f.duration > 0 ? 'sprite' : f.motion ? 'motion' : null;
    if (!kind) return;
    hoverTimer = setTimeout(() => {
      hover = { key: f.key, kind, ready: kind === 'motion', frac: hover?.key === f.key ? hover.frac : 0.5 };
      if (kind === 'sprite') {
        const im = new Image();
        im.onload = () => {
          if (hover?.key === f.key) hover.ready = true;
        };
        im.src = `/d/${f.key}/sprite.jpg`;
      }
    }, 250);
  }
  function hoverMove(e: MouseEvent, f: FileItem) {
    if (hover?.key !== f.key || hover.kind !== 'sprite') return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hover.frac = Math.min(0.999, Math.max(0, (e.clientX - r.left) / r.width));
  }
  function hoverEnd() {
    clearTimeout(hoverTimer);
    hover = null;
  }

  async function hideFolder(path: string) {
    folderMenu = null;
    if (!confirm(`Hide "${path}" from the library?\n\nIts files are left alone on disk; they just aren't listed or indexed. You can show it again from the library menu.`)) return;
    await prefs.setHidden([...prefs.hidden, { libraryId: libId, path, dir: true }]);
    if (dir === path || dir.startsWith(path + '/')) dir = '';
    toasts.show('Folder hidden — the library refreshes after the rescan', 'ok');
    setTimeout(() => { loadTree(); reset(); }, 2500);
  }

  async function unhide(i: number) {
    await prefs.setHidden(prefs.hidden.filter((_, j) => j !== i));
    toasts.show('Folder shown again — rescanning', 'ok');
    setTimeout(() => { loadTree(); reset(); }, 3000);
  }

  async function rescan() {
    scanning = true;
    await api.scan();
    const poll = setInterval(async () => {
      const s = await api.status();
      if (!s.scan.running) {
        clearInterval(poll);
        scanning = false;
        await loadLibraries();
        await loadTree();
        reset();
        toasts.show(`Scan finished: ${s.scan.changed} new/changed, ${s.scan.removed} removed`, 'ok');
      }
    }, 1000);
  }

  function selectDir(p: string) {
    dir = p;
    search = '';
    q = '';
  }

  function toggle(p: string) {
    if (expanded.has(p)) expanded.delete(p);
    else expanded.add(p);
  }

  function itemKey(f: FileItem) {
    return f.libraryId + '\u0000' + f.path;
  }

  function toggleTile(i: number, f: FileItem) {
    const k = itemKey(f);
    if (selected.has(k)) selected.delete(k);
    else selected.add(k);
    lastClicked = i;
  }

  function selectRange(i: number) {
    const [a, b] = [Math.min(lastClicked, i), Math.max(lastClicked, i)];
    for (let j = a; j <= b; j++) {
      const it = itemAt(j);
      if (it) selected.add(itemKey(it));
    }
    lastClicked = i;
  }

  /**
   * Shift+click: a range. Ctrl+click, the tick, or any click while something
   * is selected: add/remove one. Otherwise the Library page selects the one
   * photo, and the story editor inserts it (a moment later, so a double-click
   * previews instead of inserting twice).
   */
  function onTileClick(e: MouseEvent, i: number, f: FileItem) {
    if (e.detail > 1) return; // the second click of a double-click
    if (e.shiftKey && lastClicked >= 0) return selectRange(i);
    if (e.ctrlKey || e.metaKey || e.shiftKey || selecting) return toggleTile(i, f);
    lastClicked = i;
    if (onselect) {
      selected.clear();
      selected.add(itemKey(f));
      return;
    }
    const alt = e.altKey;
    clearTimeout(clickTimer);
    clickTimer = setTimeout(() => onpick([f], alt), 240);
  }

  function onTileDblClick(f: FileItem) {
    clearTimeout(clickTimer);
    preview = f;
  }

  function onCheck(e: MouseEvent, i: number, f: FileItem) {
    e.stopPropagation();
    e.preventDefault();
    clearTimeout(clickTimer);
    if (e.shiftKey && lastClicked >= 0) selectRange(i);
    else toggleTile(i, f);
  }

  let selectingAll = $state(false);
  /** Tick everything in the current view (up to 5,000 files). */
  async function selectAll() {
    const n = Math.min(total, 5000);
    selectingAll = true;
    try {
      const g = gen;
      for (let p = 0; p * PAGE < n; p++) {
        if (!pages.has(p)) {
          while (loading.has(p)) await new Promise((r) => setTimeout(r, 50));
          await loadPage(p, g);
        }
        if (g !== gen) return;
      }
      for (let i = 0; i < n; i++) {
        const it = itemAt(i);
        if (it) selected.add(itemKey(it));
      }
      if (total > n) toasts.show(`Selected the first ${n.toLocaleString()} of ${total.toLocaleString()}`, 'info');
    } finally {
      selectingAll = false;
    }
  }

  function clearSel() {
    selected.clear();
    selectMode = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !selecting) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest?.('input, textarea, select, [contenteditable="true"]')) return;
    clearSel();
  }

  function selectedItems(): FileItem[] {
    const out: FileItem[] = [];
    for (const items of pages.values()) for (const f of items) if (selected.has(itemKey(f))) out.push(f);
    return out;
  }

  $effect(() => {
    void selected.size;
    if (onselect) untrack(() => onselect(selectedItems()));
  });

  /** Clear the Ctrl/Shift selection (from the host's action bar). */
  export function clearSelection() {
    clearSel();
  }

  function insertSelected() {
    const items = selectedItems();
    if (items.length) onpick(items, true);
    clearSel();
  }

  function onDragStart(e: DragEvent, f: FileItem) {
    const k = itemKey(f);
    const items = selected.has(k) ? selectedItems() : [f];
    e.dataTransfer!.setData(MEDIA_MIME, JSON.stringify(items.map((x) => ({ libraryId: x.libraryId, path: x.path, kind: x.kind }))));
    e.dataTransfer!.setData('text/plain', items.map((x) => x.path).join('\n'));
    e.dataTransfer!.effectAllowed = 'copy';
    dnd.dragging = true;
  }

  const lib = $derived(libraries.find((l) => l.id === libId));
  const crumbs = $derived(dir ? dir.split('/') : []);
</script>

<svelte:window onkeydown={onKey} />

<div class="panel" class:wide class:collapsed bind:clientWidth={panelW}>
  {#if !collapsed}
  <div class="pside">
  <div class="bar">
    {#if libraries.length > 1}
      <select class="input lib" bind:value={libId} onchange={() => (dir = '')}>
        {#each libraries as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
      </select>
    {:else}
      <div class="libname" title={lib ? `${lib.counts.total} files · ${fmtBytes(lib.counts.bytes)}` : ''}>{lib?.name ?? 'Library'}</div>
    {/if}
    <button class="icon-btn" class:on={showTree} title={showTree ? 'Hide the folder tree' : 'Show the folder tree'} onclick={() => { showTree = !showTree; store('tree', showTree ? '1' : '0'); }}><Icon name="folder" /></button>
    <button class="icon-btn" class:on={blur} title={blur ? 'Thumbnails are blurred (hover to peek) — click to unblur' : 'Blur thumbnails (for screen sharing)'} onclick={() => setBlur(!blur)}><Icon name={blur ? 'eye-off' : 'eye'} /></button>
    <div class="menuwrap">
      <button class="icon-btn" title="More" onclick={() => (menu = !menu)}><Icon name="more" /></button>
      {#if menu}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="menu-backdrop" onclick={() => (menu = false)}></div>
        <div class="menu">
          <button onclick={() => { menu = false; rescan(); }} disabled={scanning}><Icon name="refresh" size={15} /> Rescan folders</button>
          <button onclick={() => { menu = false; dupes = true; }}><Icon name="copy" size={15} /> Find duplicates…</button>
          <button onclick={() => { menu = false; hiddenOpen = true; }}><Icon name="eye-off" size={15} /> Hidden folders ({prefs.hidden.length})</button>
        </div>
      {/if}
    </div>
    <button class="icon-btn" title={wide ? 'Hide the folders and filters (more room for photos)' : 'Fold away the folders and filters (more room for photos)'} onclick={() => setCollapsed(true)}><Icon name={wide ? 'panel-left' : 'chev-up'} /></button>
  </div>
  <div class="srcs">
    <button class:on={source === 'browse'} onclick={() => (source = 'browse')}><Icon name="folder" size={14} /> Browse</button>
    <button class:on={source === 'fav'} onclick={() => (source = 'fav')}><Icon name="star" size={14} /> Favourites</button>
    <button class:on={source === 'recent'} onclick={() => (source = 'recent')}><Icon name="history" size={14} /> Recent</button>
  </div>
  <div class="bar">
    <div class="searchbox">
      <Icon name="search" size={15} />
      <input placeholder={dir ? 'Search in this folder…' : 'Search all files…'} bind:value={search} />
      {#if search}<button class="clear" onclick={() => (search = '')} aria-label="Clear"><Icon name="x" size={14} /></button>{/if}
    </div>
    {#if source === 'browse'}
      <button class="icon-btn fbtn" class:on={showFilters || activeFilters > 0} title="Filters: date, resolution, duration, camera…" onclick={() => (showFilters = !showFilters)}>
        <Icon name="sliders" />{#if activeFilters}<span class="fcount">{activeFilters}</span>{/if}
      </button>
    {/if}
  </div>
  {#if showFilters && source === 'browse'}
    <div class="fpanel">
      <label>Taken from <input class="input" type="date" bind:value={filters.from} /></label>
      <label>to <input class="input" type="date" bind:value={filters.to} /></label>
      <label>Resolution
        <select class="input" bind:value={filters.minShort}>
          <option value={0}>any</option><option value={720}>720p+</option><option value={1080}>1080p+</option><option value={1440}>1440p+</option><option value={2160}>4K+</option>
        </select>
      </label>
      <label>Orientation
        <select class="input" bind:value={filters.orientation}>
          <option value="">any</option><option value="landscape">landscape</option><option value="portrait">portrait</option><option value="square">square</option>
        </select>
      </label>
      <label>Duration
        <select class="input" value={`${filters.minDur}-${filters.maxDur}`} onchange={(e) => { const [a, b] = e.currentTarget.value.split('-').map(Number); filters.minDur = a; filters.maxDur = b; }}>
          <option value="0-0">any</option><option value="0-10">under 10 s</option><option value="10-60">10 s – 1 min</option><option value="60-600">1 – 10 min</option><option value="600-0">over 10 min</option>
        </select>
      </label>
      <label>Size at least
        <select class="input" bind:value={filters.minSizeMB}>
          <option value={0}>any</option><option value={1}>1 MB</option><option value={10}>10 MB</option><option value={100}>100 MB</option><option value={1000}>1 GB</option>
        </select>
      </label>
      <label class="wide">Camera
        <select class="input" bind:value={filters.camera}>
          <option value="">any</option>
          {#each cameras as c (c.name)}<option value={c.name}>{c.name} ({c.count})</option>{/each}
        </select>
      </label>
      <label class="chk"><input type="checkbox" bind:checked={filters.gps} /> has location</label>
      <label class="chk"><input type="checkbox" bind:checked={filters.motion} /> live photos</label>
      {#if activeFilters}<button class="btn sm" onclick={() => (filters = { ...noFilters })}>Clear filters</button>{/if}
    </div>
  {/if}
  <div class="bar filters">
    {#each [['all', 'All'], ['image', 'Images'], ['video', 'Videos'], ['gif', 'GIFs'], ['audio', 'Audio']] as [k, label] (k)}
      <button class="seg" class:on={kind === k} onclick={() => (kind = k)}>{label}</button>
    {/each}
    <div class="grow"></div>
    <span class="sortbox">
    <select class="input mini" bind:value={sort} title="Sort">
      <option value="mtime">File date</option>
      <option value="taken">Date taken</option>
      <option value="name">Name</option>
      <option value="duration">Duration</option>
      <option value="size">Size</option>
      <option value="path">Path</option>
    </select>
    <button class="icon-btn" title={desc ? 'Descending' : 'Ascending'} onclick={() => (desc = !desc)}><Icon name={desc ? 'arrow-down' : 'arrow-up'} size={15} /></button>
    </span>
  </div>

  {#if source !== 'browse'}
    {#if source === 'fav' && prefs.favorites.some((f) => f.dir)}
      <div class="favdirs">
        {#each prefs.favorites.filter((f) => f.dir) as f (f.libraryId + f.path)}
          <button class="chip" onclick={() => { libId = f.libraryId; source = 'browse'; selectDir(f.path); }}><Icon name="folder" size={12} /> {f.path.split('/').pop()}</button>
        {/each}
      </div>
    {/if}
    {#if source === 'recent' && prefs.recent.length}
      <div class="favdirs"><span class="muted">Recently inserted, newest first</span><span class="grow"></span><button class="btn sm ghost" onclick={() => prefs.clearRecent()}>Clear</button></div>
    {/if}
  {:else if showTree && tree}
    <div class="tree">
      <div class="row-root" class:sel={dir === ''}>
        <button class="name" onclick={() => selectDir('')}><Icon name="home" size={14} /> <span>All folders</span> <span class="count">{tree.total}</span></button>
      </div>
      <FolderTree node={tree} selected={dir} {expanded} onselect={selectDir} ontoggle={toggle} oncontext={(path, e) => (folderMenu = { x: e.clientX, y: e.clientY, path })} />
    </div>
  {/if}

  </div>
  {/if}
  <div class="pmain">
  {#if relinking}
    <div class="relink">
      <Icon name="link" /> <span>Pick the file to relink to</span>
      <button class="btn sm" onclick={oncancelrelink}>Cancel</button>
    </div>
  {/if}
  {#if source === 'browse' || collapsed}
  <div class="crumbs">
    {#if collapsed}
      <button class="expand" title="Show the folders, search and filters" onclick={() => setCollapsed(false)}><Icon name={wide ? 'panel-left' : 'chev-down'} size={15} /> Folders</button>
    {/if}
    {#if source === 'browse'}
      <button onclick={() => selectDir('')}>{lib?.name ?? ''}</button>
      {#each crumbs as c, i (i)}
        <span class="sep">/</span><button onclick={() => selectDir(crumbs.slice(0, i + 1).join('/'))}>{c}</button>
      {/each}
    {:else}
      <span>{source === 'fav' ? 'Favourites' : 'Recently inserted'}</span>
    {/if}
    {#if collapsed && q}<span class="qchip" title="Search">“{q}”</span>{/if}
    <div class="grow"></div>
    {#if source === 'browse'}<label class="sub" title="Include files in subfolders"><input type="checkbox" bind:checked={recursive} /> subfolders</label>{/if}
  </div>
  {/if}

  <div class="status">
    <span>{total.toLocaleString()} item{total === 1 ? '' : 's'}</span>
    {#if selecting}
      <span class="selcount">{selected.size} selected</span>
      <span class="grow"></span>
      <button class="btn sm ghost" disabled={selectingAll || !total} onclick={() => void selectAll()} title="Select every file in this view">All</button>
      <button class="btn sm ghost" onclick={clearSel} title="Clear the selection (Esc)">Clear</button>
      {#if !onselect}<button class="btn sm primary" disabled={!selected.size} onclick={insertSelected}>Insert {selected.size || ''}</button>{/if}
    {:else}
      <span class="grow"></span>
      <button class="btn sm ghost" onclick={() => (selectMode = true)} title="Pick several files: click tiles to tick them"><Icon name="check" size={14} /> Select</button>
      <input type="range" min="72" max="220" step="4" value={tileMin} oninput={(e) => (uiPrefs.tile = Number(e.currentTarget.value))} title="Thumbnail size" />
    {/if}
  </div>

  <div
    class="grid"
    class:blur
    class:selecting
    bind:this={scroller}
    bind:clientWidth={width}
    bind:clientHeight={height}
    onscroll={() => (scrollTop = scroller!.scrollTop)}
  >
    <div class="spacer" style="height:{rows * rowH}px">
      {#each visible as i (i)}
        {@const f = itemAt(i)}
        {@const x = (i % cols) * (tile + GAP)}
        {@const y = Math.floor(i / cols) * rowH}
        {#if f}
          <button
            class="tile"
            class:sel={selected.has(f.libraryId + '\u0000' + f.path)}
            style="transform:translate({x}px,{y}px);width:{tile}px;height:{tile}px"
            title="{f.path}\n{fmtBytes(f.size)}{f.width ? ` · ${f.width}×${f.height}` : ''}{f.vcodec ? ` · ${f.vcodec}` : ''}"
            draggable="true"
            ondragstart={(e) => onDragStart(e, f)}
            ondragend={() => (dnd.dragging = false)}
            onclick={(e) => onTileClick(e, i, f)}
            ondblclick={() => onTileDblClick(f)}
            onmouseenter={() => hoverStart(f)}
            onmouseleave={hoverEnd}
            onmousemove={(e) => hoverMove(e, f)}
          >
            {#if f.kind === 'audio'}<span class="audioic"><Icon name="music" size={32} /></span>{:else}
            <img src="/d/{f.key}/t320.webp" alt="" loading="lazy" decoding="async" draggable="false" onerror={(e) => ((e.currentTarget as HTMLImageElement).style.visibility = 'hidden')} />{/if}
            {#if hover?.key === f.key && hover.ready}
              {#if hover.kind === 'sprite'}
                {@const L = spriteLayout(f)}
                {@const rowsN = Math.ceil(L.n / L.cols)}
                {@const fi = Math.min(L.n - 1, Math.floor(hover.frac * L.n))}
                <span
                  class="scrub"
                  style="aspect-ratio:{L.w}/{L.h};background-image:url(/d/{f.key}/sprite.jpg);background-size:{L.cols * 100}% {rowsN * 100}%;background-position:{((fi % L.cols) / (L.cols - 1)) * 100}% {rowsN > 1 ? (Math.floor(fi / L.cols) / (rowsN - 1)) * 100 : 0}%"
                ></span>
                <span class="scrubbar" style="width:{hover.frac * 100}%"></span>
              {:else}
                <video class="motionclip" src="/d/{f.key}/motion.mp4" autoplay loop muted playsinline disablepictureinpicture></video>
              {/if}
            {/if}
            <span class="nm">{f.name}</span>
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span
              class="check"
              class:on={selected.has(f.libraryId + '\u0000' + f.path)}
              role="checkbox"
              tabindex="-1"
              aria-checked={selected.has(f.libraryId + '\u0000' + f.path)}
              title="Select (Shift: a range)"
              onclick={(e) => onCheck(e, i, f)}
              ondblclick={(e) => e.stopPropagation()}><Icon name="check" size={13} /></span
            >
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span
              class="star"
              class:on={prefs.isFavorite(f)}
              role="button"
              tabindex="-1"
              title={prefs.isFavorite(f) ? 'Remove from favourites' : 'Add to favourites'}
              onclick={(e) => { e.stopPropagation(); prefs.toggleFavorite({ libraryId: f.libraryId, path: f.path }); }}
              ondblclick={(e) => e.stopPropagation()}><Icon name="star" size={14} /></span
            >
            {#if f.motion}<span class="badge live" title="Live / motion photo — hover to play">LIVE</span>{/if}
            {#if f.hdr}<span class="badge hdr" title="HDR video (tone-mapped for the browser)">HDR</span>{/if}
            {#if f.kind === 'video'}
              <span class="badge">{fmtDuration(f.duration) || 'video'}</span>
              {#if f.probed && !f.playable}<span class="badge warn" title="The browser can't play {f.vcodec || 'this'} directly; a compatible copy is made when needed">{f.vcodec?.toUpperCase() || '!'}</span>{/if}
            {:else if f.kind === 'gif'}
              <span class="badge">GIF</span>
            {/if}
          </button>
        {:else}
          <div class="tile ph" style="transform:translate({x}px,{y}px);width:{tile}px;height:{tile}px"></div>
        {/if}
      {/each}
    </div>
    {#if total === 0 && pages.has(0)}
      <div class="none muted">{q ? 'No matches.' : 'No media in this folder.'}</div>
    {/if}
  </div>
  <div class="hint muted">{hint}</div>
  </div>
</div>

{#if folderMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="menu-backdrop" onclick={() => (folderMenu = null)} oncontextmenu={(e) => { e.preventDefault(); folderMenu = null; }}></div>
  <div class="menu fmenu" style="left:{folderMenu.x}px;top:{folderMenu.y}px">
    <button onclick={() => { selectDir(folderMenu!.path); folderMenu = null; }}><Icon name="folder" size={15} /> Open</button>
    <button onclick={() => { prefs.toggleFavorite({ libraryId: libId, path: folderMenu!.path, dir: true }); folderMenu = null; }}>
      <Icon name="star" size={15} /> {prefs.isFavorite({ libraryId: libId, path: folderMenu.path }) ? 'Remove from favourites' : 'Add to favourites'}
    </button>
    <button onclick={() => hideFolder(folderMenu!.path)}><Icon name="eye-off" size={15} /> Hide from the library…</button>
  </div>
{/if}

{#if hiddenOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => (hiddenOpen = false)}>
    <div class="dialog small" onclick={(e) => e.stopPropagation()}>
      <header><h2>Hidden folders</h2><button class="icon-btn" onclick={() => (hiddenOpen = false)}><Icon name="x" /></button></header>
      <div class="body">
        <p class="muted">These folders are not listed or indexed. Nothing on disk is touched. Right-click a folder in the tree to hide it. Patterns in <code>LETOPIS_EXCLUDE</code> (in <code>.env</code>) work the same way.</p>
        {#each prefs.hidden as h, i (h.libraryId + h.path)}
          <div class="hrow"><Icon name="folder" size={15} /><span>{h.path}</span><button class="btn sm" onclick={() => unhide(i)}>Show again</button></div>
        {:else}
          <p>No hidden folders.</p>
        {/each}
      </div>
    </div>
  </div>
{/if}

{#if dupes}
  <DuplicatesDialog onclose={() => (dupes = false)} onopen={(f) => { dupes = false; libId = f.libraryId; source = 'browse'; selectDir(f.dir); search = f.name; }} />
{/if}

{#if preview}
  <Lightbox file={preview} onclose={() => (preview = null)} oninsert={() => { onpick([preview!], false); preview = null; }} />
{/if}

<style>
  /* wide: folders, search and filters in a column on the left */
  .pside,
  .pmain {
    display: contents;
  }
  .panel.wide {
    flex-direction: row;
  }
  .panel.wide .pside {
    display: flex;
    flex-direction: column;
    width: 320px;
    flex: none;
    min-height: 0;
    overflow-y: auto;
    border-right: 1px solid var(--line);
    padding-bottom: 10px;
  }
  .panel.wide .pmain {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .panel.wide .tree {
    max-height: none;
    flex: 1 1 auto;
    min-height: 120px;
  }
  .expand {
    display: inline-flex !important;
    align-items: center;
    gap: 4px;
    margin-right: 6px;
    padding: 2px 8px !important;
    border: 1px solid var(--line-strong) !important;
    border-radius: 6px;
    color: var(--text) !important;
  }
  .qchip {
    margin-left: 6px;
    color: var(--text-3);
  }
  .selcount {
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .grid {
    user-select: none;
    -webkit-user-select: none;
  }
  .check {
    position: absolute;
    top: 5px;
    left: 5px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 2px solid #fff;
    background: #0006;
    color: transparent;
    box-shadow: 0 1px 3px #0008;
    opacity: 0;
    transition: opacity 0.12s;
    z-index: 3;
    cursor: pointer;
  }
  .tile:hover .check,
  .grid.selecting .check {
    opacity: 1;
  }
  .check.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
    opacity: 1;
  }
  .grid.selecting .tile:not(.sel) img {
    opacity: 0.85;
  }
  .srcs {
    display: flex;
    gap: 2px;
    margin: 8px 10px 0;
    padding: 2px;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .srcs button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 12.5px;
    cursor: pointer;
  }
  .srcs button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .fbtn {
    position: relative;
  }
  .fcount {
    position: absolute;
    top: 1px;
    right: 1px;
    min-width: 14px;
    height: 14px;
    border-radius: 7px;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 10px;
    line-height: 14px;
  }
  .fpanel {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 8px;
    margin: 8px 10px 0;
    padding: 8px;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--text-2);
  }
  .fpanel label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .fpanel .wide {
    grid-column: 1 / -1;
  }
  .fpanel .chk {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .fpanel .input {
    height: 28px;
    font-size: 12px;
    padding: 0 6px;
  }
  .favdirs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 6px 10px 0;
    font-size: 12px;
  }
  .favdirs .chip {
    border: none;
    cursor: pointer;
  }
  .menuwrap {
    position: relative;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 34px;
    z-index: 51;
    min-width: 210px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .menu.fmenu {
    position: fixed;
    right: auto;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 9px;
    border: none;
    background: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    font-size: 13px;
  }
  .menu button:hover {
    background: var(--surface-2);
  }
  .dialog.small {
    width: min(560px, 100%);
  }
  .hrow {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
  }
  .hrow span {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .grid.blur .tile img,
  .grid.blur .tile video,
  .grid.blur .tile .scrub {
    filter: blur(14px) saturate(0.6);
    transition: filter 0.15s;
  }
  .grid.blur .tile:hover img,
  .grid.blur .tile:hover video,
  .grid.blur .tile:hover .scrub {
    filter: none;
  }
  .scrub {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    background-repeat: no-repeat;
    background-color: #000;
    pointer-events: none;
  }
  .scrubbar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent);
    pointer-events: none;
  }
  .motionclip {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }
  .audioic {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .star {
    position: absolute;
    top: 5px;
    left: 32px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: #0008;
    color: #fff;
    opacity: 0;
    transition: opacity 0.12s;
    z-index: 2;
  }
  .star.on {
    opacity: 1;
    color: #ffd25e;
  }
  .tile:hover .star {
    opacity: 1;
  }
  .badge.hdr {
    top: 24px;
  }
  .badge.live {
    background: #fffc;
    color: #111;
    font-weight: 700;
  }
  .panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-2);
  }
  .relink {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--accent-soft);
    color: var(--accent);
    border-bottom: 1px solid var(--line);
  }
  .relink span {
    flex: 1;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 10px 0;
  }
  .libname {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lib {
    flex: 1;
  }
  .searchbox {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--text-3);
  }
  .searchbox:focus-within {
    border-color: var(--accent);
  }
  .searchbox input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    outline: none;
    color: var(--text);
  }
  .clear {
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 0;
    display: grid;
  }
  .filters {
    gap: 2px;
    flex-wrap: wrap;
  }
  .seg {
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    border: none;
    background: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12.5px;
  }
  .seg:hover {
    background: var(--surface-2);
  }
  .seg.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .mini {
    height: 26px;
    font-size: 12px;
    padding: 0 4px;
    width: 96px;
  }
  .sortbox {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
  }
  .tree {
    max-height: 34%;
    overflow: auto;
    margin: 8px 6px 0;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg);
    flex: none;
  }
  .row-root {
    height: 26px;
    border-radius: 6px;
    display: flex;
  }
  .row-root.sel {
    background: var(--accent-soft);
  }
  .row-root .name {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    padding: 0 6px;
  }
  .row-root .name span:first-of-type {
    flex: 1;
    text-align: left;
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
  }
  .crumbs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    padding: 8px 12px 0;
    font-size: 12.5px;
  }
  .crumbs button {
    background: none;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    padding: 0 2px;
  }
  .crumbs button:hover {
    color: var(--accent);
  }
  .sep {
    color: var(--text-3);
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-3);
    cursor: pointer;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    font-size: 12px;
    color: var(--text-3);
    min-height: 34px;
  }
  .status input[type='range'] {
    width: 90px;
    accent-color: var(--accent);
  }
  .grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    margin: 0 4px 0 10px;
    padding-right: 6px;
    position: relative;
  }
  .spacer {
    position: relative;
  }
  .tile {
    position: absolute;
    left: 0;
    top: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface);
    cursor: pointer;
    contain: strict;
  }
  .tile.ph {
    background: var(--surface);
    opacity: 0.5;
  }
  .tile:hover {
    border-color: var(--accent);
  }
  .tile.sel {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  .tile.sel img {
    transform: scale(0.92);
    border-radius: 6px;
  }
  .tile img {
    transition: transform 0.12s;
  }
  .tile img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .nm {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 14px 6px 4px;
    font-size: 11px;
    text-align: left;
    color: #fff;
    background: linear-gradient(transparent, #000c);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .tile:hover .nm {
    opacity: 1;
  }
  .badge {
    position: absolute;
    top: 5px;
    right: 5px;
    font-size: 10.5px;
    padding: 0 5px;
    border-radius: 4px;
    background: #000b;
    color: #fff;
  }
  .badge.warn {
    top: auto;
    bottom: 5px;
    right: 5px;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .none {
    padding: 30px 10px;
    text-align: center;
  }
  .hint {
    font-size: 11px;
    padding: 6px 12px 8px;
    border-top: 1px solid var(--line);
  }
  :global(.spin) {
    display: inline-grid;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
