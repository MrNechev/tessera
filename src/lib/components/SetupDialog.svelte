<script lang="ts">
  /** Desktop, first start: which folders to show in the Library, and where to save results. */
  import Modal from './ui/Modal.svelte';
  import LibrariesEditor from './LibrariesEditor.svelte';
  import { pickFolder } from '../api/backend';
  import type { LibrarySetting } from '../api/types';
  import { app } from '../stores/app.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let libraries = $state<LibrarySetting[]>(app.settings.libraries.length ? app.settings.libraries : suggested());
  let saveDirectory = $state(app.settings.saveDirectory);

  function suggested(): LibrarySetting[] {
    // ~/Pictures (from the default save folder ~/Pictures/Tessera), read-only.
    const pics = app.settings.saveDirectory.replace(/\/Tessera\/?$/, '');
    return pics && pics !== app.settings.saveDirectory ? [{ id: 'pictures', name: 'Pictures', path: pics, writable: false }] : [];
  }

  async function chooseSave() {
    const dir = await pickFolder('Where should Tessera save what it makes?', saveDirectory, true);
    if (dir) saveDirectory = dir;
  }

  async function done() {
    await app.saveSettings({ libraries: $state.snapshot(libraries), saveDirectory, setupDone: true });
    onclose();
  }
</script>

<Modal title="Welcome to Tessera" width={760} onclose={() => void done()}>
  <div class="setup">
    <section>
      <h4>Where are your photos and videos?</h4>
      <p class="hint">
        These folders appear in the <b>Library</b>, where you can browse and search them and use them in collages and documents.
        Tessera only reads them unless you switch a folder to <b>Read-write</b> (needed only if the image tools should save into it or replace originals).
      </p>
      <LibrariesEditor value={libraries} onchange={(v) => (libraries = v)} />
    </section>
    <section>
      <h4>Where should Tessera save what it makes?</h4>
      <p class="hint">Collages, converted and compressed photos go here by default. It also appears in the Library as <b>Outputs</b>.</p>
      <div class="row">
        <span class="path truncate" title={saveDirectory}>{saveDirectory}</span>
        <button class="btn small" onclick={() => void chooseSave()}>Change…</button>
      </div>
    </section>
    <p class="hint">You can change all of this later in Settings → Libraries.</p>
  </div>
  {#snippet footer()}
    <button class="btn primary" onclick={() => void done()}>Done</button>
  {/snippet}
</Modal>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h4 {
    margin: 0 0 4px;
    font-size: 14px;
  }
  .hint {
    color: var(--text-2);
    margin: 0 0 10px;
    line-height: 1.5;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .path {
    flex: 1;
    color: var(--text-2);
  }
</style>
