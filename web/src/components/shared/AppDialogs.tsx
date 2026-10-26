import { createContext, useContext, useId, useLayoutEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from 'react';
import { AlertDialog as AlertPrimitive, Dialog as DialogPrimitive } from 'radix-ui';
import { AlertDialog, AlertDialogTitle, AlertDialogDescription, AlertDialogCancel, AlertDialogAction, AlertDialogOverlay, AlertDialogPortal } from '../ui/alert-dialog';
import { Dialog, DialogTitle, DialogDescription, DialogOverlay, DialogPortal } from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import type { Lang } from '../../lib/i18n';
import { useUi } from './UiContext';
import { DialogRequests, type DialogRequest } from './dialogRequests';
import './appDialogs.css';

const DialogContext = createContext<DialogRequests | null>(null);
const labels = {
  'en-US': { confirm: 'Confirm action', prompt: 'Enter a name', cancel: 'Cancel', accept: 'Confirm', save: 'Save' },
  'zh-CN': { confirm: '确认操作', prompt: '输入名称', cancel: '取消', accept: '确认', save: '保存' },
  'zh-TW': { confirm: '確認操作', prompt: '輸入名稱', cancel: '取消', accept: '確認', save: '儲存' },
};

export function AppDialogsProvider({ children, scope }: { children: ReactNode; scope?: string }) {
  const requests = useMemo(() => new DialogRequests(), [scope]);
  const request = useSyncExternalStore(requests.subscribe, requests.snapshot, () => null);
  useLayoutEffect(() => { requests.activate(); return () => requests.dispose(); }, [requests]);
  return <DialogContext.Provider value={requests}>
    {children}
    {request && <RequestDialog key={`${scope}:${request.kind}:${request.message}`} request={request} requests={requests} />}
  </DialogContext.Provider>;
}

export function useAppDialogs(lang: Lang): {
  confirm: (message: string) => Promise<boolean>;
  prompt: (message: string, defaultValue?: string) => Promise<string | null>;
} {
  const requests = useContext(DialogContext);
  return useMemo(() => ({
    confirm: async (message: string) => await requests?.open('confirm', message, lang) === true,
    prompt: async (message: string, defaultValue = '') => {
      const value = await requests?.open('prompt', message, lang, defaultValue);
      return typeof value === 'string' ? value : null;
    },
  }), [requests, lang]);
}

function RequestDialog({ request, requests }: { request: DialogRequest; requests: DialogRequests }) {
  const { v2 } = useUi();
  const text = labels[request.lang];
  const [value, setValue] = useState(request.defaultValue);
  const inputId = useId();
  const style = v2 ? 'app-dialog v2-dialog' : 'app-dialog app-dialog-legacy';
  const cancel = () => requests.finish(request, request.kind === 'confirm' ? false : null);
  const restoreFocus = (event: Event) => { event.preventDefault(); if (request.trigger?.isConnected) request.trigger.focus(); };
  if (request.kind === 'confirm') return <AlertDialog open onOpenChange={open => { if (!open) cancel(); }}>
    <AlertDialogPortal>
      <AlertDialogOverlay className="app-dialog-overlay" />
      <AlertPrimitive.Content className={style} onCloseAutoFocus={restoreFocus}>
        <AlertDialogTitle>{text.confirm}</AlertDialogTitle>
        <AlertDialogDescription>{request.message}</AlertDialogDescription>
        <div className="app-dialog-actions">
          <AlertDialogCancel onClick={cancel}>{text.cancel}</AlertDialogCancel>
          <AlertDialogAction onClick={() => requests.finish(request, true)}>{text.accept}</AlertDialogAction>
        </div>
      </AlertPrimitive.Content>
    </AlertDialogPortal>
  </AlertDialog>;
  return <Dialog open onOpenChange={open => { if (!open) cancel(); }}>
    <DialogPortal>
      <DialogOverlay className="app-dialog-overlay" />
      <DialogPrimitive.Content className={style} onCloseAutoFocus={restoreFocus}
        onOpenAutoFocus={event => { event.preventDefault(); const input = document.getElementById(inputId) as HTMLInputElement | null; input?.focus(); input?.select(); }}>
        <DialogTitle>{text.prompt}</DialogTitle>
        <DialogDescription>{request.message}</DialogDescription>
        <form onSubmit={event => { event.preventDefault(); requests.finish(request, value); }}>
          <Input id={inputId} aria-label={request.message} value={value} onChange={event => setValue(event.target.value)} autoComplete="off" />
          <div className="app-dialog-actions">
            <Button type="button" variant="outline" onClick={cancel}>{text.cancel}</Button>
            <Button type="submit">{text.save}</Button>
          </div>
        </form>
      </DialogPrimitive.Content>
    </DialogPortal>
  </Dialog>;
}
