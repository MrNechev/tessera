<script lang="ts">
  import { api } from '../lib/api';

  let { onok }: { onok: () => void } = $props();
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      await api.login(password);
      onok();
    } catch (err) {
      error = (err as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <form class="card" onsubmit={submit}>
    <div class="brand">
      <img src="/tessera.svg" alt="" width="40" height="40" />
      <div>
        <h1>Tessera</h1>
        <p class="muted">collages, documents and presentations from your photos</p>
      </div>
    </div>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="input" type="password" placeholder="Password" bind:value={password} autofocus autocomplete="current-password" />
    {#if error}<p class="err">{error}</p>{/if}
    <button class="btn primary" disabled={busy || !password}>{busy ? 'Signing in…' : 'Sign in'}</button>
  </form>
</div>

<style>
  .wrap {
    min-height: 100%;
    display: grid;
    place-items: center;
    padding: 24px;
    background:
      radial-gradient(1200px 600px at 20% -10%, #e8b04b14, transparent 60%),
      radial-gradient(900px 500px at 110% 110%, #7aa2f714, transparent 60%),
      var(--bg);
  }
  .card {
    width: min(360px, 100%);
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 28px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
  }
  .brand {
    display: flex;
    gap: 14px;
    align-items: center;
    margin-bottom: 6px;
  }
  h1 {
    margin: 0;
    font: 800 26px/1.1 var(--ui);
    letter-spacing: -0.02em;
  }
  p {
    margin: 2px 0 0;
  }
  .input,
  .btn {
    height: 40px;
  }
  .btn {
    justify-content: center;
  }
  .err {
    color: var(--red);
    margin: 0;
  }
</style>
