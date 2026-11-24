<script lang="ts">
  import { ArrowLeftRight, BookOpen, CalendarDays, FileText, FolderOpen, Hand, Images, LayoutDashboard, LayoutTemplate, Minimize2, PenTool, Presentation, Sparkles, Trash2, Wand } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, fileUrl } from '../api/backend';
  import type { ProjectSummary } from '../api/types';
  import TemplatesPanel from '../components/editor/panels/TemplatesPanel.svelte';
  import TemplateThumb from '../components/editor/TemplateThumb.svelte';
  import Modal from '../components/ui/Modal.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import { editor } from '../editor/editor.svelte';
  import { closeEditor, newBlank, newFromTemplate, newQuickCollage, openProjectDialog, openProjectPath } from '../editor/session.svelte';
  import { app } from '../stores/app.svelte';
  import { library } from '../stores/library.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { templateById } from '../templates/catalog';
  import type { TemplateDef } from '../templates/types';
  import { api as docApi, thumbUrl } from '../documents/lib/api';
  import { resolver } from '../documents/lib/resolver.svelte';
  import { router } from '../documents/lib/router.svelte';
  import { fromTemplate, TEMPLATES as DOC_TEMPLATES, type Template as DocTemplate } from '../documents/lib/templates';
  import type { DocMeta } from '../documents/lib/types';
  import DocIcon from '../documents/components/Icon.svelte';

  let projects = $state<ProjectSummary[]>([]);
  let docs = $state<DocMeta[]>([]);
  let recentKind = $state<'all' | 'collage' | 'doc'>('all');

  type Recent = { kind: 'collage'; at: number; p: ProjectSummary } | { kind: 'doc'; at: number; d: DocMeta };
  const recent = $derived.by(() => {
    const all: Recent[] = [
      ...projects.map((p) => ({ kind: 'collage' as const, at: Date.parse(p.modified), p })),
      ...docs.map((d) => ({ kind: 'doc' as const, at: Date.parse(d.updatedAt), d })),
    ];
    return all.filter((r) => recentKind === 'all' || r.kind === recentKind).sort((a, b) => b.at - a.at).slice(0, 18);
  });

  function docCover(d: DocMeta): string {
    if (!d.coverPath) return '';
    const r = resolver.get({ libraryId: d.coverLibraryId!, path: d.coverPath });
    return r?.found ? thumbUrl(r.derived, 320) : '';
  }

  function openDoc(d: DocMeta) {
    router.go(`/doc/${d.id}`);
    app.view = 'documents';
  }

  async function newDoc(canvas = false) {
    try {
      const d = await docApi.createDoc(canvas ? 'Untitled canvas' : 'Untitled');
      router.go(`/doc/${d.id}${canvas ? '/canvas' : ''}`);
      app.view = 'documents';
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not create the document', message: String(e instanceof Error ? e.message : e) });
    }
  }

  async function newDocFrom(t: DocTemplate) {
    try {
      const d = await docApi.importDoc(fromTemplate(t));
      router.go(`/doc/${d.id}${t.canvas ? '/canvas' : ''}`);
      app.view = 'documents';
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not create the document', message: String(e instanceof Error ? e.message : e) });
    }
  }
  let chosen = $state<TemplateDef | null>(null);
  let useLibrary = $state(true);
  let deleting = $state<ProjectSummary | null>(null);

  const featured = ['letters-love', 'cal-photo-month', 'love-be-mine', 'xmas-merry', 'letters-big-birthday', 'baby-first-year']
    .map((id) => templateById(id))
    .filter((t): t is TemplateDef => !!t);

  async function refresh() {
    try {
      projects = await api.listProjects();
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not list projects', message: String(e) });
    }
    try {
      docs = await docApi.docs();
      resolver.prefetch(docs.filter((d) => d.coverPath).map((d) => ({ libraryId: d.coverLibraryId!, path: d.coverPath! })));
    } catch {
      docs = [];
    }
  }

  async function remove(p: ProjectSummary) {
    deleting = null;
    try {
      // Otherwise the next autosave of the open project would bring it back.
      if (editor.open && editor.doc.id === p.id) await closeEditor();
      await api.deleteProject(p.id);
      toasts.push({ kind: 'info', title: `Deleted “${p.name}”` });
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not delete the project', message: String(e) });
    }
    await refresh();
  }

  function pickTemplate(t: TemplateDef) {
    app.templateGallery = false;
    if (library.items.length) chosen = t;
    else newFromTemplate(t, false);
  }

  function ago(iso: string): string {
    const d = (Date.now() - Date.parse(iso)) / 1000;
    if (d < 60) return 'just now';
    if (d < 3600) return `${Math.floor(d / 60)} min ago`;
    if (d < 86400) return `${Math.floor(d / 3600)} h ago`;
    if (d < 86400 * 7) return `${Math.floor(d / 86400)} d ago`;
    return new Date(iso).toLocaleDateString();
  }

  onMount(() => {
    void refresh();
  });
