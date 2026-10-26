import { AppButton, AppInput, AppSelectControl, AppTextarea } from '../shared/controls';
import { PartyEditor } from './PartyEditor';
import { NamedSets } from '../shared/NamedSets';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { ConfigInputValue, ConfigOption, CustomModifierBlock, EnemyTier } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSkillName } from '../../hooks/useSkillName';
import { bindT, configSectionLabel, damageTypeLabel, enemyTierLabel, t, type Lang } from '../../lib/i18n';
import { CONFIG_LABEL_ZH, configLabel, configListOptionLabel, configTooltip } from '../../lib/configLabels';
import { configDefaultValue } from '../../lib/configDefaults';
import { configApplicability, configExistsInGame, configVisibilityInputs, showConfigOption, type Applicability } from '../../lib/configVisibility';
import { useUi } from '../shared/UiContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/tabs';
import { workflowLabel } from './workflowLabels';
import { RawConfigValue } from './RawConfigValue';
import './config.css';
import '../../v2/config-workflow.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const ENEMY_TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];

// How `unsupported_modifiers` marks a line of the configuration (a custom
// modifier or a quest reward) the calculation could not apply.
const UNAPPLIED_PREFIX = 'Config: ';

const QUICK_OPTIONS = [
  { key: 'conditionEnemyShocked', damageType: null },
  { key: 'conditionShockEffect', damageType: null },
  { key: 'enemyPhysicalReduction', damageType: 'Physical' },
  { key: 'enemyFireResist', damageType: 'Fire' },
  { key: 'enemyColdResist', damageType: 'Cold' },
  { key: 'enemyLightningResist', damageType: 'Lightning' },
  { key: 'enemyChaosResist', damageType: 'Chaos' },
] as const;

// These catalog count effects target either the player or a nested MinionModifier.
// Keep this display distinction bounded; other controls retain their catalog labels.
const CHARGE_ACTORS: Record<string, 'config.player' | 'config.minions'> = {
  overridePowerCharges: 'config.player',
  overrideFrenzyCharges: 'config.player',
  overrideEnduranceCharges: 'config.player',
  minionsOverridePowerCharges: 'config.minions',
  minionsOverrideFrenzyCharges: 'config.minions',
  minionsOverrideEnduranceCharges: 'config.minions',
};

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
  const label = configLabel(lang, option);
  const actor = Object.hasOwn(CHARGE_ACTORS, option.var) ? CHARGE_ACTORS[option.var] : undefined;
  return actor ? `${t(lang, actor)}: ${label}` : label;
}

