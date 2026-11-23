<script lang="ts">
  import { Activity, ArrowLeftRight, BookOpen, House, Images, LayoutDashboard, Minimize2, Settings, Wand } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { isTauri } from './lib/api/backend';
  import ServerPicker from './lib/components/ServerPicker.svelte';
  import { picker } from './lib/platform/picker.svelte';
  import { sendFiles } from './lib/platform/upload';
  import { toasts } from './lib/stores/toasts.svelte';
  import { maybeRunAutotest } from './lib/autotest';
  import Toasts from './lib/components/ui/Toasts.svelte';
  import { editor } from './lib/editor/editor.svelte';
  import { flushAutosave, newQuickCollage, openProjectPath } from './lib/editor/session.svelte';
  import { app, type View } from './lib/stores/app.svelte';
  import { compressQueue, convertQueue, toolsQueue } from './lib/stores/batch.svelte';
  import { library } from './lib/stores/library.svelte';
  import CompressView from './lib/views/CompressView.svelte';
  import BatchView from './lib/views/BatchView.svelte';
  import ConvertView from './lib/views/ConvertView.svelte';
  import EditorView from './lib/views/EditorView.svelte';
  import HomeView from './lib/views/HomeView.svelte';
  import SettingsView from './lib/views/SettingsView.svelte';
  import LibraryView from './lib/views/LibraryView.svelte';
  import DocumentsView from './lib/views/DocumentsView.svelte';
  import SystemView from './lib/views/SystemView.svelte';
  import { api as docApi, setUnauthorizedHandler } from './lib/documents/lib/api';
  import { router } from './lib/documents/lib/router.svelte';
  import { palette } from './lib/documents/lib/palette.svelte';
  import Login from './lib/documents/components/Login.svelte';
  import Present from './lib/documents/components/Present.svelte';
  import CanvasPresent from './lib/documents/components/CanvasPresent.svelte';
  import Presenter from './lib/documents/components/Presenter.svelte';
  import PrintView from './lib/documents/components/PrintView.svelte';
  import RemoteView from './lib/documents/components/RemoteView.svelte';
  import CommandPalette from './lib/documents/components/CommandPalette.svelte';
  import SetupDialog from './lib/components/SetupDialog.svelte';
  import './lib/documents/documents.css';

  let editorView: EditorView | undefined = $state();
  let dropping = $state(false);
  /** Sign-in state (web app with a password; the desktop app is always signed in). */
  let phase = $state<'loading' | 'login' | 'ready'>('loading');
  let authEnabled = $state(false);
  let started = false;

  /** Presentations, speaker notes, print layouts and the phone remote fill the whole window. */
  const fullscreen = $derived(['present', 'presenter', 'print', 'remote'].includes(router.route.name));

  const nav: { id: View; label: string; icon: typeof House }[] = [
    { id: 'home', label: 'Home', icon: House },
    { id: 'library', label: 'Library', icon: Images },
    { id: 'documents', label: 'Documents', icon: BookOpen },
    { id: 'editor', label: 'Collage', icon: LayoutDashboard },
    { id: 'convert', label: 'Convert', icon: ArrowLeftRight },
    { id: 'compress', label: 'Compress', icon: Minimize2 },
    { id: 'batch', label: 'Batch', icon: Wand },
  ];

  async function handleDrop(paths: string[], x: number, y: number) {
    if (!paths.length) return;
    if (paths.length === 1 && paths[0].endsWith('.tessera')) {
      await openProjectPath(paths[0], true);
      return;
    }
    switch (app.view) {
      case 'convert':
        await convertQueue.add(paths);
        break;
      case 'compress':
        await compressQueue.add(paths);
        break;
      case 'batch':
        await toolsQueue.add(paths);
        break;
      case 'editor': {
        const imgs = await library.import(paths);
        const ids = imgs.map((i) => i.id);
        if (ids.length && !editorView?.dropFiles(ids, x, y)) {
          if (editor.hasLayout || !editor.doc.elements.length) editor.addToLayout(ids);
          else editor.addFreePhotos(ids);
        }
        break;
      }
      default:
        await newQuickCollage(paths);
    }
  }

  function go(view: View) {
    app.view = view;
    if (view === 'system') router.go('/system');
    else if (router.route.name === 'system') router.go('/');
  }

  // Document links (#/doc/…) and the System page open their views.
  $effect(() => {
    const r = router.route.name;
    if (r === 'doc') app.view = 'documents';
    else if (r === 'system') app.view = 'system';
  });

  async function checkSession() {
    try {
      const s = await docApi.session();
      authEnabled = s.authEnabled;
      phase = s.authenticated ? 'ready' : 'login';
    } catch {
      phase = 'login';
    }
    if (phase === 'ready') start();
  }

  function start() {
    if (started) return;
    started = true;
    void app.init().then(async () => {
      if (!isTauri) return;
      const { invoke } = await import('@tauri-apps/api/core');
      const files = await invoke<string[]>('startup_files').catch(() => [] as string[]);
      if (files.length) await handleDrop(files, -1, -1);
      await maybeRunAutotest();
    });
  }

  /** Browser: files dropped from the user's computer are matched or uploaded first. */
  async function handleWebDrop(e: DragEvent) {
    // The documents and library views handle their own drops.
    if (e.defaultPrevented || !['home', 'editor', 'convert', 'compress', 'batch'].includes(app.view)) {
      dropping = false;
      return;
    }
    e.preventDefault();
    dropping = false;
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (!files.length) return;
    const x = e.clientX;
    const y = e.clientY;
    const id = toasts.push({ kind: 'info', title: `Sending ${files.length} file${files.length === 1 ? '' : 's'} to Tessera…`, timeout: 0 });
    try {
      const paths = await sendFiles(files, 'temp');
      toasts.dismiss(id);
      await handleDrop(paths, x, y);
    } catch (err) {
      toasts.dismiss(id);
      toasts.push({ kind: 'error', title: 'Upload failed', message: String(err instanceof Error ? err.message : err) });
    }
  }

  onMount(() => {
    setUnauthorizedHandler(() => (phase = 'login'));
    const unauthorized = () => (phase = 'login');
    window.addEventListener('tessera:unauthorized', unauthorized);
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'k' && phase === 'ready' && !fullscreen) {
        e.preventDefault();
        palette.open = !palette.open;
      }
    };
    window.addEventListener('keydown', onKey);
    if (router.route.name !== 'remote') void checkSession();
    if (!isTauri) {
      // Browser: save pending edits when the tab closes.
      const flush = () => void flushAutosave();
      const dragover = (e: DragEvent) => {
        if (e.dataTransfer?.types.includes('Files')) {
          e.preventDefault();
          dropping = true;
        }
      };
      const dragleave = (e: DragEvent) => {
        if (!e.relatedTarget) dropping = false;
      };
      window.addEventListener('pagehide', flush);
      window.addEventListener('dragover', dragover);
      window.addEventListener('dragleave', dragleave);
      window.addEventListener('drop', handleWebDrop);
      return () => {
        window.removeEventListener('tessera:unauthorized', unauthorized);
        window.removeEventListener('keydown', onKey);
        window.removeEventListener('pagehide', flush);
        window.removeEventListener('dragover', dragover);
        window.removeEventListener('dragleave', dragleave);
        window.removeEventListener('drop', handleWebDrop);
      };
    }
    let unlisten: (() => void) | undefined;
    let unlistenClose: (() => void) | undefined;
    // Edits from the last moment before closing are still waiting for the
    // debounced autosave: write them before the window goes away.
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      unlistenClose = await getCurrentWindow().onCloseRequested(async () => {
        await Promise.race([flushAutosave(), new Promise((r) => setTimeout(r, 5000))]);
      });
    });
    void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const p = event.payload;
        const dpr = window.devicePixelRatio || 1;
        if (p.type === 'enter' || p.type === 'over') {
          dropping = !['documents', 'library', 'system'].includes(app.view);
          if (app.view === 'editor') editorView?.dragOverFiles(p.position.x / dpr, p.position.y / dpr);
        } else if (p.type === 'leave') {
          dropping = false;
        } else if (p.type === 'drop') {
          dropping = false;
          if (app.view === 'documents') {
            // The document editor matches the paths against the libraries.
            window.dispatchEvent(new CustomEvent('tessera:paths-drop', { detail: { paths: p.paths, x: p.position.x / dpr, y: p.position.y / dpr } }));
          } else if (app.view !== 'library' && app.view !== 'system') {
            void handleDrop(p.paths, p.position.x / dpr, p.position.y / dpr);
          }
        }
      });
    });
    return () => {
      window.removeEventListener('tessera:unauthorized', unauthorized);
      window.removeEventListener('keydown', onKey);
      unlisten?.();
      unlistenClose?.();
    };
  });
