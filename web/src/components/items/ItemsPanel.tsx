import { DisclosureChevron } from '../shared/DisclosureChevron';
import { localizedName } from '../../lib/chinese';
import { replaceUtility } from '../../lib/utilityItems';
import { AppButton, AppInput, AppTextarea } from '../shared/controls';
import { UniqueCatalog } from './UniqueCatalog';
import { uniqueT } from '../../lib/uniqueCatalogText';
import { ItemCrafter, EquippedItemCrafter, useEquippedItemDraft } from './ItemCrafter';
import { craftT } from '../../lib/itemCraftingText';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { SlotSymbol } from '../shared/SlotSymbol';
import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { getBackend } from '../../api/backend';
import type { ItemAugmentInfo, RuneCatalogEntry, SlotItemInput } from '../../api/types';
import type { BuildSession, LibraryItem } from '../../hooks/useBuildSession';
import {
  cleanLine,
  itemLines,
  rarityOf,
  useItemDisplayNames,
  useLocalizedLines,
} from '../../hooks/useLocalizedLines';
import { bindT, slotLabel, type Lang } from '../../lib/i18n';
import { shownLines } from '../../lib/continuedLines';
import { upgradeT } from '../../lib/upgradeText';
import { tradeAnalysisText } from '../../lib/tradeAnalysisText';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import { applyEquippedAugmentLimits, planReplacementAugments } from '../../lib/replacementAugments';
import type { DisplayStatValue } from '../../api/types';
import { SwapGainCard } from './SwapGainCard';
import { ItemContribution } from './ItemContribution';
import { CopyButton } from '../shared/CopyButton';
import { NoteEditor } from '../shared/NoteEditor';
import { AppSelect } from '../shared/AppSelect';
import { RuneSocketPicker, type RuneChoice } from './RuneSocketPicker';
import { ItemOptimizer } from './ItemOptimizer';
import { PaperDoll } from './PaperDoll';
import { ItemDisplay } from './ItemDisplay';
import { usePreview } from '../shared/PreviewContext';
import './items.css';

interface Props {
  onUpgrade?: (slot:string) => void;
  session: BuildSession;
  lang: Lang;
}

// Line extraction and translation utilities live in `hooks/useLocalizedLines` (shared with tree jewel selector).

/** Compact item row (shared by item library and jewel list): name + slot + actions; clicking header expands full mods. */
function ItemRow({
  text,
  name,
  lang,
  slotTag,
  open,
  onToggle,
  actions,
  hover,
  children,
  source,
}: {
  text: string;
  name: string;
  lang: Lang;
  /** Preview handlers; event props add no attributes to the markup. */
  hover?: { onEnter: (event: { currentTarget: Element }) => void; onLeave: () => void };
  slotTag?: string;
  source?: string;
  open: boolean;
  onToggle: () => void;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <article className={`library-row rarity-${rarityOf(text)}`} onMouseEnter={hover?.onEnter} onMouseLeave={hover?.onLeave} onFocus={hover?.onEnter} onBlur={hover?.onLeave}>
      <header className="library-row-head">
        <AppButton className="library-row-toggle disclosure" data-disclosure-level="group" aria-expanded={open} onClick={onToggle}>
          <DisclosureChevron className="row-caret" />
          <span className="item-name">{name}</span>
          {slotTag && <span className="library-row-slot">{slotTag}</span>}
        </AppButton>
        <span className="item-actions" role="group" aria-label={bindT(lang)('lib.actions')}>{actions}</span>
      </header>
      {source && <p className="library-source">{source}</p>}
      {open && (
        <div className="library-row-body">
          <ItemDisplay text={text} lang={lang} />
          {children}
        </div>
      )}
    </article>
  );
}

