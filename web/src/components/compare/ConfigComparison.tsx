import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { getBackend } from '../../api/backend';
import type { ConfigInputValue, ConfigOption, CustomModifierBlock } from '../../api/types';
import { cleanLine, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { isContinuation } from '../../lib/continuedLines';
import { configLabel, stripConfigMarkup } from '../../lib/configLabels';
import { configValueLabel } from '../../lib/configDisplay';
import type { CopySection } from '../../lib/compare';
import { bindT, type Lang } from '../../lib/i18n';

interface Row { key: string; before: unknown; after: unknown; action: CopySection }

/** Translate each block in order, retaining blank separators and covered-line positions. */
function ModifierLines({ text, lang }: { text: string; lang: Lang }) {
  const tt = bindT(lang);
  const lines = useMemo(() => text.split(/\r?\n/).map(line => cleanLine(stripConfigMarkup(line))), [text]);
  const translated = useLocalizedLines(lines, lang);
  return <>{lines.map((source, index) => {
    if (!source || isContinuation(source, translated[index])) return null;
    const shown = cleanLine(stripConfigMarkup(translated[index] ?? source));
    const original = lang !== 'en-US' && shown === source && /[a-z]/i.test(source) && !/\p{Script=Han}/u.test(source);
    return <p key={index} lang={original || lang === 'en-US' ? 'en' : lang}>{shown}
      {original && <span className="cmp-note"> · {tt('common.originalText')}</span>}
    </p>;
  })}</>;
}

export function ConfigComparison({ rows, lang, applyButton }: { rows: Row[]; lang: Lang; applyButton: (section: CopySection) => ReactNode }) {
  const tt = bindT(lang);
  const [loaded, setLoaded] = useState<{ options: ConfigOption[]; error?: boolean }>({ options: [] });
  useEffect(() => {
    let cancelled = false;
    getBackend().then(backend => backend.loadConfigOptions())
      .then(options => { if (!cancelled) setLoaded({ options }); })
      .catch(() => { if (!cancelled) setLoaded({ options: [], error: true }); });
    return () => { cancelled = true; };
  }, []);
  const options = useMemo(() => new Map(loaded.options.map(option => [option.var, option])), [loaded.options]);
  const pending = useMemo(() => [...new Set(rows.flatMap(row => {
    const option = options.get(row.key);
    return [...(option?.list_options ?? []).flatMap(entry => stripConfigMarkup(entry.label).split(/\r?\n/).map(line => line.trim())),
      ...(option?.section === 'Quest Rewards' && option.input_type === 'check' ? stripConfigMarkup(option.tooltip ?? '').split(/\r?\n/).slice(1) : [])];
  }))], [rows, options]);
  const translated = useLocalizedLines(pending, lang);
  const labels = Object.fromEntries(pending.map((text, index) => [text, translated[index] ?? text]));
  const show = (row: Row, value: unknown) => {
    if (row.action.kind === 'modifiers') {
      if (!Array.isArray(value) || !value.length) return tt('compare.notSet');
      // Legacy arrays are consecutive lines of one text, including multi-line descriptions.
      if (value.every((entry): entry is string => typeof entry === 'string')) {
        return <ModifierLines text={value.join('\n')} lang={lang} />;
      }
      return value.map((entry: string | CustomModifierBlock, index) => typeof entry === 'string'
        ? <ModifierLines key={index} text={entry} lang={lang} />
        : <div key={index}><strong>{cleanLine(stripConfigMarkup(entry.title))} · {tt(entry.enabled ? 'compare.enabled' : 'compare.disabled')}</strong><ModifierLines text={entry.text} lang={lang} /></div>);
    }
    return configValueLabel(lang, options.get(row.key), value as ConfigInputValue | undefined, labels);
  };
  return <>
    {loaded.error && <p role="alert" className="cmp-note">{tt('compare.configLoadError')}</p>}
    {rows.map(row => {
      const option = options.get(row.key);
      const known = row.action.kind !== 'config' || !!option;
      const title = row.action.kind === 'config' ? option ? configLabel(lang, {
        ...option, label: stripConfigMarkup(option.label ?? '') || tt('compare.otherSetting'),
      }) : tt('compare.otherSetting') : row.key;
      const rewards = option?.section === 'Quest Rewards' && option.input_type === 'check'
        ? stripConfigMarkup(option.tooltip ?? '').split(/\r?\n/).slice(1).filter(Boolean).map(line => labels[line] ?? line) : [];
      return <article className="compare-difference" key={row.key} data-config-key={row.key}>
        <header><h4>{title}</h4>{applyButton(row.action)}</header>
        {rewards.length > 0 && <p>{rewards.join(' · ')}</p>}
        {known && <div className="compare-pair">{[row.before, row.after].map((value, index) => <div key={index}>
          <strong>{tt(index === 0 ? 'compare.before' : 'compare.after')}</strong><div className="cmp-config-value">{show(row, value)}</div>
        </div>)}</div>}
        <details><summary>{tt('compare.advanced')}</summary><pre>{JSON.stringify({ key: row.key, A: row.before ?? null, B: row.after ?? null }, null, 2)}</pre></details>
      </article>;
    })}
  </>;
}
