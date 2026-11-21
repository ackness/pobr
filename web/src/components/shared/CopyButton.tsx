import { AppButton, AppTextarea } from './controls';
import { useEffect, useRef, useState } from 'react';
import { bindT, type Lang } from '../../lib/i18n';

interface Props {
  text: string;
  lang: Lang;
  /** Optional label override (defaults to "Copy"). */
  label?: string;
  /** Icon-sized quiet button (label is used only for aria/title attributes). */
  compact?: boolean;
}

/** One-click clipboard copy for raw text (item mods, etc.); briefly shows "Copied" on success. */
export function CopyButton({ text, lang, label, compact }: Props) {
  const tt = bindT(lang);
  const [status, setStatus] = useState<'idle' | 'done' | 'error'>('idle');
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    setStatus('idle');
    return () => clearTimeout(timer.current);
  }, [text]);
  return (
    <span className={`copy-control${compact ? ' copy-control-compact' : ''}`} onClick={event => event.stopPropagation()}>
      <AppButton
        type="button"
        title={label ?? tt('common.copy')}
        aria-live={compact ? undefined : 'polite'}
        className={compact ? 'copy-compact-button' : undefined}
        onClick={async (e) => {
          e.stopPropagation();
          clearTimeout(timer.current);
          try {
            await navigator.clipboard.writeText(text);
            setStatus('done');
            timer.current = setTimeout(() => setStatus('idle'), 1200);
          } catch {
            setStatus('error');
          }
        }}
      >
        {compact ? <span aria-hidden>{status === 'done' ? '✓' : '⧉'}</span>
          : status === 'done' ? tt('common.copied') : (label ?? tt('common.copy'))}
      </AppButton>
      {compact && <span className="sr-only" aria-live="polite">{status === 'done' ? tt('common.copied') : ''}</span>}
      {status === 'error' && <span className="copy-fallback">
        <span role="alert">{tt('common.copyFailed')}</span>
        <AppTextarea readOnly value={text} rows={3} aria-label={tt('common.manualCopy')}
          onFocus={event => event.target.select()} />
      </span>}
    </span>
  );
}
