<script lang="ts">
  import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from '@lucide/svelte';
  import { toasts } from '../../stores/toasts.svelte';
  const icons = { info: Info, success: CircleCheck, warning: TriangleAlert, error: CircleAlert };
</script>

<div class="toasts" aria-live="polite">
  {#each toasts.list as t (t.id)}
    {@const Icon = icons[t.kind]}
    <div class="toast card {t.kind}">
      <span class="icon"><Icon size={18} /></span>
      <div class="body">
        <div class="title">{t.title}</div>
        {#if t.message}<div class="msg">{t.message}</div>{/if}
      </div>
      {#if t.action}
        <button
          class="btn small"
          onclick={() => {
            t.action!.run();
            toasts.dismiss(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      <button class="icon-btn small" onclick={() => toasts.dismiss(t.id)} aria-label="Dismiss"><X size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 380px;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 10px 12px 14px;
    box-shadow: var(--shadow);
    animation: fade-in 0.18s ease-out;
    border-left: 3px solid var(--text-3);
  }
  .toast.success {
    border-left-color: var(--success);
  }
  .toast.success .icon {
    color: var(--success);
  }
  .toast.warning {
    border-left-color: var(--warning);
  }
  .toast.warning .icon {
    color: var(--warning);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .toast.error .icon {
    color: var(--danger);
  }
  .icon {
    margin-top: 1px;
    color: var(--text-2);
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .msg {
    color: var(--text-2);
    font-size: 12px;
    margin-top: 2px;
    word-break: break-word;
  }
</style>