/** Display order matches the equipment and accessory groups. */
const DOLL_SLOTS = ['weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'belt'];

const ITEM_TEMPLATE = 'Rarity: RARE\nNew Item\nSapphire Ring\n+50 to maximum Life';
const FLASK_TEMPLATE = 'Rarity: MAGIC\nUltimate Life Flask';
const CHARM_TEMPLATE = 'Rarity: MAGIC\nRuby Charm';

/** Equipped PoB utility slots, including unselected items. */
const UTILITY_SLOTS = ['Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
const isUtilitySlot = (slot: string) => slot.startsWith('Flask') || slot.startsWith('Charm');
/** Graft/limb slots (matches wasm contract limb slot IDs): sibling row alongside gear, no rune sockets, excluded from trade. */
const LIMB_SLOTS = ['arm1', 'arm2', 'leg1', 'leg2'];
const isLimbSlot = (slot: string) => LIMB_SLOTS.includes(slot);
/** Equipment proper: the slots with rune sockets and market searches. */
const isEquipmentSlot = (slot: string) => !isUtilitySlot(slot) && !isLimbSlot(slot);
const ARM_TEMPLATE = 'Rarity: NORMAL\nCombat Arm\nImplicits: 1\n6% increased Attack Speed';
const LEG_TEMPLATE = 'Rarity: NORMAL\nEvasive Leg\nImplicits: 1\n20% increased Evasion Rating';
const ALL_SLOTS = [...DOLL_SLOTS, ...UTILITY_SLOTS, ...LIMB_SLOTS];
const itemDraftKey = (slot: string, weaponSet: number) =>
  `item:${slot.startsWith('weapon') ? weaponSet : 0}:${slot}`;

/** Items page: PoB2-style paper-doll layout; clicking a slot edits PoB text below and saving recalculates. */
export function ItemsPanel({ session, lang, onUpgrade }: Props) {
  // Slots pulse when their item changes after the page is up, never on the first render.
  const shown = useRef(false);
  useEffect(() => { shown.current = true; }, []);
  const [crafting, setCrafting] = useState(false);
  const [uniques, setUniques] = useState(false);
  const [craftingItem, setCraftingItem] = useState<{ slot: string; text: string } | null>(null);
  const tt = bindT(lang);
  const [selected, setSelected] = useState<string | null>(() => ALL_SLOTS
    .find(slot => session.editorDrafts[itemDraftKey(slot, session.activeWeaponSet)] !== undefined) ?? null);
  const detailRef = useRef<HTMLDivElement>(null);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const build = session.build;
  const items = session.items;
  /** The list a slot's item belongs to (equipment, flasks/charms or limbs), each replaced whole. */
  const listOf = (slot: string): [SlotItemInput[], (list: SlotItemInput[]) => void] =>
    isUtilitySlot(slot) ? [session.flasks, session.setFlasks]
      : isLimbSlot(slot) ? [session.limbs, session.setLimbs]
        : [items, session.setItems];
  const textOf = (slot: string) => listOf(slot)[0].find((item) => item.slot === slot)?.text;
  /** Puts `text` in `slot`, or empties it with `null`. */
  const equip = (slot: string, text: string | null) => {
    if (isUtilitySlot(slot)) {
      session.setFlasks(replaceUtility(session.flasks, slot, text));
      return;
    }
    const [list, setList] = listOf(slot);
    const rest = list.filter((item) => item.slot !== slot);
    setList(text === null ? rest : [...rest, { slot, text }]);
  };
  // Item display names `"Name·Base"` on slot buttons (batched for translation; localized in Chinese UI).
  const slotNames = useItemDisplayNames(ALL_SLOTS.map((slot) => textOf(slot)), lang);
  const slotName = (slot: string) => slotNames[ALL_SLOTS.indexOf(slot)] || null;
  const templateOf = (slot: string) =>
    slot.startsWith('Charm') ? CHARM_TEMPLATE : slot.startsWith('Flask') ? FLASK_TEMPLATE
      : slot.startsWith('arm') ? ARM_TEMPLATE : slot.startsWith('leg') ? LEG_TEMPLATE : ITEM_TEMPLATE;
  const draftKey = selected ? itemDraftKey(selected, session.activeWeaponSet) : '';
  const savedDraft = session.editorDrafts[draftKey];
  const draft = savedDraft ?? (selected ? textOf(selected) ?? templateOf(selected) : '');
  const editing = editingKey === draftKey || savedDraft !== undefined;
  const setDraft = (text: string) => session.setEditorDraft(draftKey,
    text === (selected ? textOf(selected) ?? templateOf(selected) : '') ? null : text);
  const setEditing = (value: boolean) => {
    setEditingKey(value ? draftKey : null);
    if (!value) session.setEditorDraft(draftKey, null);
  };
  useEffect(() => { setEditingKey(null); }, [session.build, session.activeWeaponSet]);

  const select = (slot: string) => {
    setSelected(slot);
    requestAnimationFrame(() => {
      const detail = detailRef.current;
      if (!detail) return;
      detail.focus({ preventScroll: true });
      const header = detail.querySelector('header')!.getBoundingClientRect();
      const bounds = detail.closest('main')!.getBoundingClientRect();
      // Inserting details above the library can move the browser's scroll anchor.
      if (header.top < bounds.top || header.bottom > bounds.bottom) {
        detail.scrollIntoView({ block: 'start' });
      }
    });
    setEditingKey(textOf(slot) === undefined ? itemDraftKey(slot, session.activeWeaponSet) : null);
  };
  const applyEdit = () => {
    if (!selected) return;
    equip(selected, draft);
    setEditing(false);
  };
  const removeItem = () => {
    if (!selected) return;
    equip(selected, null);
    setEditing(false);
  };

  /** Equip an item directly from the library into the active slot (PoB2-style dropdown swap). */
  const switchTo = (text: string) => {
    if (!selected) return;
    equip(selected, text);
    setDraft(text);
    setEditing(false);
  };

  const selectedText = selected ? textOf(selected) : undefined;
  const { draft: craftingDraft, error: craftingError } = useEquippedItemDraft(
    selected && isEquipmentSlot(selected) ? selectedText : undefined);

  /** Slot -> annotation key (separate `flask:` namespace avoids slot name collisions with equipment). */
  const noteKey = (slot: string) => `${isUtilitySlot(slot) ? 'flask' : 'item'}:${slot}`;
  const hasNote = (slot: string) => Boolean(session.annotations[noteKey(slot)]?.trim());

  // Metadata belongs to an exact item selection; never display stale options
  // while an asynchronous request for a different item is pending.
  const [augmentResult, setAugmentResult] = useState<{
    slot: string; text: string; info: ItemAugmentInfo; catalog: RuneCatalogEntry[];
  } | null>(null);
  const [runeError, setRuneError] = useState<{ slot: string; text: string; message: string } | null>(null);
  const [runePending, setRunePending] = useState<{ slot: string; text: string } | null>(null);
  const reforgeSequence = useRef(0);
  const currentEdit = useRef({ selected, selectedText, editing, session });
  currentEdit.current = { selected, selectedText, editing, session };
  useEffect(() => {
    let cancelled = false;
    setAugmentResult(null);
    setRuneError(null);
    if (!selected || !selectedText || !isEquipmentSlot(selected)) return;
    const slot = selected;
    const text = selectedText;
    getBackend()
      .then(async backend => ({ info: await backend.itemAugmentInfo(text), catalog: await backend.runeCatalog() }))
      .then(({ info, catalog }) => {
        if (!cancelled) setAugmentResult({ slot, text, info, catalog });
      })
      .catch((error: unknown) => {
        if (!cancelled) setRuneError({ slot, text, message: formatApiError(error) });
      });
    return () => { cancelled = true; };
  }, [selected, selectedText]);
  useEffect(() => () => { reforgeSequence.current++; }, []);
  const rawAugmentInfo = augmentResult?.slot === selected && augmentResult.text === selectedText
    ? augmentResult.info : null;
  const augmentUsage = useMemo(() => ({ catalog: augmentResult?.catalog ?? [], items: session.items, slot: selected ?? '' }),
    [augmentResult, session.items, selected]);
  const limitedAugments = useMemo(() => rawAugmentInfo ? applyEquippedAugmentLimits(rawAugmentInfo, augmentUsage) : null,
    [rawAugmentInfo, augmentUsage]);
  const augmentInfo = limitedAugments?.info ?? null;
  const runeBusy = runePending?.slot === selected && runePending.text === selectedText;
  const visibleRuneError = runeError?.slot === selected && runeError.text === selectedText
    ? runeError.message : null;
  const characterLevel = session.character?.level ?? 1;
  const runeCatalog = useMemo(() => (augmentInfo?.options ?? [])
    .filter(entry => (entry.required_level ?? 0) <= characterLevel), [augmentInfo, characterLevel]);
  const showAugments = augmentInfo && isEquipmentSlot(selected ?? '')
    && (augmentInfo.sockets > 0 || augmentInfo.max_sockets > 0
      || (!augmentInfo.editable && !['unsupported_item_type', 'no_augment_sockets'].includes(augmentInfo.reason ?? '')));

  const runeLines = useMemo(() => (augmentInfo?.options ?? []).map(entry => entry.lines.map(cleanLine)), [augmentInfo]);
  const runeEffectLines = useLocalizedLines(runeLines.flat(), lang);
  const runeOptions = useMemo(() => {
    const groups = { Rune: upgradeT(lang, 'augmentRune'), SoulCore: upgradeT(lang, 'augmentCore'), Idol: upgradeT(lang, 'augmentIdol') };
    let offset = 0;
    const options = (augmentInfo?.options ?? []).map((entry, index): RuneChoice => {
      const lines = runeLines[index];
      const effects = shownLines(lines, runeEffectLines.slice(offset, offset + lines.length));
      offset += lines.length;
      const label = localizedName(lang, entry.name, entry.name_zh_cn, entry.name_zh_tw);
      const group = groups[entry.kind ?? (entry.is_soul_core ? 'SoulCore' : 'Rune')];
      return { value: entry.name, label, group, effects,
        searchText: [entry.name, entry.name_zh_cn, entry.name_zh_tw, group, ...lines, ...effects].join(' ').toLowerCase() };
    });
    return Object.values(groups).flatMap(group => options.filter(option => option.group === group));
  }, [augmentInfo, runeLines, runeEffectLines, lang]);

  const runeAllowedAt = (index: number, name: string) => {
    if (!name) return true;
    const entry = runeCatalog.find(candidate => candidate.name === name);
    return entry != null && (entry.limit == null
      || (augmentInfo?.runes.filter((rune, slot) => slot !== index &&
        (augmentInfo.options.find(option => option.name === rune)?.limit_id ?? rune) === (entry.limit_id ?? name)).length ?? 0) < entry.limit);
  };

  const reforge = async (runes: string[], sockets: number) => {
    if (!selected || !selectedText || !augmentInfo?.editable || !isEquipmentSlot(selected) || runeBusy) return;
    if (!Number.isInteger(sockets) || sockets < 0 || sockets > augmentInfo.max_sockets) return;
    const origin = { slot: selected, text: selectedText, version: session.stateVersion };
    const sequence = ++reforgeSequence.current;
    const stillCurrent = () => {
      const current = currentEdit.current;
      return sequence === reforgeSequence.current && current.selected === origin.slot
        && current.selectedText === origin.text && !current.editing
        && current.session.stateVersion === origin.version;
    };
    setRunePending(origin);
    setRuneError(null);
    try {
      planReplacementAugments(rawAugmentInfo!, undefined, characterLevel, { mode: 'custom', runes, sockets }, augmentUsage);
      const backend = await getBackend();
      if (!stillCurrent()) return;
      const text = await backend.reforgeRunes(origin.text, runes, sockets);
      if (!stillCurrent()) return;
      const current = currentEdit.current.session;
      current.setItems([...current.items.filter(item => item.slot !== origin.slot), { slot: origin.slot, text }]);
      setDraft(text);
      setEditing(false);
    } catch (error) {
      if (stillCurrent()) setRuneError({ ...origin, message: error instanceof Error && error.message === 'augment-limit'
        ? upgradeT(lang, 'augmentLimit') : formatApiError(error) });
    } finally {
      if (sequence === reforgeSequence.current) setRunePending(null);
    }
  };

  const resocketRune = (index: number, name: string) => {
    if (!augmentInfo || index < 0 || index >= augmentInfo.sockets) return;
    if (!runeAllowedAt(index, name)) return;
    const runes = Array.from({ length: augmentInfo.sockets }, (_, i) => augmentInfo.runes[i] ?? '');
    runes[index] = name;
    void reforge(runes, augmentInfo.sockets);
  };
  const setSocketCount = (sockets: number) => {
    if (!augmentInfo || !Number.isInteger(sockets) || sockets < 0 || sockets > augmentInfo.max_sockets) return;
    const runes = Array.from({ length: sockets }, (_, i) => augmentInfo.runes[i] ?? '');
    void reforge(runes, sockets);
  };

  // Slot swap candidates: collect only library items with an explicit matching slot (normalized by family; ring1/2 share).
  // Legacy entries lacking a recorded slot are skipped (so an amulet is not offered for main hand); re-importing backfills slots.
  const candidates = selected
    ? session.library.items.filter(
        (i) => i.kind === 'item' && i.slot != null && slotFamily(i.slot) === slotFamily(selected),
      )
    : [];
  const candidateNames = useItemDisplayNames(
    candidates.map((c) => c.text),
    lang,
  );
  const switcherValue =
    candidates.find((c) => c.text === selectedText)?.id ?? (selectedText ? '__current' : '');

  const slotButton = (slot: string, label = slotLabel(lang, slot)) => {
    const text = textOf(slot);
    const name = text ? slotName(slot) : null;
    return (
      <AppButton
        key={slot}
        className={`doll-slot${text ? ` rarity-${rarityOf(text)}` : ' doll-slot-empty'}${
          selected === slot ? ' is-selected' : ''
        }`}
        onClick={() => select(slot)}
        aria-pressed={selected === slot}
        aria-label={label}
      >
        {shown.current && text && <span key={text} className="doll-pulse" aria-hidden="true" />}
        <span className="doll-symbol"><SlotSymbol slot={slot} /></span>
        <span className="doll-slot-label">
          {slotLabel(lang, slot)}
          {hasNote(slot) && <span className="note-dot" aria-hidden />}
          {session.editorDrafts[itemDraftKey(slot, session.activeWeaponSet)] !== undefined && <span className="ui-badge">{tt('editor.draft')}</span>}
        </span>
        {name ? (
          // The name joins a rare or unique item's own name and its base with a dot; the figure shows them on two lines.
          <>
            <span className="doll-item-name item-name">{name.split('·')[0]}</span>
            {name.includes('·') && <span className="doll-item-base">{name.split('·').slice(1).join(' · ')}</span>}
          </>
        ) : (
          <span className="doll-empty">{tt('items.empty')}</span>
        )}
      </AppButton>
    );
  };

  return (
    <section className="ui-page items-page" aria-labelledby="items-heading">
      <PageHeader id="items-heading" title={tt('items.title')} description={tt('ui.itemsHint')}>
        <WeaponSetControl session={session} lang={lang} />
        <AppButton aria-expanded={crafting} onClick={() => { setCrafting(value => !value); setUniques(false); }}>{craftT(lang, crafting ? 'close' : 'craft')}</AppButton>
        <AppButton aria-expanded={uniques} onClick={() => { setUniques(value => !value); setCrafting(false); }}>{uniqueT(lang, uniques ? 'close' : 'title')}</AppButton>
      </PageHeader>
      <p className="weapon-set-hint">{tt('weapons.hint')}</p>
      {uniques && <UniqueCatalog selectedSlot={selected} session={session} lang={lang} />}
      {crafting && <ItemCrafter selectedSlot={selected} session={session} lang={lang} />}
      <div className="items-workspace">
      <article className="items-equipped items-doll">
      <PaperDoll slot={(id) => slotButton(id, isUtilitySlot(id) ? id : undefined)} utilitySlots={UTILITY_SLOTS} limbSlots={LIMB_SLOTS}
        limbsInUse={session.limbs.length > 0} labels={{ body: tt('items.title'), utility: tt('items.flasks'), limbs: tt('items.limbs'), limbsHint: tt('items.limbsHint') }} />
      </article>
      <div className="items-collection">
      {selected && (
        <div ref={detailRef} tabIndex={-1} className={`item-detail${selectedText ? ` rarity-${rarityOf(selectedText)}` : ''}`}>
          <header className="item-detail-header">
            <label className="item-position"><span className="item-slot">{tt('items.position')}</span>
              <AppSelect ariaLabel={tt('items.position')} value={selected}
                options={ALL_SLOTS.map(slot => ({ value: slot, label: slotLabel(lang, slot) }))}
                onChange={select} />
            </label>
            {candidates.length > 0 && (
              <AppSelect
                ariaLabel={tt('items.switcher')}
                value={switcherValue}
                disabled={session.busy}
                options={[
                  ...(selectedText && !candidates.some((c) => c.text === selectedText)
                    ? [{ value: '__current', label: tt('items.currentItem') }]
                    : []),
                  { value: '', label: tt('items.unequip') },
                  ...candidates.map((c, i) => ({ value: c.id, label: candidateNames[i] || c.name })),
                ]}
                onChange={(v) => {
                  if (v === '__current') return;
                  if (v === '') {
                    removeItem();
                    return;
                  }
                  const entry = candidates.find((c) => c.id === v);
                  if (entry) switchTo(entry.text);
                }}
              />
            )}
            <span className="item-actions">
              {onUpgrade && !isLimbSlot(selected) && <AppButton className="item-find-upgrade" onClick={() => onUpgrade(selected)}>{tradeAnalysisText(lang, 'findBetter')}</AppButton>}
              {!editing && (
                <AppButton disabled={session.busy} onClick={() => setEditing(true)}>
                  {selectedText ? tt('items.edit') : tt('items.add')}
                </AppButton>
              )}
              {selectedText && (
                <>
                  {isEquipmentSlot(selected) && <AppButton disabled={session.busy || !craftingDraft?.base}
                    aria-label={tt('items.editInCrafter')}
                    title={!craftingDraft?.base ? craftT(lang, craftingError ? 'error' : craftingDraft ? 'editUnavailable' : 'loading') : undefined}
                    aria-expanded={craftingItem?.slot === selected && craftingItem.text === selectedText}
                    onClick={() => setCraftingItem(current => current?.slot === selected && current.text === selectedText
                      ? null : { slot: selected, text: selectedText })}>
                    {tt('items.editInCrafter')}
                  </AppButton>}
                  <CopyButton text={selectedText} lang={lang} />
                  <AppButton
                    disabled={session.busy}
                    onClick={() => session.saveLibraryItem('item', selectedText, selected)}
                  >
                    {tt('lib.save')}
                  </AppButton>
                  <AppButton className="skill-remove" disabled={session.busy} onClick={removeItem}>
                    {tt('items.remove')}
                  </AppButton>
                </>
              )}
            </span>
          </header>
          {craftingItem?.slot === selected && craftingItem.text === selectedText && craftingDraft?.base && (
            <EquippedItemCrafter key={`${selected}:${selectedText}`} draft={craftingDraft} session={session} lang={lang} />
          )}
          {selectedText && isUtilitySlot(selected) && (
            <div className="flex flex-col gap-2">
              <label className="flex items-center gap-2">
                <AppInput type="checkbox" checked={session.flasks.find(item => item.slot === selected)?.active ?? true}
                  disabled={session.busy} aria-describedby="utility-selection-hint"
                  onChange={event => session.setFlasks(session.flasks.map(item => item.slot === selected
                    ? { ...item, active: event.target.checked } : item))} />
                {tt('items.utilitySelected')}
              </label>
              <p id="utility-selection-hint" className="items-hint">{tt('items.utilitySelectionHint')}</p>
            </div>
          )}
          {editing ? (
            <div className="item-editor">
              <p className="items-hint">{tt('items.hint')}</p>
              <AppTextarea
                rows={10}
                value={draft}
                spellCheck={false}
                aria-label={`${selected} item text`}
                onChange={(e) => setDraft(e.target.value)}
              />
              <div className="item-editor-actions">
                <AppButton className="button-primary" disabled={session.busy || !draft.trim()} onClick={applyEdit}>
                  {tt('items.apply')}
                </AppButton>
                <AppButton onClick={() => setEditing(false)}>{tt('items.cancel')}</AppButton>
              </div>
            </div>
          ) : selectedText ? (
            <>
              {showAugments && augmentInfo && (
                <div className="rune-editor" role="group" aria-label={upgradeT(lang, 'augmentTitle')}>
                  <span className="rune-editor-title">{upgradeT(lang, 'augmentTitle')}</span>
                  {Array.from({ length: augmentInfo.sockets }, (_, index) => {
                    const equipped = augmentInfo.runes[index] ?? '';
                    const label = `${upgradeT(lang, 'augmentSocket')} ${index + 1}`;
                    const empty = tt('items.emptySocket');
                    const emptyChoice = { value: '', label: empty, effects: [], searchText: empty.toLowerCase() };
                    return <RuneSocketPicker key={`${draftKey}:${selectedText}:${index}`}
                      lang={lang} socket={label} destination={`${slotLabel(lang, selected)} · ${label}`}
                      current={runeOptions.find(option => option.value === equipped)
                        ?? { ...emptyChoice, value: equipped, label: equipped || empty }}
                      disabled={session.busy || runeBusy || !augmentInfo.editable}
                      options={[emptyChoice, ...runeOptions.filter(option => runeAllowedAt(index, option.value))]}
                      onApply={value => resocketRune(index, value)} />;
                  })}
                  {!augmentInfo.editable ? (
                    <p className="items-hint">{upgradeT(lang, augmentInfo.reason === 'unknown_base'
                      ? 'unknownBase' : augmentInfo.reason === 'invalid_sockets' ? 'augmentInvalid' : 'augmentReadOnly')}</p>
                  ) : augmentInfo.sockets === 0 ? (
                    <AppButton disabled={session.busy || runeBusy || augmentInfo.max_sockets <= 0} onClick={() => setSocketCount(1)}>
                      {tt('items.addSockets')}
                    </AppButton>
                  ) : (
                    <span className="socket-count-controls">
                      <AppButton
                        disabled={session.busy || runeBusy}
                        title={tt('items.socketRemove')}
                        aria-label={tt('items.socketRemove')}
                        onClick={() => setSocketCount(augmentInfo.sockets - 1)}
                      >{tt('items.removeSocketLabel')}</AppButton>
                      <AppButton
                        disabled={session.busy || runeBusy || augmentInfo.sockets >= augmentInfo.max_sockets}
                        title={tt('items.socketAdd')}
                        aria-label={tt('items.socketAdd')}
                        onClick={() => setSocketCount(augmentInfo.sockets + 1)}
                      >{tt('items.addSocketLabel')}</AppButton>
                    </span>
                  )}
                </div>
              )}
              {limitedAugments?.limitWarning && <p className="items-hint">{upgradeT(lang, 'augmentLimitUnknown')}</p>}
              {visibleRuneError && <p className="calc-error" role="alert">{visibleRuneError}</p>}
              <ItemDisplay text={selectedText} lang={lang} />
              <NoteEditor
                value={session.annotations[noteKey(selected)] ?? ''}
                onCommit={(text) => session.setAnnotation(noteKey(selected), text)}
                lang={lang}
              />
            </>
          ) : (
            <p className="item-empty-hint">{tt('items.empty')}</p>
          )}
          {selectedText && <ItemContribution session={session} slot={selected} lang={lang} />}
          {isEquipmentSlot(selected) && (
            <ItemOptimizer
              session={session}
              lang={lang}
              slot={selected}
              candidates={candidates}
              candidateNames={candidateNames}
              currentText={selectedText}
              onApply={(text) => (text === null ? removeItem() : switchTo(text))}
            />
          )}
        </div>
      )}

      <article className="ui-card items-library">
        <h3 className="section-heading">{tt('lib.title')} <span className="ui-badge">{session.library.items.filter(item => item.kind === 'item').length}</span></h3>
        {!selected && <p id="library-slot-hint" className="items-hint">{tt('lib.selectSlotFirst')}</p>}
        <LibrarySection session={session} lang={lang} selectedSlot={selected} onEquip={switchTo} />
      </article>
      </div>
      </div>

      {build && build.items.jewels.length > 0 && (
        <>
          <h3 className="panel-subheading">{tt('items.jewels')}</h3>
          <JewelList texts={build.items.jewels} lang={lang} />
        </>
      )}
    </section>
  );
}

/** Jewel list (read-only from imported build; edit via sockets on the Tree page). */
function JewelList({ texts, lang }: { texts: string[]; lang: Lang }) {
  const [openIdx, setOpenIdx] = useState<number | null>(null);
  const names = useItemDisplayNames(texts, lang);
  return (
    <div className="library-list">
      {texts.map((text, i) => (
        <ItemRow
          key={i}
          text={text}
          name={names[i] || itemLines(text)[0] || ''}
          lang={lang}
          open={openIdx === i}
          onToggle={() => setOpenIdx(openIdx === i ? null : i)}
          actions={<CopyButton text={text} lang={lang} />}
        />
      ))}
    </div>
  );
}

/** Numbered rings share candidates; main hand and offhand remain distinct. */
function slotFamily(slot: string): string {
  const normalized = slot.toLowerCase();
  return /^weapon[12]$/.test(normalized) ? normalized : normalized.replace(/\s*\d+$/, '');
}

/** Item library: saved gear and jewels; search/filter by selected slot, compare diffs, and equip in one click. */
function LibrarySection({
  session,
  lang,
  selectedSlot,
  onEquip,
}: {
  session: BuildSession;
  lang: Lang;
  selectedSlot: string | null;
  onEquip: (text: string) => void;
}) {
  const damage = useHeadlineDamage(session.calc, lang);
  const tt = bindT(lang);
  const [comparison, setComparison] = useState<{ id: string; slot: string;
    source: BuildSession['currentRequest']; before: DisplayStatValue[]; after: DisplayStatValue[]; error?: string } | null>(null);
  const diffFor = comparison?.slot === selectedSlot && comparison.source === session.currentRequest ? comparison : null;
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [slotOnly, setSlotOnly] = useState(true);
  const [buildOnly, setBuildOnly] = useState(false);
  const all = session.library.items.filter((i) => i.kind === 'item');
  // Translate `"Name·Base"` display names in batch so Chinese UI search matches both localized and English names.
  const names = useItemDisplayNames(
    all.map((i) => i.text),
    lang,
  );
  const q = query.trim().toLowerCase();
  const items = all.filter((entry, i) => {
    if (buildOnly && (!entry.sourceBuild || entry.sourceBuild.id !== session.workspace?.activeBuild)) return false;
    // When "Selected slot only" is checked, hide legacy items lacking a recorded slot (unknown slot != matching slot).
    if (
      slotOnly &&
      selectedSlot &&
      (!entry.slot || slotFamily(entry.slot) !== slotFamily(selectedSlot))
    ) {
      return false;
    }
    if (!q) return true;
    return (
      entry.text.toLowerCase().includes(q) || (names[i] ?? '').toLowerCase().includes(q)
    );
  });

  const canUse = (entry: LibraryItem) => Boolean(selectedSlot && entry.slot && slotFamily(entry.slot) === slotFamily(selectedSlot));
  /** The current build with `entry` in the selected slot. */
  const withEntry = (entry: LibraryItem) => {
    const request = session.currentRequest();
    if (!request || !selectedSlot) return null;
    if (isUtilitySlot(selectedSlot)) return { ...request, flasks: replaceUtility(request.flasks ?? [], selectedSlot, entry.text) };
    const field = isLimbSlot(selectedSlot) ? 'limbs' : 'items';
    const rest = (request[field] ?? []).filter((it) => it.slot !== selectedSlot);
    return { ...request, [field]: [...rest, { slot: selectedSlot, text: entry.text }] };
  };
  const preview = usePreview();
  const compare = async (entry: LibraryItem) => {
    const candidate = withEntry(entry);
    if (!candidate || !selectedSlot || !session.calc || !canUse(entry)) return;
    const origin = { id: entry.id, slot: selectedSlot, source: session.currentRequest };
    try {
      const before = session.calc.stats;
      const next = await (await getBackend()).calculateBuild(candidate);
      setComparison({ ...origin, before, after: next.stats });
    } catch (error) {
      setComparison({ ...origin, before: [], after: [], error: formatApiError(error) });
    }
  };

  const equip = (entry: LibraryItem) => {
    if (!canUse(entry)) return;
    onEquip(entry.text);
    setComparison(null);
  };

  if (all.length === 0) return <p className="items-hint">{tt('lib.empty')}</p>;
  return (
    <>
      <div className="library-toolbar" role="group" aria-label={tt('lib.title')}>
        <AppInput
          type="search"
          className="library-search"
          placeholder={tt('lib.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={tt('lib.search')}
        />
        <label className="library-slot-filter"><AppInput type="checkbox" aria-label={tt('lib.thisBuild')} checked={buildOnly}
          onChange={event => setBuildOnly(event.target.checked)} />{tt('lib.thisBuild')}</label>
        {selectedSlot && (
          <label className="library-slot-filter">
            <AppInput
              type="checkbox"
              checked={slotOnly}
              onChange={(e) => setSlotOnly(e.target.checked)}
            />
            {tt('lib.filterSlot')}（{slotLabel(lang, selectedSlot)}）
          </label>
        )}
      </div>
      {items.length === 0 && <p className="items-hint">{tt('lib.noMatch')}</p>}
    <div className="library-list">
      {items.map((entry) => (
        <ItemRow
          key={entry.id}
          text={entry.text}
          name={names[all.indexOf(entry)] ?? entry.name}
          lang={lang}
          source={entry.sourceBuild ? tt('lib.sourceBuild').replace('{name}', session.workspace?.builds.find(build => build.id === entry.sourceBuild?.id)?.name || entry.sourceBuild.name || tt('lib.unnamedBuild')) : tt('lib.sourceUnknown')}
          slotTag={entry.slot ? slotLabel(lang, entry.slot) : undefined}
          hover={preview && selectedSlot && canUse(entry) ? {
            onEnter: event => preview.show({ key: `${selectedSlot}:${entry.id}`, label: `${slotLabel(lang, selectedSlot)} ← ${names[all.indexOf(entry)] ?? entry.name}`, request: () => withEntry(entry) }, event.currentTarget),
            onLeave: preview.clear,
          } : undefined}
          open={expandedId === entry.id}
          onToggle={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
          actions={
            <>
              <CopyButton text={entry.text} lang={lang} />
              {canUse(entry) && <>
                <AppButton
                  disabled={session.busy}
                  onClick={() => {
                    setExpandedId(entry.id);
                    void compare(entry);
                  }}
                >
                  {tt('lib.compare')}
                </AppButton>
                <AppButton disabled={session.busy} onClick={() => equip(entry)}>
                  {tt('lib.equip')}
                </AppButton>
              </>}
              <AppButton
                title={tt('lib.delete')}
                onClick={() => session.removeLibraryItem(entry.id)}
              >
                {tt('lib.delete')}
              </AppButton>
            </>
          }
        >
          {diffFor?.id === entry.id && (
            <div className="library-diff">
              {diffFor.error ? <p className="calc-error" role="alert">{diffFor.error}</p> : <SwapGainCard before={diffFor.before} after={diffFor.after} lang={lang} damage={damage} />}
            </div>
          )}
        </ItemRow>
      ))}
    </div>
    </>
  );
}
