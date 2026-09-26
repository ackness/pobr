import { PageHeader } from '../shared/PageHeader';
import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { ConfigInputValue, ConfigOption, CustomModifierBlock, EnemyTier } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, configSectionLabel, enemyTierLabel, type Lang } from '../../lib/i18n';
import { CONFIG_LABEL_ZH, LIST_OPTION_ZH } from '../../lib/configLabels';
import { configDefaultValue } from '../../lib/configDefaults';
import './config.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const ENEMY_TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];

/** PoB2 Config 页分区展示顺序（目录里的 section 原名）。 */
const SECTION_ORDER = [
  'General',
  'Quest Rewards',
  'Skill Options',
  'When In Combat',
  'For Effective DPS',
  'Enemy Stats',
];

/** 剥 PoB 颜色码（`^xRRGGBB` / `^N`）。 */
function stripColorCodes(text: string): string {
  return text.replace(/\^x[0-9A-Fa-f]{6}|\^[0-9]/g, '');
}

/** 本地化配置标签：中文界面查汉化表（缺条目回退英文原文）。 */
function optionLabel(lang: Lang, option: ConfigOption): string {
  const en = stripColorCodes(option.label ?? option.var);
  if (lang !== 'en-US') {
    return CONFIG_LABEL_ZH[option.var] ?? en;
  }
  return en;
}

function OptionRow({
  option,
  value,
  overridden,
  busy,
  lang,
  listLabels,
  onChange,
  onReset,
}: {
  option: ConfigOption;
  value: ConfigInputValue | undefined;
  overridden: boolean;
  busy: boolean;
  lang: Lang;
  listLabels: Record<string, string>;
  onChange: (value: ConfigInputValue) => void;
  onReset: () => void;
}) {
  const tt = bindT(lang);
  const label = optionLabel(lang, option);
  const fallback = configDefaultValue(option);
  const effective = value ?? fallback;
  const [draft, setDraft] = useState(String(value ?? ''));
  useEffect(() => setDraft(String(value ?? '')), [value]);
  return (
    <div className={`config-item${overridden ? ' is-overridden' : ''}`} title={option.var}>
      <label className="config-key" htmlFor={`config-${option.var}`}>{label}</label>
      {option.input_type === 'check' ? (
        <input
          id={`config-${option.var}`}
          type="checkbox"
          checked={typeof effective === 'number' ? effective !== 0 : effective === true}
          disabled={busy}
          onChange={(e) => onChange(e.target.checked)}
          aria-label={label}
        />
      ) : option.input_type === 'list' ? (
        <select
          id={`config-${option.var}`}
          value={String(effective ?? '')}
          disabled={busy}
          onChange={(e) => onChange(e.target.value)}
          aria-label={label}
        >
          {(option.list_options ?? []).map((opt) => {
            const en = stripColorCodes(opt.label);
            const zh = lang !== 'en-US'
              ? en.split(/\r?\n/).map(line => {
                const text = line.trim();
                return LIST_OPTION_ZH[text] ?? listLabels[text] ?? text;
              }).join(' / ')
              : en;
            return (
              <option key={opt.value} value={opt.value}>
                {zh}
              </option>
            );
          })}
        </select>
      ) : (
        <input
          id={`config-${option.var}`}
          className="config-value"
          type={option.input_type === 'text' ? 'text' : 'number'}
          value={draft}
          placeholder={
            typeof fallback === 'number' || typeof fallback === 'string'
              ? String(fallback)
              : ''
          }
          disabled={busy}
          aria-label={label}
          onChange={event => setDraft(event.target.value)}
          onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); }}
          onBlur={(e) => {
            const raw = e.target.value;
            if (option.input_type === 'text') {
              if (raw !== String(value ?? '')) onChange(raw);
            } else if (raw.trim() === '') {
              setDraft(String(value ?? ''));
              onReset();
            } else if (raw.trim() !== '' && Number.isFinite(Number(raw))) {
              if (Number(raw) !== value) onChange(Number(raw));
            }
          }}
        />
      )}
      {overridden && (
        <button className="config-reset" disabled={busy} title={tt('config.reset')}
          aria-label={`${tt('config.reset')}: ${label}`} onClick={onReset}>
          ↺
        </button>
      )}
    </div>
  );
}

