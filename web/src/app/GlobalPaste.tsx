import { useEffect } from 'react';
import { useAppDialogs } from '../components/shared/AppDialogs';
import type { BuildSession } from '../hooks/useBuildSession';
import { t, type Lang } from '../lib/i18n';
import { looksLikeBuild } from './pasteImport';
import { isTyping } from './shortcuts';

/** Paste a PoB code or a share link anywhere outside a text field and the app offers to import it. */
export function GlobalPaste({ session, lang, onImported }: { session: BuildSession; lang: Lang; onImported: () => void }) {
  const dialogs = useAppDialogs(lang);
  useEffect(() => {
    const onPaste = async (event: ClipboardEvent) => {
      if (isTyping(event.target)) return;
      const text = event.clipboardData?.getData('text') ?? '';
      if (!looksLikeBuild(text)) return;
      event.preventDefault();
      if (session.busy) return;
      if (!await dialogs.confirm(t(lang, 'import.confirmPastedBuild'))) return;
      if (await session.importCode(text.trim(), { destination: 'newBuild' })) onImported();
    };
    document.addEventListener('paste', onPaste);
    return () => document.removeEventListener('paste', onPaste);
  }, [session, dialogs, lang, onImported]);
  return null;
}
