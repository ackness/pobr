import { colon } from '../../app/text';
import { AppButton, AppTextarea } from './controls';
import { useEffect, useRef, useState } from 'react';
import { bindT, type Lang } from '../../lib/i18n';

interface Props {
  /** Current annotation text (empty string = none). */
  value: string;
  /** Commit handler (saves on blur; empty string deletes the annotation). */
  onCommit: (text: string) => void;
  lang: Lang;
}

/** Inline annotation editor shared across items, socket groups, and jewels:
 *  renders as a small button when empty or a quote block when set; click to edit, saves on blur. */
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
        <AppTextarea
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
      <AppButton
        className="note-add"
        onClick={() => {
          setDraft(value);
          setEditing(true);
        }}
      >
        ✎ {tt('note.add')}
      </AppButton>
    );
  }
  return (
    <AppButton
      type="button"
      className="note-block"
      title={tt('note.editHint')}
      aria-label={`${tt('note.editHint')}${colon(lang)}${value}`}
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
    >
      <span className="note-block-label">{tt('note.label')}</span>
      {value}
    </AppButton>
  );
}
