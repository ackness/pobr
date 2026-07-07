import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { ConfigInputValue, ConfigOption, EnemyTier } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, configSectionLabel, enemyTierLabel, type Lang } from '../../lib/i18n';
import { CONFIG_LABEL_ZH, LIST_OPTION_ZH } from '../../lib/configLabels';
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

/** list 型 default（1-based index）→ 选项值。 */
function listDefault(option: ConfigOption): string | undefined {
  const def = option.default;
  if (def && typeof def === 'object' && 'index' in def && def.index) {
    return option.list_options?.[def.index - 1]?.value;
  }
  return undefined;
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
  const label = optionLabel(lang, option);
  return (
    <div className={`config-item${overridden ? ' is-overridden' : ''}`} title={option.var}>
      <span className="config-key">{label}</span>
      {option.input_type === 'check' ? (
        <input
          type="checkbox"
          checked={value === true}
          disabled={busy}
          onChange={(e) => onChange(e.target.checked)}
          aria-label={label}
        />
      ) : option.input_type === 'list' ? (
        <select
          value={String(value ?? listDefault(option) ?? '')}
          disabled={busy}
          onChange={(e) => onChange(e.target.value)}
          aria-label={label}
        >
          {(option.list_options ?? []).map((opt) => {
            const en = stripColorCodes(opt.label);
            const zh = lang !== 'en-US' ? (LIST_OPTION_ZH[en] ?? listLabels[en] ?? en) : en;
            return (
              <option key={opt.value} value={opt.value}>
                {zh}
              </option>
            );
          })}
        </select>
      ) : (
        <input
          className="config-value"
          type={option.input_type === 'text' ? 'text' : 'number'}
          value={value === undefined ? '' : String(value)}
          placeholder={
            typeof option.default === 'number' || typeof option.default === 'string'
              ? String(option.default)
              : ''
          }
          disabled={busy}
          aria-label={label}
          onChange={(e) => {
            const raw = e.target.value;
            if (option.input_type === 'text') {
              onChange(raw);
            } else if (raw.trim() !== '' && Number.isFinite(Number(raw))) {
              onChange(Number(raw));
            }
          }}
        />
      )}
      {overridden && (
        <button className="config-reset" title="reset" onClick={onReset}>
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
  const [query, setQuery] = useState('');
  const [openSections, setOpenSections] = useState<Set<string>>(new Set(['General']));
  // 词条文本型 list 选项（如任务奖励 "+5 to all Attributes"）的反查翻译缓存。
  const [listLabels, setListLabels] = useState<Record<string, string>>({});

  useEffect(() => {
    getBackend()
      .then((b) => b.loadConfigOptions())
      .then(setOptions)
      .catch(() => setOptions([]));
  }, []);

  useEffect(() => {
    if (lang === 'en-US' || options.length === 0) return;
    const pending = [
      ...new Set(
        options
          .flatMap((o) => o.list_options ?? [])
          .map((o) => stripColorCodes(o.label))
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
          if (translated[i] !== en) map[en] = translated[i];
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

  const sections = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = options.filter(
      (o) =>
        q === '' ||
        (o.label ?? '').toLowerCase().includes(q) ||
        o.var.toLowerCase().includes(q) ||
        (CONFIG_LABEL_ZH[o.var] ?? '').includes(query.trim()),
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
  }, [options, query]);

  const searching = query.trim() !== '';

  // build 自带但不在目录里的键（导入 build 的自定义/未映射 Input）→ 高级区可见。
  const extraKeys = useMemo(() => {
    const known = new Set(options.map((o) => o.var));
    return [...new Set([...Object.keys(buildInputs), ...Object.keys(overrides)])]
      .filter((k) => !known.has(k))
      .sort();
  }, [options, buildInputs, overrides]);

  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('true');
  const parseValue = (raw: string): ConfigInputValue => {
    if (raw === 'true') return true;
    if (raw === 'false') return false;
    const n = Number(raw);
    return Number.isFinite(n) && raw.trim() !== '' ? n : raw;
  };

  return (
    <section aria-labelledby="config-heading">
      <h2 id="config-heading" className="panel-heading">
        {tt('config.title')}
      </h2>

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
          placeholder={tt('config.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={tt('config.search')}
        />
      </div>

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
              <span>{configSectionLabel(lang, name)}</span>
              <span className="config-section-count">{sectionOptions.length}</span>
            </button>
            {open && (
              <div className="config-grid">
                {sectionOptions.map((option) => (
                  <OptionRow
                    key={option.var}
                    option={option}
                    value={effective(option.var)}
                    overridden={option.var in overrides}
                    busy={session.busy}
                    lang={lang}
                    listLabels={listLabels}
                    onChange={(value) => session.setConfigInput(option.var, value)}
                    onReset={() => session.setConfigInput(option.var, null)}
                  />
                ))}
              </div>
            )}
          </section>
        );
      })}

      <h3 className="panel-subheading">{tt('config.addTitle')}</h3>
      <p className="config-hint">{tt('config.hint')}</p>
      {extraKeys.length > 0 && (
        <div className="config-grid">
          {extraKeys.map((key) => (
            <div key={key} className={`config-item${key in overrides ? ' is-overridden' : ''}`}>
              <span className="config-key">{key}</span>
              <input
                className="config-value"
                defaultValue={String(effective(key) ?? '')}
                disabled={session.busy}
                aria-label={key}
                onBlur={(e) => {
                  const parsed = parseValue(e.target.value);
                  if (parsed !== effective(key)) session.setConfigInput(key, parsed);
                }}
              />
              {key in overrides && (
                <button className="config-reset" title="reset" onClick={() => session.setConfigInput(key, null)}>
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
    </section>
  );
}
