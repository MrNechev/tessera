// The documents/library views report through Tessera's toasts (one stack for the whole app).
import { toasts as appToasts } from '../../stores/toasts.svelte';

export interface Toast {
  id: number;
  text: string;
  kind: 'info' | 'error' | 'ok';
  action?: { label: string; run: () => void };
}

class Toasts {
  show(text: string, kind: Toast['kind'] = 'info', ms = 3500, action?: Toast['action']) {
    appToasts.push({ kind: kind === 'ok' ? 'success' : kind, title: text, action, timeout: ms });
  }
  error(text: string) {
    this.show(text, 'error', 6000);
  }
  dismiss(id: number) {
    appToasts.dismiss(id);
  }
}

export const toasts = new Toasts();
