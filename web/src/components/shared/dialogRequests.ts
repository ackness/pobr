import type { Lang } from '../../lib/i18n';

export interface DialogRequest {
  id: number;
  kind: 'confirm' | 'prompt';
  message: string;
  defaultValue: string;
  acceptLabel?: string;
  lang: Lang;
  trigger: HTMLElement | null;
  resolve: (value: string | boolean | null) => void;
}

/** One scope owns its requests. Disposing it can never approve a pending action. */
export class DialogRequests {
  private requests: DialogRequest[] = [];
  private listeners = new Set<() => void>();
  private disposed = false;
  private nextId = 0;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };
  snapshot = () => this.requests[0] ?? null;
  private emit() { this.listeners.forEach(listener => listener()); }
  open(kind: DialogRequest['kind'], message: string, lang: Lang, defaultValue = '', acceptLabel?: string, returnFocus?: HTMLElement | null) {
    if (this.disposed) return Promise.resolve(kind === 'confirm' ? false : null);
    return new Promise<string | boolean | null>(resolve => {
      this.requests.push({ id: this.nextId++, kind, message, lang, defaultValue, acceptLabel, resolve,
        trigger: returnFocus ?? (typeof document !== 'undefined' && document.activeElement instanceof HTMLElement ? document.activeElement : null) });
      this.emit();
    });
  }
  finish(request: DialogRequest, value: string | boolean | null) {
    if (request !== this.snapshot()) return;
    this.requests.shift();
    request.resolve(value);
    this.emit();
  }
  dispose() {
    this.disposed = true;
    const pending = this.requests.splice(0);
    pending.forEach(request => request.resolve(request.kind === 'confirm' ? false : null));
    this.emit();
  }
  activate() { this.disposed = false; }
}

/** The caller may provide a persistent trigger when a menu item opens a dialog. */
export function restoreDialogFocus(request: DialogRequest, event: Pick<Event, 'preventDefault'>) {
  event.preventDefault();
  if (request.trigger?.isConnected) request.trigger.focus();
}