/** Config 页：敌人档位 + 内置配置目录（分区/搜索）+ build 原始 `<Input>` 高级区。 */
export function ConfigPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [options, setOptions] = useState<ConfigOption[]>([]);
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>('loading');
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [query, setQuery] = useState('');
  const [configuredOnly, setConfiguredOnly] = useState(false);
  const [openSections, setOpenSections] = useState<Set<string>>(new Set(['General', 'Quest Rewards', 'When In Combat']));
  // 词条文本型 list 选项（如任务奖励 "+5 to all Attributes"）的反查翻译缓存。
  const [listLabels, setListLabels] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    setLoadState('loading');
    getBackend()
      .then((b) => b.loadConfigOptions())
      .then(result => {
        if (cancelled) return;
        // The calculation catalog resolves repeated variables with the last definition.
        setOptions([...new Map(result.map(option => [option.var, option])).values()]);
        setLoadState('ready');
      })
      .catch(() => { if (!cancelled) setLoadState('error'); });
    return () => { cancelled = true; };
  }, [loadAttempt]);

  useEffect(() => {
    if (lang === 'en-US' || options.length === 0) return;
    const pending = [
      ...new Set(
        options
          .flatMap((o) => o.list_options ?? [])
          .flatMap((o) => stripColorCodes(o.label).split(/\r?\n/).map(line => line.trim()))
          .filter((l) => !(l in LIST_OPTION_ZH)),
      ),
    ];
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending))
      .then((translated) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        pending.forEach((en, i) => {
          if (translated[i] && translated[i] !== en) map[en] = translated[i];
        });
        setListLabels(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [lang, options]);

  const overrides = session.calcParams.config_inputs;
  const buildInputs = session.build?.config_inputs ?? {};
  const effective = (key: string): ConfigInputValue | undefined =>
    key in overrides ? overrides[key] : buildInputs[key];
  const isOverridden = (key: string) => key in overrides && overrides[key] !== buildInputs[key];
  const resetInput = (key: string) => session.setConfigInput(key, buildInputs[key] ?? null);

  const sections = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = options.filter(
      (o) =>
        o.var !== 'customMods' && o.var !== 'enemyIsBoss' &&
        (!configuredOnly || effective(o.var) !== undefined) && (q === '' ||
        (o.label ?? '').toLowerCase().includes(q) ||
        o.var.toLowerCase().includes(q) ||
        (CONFIG_LABEL_ZH[o.var] ?? '').includes(query.trim())),
    );
    const bySection = new Map<string, ConfigOption[]>();
    for (const option of filtered) {
      const section = option.section ?? 'General';
      if (!bySection.has(section)) bySection.set(section, []);
      bySection.get(section)!.push(option);
    }
    const known = SECTION_ORDER.filter((s) => bySection.has(s));
    const rest = [...bySection.keys()].filter((s) => !SECTION_ORDER.includes(s)).sort();
    return [...known, ...rest].map((name) => ({ name, options: bySection.get(name)! }));
  }, [options, query, configuredOnly, overrides, buildInputs]);

  const searching = query.trim() !== '' || configuredOnly;
  const configuredCount = options.filter(option =>
    option.var !== 'customMods' && option.var !== 'enemyIsBoss' && effective(option.var) !== undefined).length;

  // build 自带但不在目录里的键（导入 build 的自定义/未映射 Input）→ 高级区可见。
  const extraKeys = useMemo(() => {
    const known = new Set(options.map((o) => o.var));
    return [...new Set([...Object.keys(buildInputs), ...Object.keys(overrides)])]
      .filter((k) => k !== 'customMods' && k !== 'enemyIsBoss' && !known.has(k))
      .sort();
  }, [options, buildInputs, overrides]);

  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('true');
  const modifierBlocks: CustomModifierBlock[] = session.calcParams.custom_modifier_blocks ?? [{
    title: 'Default',
    enabled: true,
    text: (session.calcParams.extra_modifiers ?? []).join('\n'),
  }];
  const updateModifierBlocks = (blocks: CustomModifierBlock[]) => {
    session.updateParams({ custom_modifier_blocks: blocks, extra_modifiers: undefined });
  };
  const updateModifierBlock = (index: number, patch: Partial<CustomModifierBlock>) => {
    updateModifierBlocks(modifierBlocks.map((block, currentIndex) =>
      currentIndex === index ? { ...block, ...patch } : block));
  };
  const parseValue = (raw: string): ConfigInputValue => {
    if (raw === 'true') return true;
    if (raw === 'false') return false;
    const n = Number(raw);
    return Number.isFinite(n) && raw.trim() !== '' ? n : raw;
  };

  return (
    <section className="ui-page config-page" aria-labelledby="config-heading">
      <PageHeader id="config-heading" title={tt('config.title')} description={tt('ui.configHint')} />

      <div className="config-toolbar">
        <label className="config-row">
          {tt('config.enemyTier')}
          <select
            value={session.calcParams.enemy_tier ?? 'pinnacle'}
            disabled={session.busy}
            onChange={(e) => session.updateParams({ enemy_tier: e.target.value as EnemyTier })}
          >
            {ENEMY_TIERS.map((tier) => (
              <option key={tier} value={tier}>
                {enemyTierLabel(lang, tier)}
              </option>
            ))}
          </select>
        </label>
        <input
          className="config-search"
          type="search"
          placeholder={tt('config.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={tt('config.search')}
        />
        <label className="config-filter"><input type="checkbox" checked={configuredOnly}
          onChange={event => setConfiguredOnly(event.target.checked)} />{tt('config.configuredOnly')} ({configuredCount})</label>
      </div>
      <p className="config-hint">{tt('config.editHint')}</p>
      <p className="config-hint">{tt('config.defaultsHint')}</p>
      {loadState === 'loading' && <p role="status">{tt('config.loading')}</p>}
      {loadState === 'error' && <div className="calc-error" role="alert">
        {tt('config.loadFailed')} <button onClick={() => setLoadAttempt(value => value + 1)}>{tt('common.retry')}</button>
      </div>}
      {loadState === 'ready' && searching && sections.length === 0 && <div className="search-empty" role="status">
        <p>{tt('common.noResults')}</p>
        <button onClick={() => { setQuery(''); setConfiguredOnly(false); }}>{tt(configuredOnly ? 'config.showAll' : 'common.clearSearch')}</button>
      </div>}

      {sections.map(({ name, options: sectionOptions }) => {
        const open = searching || openSections.has(name);
        return (
          <section key={name} className="config-section">
            <button
              className="config-section-header"
              aria-expanded={open}
              onClick={() => {
                const next = new Set(openSections);
                if (next.has(name)) {
                  next.delete(name);
                } else {
                  next.add(name);
                }
                setOpenSections(next);
              }}
            >
              <span><span className="row-caret" aria-hidden>{open ? '▾' : '▸'}</span> {configSectionLabel(lang, name)}</span>
              <span className="config-section-count">{sectionOptions.length}</span>
            </button>
            {open && (
              <div className="config-grid">
                {sectionOptions.map((option) => (
                  <OptionRow
                    key={option.var}
                    option={option}
                    value={effective(option.var)}
                    overridden={isOverridden(option.var)}
                    busy={session.busy}
                    lang={lang}
                    listLabels={listLabels}
                    onChange={(value) => session.setConfigInput(option.var, value)}
                    onReset={() => resetInput(option.var)}
                  />
                ))}
              </div>
            )}
          </section>
        );
      })}

      <article className="ui-card config-custom">
      <h3 className="section-heading">{tt('config.extraMods')}</h3>
      <p className="config-hint">{tt('config.extraModsHint')}</p>
      <div className="config-modifier-blocks">
        {modifierBlocks.map((block, index) => (
          <div className="config-modifier-block" key={index}>
            <div className="config-modifier-block-header">
              <label className="config-modifier-enabled">
                <input type="checkbox" checked={block.enabled} disabled={session.busy}
                  onChange={event => updateModifierBlock(index, { enabled: event.target.checked })} />
                {tt('config.groupEnabled')}
              </label>
              <label className="config-modifier-title">
                <span>{tt('config.groupTitle')}</span>
                <input type="text" key={block.title} defaultValue={block.title}
                  placeholder={tt('config.groupUntitled')} disabled={session.busy}
                  onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); }}
                  onBlur={event => {
                    if (event.target.value !== block.title) {
                      updateModifierBlock(index, { title: event.target.value });
                    }
                  }} />
              </label>
              <button type="button" disabled={session.busy}
                aria-label={`${tt('config.groupRemove')}: ${block.title || `${index + 1}`}`}
                onClick={() => updateModifierBlocks(modifierBlocks.filter((_, currentIndex) => currentIndex !== index))}>
                {tt('config.groupRemove')}
              </button>
            </div>
            <textarea
              className="config-extra-mods"
              key={block.text}
              rows={4}
              spellCheck={false}
              placeholder={'20% increased Fire Damage\n+50 to maximum Life'}
              defaultValue={block.text}
              disabled={session.busy}
              aria-label={`${tt('config.groupText')}: ${block.title || index + 1}`}
              onBlur={event => {
                if (event.target.value !== block.text) {
                  updateModifierBlock(index, { text: event.target.value });
                }
              }}
            />
          </div>
        ))}
      </div>
      <button type="button" className="config-modifier-add" disabled={session.busy}
        onClick={() => updateModifierBlocks([...modifierBlocks, { title: '', enabled: true, text: '' }])}>
        {tt('config.groupAdd')}
      </button>
      </article>
      <details className="ui-card config-advanced">
      <summary>{tt('config.addTitle')}</summary>
      <p className="config-hint">{tt('config.hint')}</p>
      {extraKeys.length > 0 && (
        <div className="config-grid">
          {extraKeys.map((key) => (
            <div key={key} className={`config-item${isOverridden(key) ? ' is-overridden' : ''}`}>
              <span className="config-key config-key--raw">{key}</span>
              <input
                key={String(effective(key) ?? '')}
                className="config-value"
                defaultValue={String(effective(key) ?? '')}
                disabled={session.busy}
                aria-label={key}
                onBlur={(e) => {
                  const parsed = parseValue(e.target.value);
                  if (parsed !== effective(key)) session.setConfigInput(key, parsed);
                }}
              />
              {isOverridden(key) && (
                <button className="config-reset" disabled={session.busy} title={tt('config.reset')}
                  aria-label={`${tt('config.reset')}: ${key}`} onClick={() => resetInput(key)}>
                  ↺
                </button>
              )}
            </div>
          ))}
        </div>
      )}
      <div className="config-add">
        <input
          placeholder={tt('config.keyPlaceholder')}
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          aria-label={tt('config.key')}
        />
        <input
          placeholder="true / 40 / text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          aria-label={tt('config.valueLabel')}
        />
        <button
          disabled={!newKey.trim() || session.busy}
          onClick={() => {
            session.setConfigInput(newKey.trim(), parseValue(newValue));
            setNewKey('');
          }}
        >
          {tt('config.addButton')}
        </button>
      </div>
      </details>
    </section>
  );
}