</script>

<div class="home">
  <section class="hero">
    <div class="intro">
      <h1>Make something <span class="grad">beautiful</span>.</h1>
      <p>Collages, photo books, documents and presentations from your photos — and the tools to polish every image.</p>
    </div>
    <div class="actions">
      <button class="action a1" onclick={() => void newQuickCollage()}>
        <span class="ico"><Sparkles size={22} /></span>
        <span class="t">Quick collage</span>
        <span class="d">Pick photos, get a perfect layout. Regenerate until you love it.</span>
      </button>
      <button class="action a2" onclick={() => (app.templateGallery = true)}>
        <span class="ico"><LayoutTemplate size={22} /></span>
        <span class="t">From a template</span>
        <span class="d">Valentine’s, Christmas, birthdays, memes, weddings, travel & more.</span>
      </button>
      <button class="action a4" onclick={() => void newDoc()}>
        <span class="ico"><FileText size={22} /></span>
        <span class="t">Document</span>
        <span class="d">Tell a story with photos, videos and text. Present it full-screen.</span>
      </button>
      <button class="action a3" onclick={newBlank}>
        <span class="ico"><Hand size={22} /></span>
        <span class="t">Blank design</span>
        <span class="d">Free-form design with photos, text, stickers and shapes.</span>
      </button>
    </div>
    <div class="projects">
      <button class="project" onclick={() => (app.newProject = 'book')}>
        <BookOpen size={20} />
        <span><b>Photo book</b><span class="hint">Cover, layouts, page numbers. Print it or save a PDF.</span></span>
      </button>
      <button class="project" onclick={() => (app.newProject = 'calendar')}>
        <CalendarDays size={20} />
        <span><b>Photo calendar</b><span class="hint">A cover and twelve months. Mark birthdays and holidays.</span></span>
      </button>
      <button class="project" onclick={() => void newDoc(true)}>
        <PenTool size={20} />
        <span><b>Canvas</b><span class="hint">An endless whiteboard of photos, notes and frames to present.</span></span>
      </button>
      <button class="project" onclick={() => (app.view = 'library')}>
        <Images size={20} />
        <span><b>From the library</b><span class="hint">Select photos in the Library, then make a collage or convert them.</span></span>
      </button>
    </div>
    <div class="tools">
      <button class="tool" onclick={() => (app.view = 'convert')}><ArrowLeftRight size={18} /> <span><b>Convert</b> images between formats</span></button>
      <button class="tool" onclick={() => (app.view = 'compress')}><Minimize2 size={18} /> <span><b>Compress</b> images without visible loss</span></button>
      <button class="tool" onclick={() => (app.view = 'batch')}><Wand size={18} /> <span><b>Resize, watermark & rename</b> in bulk</span></button>
      <button class="tool" onclick={() => void openProjectDialog()}><FolderOpen size={18} /> <span><b>Open</b> a project file</span></button>
    </div>
  </section>

  {#if editor.open}
    <section>
      <button class="continue card" onclick={() => (app.view = 'editor')}>
        Continue editing <b>{editor.doc.name}</b> →
      </button>
    </section>
  {/if}

  <section>
    <div class="sec-head">
      <h2 class="inline">Recent</h2>
      <div class="seg" role="tablist" aria-label="Show">
        <button role="tab" aria-selected={recentKind === 'all'} class:on={recentKind === 'all'} onclick={() => (recentKind = 'all')}>All</button>
        <button role="tab" aria-selected={recentKind === 'collage'} class:on={recentKind === 'collage'} onclick={() => (recentKind = 'collage')}>Collages</button>
        <button role="tab" aria-selected={recentKind === 'doc'} class:on={recentKind === 'doc'} onclick={() => (recentKind = 'doc')}>Documents</button>
      </div>
      <button class="btn ghost" onclick={() => { router.go('/'); app.view = 'documents'; }}>All documents →</button>
    </div>
    {#if recent.length}
      <div class="recent">
        {#each recent as r (r.kind + (r.kind === 'collage' ? r.p.id : r.d.id))}
          {#if r.kind === 'collage'}
            {@const p = r.p}
            <div class="proj card" role="button" tabindex="0" onclick={() => void openProjectPath(p.path, false)} onkeydown={(e) => e.key === 'Enter' && void openProjectPath(p.path, false)}>
              <div class="pthumb">
                {#if p.thumb}<img src={fileUrl(p.thumb)} alt="" loading="lazy" />{/if}
                <span class="kind"><LayoutDashboard size={12} /> Collage</span>
              </div>
              <div class="pinfo">
                <span class="truncate"><b>{p.name}</b></span>
                <span class="hint">{ago(p.modified)} · {p.width}×{p.height}</span>
              </div>
              <button class="icon-btn small del" title="Delete" onclick={(e) => { e.stopPropagation(); deleting = p; }}><Trash2 size={14} /></button>
            </div>
          {:else}
            {@const d = r.d}
            {@const c = docCover(d)}
            <div class="proj card" role="button" tabindex="0" onclick={() => openDoc(d)} onkeydown={(e) => e.key === 'Enter' && openDoc(d)}>
              <div class="pthumb doc">
                {#if c}<img class="cover" src={c} alt="" loading="lazy" />{:else}<FileText size={30} />{/if}
                <span class="kind"><FileText size={12} /> Document</span>
              </div>
              <div class="pinfo">
                <span class="truncate"><b>{d.title || 'Untitled'}</b></span>
                <span class="hint">{ago(d.updatedAt)} · {d.sections} section{d.sections === 1 ? '' : 's'}</span>
              </div>
              <button class="icon-btn small del" title="Present" onclick={(e) => { e.stopPropagation(); router.go(`/present/${d.id}`); }}><Presentation size={14} /></button>
            </div>
          {/if}
        {/each}
      </div>
    {:else}
      <p class="hint">Your collages and documents appear here automatically as you work on them.</p>
    {/if}
  </section>

  <section>
    <div class="sec-head">
      <h2>Made for the moment</h2>
      <button class="btn ghost" onclick={() => (app.templateGallery = true)}>All templates →</button>
    </div>
    <div class="featured">
      {#each featured as t (t.id)}
        <button class="feat" onclick={() => pickTemplate(t)}>
          <TemplateThumb template={t} width={200} />
          <span>{t.name}</span>
        </button>
      {/each}
    </div>
    <div class="doc-tpls">
      {#each DOC_TEMPLATES as t (t.id)}
        <button class="project" onclick={() => void newDocFrom(t)}>
          <DocIcon name={t.icon} size={20} />
          <span><b>{t.name}</b><span class="hint">{t.description}</span></span>
        </button>
      {/each}
    </div>
  </section>
</div>

{#if app.templateGallery}
  <Modal title="Templates" width={1100} onclose={() => (app.templateGallery = false)}>
    <TemplatesPanel wide onpick={pickTemplate} />
  </Modal>
{/if}

{#if deleting}
  {@const p = deleting}
  <Modal title="Delete “{p.name}”?" width={420} onclose={() => (deleting = null)}>
    <p class="hint">The project is removed from Recent. Photos and saved images are not touched.</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (deleting = null)}>Cancel</button>
      <button class="btn danger" onclick={() => void remove(p)}>Delete</button>
    {/snippet}
  </Modal>
{/if}

{#if chosen}
  {@const t = chosen}
  <Modal title="Start from “{t.name}”" width={420} onclose={() => (chosen = null)}>
    <Toggle checked={useLibrary} label="Fill it with my imported photos" hint="{library.items.length} photos in your library" onchange={(v) => (useLibrary = v)} />
    {#snippet footer()}
      <button class="btn" onclick={() => (chosen = null)}>Cancel</button>
      <button class="btn primary" onclick={() => { newFromTemplate(t, useLibrary); chosen = null; }}>Create</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .home {
    height: 100%;
    overflow-y: auto;
    padding: 36px 44px 60px;
    display: flex;
    flex-direction: column;
    gap: 34px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  h1 {
    font-size: 38px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }
  .grad {
    background: var(--accent-grad);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .intro p {
    margin: 8px 0 0;
    color: var(--text-2);
    font-size: 15px;
  }
  .actions {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 22px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--panel);
    cursor: pointer;
    text-align: left;
    transition: transform 0.15s, box-shadow 0.15s, border-color 0.15s;
  }
  .action:hover {
    transform: translateY(-3px);
    box-shadow: var(--shadow);
    border-color: var(--border-strong);
  }
  .ico {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border-radius: 14px;
    color: #fff;
    margin-bottom: 8px;
  }
  .a1 .ico {
    background: linear-gradient(135deg, #ff5d8f, #ff8a5c);
  }
  .a2 .ico {
    background: linear-gradient(135deg, #8b5cf6, #c084fc);
  }
  .a3 .ico {
    background: linear-gradient(135deg, #14b8a6, #5eead4);
  }
  .a4 .ico {
    background: linear-gradient(135deg, #4f8cff, #7aa2f7);
  }
  .t {
    font-size: 16px;
    font-weight: 700;
  }
  .d {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .projects,
  .doc-tpls {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
  }
  .doc-tpls {
    margin-top: 18px;
  }
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    margin-right: auto;
    margin-left: 14px;
    border-radius: 9px;
    background: var(--panel-2);
    border: 1px solid var(--border);
  }
  .seg button {
    padding: 4px 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--panel-3);
    color: var(--text);
  }
  @media (max-width: 1100px) {
    .actions,
    .projects,
    .doc-tpls {
      grid-template-columns: repeat(2, 1fr);
    }
  }
  .project {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--accent);
    cursor: pointer;
    text-align: left;
  }
  .project:hover {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-sm);
  }
  .project > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--text);
  }
  .tools {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-radius: 99px;
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--text-2);
    cursor: pointer;
  }
  .tool:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .continue {
    width: 100%;
    padding: 14px 18px;
    text-align: left;
    cursor: pointer;
    color: var(--text-2);
  }
  .sec-head {
    display: flex;
    align-items: center;
    margin-bottom: 14px;
  }
  .sec-head h2 {
    flex: 1;
    font-size: 18px;
  }
  .sec-head h2.inline {
    flex: none;
  }
  .featured {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 16px;
    align-items: start;
  }
  .feat {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
    font-weight: 600;
    text-align: left;
  }
  .feat:hover :global(.thumb) {
    box-shadow: 0 0 0 2px var(--accent), var(--shadow);
  }
  .recent {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 16px;
  }
  .proj {
    position: relative;
    overflow: hidden;
    cursor: pointer;
    transition: transform 0.15s;
  }
  .proj:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow);
  }
  .pthumb {
    aspect-ratio: 4 / 3;
    background: var(--panel-3);
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .pthumb {
    position: relative;
    color: var(--text-3);
  }
  .pthumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    box-shadow: var(--shadow-sm);
  }
  .pthumb img.cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
    box-shadow: none;
  }
  .kind {
    position: absolute;
    left: 8px;
    bottom: 8px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 99px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
  }
  .pinfo {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
  }
  .del {
    position: absolute;
    top: 6px;
    right: 6px;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    display: none;
  }
  .proj:hover .del {
    display: inline-flex;
  }
</style>
