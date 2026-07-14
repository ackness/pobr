import { useEffect, useRef, useState } from 'react';
import { bindT, type Lang } from '../../lib/i18n';

interface Props {
  /** 当前注释文本（空串 = 无注释）。 */
  value: string;
  /** 提交（失焦即存；空文本 = 删除该注释）。 */
  onCommit: (text: string) => void;
  lang: Lang;
}

/** 局部注释编辑器（装备/技能组/珠宝旁通用）：无注释时是一个小按钮，
 *  有注释时展示为引用块；点击进入编辑，失焦自动保存。 */
export function NoteEditor({ value, onCommit, lang }: Props) {
  const tt = bindT(lang);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const ref = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    if (editing) ref.current?.focus();
  }, [editing]);

  if (editing) {
    return (
      <div className="note-editor">
        <textarea
          ref={ref}
          className="note-editor-input"
          rows={3}
          value={draft}
          placeholder={tt('note.placeholder')}
          spellCheck={false}
          aria-label={tt('note.label')}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            setEditing(false);
            if (draft !== value) onCommit(draft);
          }}
        />
      </div>
    );
  }
  if (!value.trim()) {
    return (
      <button
        className="note-add"
        onClick={() => {
          setDraft(value);
          setEditing(true);
        }}
      >
        ✎ {tt('note.add')}
      </button>
    );
  }
  return (
    <blockquote
      className="note-block"
      title={tt('note.editHint')}
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
    >
      <span className="note-block-label">{tt('note.label')}</span>
      {value}
    </blockquote>
  );
}
