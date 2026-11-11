import { Download } from 'lucide-react';
import { Button } from '../components/ui/button';
import { t, type Lang } from '../lib/i18n';

export function download(text: string, name: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const link = document.createElement('a'); link.href = url; link.download = name; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function BackupDownload({ lang, exportWorkspace }: { lang: Lang; exportWorkspace: () => string }) {
  return <Button onClick={() => download(exportWorkspace(), 'pobr-backup.json')}><Download data-icon="inline-start" />{t(lang, 'shell.downloadBackup')}</Button>;
}
