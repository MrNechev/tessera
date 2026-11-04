export type ToastKind = 'info' | 'success' | 'warning' | 'error';

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  message?: string;
  action?: ToastAction;
  timeout?: number;
}

class Toasts {
  list = $state<Toast[]>([]);
  private next = 1;

  push(t: Omit<Toast, 'id'>): number {
    const id = this.next++;
    this.list = [...this.list, { ...t, id }];
    const timeout = t.timeout ?? (t.kind === 'error' ? 9000 : t.action ? 7000 : 4000);
    if (timeout > 0) setTimeout(() => this.dismiss(id), timeout);
    return id;
  }

  dismiss(id: number): void {
    this.list = this.list.filter((t) => t.id !== id);
  }
}

export const toasts = new Toasts();
