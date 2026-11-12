import { t, type Lang } from '../lib/i18n';

export function DataVersionNotice({ savedVersion, currentVersion, lang }: {
  savedVersion?: string; currentVersion: string | null; lang: Lang;
}) {
  if (!savedVersion || !currentVersion || savedVersion === currentVersion) return null;
  return <p className="shell-notice" role="status">{t(lang, 'shell.dataVersionChanged')
    .replace('{saved}', savedVersion).replace('{current}', currentVersion)}</p>;
}
