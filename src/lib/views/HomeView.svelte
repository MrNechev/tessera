<script lang="ts">
  import { ArrowLeftRight, BookOpen, CalendarDays, FolderOpen, Hand, LayoutTemplate, Minimize2, Sparkles, Trash2, Wand } from '@lucide/svelte';
  import NewProjectDialog from '../components/NewProjectDialog.svelte';
  import { onMount } from 'svelte';
  import { api, fileUrl, isTauri } from '../api/backend';
  import type { ProjectSummary } from '../api/types';
  import TemplatesPanel from '../components/editor/panels/TemplatesPanel.svelte';
  import TemplateThumb from '../components/editor/TemplateThumb.svelte';
  import Modal from '../components/ui/Modal.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import { editor } from '../editor/editor.svelte';
  import { newBlank, newFromTemplate, newQuickCollage, openProjectDialog, openProjectPath } from '../editor/session.svelte';
  import { app } from '../stores/app.svelte';
  import { library } from '../stores/library.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { templateById } from '../templates/catalog';
  import type { TemplateDef } from '../templates/types';

  let projects = $state<ProjectSummary[]>([]);
  let gallery = $state(false);
  let chosen = $state<TemplateDef | null>(null);
  let useLibrary = $state(true);
  let newProject = $state<'book' | 'calendar' | null>(null);

  const featured = ['letters-love', 'cal-photo-month', 'love-be-mine', 'xmas-merry', 'letters-big-birthday', 'baby-first-year']
    .map((id) => templateById(id))
    .filter((t): t is TemplateDef => !!t);

  async function refresh() {
    if (!isTauri) return;
    try {
      projects = await api.listProjects();
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not list projects', message: String(e) });
    }
  }

  async function remove(p: ProjectSummary) {
    await api.deleteProject(p.id);
    await refresh();
    toasts.push({ kind: 'info', title: `Deleted “${p.name}”` });
  }

  function pickTemplate(t: TemplateDef) {
    gallery = false;
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
      <p>Collages for every moment — quick and automatic, or hand-crafted down to the last sticker.</p>
    </div>
    <div class="actions">
      <button class="action a1" onclick={() => void newQuickCollage()}>
        <span class="ico"><Sparkles size={22} /></span>
        <span class="t">Quick collage</span>
        <span class="d">Pick photos, get a perfect layout. Regenerate until you love it.</span>
      </button>
      <button class="action a2" onclick={() => (gallery = true)}>
        <span class="ico"><LayoutTemplate size={22} /></span>
        <span class="t">From a template</span>
        <span class="d">Valentine’s, Christmas, birthdays, weddings, travel & more.</span>
      </button>
      <button class="action a3" onclick={newBlank}>
        <span class="ico"><Hand size={22} /></span>
        <span class="t">Blank canvas</span>
        <span class="d">Free-form design with photos, text, stickers and shapes.</span>
      </button>
    </div>
    <div class="projects">
      <button class="project" onclick={() => (newProject = 'book')}>
        <BookOpen size={20} />
        <span><b>Photo book</b><span class="hint">Every photo on its own pages — cover, layouts, page numbers. Print it or save a PDF.</span></span>
      </button>
      <button class="project" onclick={() => (newProject = 'calendar')}>
        <CalendarDays size={20} />
        <span><b>Photo calendar</b><span class="hint">A cover and twelve months with your photos. Mark birthdays and holidays.</span></span>
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
      <h2>Made for the moment</h2>
      <button class="btn ghost" onclick={() => (gallery = true)}>All templates →</button>
    </div>
    <div class="featured">
      {#each featured as t (t.id)}
        <button class="feat" onclick={() => pickTemplate(t)}>
          <TemplateThumb template={t} width={200} />
          <span>{t.name}</span>
        </button>
      {/each}
    </div>
  </section>

  <section>
    <div class="sec-head">
      <h2>Recent collages</h2>
    </div>
    {#if projects.length}
      <div class="recent">
        {#each projects as p (p.id)}
          <div class="proj card" role="button" tabindex="0" onclick={() => void openProjectPath(p.path, false)} onkeydown={(e) => e.key === 'Enter' && void openProjectPath(p.path, false)}>
            <div class="pthumb">
              {#if p.thumb}<img src={fileUrl(p.thumb)} alt="" loading="lazy" />{/if}
            </div>
            <div class="pinfo">
              <span class="truncate"><b>{p.name}</b></span>
              <span class="hint">{ago(p.modified)} · {p.width}×{p.height}</span>
            </div>
            <button class="icon-btn small del" title="Delete" onclick={(e) => { e.stopPropagation(); void remove(p); }}><Trash2 size={14} /></button>
          </div>
        {/each}
      </div>
    {:else}
      <p class="hint">Your collages appear here automatically as you work on them.</p>
    {/if}
  </section>
</div>

{#if gallery}
  <Modal title="Templates" width={1100} onclose={() => (gallery = false)}>
    <TemplatesPanel wide onpick={pickTemplate} />
  </Modal>
{/if}

{#if newProject}
  <NewProjectDialog kind={newProject} onclose={() => (newProject = null)} />
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
    grid-template-columns: repeat(3, 1fr);
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
  .t {
    font-size: 16px;
    font-weight: 700;
  }
  .d {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .projects {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
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
  .pthumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    box-shadow: var(--shadow-sm);
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
