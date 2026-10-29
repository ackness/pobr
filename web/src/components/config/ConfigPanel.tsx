import { AppButton, AppInput, AppSelectControl, AppTextarea } from '../shared/controls';
import { PartyEditor } from './PartyEditor';
import { NamedSets } from '../shared/NamedSets';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useMemo, useState } from 'react';
import { usePanelActive } from '../shared/UiContext';
import { getBackend } from '../../api/backend';
import type { ConfigInputValue, ConfigOption, CustomModifierBlock } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSkillName } from '../../hooks/useSkillName';
import { bindT, configSectionLabel, damageTypeLabel, t, type Lang } from '../../lib/i18n';
import { CONFIG_LABEL_ZH, configLabel, configListOptionLabel, configTooltip } from '../../lib/configLabels';
import { configDefaultValue } from '../../lib/configDefaults';
import { configApplicability, configExistsInGame, configVisibilityInputs, showConfigOption, type Applicability } from '../../lib/configVisibility';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/tabs';
import { workflowLabel } from './workflowLabels';
import { RawConfigValue } from './RawConfigValue';
import { ScenarioBar } from '../../app/pages/config/ScenarioBar';
import { confirmedByBuild, partitionRelevant } from '../../app/pages/config/scenario';
import { words } from '../../app/text';
import './config.css';
import '../../app/config-workflow.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

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