</script>

<svelte:window
  onkeydown={(e) => {
    // A web-style reload would throw away the UI state of a desktop app.
    if (isTauri && (e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r' && app.view !== 'editor'))) e.preventDefault();
  }}
/>

{#if router.route.name === 'remote'}
  <!-- the phone remote has its own (limited) session -->
  <div class="lt full"><RemoteView id={router.route.id} /></div>
{:else if phase === 'loading'}
  <div class="splash"></div>
{:else if phase === 'login'}
  <div class="lt full"><Login onok={() => { phase = 'ready'; start(); }} /></div>
{:else if fullscreen}
  <div class="lt full">
    {#if router.route.name === 'present' && router.route.canvas}
      <CanvasPresent id={router.route.id} start={router.route.start} />
    {:else if router.route.name === 'present'}
      <Present id={router.route.id} start={router.route.start} />
    {:else if router.route.name === 'presenter'}
      <Presenter id={router.route.id} canvas={router.route.canvas} />
    {:else if router.route.name === 'print'}
      <PrintView id={router.route.id} mode={router.route.mode} />
    {/if}
  </div>
{:else}
<div class="app">
  <nav class="nav" aria-label="Main">
    <div class="logo" title="Tessera">
      <svg viewBox="0 0 32 32" aria-hidden="true">
        <rect x="3" y="3" width="13" height="15" rx="3" fill="#ff5d8f" />
        <rect x="18" y="3" width="11" height="8" rx="3" fill="#ffc145" />
        <rect x="18" y="13" width="11" height="7" rx="3" fill="#2dd4bf" />
        <rect x="3" y="20" width="9" height="9" rx="3" fill="#4f8cff" />
        <rect x="14" y="22" width="15" height="7" rx="3" fill="#8b5cf6" />
      </svg>
    </div>
    {#each nav as n (n.id)}
      <button
        class="nav-btn"
        class:on={app.view === n.id}
        title={n.label}
        disabled={n.id === 'editor' && !editor.open}
        onclick={() => go(n.id)}
      >
        <n.icon size={20} />
        <span>{n.label}</span>
      </button>
    {/each}
    <span class="spacer"></span>
    <button class="nav-btn" class:on={app.view === 'system'} title="System status" onclick={() => go('system')}>
      <Activity size={20} />
      <span>System</span>
    </button>
    <button class="nav-btn" class:on={app.view === 'settings'} title="Settings" onclick={() => go('settings')}>
      <Settings size={20} />
      <span>Settings</span>
    </button>
  </nav>

  <main class="view">
    {#if app.ready}
      {#if editor.open}
        <div class="keep" class:hidden={app.view !== 'editor'}><EditorView bind:this={editorView} /></div>
      {/if}
      {#if app.view === 'home'}<HomeView />
      {:else if app.view === 'convert'}<ConvertView />
      {:else if app.view === 'compress'}<CompressView />
      {:else if app.view === 'batch'}<BatchView />
      {:else if app.view === 'settings'}<SettingsView />
      {:else if app.view === 'library'}<LibraryView />
      {:else if app.view === 'documents'}<DocumentsView {authEnabled} />
      {:else if app.view === 'system'}<SystemView />
      {/if}
    {/if}
  </main>

  {#if dropping}
    <div class="dropzone">
      <div class="dz-inner">
        Drop to {app.view === 'convert' ? 'convert' : app.view === 'compress' ? 'compress' : app.view === 'batch' ? 'process' : app.view === 'editor' ? 'add to your collage' : 'start a quick collage'}
      </div>
    </div>
  {/if}
</div>

{#if library.importing && library.importing.total > 1}
  {@const imp = library.importing}
  <div class="import-progress" role="status">
    Importing photos… {imp.done} / {imp.total}
    <span class="bar"><span style:width="{(100 * imp.done) / Math.max(1, imp.total)}%"></span></span>
  </div>
{/if}

{#if palette.open}<div class="lt"><CommandPalette /></div>{/if}
{#if app.ready && isTauri && !app.settings.setupDone}<SetupDialog onclose={() => undefined} />{/if}
{/if}

<Toasts />
{#if picker.current}<ServerPicker />{/if}

<style>
  .full {
    height: 100%;
    overflow: auto;
    background: var(--bg);
  }
  .splash {
    height: 100%;
    background: var(--bg);
  }
  .app {
    display: flex;
    height: 100%;
  }
  .nav {
    width: var(--rail);
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 12px 0;
    background: var(--bg);
    border-right: 1px solid var(--border);
  }
  .logo {
    width: 36px;
    height: 36px;
    margin-bottom: 14px;
  }
  .logo svg {
    width: 100%;
    height: 100%;
  }
  .nav-btn {
    width: 58px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 9px 0 7px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--text-3);
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .nav-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--hover);
  }
  .nav-btn.on {
    color: var(--text);
    background: var(--panel-3);
  }
  .nav-btn.on :global(svg) {
    color: var(--accent);
  }
  .nav-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .view {
    flex: 1;
    min-width: 0;
    position: relative;
  }
  .keep {
    position: absolute;
    inset: 0;
  }
  .keep.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .dropzone {
    position: fixed;
    inset: 0;
    z-index: 150;
    pointer-events: none;
    background: rgba(255, 93, 143, 0.08);
    border: 3px dashed var(--accent);
    display: grid;
    place-items: end center;
  }
  .dz-inner {
    margin-bottom: 40px;
    padding: 12px 22px;
    border-radius: 99px;
    background: var(--accent);
    color: #fff;
    font-weight: 700;
    box-shadow: var(--shadow);
  }
  .import-progress {
    position: fixed;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    z-index: 90;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    border-radius: 999px;
    background: var(--panel-3);
    border: 1px solid var(--border-strong);
    color: var(--text);
    box-shadow: var(--shadow);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .import-progress .bar {
    width: 120px;
    height: 4px;
    border-radius: 2px;
    background: rgba(127, 127, 127, 0.3);
    overflow: hidden;
  }
  .import-progress .bar span {
    display: block;
    height: 100%;
    background: var(--accent-grad);
    transition: width 0.15s;
  }
</style>
