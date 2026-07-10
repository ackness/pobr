import { useState } from 'react';
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
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      title={tt('common.copy')}
      onClick={async (e) => {
        e.stopPropagation();
        try {
          await navigator.clipboard.writeText(text);
          setDone(true);
          setTimeout(() => setDone(false), 1200);
        } catch {
          // 剪贴板不可用（非安全上下文/权限拒绝）时静默——复制是增强，不阻断。
        }
      }}
    >
      {done ? tt('common.copied') : (label ?? tt('common.copy'))}
    </button>
  );
}