/** The options the calculation has confirmed matter to this build, listed before the rest. */
const RELEVANT_SECTION = 'Relevant to this build';
/** Every group opens but quest rewards, which an import has already settled. */
const DEFAULT_OPEN_SECTIONS = [RELEVANT_SECTION, 'General', 'Skill Options', 'When In Combat', 'For Effective DPS', 'Enemy Stats'];

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
        <details className="config-option-help">
          <summary>{workflowLabel(lang, 'help')}</summary>
          {tooltip && <p className="config-help" id={`config-help-${option.var}`}>{stripColorCodes(tooltip)}</p>}
          {tooltip && lang !== 'en-US' && tooltip === option.tooltip && <p className="config-source">{workflowLabel(lang, 'sourceHelp')}</p>}
          <span className="config-source">{workflowLabel(lang, 'rawKey')}</span>
          <code className="config-raw-key">{option.var}</code>
          <span className="config-source">{tt(value === undefined ? 'config.sourceDefault' : 'config.sourceSaved')}</span>
          {value !== undefined && <span className="config-source config-saved-type">
            {workflowLabel(lang, typeof value as 'boolean' | 'number' | 'string')}: <code>{JSON.stringify(value)}</code>
          </span>}
        </details>
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
          className="w-full"
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
  const [view, setView] = useState('options');
  const skillName = useSkillName(lang);
  const [focusTarget, setFocusTarget] = useState<string | null>(null);
  const [options, setOptions] = useState<ConfigOption[]>([]);
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>('loading');
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [query, setQuery] = useState('');
  const [configuredOnly, setConfiguredOnly] = useState(false);
  const [allOptions, setAllOptions] = useState(false);
  const [selectedSection, setSelectedSection] = useState<string | null>(null);
  const [openSections, setOpenSections] = useState<Set<string>>(new Set(DEFAULT_OPEN_SECTIONS));
  // Where each option the reader changed sat (in the leading group or not) when they changed it. Which group an
  // option belongs to is read from the calculation, which can start to read the very option just set; its row stays
  // put until the page or the stage is left, instead of moving to the top, or into a folded group, under the pointer.
  const [placed, setPlaced] = useState<Record<string, boolean>>({});
  const active = usePanelActive();
  const stage = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild)?.activeStage;
  useEffect(() => { setPlaced(current => Object.keys(current).length ? {} : current); }, [active, stage, session.workspace?.activeBuild]);
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
    setSelectedSection(groups.find(group => group.options.some(entry => entry.var === option.var))?.name ?? option.section ?? 'General');
    setOpenSections(previous => new Set([...previous, option.section ?? 'General', RELEVANT_SECTION]));
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

  // The options the calculation has confirmed for this build form their own leading group, and
  // appear nowhere else, so every control (and its #config-<var> id) is still on the page once.
  const groups = useMemo(() => {
    const { relevant, rest } = partitionRelevant(sections.flatMap(section => section.options), confirmedByBuild(context, placed));
    if (relevant.length === 0) return sections;
    const kept = new Set(rest.map(option => option.var));
    return [{ name: RELEVANT_SECTION, options: relevant },
      ...sections.map(section => ({ ...section, options: section.options.filter(option => kept.has(option.var)) }))
        .filter(section => section.options.length > 0)];
  }, [sections, context, placed]);
  const settle = (variable: string) => setPlaced(current => variable in current ? current
    : { ...current, [variable]: !!groups.find(group => group.name === RELEVANT_SECTION)?.options.some(option => option.var === variable) });
  const groupTitle = (name: string) => name === RELEVANT_SECTION
    ? words(lang, 'Relevant to this build', '与此 BD 相关', '與此 BD 相關') : configSectionLabel(lang, name);

  useEffect(() => {
    if (!focusTarget || session.busy) return;
    const control = document.getElementById(`config-${focusTarget}`);
    if (control instanceof HTMLElement) {
      control.focus({ preventScroll: true });
      control.scrollIntoView({ block: 'center' });
    }
    setFocusTarget(null);
  }, [focusTarget, groups, session.busy]);

  const searching = query.trim() !== '' || configuredOnly;
  const activeSection = groups.find(group => group.name === selectedSection)?.name ?? groups[0]?.name;
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

  const searchToolbar = (
      <div className="config-toolbar">
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
  );
  const viewToggle = (
      <div className="config-view" role="group" aria-label={tt('config.view')}>
        <AppButton className="aria-pressed:bg-accent aria-pressed:text-accent-foreground" type="button" aria-pressed={!allOptions} onClick={() => setAllOptions(false)}>{tt('config.applicable')}</AppButton>
        <AppButton className="aria-pressed:bg-accent aria-pressed:text-accent-foreground" type="button" aria-pressed={allOptions} onClick={() => setAllOptions(true)}>{tt('config.showAll')}</AppButton>
      </div>
  );

  const catalog = (<>
      <div className="config-controls">{searchToolbar}{viewToggle}</div>
      <div className="config-help-tools"><details className="config-guidance"><summary>{workflowLabel(lang, 'guidance')}</summary>
      <p className="config-hint">{tt('config.visibilityHint')}</p>
      <p className="config-hint">{tt('config.editHint')}</p>
      <p className="config-hint">{tt('config.defaultsHint')}</p>
      </details>
      {loadState === 'loading' && <p role="status">{tt('config.loading')}</p>}
      {loadState === 'error' && <div className="calc-error" role="alert">
        {tt('config.loadFailed')} <AppButton onClick={() => setLoadAttempt(value => value + 1)}>{tt('common.retry')}</AppButton>
      </div>}
    <details className="config-quick-disclosure"><summary>{tt('config.quickTitle')}</summary>{quickAccess}</details></div>
    {groups.length > 0 && <label className="config-mobile-category">
      <span>{words(lang, 'Category', '分类', '分類')}</span>
      <AppSelectControl aria-label={words(lang, 'Configuration category', '配置分类', '設定分類')} value={searching ? 'search' : activeSection}
        onChange={event => { setQuery(''); setConfiguredOnly(false); setSelectedSection(event.target.value); setOpenSections(previous => new Set([...previous, event.target.value])); }}>
        {searching && <option value="search">{words(lang, 'Search results', '搜索结果', '搜尋結果')}</option>}
        {groups.map(group => <option key={group.name} value={group.name}>{groupTitle(group.name)} ({group.options.length})</option>)}
      </AppSelectControl>
    </label>}
    <div className="config-options-workspace">
      {groups.length > 0 && <nav className="config-group-nav" aria-label={workflowLabel(lang, 'jump')}>
        <h3>{workflowLabel(lang, 'jump')}</h3>
        {groups.map(section => <AppButton className="h-auto min-h-10 justify-between whitespace-normal text-left" key={section.name}
          aria-pressed={!searching && activeSection === section.name} onClick={() => {
          setQuery(''); setConfiguredOnly(false); setSelectedSection(section.name);
          setOpenSections(previous => new Set([...previous, section.name]));
          requestAnimationFrame(() => {
            const group = document.getElementById(`config-section-${section.name.replace(/\s+/g, '-')}`);
            group?.scrollIntoView({ block: 'start' });
            group?.querySelector<HTMLButtonElement>('.config-section-header')?.focus({ preventScroll: true });
          });
        }}><span>{groupTitle(section.name)}</span><span>{section.options.length}</span></AppButton>)}
      </nav>}
      <div className="config-option-groups">
      {loadState === 'ready' && searching && groups.length === 0 && <div className="search-empty" role="status">
        <p>{tt('common.noResults')}</p>
        <AppButton onClick={() => { setQuery(''); setConfiguredOnly(false); setAllOptions(true); }}>{tt('config.showAll')}</AppButton>
      </div>}

      {groups.map(({ name, options: sectionOptions }) => {
        const open = searching || openSections.has(name);
        return (
          <section key={name} id={`config-section-${name.replace(/\s+/g, '-')}`} className="config-section" hidden={!searching && activeSection !== name}>
            <AppButton
              className="config-section-header h-auto min-h-12 w-full justify-between whitespace-normal rounded-none border-0 bg-muted text-left"
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
              <span><span className="row-caret" aria-hidden>{open ? '▾' : '▸'}</span> {groupTitle(name)}</span>
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
                    onChange={(value) => { settle(option.var); session.setConfigInput(option.var, value); }}
                    onReset={() => { settle(option.var); resetInput(option.var); }}
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
      <details className="ui-card config-advanced" open>
      <summary>{tt('config.addTitle')}</summary>
      <p className="config-hint">{tt('config.hint')}</p>
      {extraKeys.length > 0 && (
        <div className="config-grid">
          {extraKeys.map((key) => (
            <div key={key} className={`config-item${isOverridden(key) ? ' is-overridden' : ''}`}>
              <span className="config-key config-key--raw">{key}</span>
              <RawConfigValue name={key} value={effective(key)} lang={lang} disabled={session.busy}
                onChange={value => session.setConfigInput(key, value)} />
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
        <RawConfigValue name={tt('config.valueLabel')} value={newTypedValue ?? undefined} lang={lang} disabled={session.busy}
          onChange={setNewTypedValue} />
        <AppButton
          disabled={!newKey.trim() || session.busy}
          onClick={() => {
            session.setConfigInput(newKey.trim(), newTypedValue);
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
    <div className="config-scenario"><ScenarioBar session={session} lang={lang} /><NamedSets session={session} lang={lang} /></div>
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
  </section>;
}