export function OptionRow({
  option,
  value,
  overridden,
  busy,
  lang,
  listLabels,
  applicability,
  onChange,
  onReset,
}: {
  option: ConfigOption;
  value: ConfigInputValue | undefined;
  overridden: boolean;
  busy: boolean;
  lang: Lang;
  listLabels: Record<string, string>;
  applicability: Applicability;
  onChange: (value: ConfigInputValue) => void;
  onReset: () => void;
}) {
  const tt = bindT(lang);
  const { v2 } = useUi();
  const label = optionLabel(lang, option);
  const tooltip = configTooltip(lang, option.var, option.tooltip);
  const fallback = configDefaultValue(option);
  const effective = value ?? fallback;
  const [draft, setDraft] = useState(String(value ?? ''));
  useEffect(() => setDraft(String(value ?? '')), [value]);
  return (
    <div className={`config-item${overridden ? ' is-overridden' : ''}`} data-config-var={option.var}>
      <div className="config-description">
        <label className="config-key" htmlFor={`config-${option.var}`}>{label}</label>
        {v2 ? <details className="config-option-help">
          <summary>{workflowLabel(lang, 'help')}</summary>
          {tooltip && <p className="config-help" id={`config-help-${option.var}`}>{stripColorCodes(tooltip)}</p>}
          {tooltip && lang !== 'en-US' && tooltip === option.tooltip && <p className="config-source">{workflowLabel(lang, 'sourceHelp')}</p>}
          <span className="config-source">{workflowLabel(lang, 'rawKey')}</span>
          <code className="config-raw-key">{option.var}</code>
        </details> : <>
          {tooltip && <p className="config-help" id={`config-help-${option.var}`}>{stripColorCodes(tooltip)}</p>}
          <code className="config-raw-key">{option.var}</code>
        </>}
        <span className="config-source">{tt(value === undefined ? 'config.sourceDefault' : 'config.sourceSaved')}</span>
        {v2 && value !== undefined && <span className="config-source config-saved-type">
          {workflowLabel(lang, typeof value as 'boolean' | 'number' | 'string')}: <code>{JSON.stringify(value)}</code>
        </span>}
        {applicability !== 'applicable' && <span className="config-applicability">
          {tt(applicability === 'unknown' ? 'config.applicabilityUnknown' : 'config.notApplicable')}
          {value !== undefined && <> · {tt('config.savedRetained')}</>}
        </span>}
      </div>
      {option.input_type === 'check' ? (
        <AppInput
          id={`config-${option.var}`}
          aria-describedby={tooltip ? `config-help-${option.var}` : undefined}
          type="checkbox"
          checked={typeof effective === 'number' ? effective !== 0 : effective === true}
          disabled={busy}
          onChange={(e) => onChange(e.target.checked)}
          aria-label={label}
        />
      ) : option.input_type === 'list' ? (
        <AppSelectControl
          className={v2 ? "w-full" : undefined}
          id={`config-${option.var}`}
          aria-describedby={tooltip ? `config-help-${option.var}` : undefined}
          value={String(effective ?? '')}
          disabled={busy}
          onChange={(e) => onChange(e.target.value)}
          aria-label={label}
        >
          {!(option.list_options ?? []).some(opt => String(opt.value) === String(effective ?? '')) &&
            <option value={String(effective ?? '')}>{effective === undefined ? workflowLabel(lang, 'unset') : String(effective)}</option>}
          {(option.list_options ?? []).map((opt) => {
            const en = stripColorCodes(opt.label);
            const zh = lang !== 'en-US'
              ? en.split(/\r?\n/).map(line => {
                const text = line.trim();
                return configListOptionLabel(lang, text) ?? listLabels[text] ?? text;
              }).join(' / ')
              : en;
            return (
              <option key={opt.value} value={opt.value}>
                {zh}
              </option>
            );
          })}
        </AppSelectControl>
      ) : (
        <AppInput
          id={`config-${option.var}`}
          aria-describedby={tooltip ? `config-help-${option.var}` : undefined}
          className="config-value"
          type={option.input_type === 'text' || typeof value === 'string' || typeof value === 'boolean' ? 'text' : 'number'}
          inputMode={option.input_type === 'text' ? undefined : 'decimal'}
          step="any"
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
            if (raw === String(value ?? '')) return;
            if (option.input_type === 'text') {
              onChange(raw);
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
        <AppButton className="config-reset" disabled={busy} title={tt('config.reset')}
          aria-label={`${tt('config.reset')}: ${label}`} onClick={onReset}>
          ↺
        </AppButton>
      )}
    </div>
  );
}

/** Config 页：敌人档位 + 内置配置目录（分区/搜索）+ build 原始 `<Input>` 高级区。 */
export function ConfigPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const { v2 } = useUi();
  const [view, setView] = useState('options');
  const skillName = useSkillName(lang);
  const [focusTarget, setFocusTarget] = useState<string | null>(null);
  const [options, setOptions] = useState<ConfigOption[]>([]);
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>('loading');
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [query, setQuery] = useState('');
  const [configuredOnly, setConfiguredOnly] = useState(false);
  const [allOptions, setAllOptions] = useState(false);
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
    setListLabels({});
    if (lang === 'en-US' || options.length === 0) return;
    const pending = [
      ...new Set(
        options
          .flatMap((o) => o.list_options ?? [])
          .flatMap((o) => stripColorCodes(o.label).split(/\r?\n/).map(line => line.trim()))
          .filter((l) => !configListOptionLabel(lang, l)),
      ),
    ];
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending, lang))
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

  // The session already materializes imported config. Missing keys are unset;
  // source XML is only the Reset baseline, never a fallback for current values.
  const inputs = session.calcParams.config_inputs;
  const buildInputs = session.build?.config_inputs ?? {};
  const effective = (key: string): ConfigInputValue | undefined => inputs[key];
  const isOverridden = (key: string) => inputs[key] !== buildInputs[key];
  const resetInput = (key: string) => session.setConfigInput(key, buildInputs[key] ?? null);
  const visibilityInputs = useMemo(() => configVisibilityInputs(options, inputs), [options, inputs]);
  const context = !session.busy && !session.calcError ? session.calc?.config_context : null;

  // Previous output remains visible while the baseline recalculates or has failed.
  const calculatedMain = !session.busy && !session.calcError ? session.calc?.main_skill : null;
  const quickOptions = QUICK_OPTIONS.flatMap(({ key, damageType }) => {
    if (damageType && !calculatedMain?.hit_damage.some(part =>
      part.damage_type === damageType && Number.isFinite(part.avg) && part.avg > 0)) return [];
    const option = options.find(candidate => candidate.var === key);
    return option ? [{ option, damageType }] : [];
  });
  const openSetting = (option: ConfigOption) => {
    setQuery('');
    setConfiguredOnly(false);
    setAllOptions(true);
    setOpenSections(previous => new Set([...previous, option.section ?? 'General']));
    setFocusTarget(option.var);
  };

  const sections = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = options.filter(
      (o) =>
        o.var !== 'customMods' && o.var !== 'enemyIsBoss' &&
        configExistsInGame(o, session.dataVersion) &&
        showConfigOption(o, configApplicability(o, context, visibilityInputs), inputs, allOptions) &&
        (!configuredOnly || effective(o.var) !== undefined) && (q === '' ||
        (o.label ?? '').toLowerCase().includes(q) ||
        o.var.toLowerCase().includes(q) ||
        optionLabel(lang, o).toLowerCase().includes(q) ||
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
  }, [options, query, configuredOnly, lang, context, inputs, visibilityInputs, allOptions, session.dataVersion]);

  useEffect(() => {
    if (!focusTarget || session.busy) return;
    const control = document.getElementById(`config-${focusTarget}`);
    if (control instanceof HTMLElement) {
      control.focus({ preventScroll: true });
      control.scrollIntoView({ block: 'center' });
    }
    setFocusTarget(null);
  }, [focusTarget, sections, session.busy]);

  const searching = query.trim() !== '' || configuredOnly;
  const configuredCount = options.filter(option =>
    option.var !== 'customMods' && option.var !== 'enemyIsBoss' &&
    configExistsInGame(option, session.dataVersion) && effective(option.var) !== undefined).length;

  // Unknown and game-excluded inputs remain editable, including cleared source
  // keys so their original values can still be restored with Reset.
  const extraKeys = useMemo(() => {
    const known = new Set(options.filter(o => configExistsInGame(o, session.dataVersion)).map((o) => o.var));
    return [...new Set([...Object.keys(buildInputs), ...Object.keys(inputs)])]
      .filter((k) => k !== 'customMods' && k !== 'enemyIsBoss' && !known.has(k))
      .sort();
  }, [options, buildInputs, inputs, session.dataVersion]);

  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('true');
  const [newTypedValue, setNewTypedValue] = useState<ConfigInputValue | null>(true);
  const modifierBlocks: CustomModifierBlock[] = session.calcParams.custom_modifier_blocks ?? [{
    title: 'Default',
    enabled: true,
    text: (session.calcParams.extra_modifiers ?? []).join('\n'),
  }];
  const unappliedLines = (session.calc?.unsupported_modifiers ?? [])
    .filter(line => line.startsWith(UNAPPLIED_PREFIX))
    .map(line => line.slice(UNAPPLIED_PREFIX.length));
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

  const enemyControl = <label className="config-row">
          {tt('config.enemyTier')}
          <AppSelectControl
            value={session.calcParams.enemy_tier ?? 'pinnacle'}
            disabled={session.busy}
            onChange={(e) => session.updateParams({ enemy_tier: e.target.value as EnemyTier })}
          >
            {ENEMY_TIERS.map((tier) => (
              <option key={tier} value={tier}>
                {enemyTierLabel(lang, tier)}
              </option>
            ))}
          </AppSelectControl>
        </label>;

  const quickAccess = (
      <article className="ui-card config-quick-access" aria-labelledby="config-quick-heading">
        <h3 id="config-quick-heading" className="section-heading">{tt('config.quickTitle')}</h3>
        <p className="config-hint">{tt('config.quickHint')}</p>
        <p className="config-quick-context" role="status">
          {session.busy ? tt('config.quickUpdating')
            : calculatedMain ? <>{tt('config.quickSkill')}: <span className="config-quick-skill">{skillName(calculatedMain.skill_id)}</span></>
            : tt('config.quickUnavailable')}
        </p>
        <p className="config-hint">{tt('config.quickScope')}</p>
        <ul className="config-quick-list">
          {quickOptions.map(({ option, damageType }) => {
            const raw = effective(option.var);
            const value = raw ?? configDefaultValue(option);
            const source = raw === undefined ? tt('config.quickDefault')
              : isOverridden(option.var) || buildInputs[option.var] === undefined
                ? tt('config.quickConfigured') : tt('config.quickImported');
            const type = typeof value === 'boolean' ? tt('config.quickBoolean')
              : typeof value === 'number' ? tt('config.quickNumber') : tt('config.quickString');
            const label = optionLabel(lang, option);
            return (
              <li key={option.var} className="config-quick-entry" data-config-var={option.var}>
                <div className="config-quick-description">
                  <strong>{label}</strong>
                  <p className="config-quick-value">
                    {source}: <output aria-label={`${tt('config.valueLabel')}: ${label}`}>
                      {value === undefined ? tt('config.quickUnset')
                        : <><span>{type}</span> <code>{JSON.stringify(value)}</code></>}
                    </output>
                  </p>
                  <p className="config-hint">
                    {damageType
                      ? `${tt('config.quickHitReason')}: ${damageTypeLabel(lang, damageType)}. ${tt('config.quickMitigationReason')}`
                      : tt(option.var === 'conditionShockEffect' ? 'config.quickShockMagnitude' : 'config.quickShockState')}
                  </p>
                </div>
                <AppButton type="button" disabled={session.busy || loadState !== 'ready'}
                  aria-label={`${tt('config.quickOpen')}: ${label}`} onClick={() => openSetting(option)}>
                  {tt('config.quickOpen')}
                </AppButton>
              </li>
            );
          })}
        </ul>
      </article>
  );

  const catalog = (<>
      <div className="config-toolbar">
        {!v2 && enemyControl}
        <AppInput
          className="config-search"
          type="search"
          placeholder={tt('config.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={tt('config.search')}
        />
        <label className="config-filter"><AppInput type="checkbox" checked={configuredOnly}
          onChange={event => setConfiguredOnly(event.target.checked)} />{tt('config.configuredOnly')} ({configuredCount})</label>
      </div>
      <div className="config-view" role="group" aria-label={tt('config.view')}>
        <AppButton className={v2 ? "aria-pressed:bg-accent aria-pressed:text-accent-foreground" : undefined} type="button" aria-pressed={!allOptions} onClick={() => setAllOptions(false)}>{tt('config.applicable')}</AppButton>
        <AppButton className={v2 ? "aria-pressed:bg-accent aria-pressed:text-accent-foreground" : undefined} type="button" aria-pressed={allOptions} onClick={() => setAllOptions(true)}>{tt('config.showAll')}</AppButton>
      </div>
      {v2 ? <details className="config-guidance"><summary>{workflowLabel(lang, 'guidance')}</summary>
      <p className="config-hint">{tt('config.visibilityHint')}</p>
      <p className="config-hint">{tt('config.editHint')}</p>
      <p className="config-hint">{tt('config.defaultsHint')}</p>
      </details> : <>
      <p className="config-hint">{tt('config.visibilityHint')}</p>
      <p className="config-hint">{tt('config.editHint')}</p>
      <p className="config-hint">{tt('config.defaultsHint')}</p>
      </>}
      {loadState === 'loading' && <p role="status">{tt('config.loading')}</p>}
      {loadState === 'error' && <div className="calc-error" role="alert">
        {tt('config.loadFailed')} <AppButton onClick={() => setLoadAttempt(value => value + 1)}>{tt('common.retry')}</AppButton>
      </div>}
    {v2 ? <details className="config-quick-disclosure"><summary>{tt('config.quickTitle')}</summary>{quickAccess}</details> : quickAccess}
    <div className={v2 ? 'config-options-workspace' : undefined}>
      {v2 && sections.length > 0 && <nav className="config-group-nav" aria-label={workflowLabel(lang, 'jump')}>
        <h3>{workflowLabel(lang, 'jump')}</h3>
        {sections.map(section => <AppButton className="h-auto min-h-10 justify-between whitespace-normal text-left" key={section.name} onClick={() => {
          setOpenSections(previous => new Set([...previous, section.name]));
          requestAnimationFrame(() => {
            const group = document.getElementById(`config-section-${section.name.replace(/\s+/g, '-')}`);
            group?.scrollIntoView({ block: 'start' });
            group?.querySelector<HTMLButtonElement>('.config-section-header')?.focus({ preventScroll: true });
          });
        }}><span>{configSectionLabel(lang, section.name)}</span><span>{section.options.length}</span></AppButton>)}
      </nav>}
      <div className="config-option-groups">
      {loadState === 'ready' && searching && sections.length === 0 && <div className="search-empty" role="status">
        <p>{tt('common.noResults')}</p>
        <AppButton onClick={() => { setQuery(''); setConfiguredOnly(false); setAllOptions(true); }}>{tt('config.showAll')}</AppButton>
      </div>}

      {sections.map(({ name, options: sectionOptions }) => {
        const open = searching || openSections.has(name);
        return (
          <section key={name} id={`config-section-${name.replace(/\s+/g, '-')}`} className="config-section">
            <AppButton
              className={v2 ? "config-section-header h-auto min-h-12 w-full justify-between whitespace-normal rounded-none border-0 bg-muted text-left" : "config-section-header"}
              aria-controls={`config-group-${name.replace(/\s+/g, '-')}`}
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
            </AppButton>
            {open && (
              <div className="config-grid" id={`config-group-${name.replace(/\s+/g, '-')}`}>
                {sectionOptions.map((option) => (
                  <OptionRow
                    key={option.var}
                    option={option}
                    value={effective(option.var)}
                    overridden={isOverridden(option.var)}
                    busy={session.busy}
                    lang={lang}
                    listLabels={listLabels}
                    applicability={configApplicability(option, context, visibilityInputs)}
                    onChange={(value) => session.setConfigInput(option.var, value)}
                    onReset={() => resetInput(option.var)}
                  />
                ))}
              </div>
            )}
          </section>
        );
      })}

      </div>
    </div>
  </>);
  const customModifiers = (
      <article className="ui-card config-custom">
      <h3 className="section-heading">{tt('config.extraMods')}</h3>
      <p className="config-hint">{tt('config.extraModsHint')}</p>
      <div className="config-modifier-blocks">
        {modifierBlocks.map((block, index) => (
          <div className="config-modifier-block" key={index}>
            <div className="config-modifier-block-header">
              <label className="config-modifier-enabled">
                <AppInput type="checkbox" checked={block.enabled} disabled={session.busy}
                  onChange={event => updateModifierBlock(index, { enabled: event.target.checked })} />
                {tt('config.groupEnabled')}
              </label>
              <label className="config-modifier-title">
                <span>{tt('config.groupTitle')}</span>
                <AppInput type="text" key={block.title} defaultValue={block.title}
                  placeholder={tt('config.groupUntitled')} disabled={session.busy}
                  onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); }}
                  onBlur={event => {
                    if (event.target.value !== block.title) {
                      updateModifierBlock(index, { title: event.target.value });
                    }
                  }} />
              </label>
              <AppButton type="button" disabled={session.busy}
                aria-label={`${tt('config.groupRemove')}: ${block.title || `${index + 1}`}`}
                onClick={() => updateModifierBlocks(modifierBlocks.filter((_, currentIndex) => currentIndex !== index))}>
                {tt('config.groupRemove')}
              </AppButton>
            </div>
            <AppTextarea
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
      {unappliedLines.length > 0 && (
        <div className="config-unapplied" role="status">
          <p>{tt('config.unappliedLines')}</p>
          <ul>{unappliedLines.map((line, index) => <li key={index}>{line}</li>)}</ul>
        </div>
      )}
      <AppButton type="button" className="config-modifier-add" disabled={session.busy}
        onClick={() => updateModifierBlocks([...modifierBlocks, { title: '', enabled: true, text: '' }])}>
        {tt('config.groupAdd')}
      </AppButton>
      </article>
  );

  const advancedValues = (
      <details className="ui-card config-advanced" open={v2 || undefined}>
      <summary>{tt('config.addTitle')}</summary>
      <p className="config-hint">{tt('config.hint')}</p>
      {extraKeys.length > 0 && (
        <div className="config-grid">
          {extraKeys.map((key) => (
            <div key={key} className={`config-item${isOverridden(key) ? ' is-overridden' : ''}`}>
              <span className="config-key config-key--raw">{key}</span>
              {v2 ? <RawConfigValue name={key} value={effective(key)} lang={lang} disabled={session.busy}
                onChange={value => session.setConfigInput(key, value)} /> : (
              <AppInput
                key={String(effective(key) ?? '')}
                className="config-value"
                defaultValue={String(effective(key) ?? '')}
                disabled={session.busy}
                aria-label={key}
                onBlur={(e) => {
                  if (e.target.value === String(effective(key) ?? '')) return;
                  const parsed = parseValue(e.target.value);
                  if (parsed !== effective(key)) session.setConfigInput(key, parsed);
                }}
              />
              )}
              {isOverridden(key) && (
                <AppButton className="config-reset" disabled={session.busy} title={tt('config.reset')}
                  aria-label={`${tt('config.reset')}: ${key}`} onClick={() => resetInput(key)}>
                  ↺
                </AppButton>
              )}
            </div>
          ))}
        </div>
      )}
      <div className="config-add">
        <AppInput
          placeholder={tt('config.keyPlaceholder')}
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          aria-label={tt('config.key')}
        />
        {v2 ? <RawConfigValue name={tt('config.valueLabel')} value={newTypedValue ?? undefined} lang={lang} disabled={session.busy}
          onChange={setNewTypedValue} /> : (
        <AppInput
          placeholder="true / 40 / text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          aria-label={tt('config.valueLabel')}
        />
        )}
        <AppButton
          disabled={!newKey.trim() || session.busy}
          onClick={() => {
            session.setConfigInput(newKey.trim(), v2 ? newTypedValue : parseValue(newValue));
            setNewKey('');
          }}
        >
          {tt('config.addButton')}
        </AppButton>
      </div>
      </details>
  );

  return <section className="ui-page config-page" aria-labelledby="config-heading">
    <PageHeader id="config-heading" title={tt('config.title')} description={tt('ui.configHint')} />
    {v2 ? <>
      <div className="config-scenario"><NamedSets session={session} lang={lang} />{enemyControl}</div>
      <Tabs className="flex-col" value={view} onValueChange={setView}>
        <TabsList className="config-view-tabs group-data-horizontal/tabs:h-auto" aria-label={tt('config.title')}>
          <TabsTrigger className="h-auto min-h-10 whitespace-nowrap max-[600px]:whitespace-normal px-3 py-2 max-[600px]:flex-[1_1_42%] data-[state=active]:bg-background data-[state=active]:text-foreground" value="options">{workflowLabel(lang, 'options')}</TabsTrigger>
          <TabsTrigger className="h-auto min-h-10 whitespace-nowrap max-[600px]:whitespace-normal px-3 py-2 max-[600px]:flex-[1_1_42%] data-[state=active]:bg-background data-[state=active]:text-foreground" value="modifiers">{tt('config.extraMods')}</TabsTrigger>
          <TabsTrigger className="h-auto min-h-10 whitespace-nowrap max-[600px]:whitespace-normal px-3 py-2 max-[600px]:flex-[1_1_42%] data-[state=active]:bg-background data-[state=active]:text-foreground" value="party">{workflowLabel(lang, 'party')}</TabsTrigger>
          <TabsTrigger className="h-auto min-h-10 whitespace-nowrap max-[600px]:whitespace-normal px-3 py-2 max-[600px]:flex-[1_1_42%] data-[state=active]:bg-background data-[state=active]:text-foreground" value="advanced">{workflowLabel(lang, 'advanced')}</TabsTrigger>
        </TabsList>
        <TabsContent className="min-w-0 w-full" value="options">{catalog}</TabsContent>
        <TabsContent className="min-w-0 w-full" value="modifiers">{customModifiers}</TabsContent>
        <TabsContent className="min-w-0 w-full" value="party"><PartyEditor session={session} lang={lang} /></TabsContent>
        <TabsContent className="min-w-0 w-full" value="advanced">{advancedValues}</TabsContent>
      </Tabs>
    </> : <>
      <NamedSets session={session} lang={lang} />
      <PartyEditor session={session} lang={lang} />
      {catalog}{customModifiers}{advancedValues}
    </>}
  </section>;
}
