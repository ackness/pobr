import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import { stripReferenceMarkup } from '../../lib/diagnosticText';

/** Only complete identifiers or complete engine templates are mapped, never word fragments. */
export function modifierText(raw: string, translated: string | undefined, lang: Lang): { text: string; missing: boolean; internal: boolean } {
  const clean = stripReferenceMarkup(raw);
  const internal = /^[A-Za-z][A-Za-z0-9_.:]*$/.test(clean);
  if (internal) {
    const label = statNameLabel(lang, clean);
    return { text: label === clean ? bindT(lang)('calcs.missingModifier') : label, missing: label === clean, internal: true };
  }
  const generated = /^([+-]?\d+(?:\.\d+)?)% to (FireResist|ColdResist|LightningResist|ChaosResist)$/.exec(clean);
  if (generated) return { text: `${statNameLabel(lang, generated[2])}${lang === 'en-US' ? ': ' : '：'}${generated[1]}%`, missing: false, internal: false };
  const text = translated || clean;
  return { text, missing: lang !== 'en-US' && text === clean && /[A-Za-z]{3}/.test(clean), internal: false };
}

export function ModifierText({ raw, translated, lang }: { raw: string; translated?: string; lang: Lang }) {
  const tt = bindT(lang);
  const display = modifierText(raw, translated, lang);
  return <><span>{display.text}</span>{display.missing && !display.internal && <small className="modifier-fallback">{tt('calcs.untranslated')}</small>}
    <details className="modifier-raw"><summary>{tt('calcs.details')}</summary><pre>{raw}</pre></details></>;
}
