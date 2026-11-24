/**
 * Tessera's own commands for the command palette (Ctrl+K): go to any view,
 * start any kind of project, switch the theme. Views add their own commands
 * (the collage editor, the library selection, the open document) through
 * `palette.register` while they are on screen.
 */
import { api as docApi } from './documents/lib/api';
import { palette, type Command } from './documents/lib/palette.svelte';
import { router } from './documents/lib/router.svelte';
import { editor } from './editor/editor.svelte';
import { newBlank, newQuickCollage, openProjectDialog } from './editor/session.svelte';
import { app, type View } from './stores/app.svelte';

export function themeCommand(): Command {
  const dark = document.documentElement.dataset.theme !== 'light';
  return {
    id: 'app:theme',
    group: 'Actions',
    icon: dark ? 'sun' : 'moon',
    title: dark ? 'Switch to the light theme' : 'Switch to the dark theme',
    keywords: 'theme dark light mode appearance',
    run: () => void app.saveSettings({ theme: dark ? 'light' : 'dark' }),
  };
}

export function registerAppCommands(go: (v: View) => void): () => void {
  return palette.register('app', () => {
    const views: [View, string, string, string][] = [
      ['home', 'Home', 'home', 'start recent'],
      ['library', 'Library', 'image', 'photos videos files media'],
      ['documents', 'Documents', 'notes', 'all documents list'],
      ['convert', 'Convert images', 'refresh', 'format jpeg png webp avif heic'],
      ['compress', 'Compress images', 'package', 'smaller size'],
      ['batch', 'Batch: resize, watermark, rename', 'sliders', 'resize watermark rename bulk'],
      ['system', 'System status', 'activity', 'queues gpu cache status'],
      ['settings', 'Settings', 'settings', 'preferences options save folder libraries'],
    ];
    const out: Command[] = views
      .filter(([v]) => v !== app.view)
      .map(([v, title, icon, keywords]) => ({
        id: 'go:' + v,
        group: 'Go to',
        icon,
        title,
        keywords,
        run: () => {
          if (v === 'documents' && router.route.name !== 'doc') router.go('/');
          go(v);
        },
      }));
    if (editor.open && app.view !== 'editor')
      out.unshift({ id: 'go:editor', group: 'Go to', icon: 'layout', title: `Collage: ${editor.doc.name}`, keywords: 'editor continue', run: () => go('editor') });
    out.push(
      { id: 'new:quick', group: 'New', icon: 'grid', title: 'Quick collage', keywords: 'new collage photos automatic', run: () => void newQuickCollage() },
      { id: 'new:blank', group: 'New', icon: 'square', title: 'Blank collage', keywords: 'new collage canvas free', run: () => newBlank() },
      {
        id: 'new:template',
        group: 'New',
        icon: 'layout',
        title: 'Collage from a template…',
        keywords: 'new valentine christmas birthday wedding meme',
        run: () => {
          go('home');
          app.templateGallery = true;
        },
      },
      { id: 'new:book', group: 'New', icon: 'notes', title: 'Photo book…', keywords: 'new book pages album', run: () => void (app.newProject = 'book') },
      { id: 'new:calendar', group: 'New', icon: 'calendar', title: 'Photo calendar…', keywords: 'new calendar months year', run: () => void (app.newProject = 'calendar') },
      {
        id: 'new:doc',
        group: 'New',
        icon: 'file',
        title: 'Document',
        keywords: 'new document story chronicle',
        run: async () => {
          const d = await docApi.createDoc('Untitled');
          router.go(`/doc/${d.id}`);
          go('documents');
        },
      },
      {
        id: 'new:canvas',
        group: 'New',
        icon: 'canvas',
        title: 'Canvas (whiteboard)',
        keywords: 'new canvas board whiteboard',
        run: async () => {
          const d = await docApi.createDoc('Untitled canvas');
          router.go(`/doc/${d.id}/canvas`);
          go('documents');
        },
      },
      { id: 'open:project', group: 'Actions', icon: 'folder', title: 'Open a collage project file…', keywords: 'open .tessera', run: () => void openProjectDialog() },
      themeCommand(),
    );
    return out;
  });
}

/** Show a library folder (optionally searching for a file name) in the Library. */
export function openInLibrary(libraryId: string, path: string, search?: string): void {
  if (palette.openFolder && (app.view === 'library' || app.view === 'documents')) {
    palette.openFolder(libraryId, path, search);
    return;
  }
  palette.pendingFolder = { libraryId, path, search };
  if (router.route.name === 'system') router.go('/');
  app.view = 'library';
}
