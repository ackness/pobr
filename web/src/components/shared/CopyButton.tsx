import { useEffect, useRef, useState } from 'react';
import { bindT, type Lang } from '../../lib/i18n';

interface Props {
  text: string;
  lang: Lang;
  /** 可选文案覆盖（默认「复制」）。 */
  label?: string;
}

/** 一键把原始文本写入剪贴板（复制物品词条等）；成功后短暂显示「已复制」。 */
export function CopyButton({ text, lang, label }: Props) {
  const tt = bindT(lang);
  const [status, setStatus] = useState<'idle' | 'done' | 'error'>('idle');
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    setStatus('idle');
    return () => clearTimeout(timer.current);
  }, [text]);
  return (
    <span className="copy-control" onClick={event => event.stopPropagation()}>
      <button
        type="button"
        title={label ?? tt('common.copy')}
        aria-live="polite"
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
        {status === 'done' ? tt('common.copied') : (label ?? tt('common.copy'))}
      </button>
      {status === 'error' && <span className="copy-fallback">
        <span role="alert">{tt('common.copyFailed')}</span>
        <textarea readOnly value={text} rows={3} aria-label={tt('common.manualCopy')}
          onFocus={event => event.target.select()} />
      </span>}
    </span>
  );
}
