<script lang="ts">
  import { ArrowLeftRight, House, LayoutDashboard, Minimize2, Settings } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { isTauri } from './lib/api/backend';
  import { maybeRunAutotest } from './lib/autotest';
  import Toasts from './lib/components/ui/Toasts.svelte';
  import { editor } from './lib/editor/editor.svelte';
  import { newQuickCollage, openProjectPath } from './lib/editor/session.svelte';
  import { app, type View } from './lib/stores/app.svelte';
  import { compressQueue, convertQueue } from './lib/stores/batch.svelte';
  import { library } from './lib/stores/library.svelte';
  import CompressView from './lib/views/CompressView.svelte';
  import ConvertView from './lib/views/ConvertView.svelte';
  import EditorView from './lib/views/EditorView.svelte';
  import HomeView from './lib/views/HomeView.svelte';
  import SettingsView from './lib/views/SettingsView.svelte';

  let editorView: EditorView | undefined = $state();
  let dropping = $state(false);

  const nav: { id: View; label: string; icon: typeof House }[] = [
    { id: 'home', label: 'Home', icon: House },
    { id: 'editor', label: 'Collage', icon: LayoutDashboard },
    { id: 'convert', label: 'Convert', icon: ArrowLeftRight },
    { id: 'compress', label: 'Compress', icon: Minimize2 },
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

  onMount(() => {
    void app.init().then(async () => {
      if (!isTauri) return;
      const files = await invoke<string[]>('startup_files').catch(() => [] as string[]);
      if (files.length) await handleDrop(files, -1, -1);
      await maybeRunAutotest();
    });
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const p = event.payload;
        const dpr = window.devicePixelRatio || 1;
        if (p.type === 'enter' || p.type === 'over') {
          dropping = true;
          if (app.view === 'editor') editorView?.dragOverFiles(p.position.x / dpr, p.position.y / dpr);
        } else if (p.type === 'leave') {
          dropping = false;
        } else if (p.type === 'drop') {
          dropping = false;
          void handleDrop(p.paths, p.position.x / dpr, p.position.y / dpr);
        }
      });
    });
    return () => unlisten?.();
  });
</script>

<svelte:window
  onkeydown={(e) => {
    // A web-style reload would throw away the UI state of a desktop app.
    if (e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r' && app.view !== 'editor')) e.preventDefault();
  }}
/>

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
        onclick={() => (app.view = n.id)}
      >
        <n.icon size={20} />
        <span>{n.label}</span>
      </button>
    {/each}
    <span class="spacer"></span>
    <button class="nav-btn" class:on={app.view === 'settings'} title="Settings" onclick={() => (app.view = 'settings')}>
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
      {:else if app.view === 'settings'}<SettingsView />
      {/if}
    {/if}
  </main>

  {#if dropping}
    <div class="dropzone">
      <div class="dz-inner">
        Drop to {app.view === 'convert' ? 'convert' : app.view === 'compress' ? 'compress' : app.view === 'editor' ? 'add to your collage' : 'start a quick collage'}
      </div>
    </div>
  {/if}
</div>

<Toasts />

<style>
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
</style>
