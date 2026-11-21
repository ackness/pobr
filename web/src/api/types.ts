/**
 * pobr-wasm JSON contract types (1:1 mirror of `apps/pobr-wasm/src/build_api.rs`).
 *
 * Shared request DTOs are generated from Rust; remaining response/view types
 * are checked by contract_golden.rs and the real WASM request-conversion tests.
 */

import type {
  CharacterOverride, GemInput, JewelInput, SlotIssue, SlotItemInput, SocketGroupInput, UtilitySlotInput, WeaponSwap,
} from './generated/request.js';
export type {
  CharacterOverride, GemInput, JewelInput, SlotIssue, SlotItemInput, SocketGroupInput, UtilitySlotInput, WeaponSwap,
} from './generated/request.js';

/**
 * JSON contract schema version paired with Rust `pobr_wasm::SCHEMA_VERSION`.
 * Bump both on breaking shape changes; verified during boot handshake (see `wasmBackend.ts`).
 */
export const EXPECTED_SCHEMA_VERSION = 22;

// ---------------------------------------------------------------------------
// Error contract (returned on Err; parsed by `./error.ts::parseApiError`)
// ---------------------------------------------------------------------------

export type { DataInfo } from './generated/dataInfo.js';

export type ApiErrorCode = 'not_initialized' | 'bad_request' | 'decode_error' | 'internal' | 'unknown_tree_version' | 'tree_version_mismatch';

/** Wasm error JSON payload: `{code, message, slot?}`. */
export interface ApiErrorJson {
  code: ApiErrorCode;
  message: string;
  /** Failing slot (equipment slot ID or Flask/Charm slot name) for targeted alerts. */
  slot?: string;
}

// ---------------------------------------------------------------------------
// decode_build_json
// ---------------------------------------------------------------------------

export interface CharacterJson {
  level: number;
  class_name: string;
  ascendancy_name: string;
}

export type AttributeChoice = 'str' | 'dex' | 'int';

export interface TreeJson {
  allocated_nodes: number[];
  tree_version: string | null;
  /** 3-way attribute choice per travel node (node skill id -> `str`/`dex`/`int`). */
  attribute_choices: Record<string, AttributeChoice>;
}

export interface SlotItemJson {
  slot: string;
  text: string;
}

// ---------------------------------------------------------------------------
// classify_item_lines_json (item text -> per-line kind, for Items panel coloring)
// ---------------------------------------------------------------------------

export type ItemLineKind =
  | 'name'
  | 'base'
  | 'struct'
  | 'implicit'
  | 'explicit'
  | 'enchant'
  | 'rune'
  | 'class_req';

/** Single item display line: `text` has markup stripped; `kind` drives styling. */
export interface ItemLineJson {
  text: string;
  kind: ItemLineKind;
  /** Affix tier (`1` = strongest in pool; emitted only for explicit mods resolved in backend lookup). */
  tier?: number;
  /** Total tiers available in the same pool for this base (paired with `tier`). */
  tier_total?: number;
  /** Confirmed prefix/suffix side (may be present even when tier is unknown). */
  affix?: 'prefix' | 'suffix';
  modifier_sources?: ('crafted' | 'fractured' | 'desecrated' | 'mutated' | 'custom')[];
  /** Verified game modifier tags, using stable English IDs such as `cold`. */
  mod_tags?: string[];
}

export interface SocketJewelJson {
  /** Passive tree node skill ID of the jewel socket. */
  socket_node: number;
  text: string;
}

export interface ItemsJson {
  equipped: SlotItemJson[];
  jewels: string[];
  /** Passive tree socketed jewels (editable on Tree page). */
  socket_jewels: SocketJewelJson[];
  /** Equipment socket jewels retain their exact PoB ItemSet slot names. */
  equipment_jewels?: SlotItemJson[];
  flasks: (SlotItemJson & { active: boolean })[];
  /** Graft/limb slots (`arm1`/`arm2`/`leg1`/`leg2`): sibling slots next to gear whose mods apply globally. */
  limbs: SlotItemJson[];
}

export interface GemJson {
  /** Omitted means enabled. */
  enabled?: boolean;
  count?: number | null;
  enable_global_1?: boolean | null;
  enable_global_2?: boolean | null;
  stat_set_index?: number | null;
  /** Raw PoB name for an unresolved slot; display/export metadata, never calculation identity. */
  name_spec?: string | null;
  /** Present and true only for a corrupted gem. */
  corrupted?: boolean;
  /** The monster a minion gem summons (PoB skillMinion); a Companion's beast or a Spectre's monster sets its Spirit reservation. */
  minion?: string;
  /** Which of the minion's skills it uses (PoB skillMinionSkill, 1-based); omitted is the first. */
  minion_skill?: number;
  skill_id: string;
  level: number;
  quality: number;
}

export interface SocketGroupJson {
  include_in_full_dps?: boolean;
  group_count?: number | null;
  weapon_set?: 1 | 2 | null;
  slot: string | null;
  enabled: boolean;
  /** PoB's 1-based selection among this group's active skills. */
  main_active_skill?: number | null;
  /** Source granting this skill from PoB equipment/tree; `null` for manual groups. */
  source: string | null;
  active_skill_id: string | null;
  gems: GemJson[];
}

export type ConfigInputValue = boolean | number | string;

export interface CustomModifierBlock {
  title: string;
  enabled: boolean;
  text: string;
}

export interface BuildPlannerExport { files: { filename: string; content: string }[]; warnings: string[] }

export interface NamedSetJson { id: number; title: string }
export interface SetSelection { tree?: number | null; item?: number | null; skill?: number | null; config?: number | null }

export interface BuildJson {
  planner_source?: string;
  config_sets?: NamedSetJson[];
  active_config_set?: number;
  skill_sets?: NamedSetJson[];
  active_skill_set?: number;
  custom_modifier_blocks?: CustomModifierBlock[];
  party?: Party;
  weapon_swap?: WeaponSwap | null;
  character: CharacterJson;
  tree: TreeJson;
  items: ItemsJson;
  socket_groups: SocketGroupJson[];
  /** 0-based main socket group index; `null` = unspecified. */
  main_socket_group: number | null;
  config_inputs: Record<string, ConfigInputValue>;
  /** Free-form `<Notes>` text (PoB Notes tab). */
  notes: string | null;
  /** Switchable loadout presets (PoB2 loadouts); a single-set build has one `Default` entry. */
  loadouts: LoadoutJson[];
  /** Index of the currently active loadout; `null` if indeterminate. */
  active_loadout: number | null;
}

export interface SelectedLoadoutJson extends BuildJson {
  /** Build code with the returned loadout selected; use as the next export base. */
  code: string;
}

/**
 * A switchable loadout — PoB2 binds passive tree, items, and skills into a group
 * via `title` naming conventions (`Build.lua::SyncLoadouts`), requiring zero format
 * extensions and keeping bidirectional compatibility with PoB2.
 *
 * Grouping syntax: matching set titles, or curly braces (e.g. `"Leveling {lvl30}"`;
 * `{a,b}` binds one set to multiple loadouts). Categories with a single set stay unlinked (`null`).
 */
export interface LoadoutJson {
  config?: number | null;
  name: string;
  /** 1-based document index, sent back to `switchLoadout`. */
  tree: number;
  item: number | null;
  skill: number | null;
}

// ---------------------------------------------------------------------------
// calculate_build_json
// ---------------------------------------------------------------------------

export type EnemyTier = 'none' | 'boss' | 'pinnacle' | 'uber';

export interface MinionCatalog {
  /** `monster_variety_id`: the game MonsterVarieties Id display names are keyed by (absent when unlinked). */
  monsters: Array<{ id: string; monster_variety_id?: string; name: string; beast: boolean; spectre: boolean;
    skills: Array<{ index: number; skill_id: string; name: string | null }> }>;
  summoners: Array<{ skill_id: string; choice: 'beast' | 'spectre' | null; minion_ids: string[] }>;
}

export type BreakdownActor = { kind: 'player' } | { kind: 'minion'; index: number };
export interface MinionOutput {
  index: number;
  minion_id: string;
  /** The game MonsterVarieties Id display names are keyed by; absent when the data links none. */
  monster_variety_id?: string;
  name: string;
  summoned_by: string;
  skill_id: string;
  level: number;
  dps: number;
  life: number;
  armour: number;
  evasion: number;
  energy_shield: number;
  stats: Record<string, number>;
  breakdowns: Record<string, Breakdown>;
  breakdown_descriptors?: Record<string, DetailKind>;
}

/** Gem catalog entry (`gem_catalog_json`). */
export interface SkillForm {
  index: number;
  id: string;
  label: string | null;
  stage: { config_input: string; min: number; max: number; default: number } | null;
}

export interface GemCatalogEntry {
  effect_forms?: Array<{ skill_id: string; stat_sets: SkillForm[] }>;

  skill_id: string;
  /** Secondary effects supplied by this gem; display aliases, not picker entries. */
  additional_skill_ids?: string[];
  /** Canonical ordinal contributors: primary then granted active effects, never stat-set aliases. */
  active_effect_ids: string[];
  name: string;
  /** Display name from locale data; English explicitly blocks an unresolved native name. */
  name_zh_tw: string | null;
  /** Display name from locale data; English explicitly blocks an unresolved native name. */
  name_zh_cn: string | null;
  /** Gem attribute color (`str` red / `dex` green / `int` blue, for picker filtering). */
  colour: 'str' | 'dex' | 'int' | null;
  is_support: boolean;
  /** Lineage special support gem (badge + optimizer candidate filtering). */
  is_lineage: boolean;
  /** Unique-granted effect hidden by the picker; retained for imported skill metadata. */
  is_unique_only: boolean;
  /**
   * Skill tags (sorted, deduplicated). Active gems = granted effect's `skill_types`; support gems =
   * `require_skill_types` (what it can support), with AND/OR/NOT logic operators stripped.
   * Contains internal engine tokens; pass through `lib/gemTags` allowlist before rendering.
   */
  tags: string[];
}

/** Node power entry (`node_power_json`; PoB2 heatmap semantics). */
export interface NodePowerEntry {
  /** Passive node skill ID. */
  skill: number;
  /** Target stat delta after single-node trial allocation (may be negative). */
  delta: number;
  /** BFS step count from the allocated frontier (1 = adjacent; explicit probe = 0; path cost comes from planner). */
  distance: number;
}

export interface NodePowerResponse {
  /** Baseline stat value. */
  base: number;
  entries: NodePowerEntry[];
  /** Total candidate count for the request; included only when `window` is specified. */
  total?: number;
}

/** Window slice of candidate list: allows stepped heatmap calculation with progress reporting and cancellation. */
export interface NodePowerWindow {
  offset: number;
  limit: number;
}

// ---------------------------------------------------------------------------
// optimize_variants_json (generic variant evaluation: compute side of optimizer)
//
// Boundary contract: Rust computes variant -> stat values; scoring/constraints/ranking live in `lib/optimize.ts`.
// ---------------------------------------------------------------------------

/** Stat constraint (`display_catalog` stat ID + optional lower/upper bounds). */
export interface StatConstraint {
  stat: string;
  min?: number;
  max?: number;
}

/** Append gems to a socket group (gem combination optimizer channel). */
export interface AddGemsInput {
  group_index: number;
  gems: GemInput[];
}

/** Single variant: channels combine freely; all empty = baseline re-eval. */
export interface VariantInput {
  /** Display label echoed into result rows. */
  label?: string;
  add_gems?: AddGemsInput;
  /** Override equipment slots (empty `text` removes the item). */
  set_items?: SlotItemInput[];
  /** Whole-list replacements, preserving the ordinary edit/import validation. */
  jewels?: JewelInput[];
  flasks?: UtilitySlotInput[];
  socket_groups?: SocketGroupInput[];
  /** Speculative passive node allocations (skips connectivity validation). */
  allocate_nodes?: number[];
  deallocate_nodes?: number[];
  /** Arbitrary mod text fallback ("anything expressible as mods can be evaluated"). */
  extra_modifiers?: string[];
}

export interface OptimizeVariantsRequest {
  request: CalculateBuildRequest;
  /** Display stat IDs to collect (unknown IDs record 0). */
  stats: string[];
  variants: VariantInput[];
  /** Defaults to true; set false in subsequent batches to skip redundant baseline calculation. */
  include_baseline?: boolean;
}

export interface VariantStats {
  /** Index within request (maps back to variant definition). */
  index: number;
  label: string | null;
  /** Empty map when evaluation fails, with reason in `error`. */
  stats: Record<string, number>;
  error: string | null;
  unsupported?: string[];
}

export interface OptimizeVariantsResponse {
  baseline: Record<string, number> | null;
  variants: VariantStats[];
}

/** Rune / Soul Core catalog entry (`rune_catalog_json`). */
export interface RuneCatalogEntry {
  /** Canonical English name (key for `Rune:` lines and reforge requests). */
  name: string;
  name_zh_tw: string | null;
  name_zh_cn: string | null;
  is_soul_core: boolean;
  kind?: 'Rune' | 'SoulCore' | 'Idol';
  required_level?: number;
  limit?: number;
  limit_id?: string;
  /** Applicable effect mod lines for the supplied item base (empty when omitted or inapplicable). */
  lines: string[];
}

export interface ItemAugmentInfo {
  sockets: number;
  max_sockets: number;
  /** One entry per socket; empty strings preserve empty positions. */
  runes: string[];
  editable: boolean;
  reason?: string;
  options: RuneCatalogEntry[];
}

/** Supply at least one of `pob_code` or `character` (no code = PoB2 "New Build" semantics). */
export interface CalculateBuildRequest {
  /** Omitted: legacy defaults; []: no details; otherwise only the opened ids. */
  breakdowns?: string[];
  breakdown_actor?: BreakdownActor;
  /**
   * Put `breakdown_descriptors` in the response. It is left out by default (the table
   * depends only on the engine build; take it once from `breakdownDescriptors`).
   */
  include_descriptors?: boolean;
  /** Collect configuration visibility facts; omitted keeps read tracking off. */
  config_context?: boolean;
  /** Evaluate included effects; omitted keeps the single-skill calculation. */
  full_dps?: boolean;
  tree_version?: string | null;
  pob_code?: string;
  character?: CharacterOverride;
  /** Replace entire allocated node set (interactive tree allocation). */
  allocated_nodes?: number[];
  /** Replace entire 3-way attribute choice map. */
  attribute_choices?: Record<string, AttributeChoice>;
  /** Replace entire socket group list (manual edits). */
  socket_groups?: SocketGroupInput[];
  /** Replace entire equipment item list (manual edits). */
  items?: SlotItemInput[];
  /**
   * `active` selects which weapon set (1 or 2) applies during this calculation, driving
   * "while using weapon set 1/2" modifiers; the other weapon set and set-exclusive passives
   * exist only for export. Omit to inherit from build code (new builds default to 1).
   */
  weapon_swap?: WeaponSwap | null;
  /** Omitted keeps baseline utilities; [] clears all. Selection defaults true for legacy entries. */
  flasks?: UtilitySlotInput[];
  /** Replace all graft/limb slots (`arm1`/`arm2`/`leg1`/`leg2`). */
  limbs?: SlotItemInput[];
  /** Replace all passive tree socketed jewels. */
  jewels?: JewelInput[];
  /** Replace equipment-socket jewels independently of the passive tree. */
  equipment_jewels?: SlotItemInput[];
  main_socket_group?: number;
  mode_effective?: boolean;
  enemy_tier?: EnemyTier;
  extra_modifiers?: string[];
  custom_modifier_blocks?: CustomModifierBlock[];
  party?: Party;
  config_inputs?: Record<string, ConfigInputValue>;
}

/** `display_catalog` category (verbatim Rust `DisplayStatCategory` enum variants). */
export type DisplayStatCategory =
  | 'Offence'
  | 'HitDamage'
  | 'DotDamage'
  | 'Ailment'
  | 'SkillMechanics'
  | 'Defence'
  | 'Resistance'
  | 'Avoidance'
  | 'Mitigation'
  | 'Resource'
  | 'Recovery'
  | 'Degen'
  | 'Cost'
  | 'Requirement'
  | 'Minion'
  | 'Enemy'
  | 'Utility'
  | 'Misc';

/** Static engine metadata from displayCatalogJson; keep contract_golden.rs in sync. */
export interface DisplayCatalogEntry {
  id: string;
  category: DisplayStatCategory;
  value_type: 'Number' | 'Percent' | 'TimeSeconds' | 'Text' | 'Bool' | 'Unknown';
  higher_is_better: boolean | null;
}

export interface DisplayStatValue {
  id: string;
  /** `null` = marked unavailable by the calculation side (e.g. `∞` for ChaosMaxHit). */
  value: number | null;
  category: DisplayStatCategory;
}

export type BreakdownModType = 'BASE' | 'INC' | 'MORE' | 'FLAG' | 'OVERRIDE' | 'LIST';

export interface BreakdownMod {
  mod_type: BreakdownModType;
  /** Applied number for an active BASE/INC/MORE line; the stored number when inactive. */
  value: number | null;
  /** False when the inspected skill's calculation does not count this line. */
  active: boolean;
  source_text: string | null;
  origin_kind: string | null;
  origin_id: string | null;
  slot: string | null;
}

export type FormulaOp =
  | 'base'
  | 'flat'
  | 'enemy_flat'
  | 'converted'
  | 'increased'
  | 'more'
  | 'added_time'
  | 'action_speed'
  | 'override'
  | 'cap'
  | 'cooldown_cap'
  | 'server_tick_cap'
  | 'floor'
  | 'bonus_to_multiplier'
  | 'gear_adjusted'
  | 'gained'
  | 'slot_scaled'
  | 'flat_after'
  | 'round'
  | 'hit_chance'
  | 'lucky'
  | 'bifurcate'
  | 'inevitable'
  | 'inevitable_penalty'
  | 'bifurcate_bonus'
  | 'enemy_increased'
  | 'accuracy_rating'
  | 'vs_evasion'
  | 'always_hits'
  | 'enemy_block'
  | 'non_crit_hit'
  | 'crit_hits'
  | 'average_hit'
  | 'enemy_mitigation'
  | 'uses_per_second'
  | 'chance_to_hit'
  | 'dps_multiplier'
  | 'skill_dot'
  | 'poison_dps'
  | 'ignite_dps'
  | 'bleed_dps'
  | 'hit_dps'
  | 'dot_dps'
  | 'ailment_source'
  | 'per_second'
  | 'active_stacks'
  | 'chance_to_apply'
  | 'ailment_effect'
  | 'ailment_rate'
  | 'enemy_increase_added'
  | 'never_crit'
  | 'no_crit_bonus'
  | 'main_hand'
  | 'off_hand_average'
  | 'off_hand_added'
  | 'off_hand_either'
  | 'off_hand_alternating'
  | 'reload'
  | 'energy_trigger'
  | 'hit_interval'
  | 'channel_attack_time'
  | 'channel_maximum_stages'
  | 'channel_default_stages'
  | 'channel_requested_stages'
  | 'channel_stages'
  | 'channel_time'
  | 'channel_release'
  | 'stronger_hand'
  | 'stronger_share'
  | 'weaker_hand';

export interface FormulaStep {
  op: FormulaOp;
  /** Amount applied: percentage points (increased, converted, hit_chance, chance_to_hit, crit_hits, enemy_block, the bonus and penalty steps), the enemy's evasion (vs_evasion), a multiplier (more, action_speed, enemy_mitigation, uses_per_second, dps_multiplier, ailment_effect, ailment_rate), seconds (added_time, channel_attack_time, channel_time, channel_release), stage counts (channel_maximum_stages, channel_default_stages, channel_requested_stages, channel_stages), the limit, the number of rolls (lucky, bifurcate), the starting damage per second (skill_dot, hit_dps), the hit an ailment starts from (ailment_source), the percentage of it dealt per second (per_second), the number of instances (active_stacks), the chance in percent (chance_to_apply), a hand's own value (main_hand, the off_hand steps, stronger_hand, weaker_hand), the percentage of instances from the stronger hand (stronger_share), a multiplier (reload), the seconds before a target can be hit again (hit_interval) or the amount added. */
  operand: number;
  /** Running value after the step. */
  result: number;
}

export interface BreakdownFormula {
  steps: FormulaStep[];
  /** The displayed stat value the steps end at. */
  value: number;
}

export type DetailKind = 'steps' | 'sources' | 'derived' | 'none';
export interface DetailInput { id: string; value: number | null }
export interface SourceDetail {
  name: string;
  more_precision: number | null;
  base_total: number;
  inc_total: number;
  more_total: number;
  mods: BreakdownMod[];
}
export interface Breakdown {
  kind?: DetailKind;
  sources?: SourceDetail[];
  derived?: { formula: string; inputs: DetailInput[]; context: DetailInput[] };
  actor?: BreakdownActor;
  /** Totals cover active lines only. */
  base_total: number;
  inc_total: number;
  /** Product of the active MORE lines as a multiplier (1 when there are none). */
  more_total: number;
  mods: BreakdownMod[];
  /** Steps from the starting value to the displayed stat; null when not available for this stat. */
  formula: BreakdownFormula | null;
  /** TotalHitAvg only: how each damage type of the hit is built. */
  damage?: HitDamageTable;
  /** TotalDPS only: the `enemy_mitigation` step per damage type. */
  mitigation?: HitMitigationRow[];
  /** TotalDotDPS only: the `skill_dot` step per damage type. */
  skill_dot?: SkillDotRow[];
  /** IgniteDPS / BleedDPS / PoisonDPS only: the hit the ailment comes from and how many instances are active. */
  ailment?: AilmentDetail;
  /** ActionRate / TotalDPS only: the cycle behind the `reload` step. */
  reload?: ReloadDetail;
  /** Armour / Evasion / EnergyShield only: the `slot_scaled` step per item slot; the `extra` amounts add up to its operand. */
  slots?: DefenceSlotRow[];
  /**
   * Two weapon passes only, for the stats each pass computes on its own (ActionRate, HitChance,
   * CritChance, CritMultiplier, TotalHitAvg, TotalDPS and an ailment both hands apply): main hand
   * then off hand. `formula` then merges the two hands' values, and `damage` / `mitigation` /
   * `ailment` are listed per hand.
   */
  hands?: BreakdownHand[];
}

/** One weapon pass's own breakdown; its `formula` ends at the value the merge starts from. */
export interface BreakdownHand extends Breakdown {
  hand: 'main_hand' | 'off_hand';
}

/** The crossbow reload cycle: firing a magazine, then reloading. */
export interface ReloadDetail {
  /** Uses per second while firing (the value before the `reload` step). */
  firing_rate: number;
  /** Bolts fired per magazine, the chance not to consume one included; null when none is ever consumed. */
  bolts: number | null;
  /** Seconds to fire the magazine. */
  firing_time: number;
  /** Seconds to reload, the chance to reload instantly included. */
  reload_time: number;
  /** `bolts / (firing_time + reload_time)`; the `reload` step multiplies by `rate / firing_rate`. */
  rate: number;
}

/** What a damaging ailment's steps are built from. */
export interface AilmentDetail {
  /** Non-critical and critical hit that applies the ailment, before the enemy's mitigation, at `damage_roll` percent of the damage range. */
  source_hit: number;
  source_crit: number;
  /** 50 = the average hit; higher when more instances are applied than can be active. */
  damage_roll: number;
  /** Percentage of instances applied by a critical hit; the starting hit is the two hits averaged with this share. */
  crit_share: number;
  /** Chance for a hit to apply the ailment, in percent. */
  apply_chance: number;
  /** Chance to hit, in percent. */
  hit_chance: number;
  /** Seconds one instance lasts. */
  duration: number;
  hits_per_second: number;
  /** `hit_chance × apply_chance × duration × hits_per_second`; 0 when the skill has no hit rate. */
  stacks: number;
  /** Most instances that deal damage at once; 0 means no limit for this existing ailment. */
  max_stacks: number;
  /** `source_hit` and `source_crit` per damage type that can apply the ailment; the rows add up to the two totals. */
  source: { damage_type: string; hit: number; crit: number }[];
  /** `duration` before modifiers, in seconds. */
  base_duration: number;
  /** Multiplier from how fast effects expire on the enemy; `duration` is `base_duration` scaled by `duration_mods`, times this, divided by `rate`. */
  enemy_duration: number;
  /** How much faster the ailment deals its damage (the `ailment_rate` step): the same damage in less time. */
  rate: number;
  /** The player's lines behind `apply_chance`, `duration`, the `ailment_effect` step and the `ailment_rate` step. Modifiers on the enemy are not listed. */
  chance_mods: BreakdownMod[];
  duration_mods: BreakdownMod[];
  effect_mods: BreakdownMod[];
  rate_mods: BreakdownMod[];
}

/** One damage type of the skill's own damage over time (not an ailment). */
export interface SkillDotRow {
  damage_type: string;
  /** Damage per second before scaling, from the skill gem. */
  base: number;
  /** INC total in percentage points. */
  increased: number;
  /** MORE product as a multiplier. */
  more: number;
  /** Damage over time multiplier in percentage points. */
  dot_multiplier: number;
  /** Aura effect multiplier (1 unless the skill is an aura). */
  aura: number;
  /** The enemy's resistance and damage taken as one multiplier. */
  enemy: number;
  /** `base × (1 + increased / 100) × more × (1 + dot_multiplier / 100) × aura × enemy`; the rows add up to the `skill_dot` step. */
  total: number;
  /** INC / MORE damage lines and BASE damage over time multiplier lines this type reads. */
  mods: BreakdownMod[];
}

/** One damage type of the main skill's hit, before the enemy's mitigation. */
export interface HitDamageRow {
  /** Physical / Fire / Cold / Lightning / Chaos. */
  damage_type: string;
  /** Range before increases: weapon or skill base, added damage, and what conversion and gain moved into this type. */
  base_min: number;
  base_max: number;
  /** INC total in percentage points. */
  increased: number;
  /** MORE product as a multiplier. */
  more: number;
  /** MORE multipliers that apply to the minimum or the maximum only. */
  more_min: number;
  more_max: number;
  /** Range of a non-critical hit. */
  min: number;
  max: number;
  /** Average non-critical hit. */
  avg: number;
  /** Average critical hit. */
  crit_avg: number;
  /** INC / MORE lines that scale this type only. With `shared_mods`, active INC values add up to `increased` and active MORE values multiply to `more` (rounded to two decimals per modifier name). */
  mods: BreakdownMod[];
  /** MORE lines behind `more_min` and `more_max`. */
  min_mods: BreakdownMod[];
  max_mods: BreakdownMod[];
}

/** What the lines scoped to one item slot add to a defence. */
export interface DefenceSlotRow {
  /** Slot name, as in `mods[].slot`. */
  slot: string;
  /** The amount in that slot the lines scale (the item's value after keystones and conversion; an amount converted in from another defence is its own row). */
  base: number;
  /** INC total (percentage points) and MORE product of the lines scoped to this slot; they add to and multiply with the global ones. */
  increased: number;
  more: number;
  /** What those lines add on top of the global increases. */
  extra: number;
}

/** One damage type of the hit before conversion and gain. */
export interface HitDamageSource {
  damage_type: string;
  /** Range before conversion: for physical the weapon's or skill's own hit plus `added × added_effect`, otherwise `added × added_effect`. */
  min: number;
  max: number;
  /** Totals of the added damage lines, before `added_effect`. */
  added_min: number;
  added_max: number;
  /** Added damage effectiveness, a multiplier. */
  added_effect: number;
  /** BASE lines of added minimum / maximum damage (a skill's own non-physical damage is one of them). */
  added_min_mods: BreakdownMod[];
  added_max_mods: BreakdownMod[];
  added_effect_mods: BreakdownMod[];
  /** Lines that convert this type to another, and lines that add a share of it as another. How much each moves is not listed. */
  convert_mods: BreakdownMod[];
  gain_mods: BreakdownMod[];
}

/** One damage type of the average hit against the configured enemy. */
export interface HitMitigationRow {
  damage_type: string;
  /** The type's share of the average hit (crits included), before the enemy. */
  average: number;
  /** Enemy resistance before penetration; null for physical. */
  resist: number | null;
  /** What a non-critical hit is reduced by, in percent: resistance after penetration, or for physical the reduction from armour, flat reduction and Overwhelm. Negative means more damage. */
  reduction: number;
  /** The enemy's damage-taken multiplier. */
  taken: number;
  /** `mitigated / average`. */
  multiplier: number;
  /** The share after the enemy; the rows add up to the `enemy_mitigation` step's result. */
  mitigated: number;
  /** Percentage of hits that treat the resistance as inverted (0 without such a modifier); `resist` already includes it. */
  resist_inverted: number;
  /** The enemy's own lines behind `resist` (exposure and curses included; an OVERRIDE line replaces the others, and the total is kept within the resistance limits), or for physical its armour and flat reduction lines. */
  defence_mods: BreakdownMod[];
  /** The enemy's own lines behind `taken`. */
  taken_mods: BreakdownMod[];
}

export interface HitDamageTable {
  rows: HitDamageRow[];
  /** Each damage type the hit starts with, before conversion and gain move it into the `rows` ranges. */
  sources: HitDamageSource[];
  /** INC / MORE lines that scale every damage type of the hit. */
  shared_mods: BreakdownMod[];
  /** Double / triple damage multiplier every range includes (1 without one). */
  damage_effect: number;
}

/** Single damage-type slice of main-skill hit damage (non-crit roll, before enemy mitigation; share uses `avg`). */
export interface HitDamagePart {
  /** `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`. */
  damage_type: string;
  min: number;
  max: number;
  avg: number;
}

/** Current inspected skill identity and damage breakdown, refreshed on calculation. */
export interface MainSkillInfo {
  /** Selected socket group (0-based, aligned with request `socket_groups`). */
  group_index: number;
  /** PoB's 1-based ordinal among all active effects in the group. */
  active_skill_index: number;
  /** Granted effect ID of this group's main active skill. */
  skill_id: string;
  hit_damage: HitDamagePart[];
  /** Hit DPS (`TotalDPS`). */
  hit_dps: number;
  /** Total DoT DPS (`TotalDotDPS`). */
  dot_dps: number;
  /** Combined DPS (`CombinedDPS`). */
  combined_dps: number;
}

export interface FullDpsSkill {
  group_index: number;
  active_skill_index: number;
  skill_id: string;
  count: number;
  hit_dps: number;
  dot_dps: number;
  /** Total contribution, including count and culling; rows sum to FullDPS. */
  dps: number;
  unsupported_modifiers: string[];
  error: string | null;
}

export interface EnergyTriggerTerms {
  maximum_energy: number;
  energy_per_trigger: number;
  source: string | null;
  at_own_rate: boolean;
  missing_source: string | null;
  events_per_second: number;
  energy_per_event: number;
  refund_half_chance: number;
  events_per_trigger: number;
  energy_limited_rate: number;
  trigger_rate_cap: number;
  triggers_per_second: number;
  invocation: boolean;
  rate_multiplier: number;
}

export interface UnmodelledMetaTrigger {
  /** Granted effect ID of the meta gem whose Energy event rate is not modelled. */
  meta_gem_id: string;
  host_skill_id: string;
}

export interface CalculateBuildResponse {
  /** Engine release and runtime input pack; absent in older saved/mock responses. */
  provenance?: CalculationProvenance;
  /** Present only when config_context was requested. */
  config_context?: ConfigContext;
  minions?: MinionOutput[];
  main_minion_index?: number | null;
  full_dps?: { dps: number; dot_dps: number; skills: FullDpsSkill[] };
  energy_trigger?: EnergyTriggerTerms;
  /** Main skill only; absent for ordinary and modelled triggers. */
  unmodelled_meta_trigger?: UnmodelledMetaTrigger;
  stats: DisplayStatValue[];
  /**
   * Lines the calculation could not apply, so their effect is missing from the
   * numbers. An item's line is given as written; a passive node's as
   * `Tree:<node>: <line>`; a configuration line (custom modifier, quest reward)
   * as `Config: <line>`.
   */
  unsupported_modifiers: string[];
  /** Key = aggregated `ModName` (`Life` / `EnergyShield` / `FireResist` / ...). */
  breakdowns: Record<string, Breakdown>;
  /** Optional for older saved/mock responses. */
  breakdown_descriptors?: Record<string, DetailKind>;
  /** Main skill breakdown (`null` = no resolvable damage main skill). */
  main_skill: MainSkillInfo | null;
  /** Parse failures per gear/flask/jewel slot (highlights slot in red; empty = all succeeded). */
  item_errors: SlotIssue[];
  tree_effects?: PassiveJewelState;
}

export interface CalculationProvenance {
  engine_version: string;
  data_version: string | null;
  /** SHA-256 of the runtime input pack, including shared overlays and patches. */
  data_sha256: string;
}

export interface PassiveAllocationGrant {
  source: number;
  nodes: number[];
  roots: number[];
}

export interface PassiveJewelState {
  class_starts: Record<string, number>;
  allocation_grants: PassiveAllocationGrant[];
  nodes: Record<string, { name: string; stats: string[]; replace: boolean }>;
  conquered: number[];
  unresolved: number[];
  rings: { source: number; center: number; inner: number; outer: number }[];
  warnings: string[];
}

// ---------------------------------------------------------------------------
// attribution_json
// ---------------------------------------------------------------------------

export interface AttributionRequest {
  /** Full calculation request (baseline; same shape as `node_power` / `optimize_variants`). */
  request: CalculateBuildRequest;
  /** `display_catalog` field IDs; defaults to `TotalDPS` / `Life` / `TotalEHP`. */
  fields?: string[];
}

export type AttributionSourceKind = 'item' | 'socket_group' | 'flask' | 'limb';

export interface AttributionEntry {
  kind: AttributionSourceKind;
  /** Equipment slot ID / socket group index string / flask slot name / limb slot ID. */
  id: string;
  /** Stat -> marginal contribution (`baseline - value_without`; positive = gain). */
  deltas: Record<string, number>;
}

export interface AttributionResponse {
  baseline: Record<string, number>;
  entries: AttributionEntry[];
}

// ---------------------------------------------------------------------------
// line_usage_json (same request as calculate_build_json): trace each mod line
// ---------------------------------------------------------------------------

/**
 * Outcome of one line. When modifiers enter the store, status derives from them:
 * all active `applied`, some `partial`, read but unused or unmet read gate `inactive`,
 * never read `unread`. Without modifiers in store: consumed early by the reader named
 * in `reason` (`outside` — local item defence/damage, gem level, jewel radius),
 * registered no-op `no_effect`, recognized by parser but unimplemented `unmodelled`,
 * unrecognized `unparsed`, parsed into modifiers that never entered store `no_modifier`;
 * excluded from this calculation pass -> `inactive` with reason.
 */
export type LineUsageStatus =
  | 'applied'
  | 'partial'
  | 'inactive'
  | 'unread'
  | 'outside'
  | 'no_effect'
  | 'unmodelled'
  | 'unparsed'
  | 'no_modifier';

/** Outcome of one stored modifier; `outside` = read directly outside the modifier store by the consumer in `reason`. */
export type LineUsageModifierStatus = 'applied' | 'inactive' | 'unread' | 'outside';

export interface LineUsageModifier {
  name: string;
  /** `BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`. */
  mod_type: string;
  /** Stored value: number, boolean, string, or `[nested N]`. */
  value: number | boolean | string;
  /** Effective value scaled in main skill context; otherwise `null`. */
  applied: number | null;
  status: LineUsageModifierStatus;
  /** Verified reader only runs under this state; returned only when the state was not reached. */
  read_gate?: { state: string; config_option: string | null };
  /** For `inactive`: unmet condition; for `outside`: consumer name. */
  reason: string | null;
  /** Breakdown stat IDs listing this modifier (active and non-zero). */
  used_in: string[];
}

export interface LineUsageLine {
  /** Attribution source kind (`ItemAffix` / `ItemImplicit` / `PassiveNode` / `Jewel` / `Flask` / `Config` ...). */
  source_kind: string | null;
  /** Source ID: `item.<slot>.<section>`, `node.<id>`, `jewel.<n>`, `flask.<slot>`, `quest`, `config.custom`. */
  source_id: string | null;
  slot: string | null;
  /** Holder label: passive node name, item base type. */
  holder: string | null;
  /** Raw text; `null` for modifiers not backed by a source line (e.g. gem stats). */
  text: string | null;
  status: LineUsageStatus;
  /** For lines with no stored modifiers: reader name, exclusion reason, or parse outcome. */
  reason: string | null;
  /** Breakdown stat IDs reached by any modifier from this line. */
  used_in: string[];
  modifiers: LineUsageModifier[];
}

export interface LineUsageSource {
  /** PassiveNode / Config / Item / Limb / Flask / Jewel。 */
  kind: string;
  id: string | null;
  holder: string | null;
}

export interface LineUsageUnparsed {
  text: string;
  count: number;
  sources: LineUsageSource[];
}

export interface LineUsageGemStat {
  stat: string;
  owner: string;
  /** `unmapped`: mapping exists but engine rejected it (see `reason`); `unknown`: no mapping exists. */
  status: 'unmapped' | 'unknown';
  reason: string | null;
}

export interface LineUsageRef {
  name: string;
  mod_type: string;
  value: string;
  /** `<source_kind>:<source_id> | <raw_text>`. */
  source: string;
}

export interface LineUsageCondition {
  condition: string;
  /** Config option that sets this condition yet is inactive; `null` = no source sets it. */
  config_option: string | null;
  flagged: boolean;
  modifiers: LineUsageRef[];
}

export interface LineUsageMultiplier {
  multiplier: string;
  modifiers: LineUsageRef[];
}

export interface LineUsageSkill {
  skill_id: string;
  level: number;
  quality: number;
}

export interface LineUsageSummary {
  lines: number;
  lines_applied: number;
  lines_partial: number;
  lines_inactive: number;
  lines_unread: number;
  lines_outside: number;
  lines_no_effect: number;
  lines_unmodelled: number;
  lines_unparsed: number;
  lines_no_modifier: number;
  modifiers: number;
  modifiers_applied: number;
  modifiers_inactive: number;
  modifiers_unread: number;
  modifiers_outside: number;
  unparsed: number;
  unmapped_gem_stats: number;
  unknown_gem_stats: number;
  unset_conditions: number;
  unprovided_multipliers: number;
}

export interface LineUsage {
  summary: LineUsageSummary;
  /** Evaluated main skill; reasons are given relative to its context. */
  main_skill: string | null;
  lines: LineUsageLine[];
  /** All unparsed lines recorded during calculation; build lines also appear under `lines` with exact sources. */
  unparsed: LineUsageUnparsed[];
  gem_stats: LineUsageGemStat[];
  conditions: LineUsageCondition[];
  /** Config options for referenced conditions, including active or calculation-derived conditions. */
  condition_configs?: Record<string, string>;
  /** Referenced conditions set by the build or a calc pass, even when unmet. */
  condition_states?: string[];
  multipliers: LineUsageMultiplier[];
  /** Each active effect in enabled groups along with its effective level (including bonuses) and quality. */
  skills: LineUsageSkill[];
  /** Each read modifier name -> queries reading it (e.g. `INC Damage+FireDamage`). */
  reads: Record<string, string[]>;
}

export interface LineUsageResponse {
  usage: LineUsage;
  item_errors: SlotIssue[];
}

// ---------------------------------------------------------------------------
// Static passive tree data (loaded directly from web/public/data/<version>/base/passive_tree.json)
// ---------------------------------------------------------------------------

export type PassiveNodeKind =
  | 'normal'
  | 'notable'
  | 'keystone'
  | 'mastery'
  | 'jewel_socket'
  | 'ascendancy_start';

export interface PassiveNode {
  skill: number;
  id: string;
  name?: string;
  kind: PassiveNodeKind;
  stats?: string[];
  group?: number;
  orbit?: number;
  orbit_index?: number;
  x?: number;
  y?: number;
  connections?: number[];
  ascendancy_id?: string;
  unlock_constraint?: { nodes: number[]; ascendancy?: string };
}

// ---------------------------------------------------------------------------
// Class/ascendancy metadata (`passive_tree_meta.json`, for new build selector)
// ---------------------------------------------------------------------------

export interface PassiveAscendancy {
  id: string;
  name: string;
}

export interface PassiveClass {
  name: string;
  base_str: number;
  base_dex: number;
  base_int: number;
  ascendancies?: PassiveAscendancy[];
}

export interface PassiveTreeMeta {
  tree: string;
  classes: PassiveClass[];
}

/** Simplified Chinese class/ascendancy lookup (`i18n/zh-CN/classes.json`, English -> zh-CN, e.g. `"Druid": "德鲁伊"`). */
export interface ClassNames {
  classes: Record<string, string>;
  ascendancies: Record<string, string>;
  /** Display-only, source-bound names; null blocks older/script-converted text. */
  locales?: Partial<Record<'zh-CN' | 'zh-TW', {
    classes: Record<string, string | null>;
    ascendancies: Record<string, string | null>;
  }>>;
}

// ---------------------------------------------------------------------------
// Built-in config catalog (`overlay/config_options.json`, static asset read —
// shared with calculation-side `config_interpreter`)
// ---------------------------------------------------------------------------

export type ConfigInputType =
  | 'check'
  | 'count'
  | 'count_allow_zero'
  | 'integer'
  | 'float'
  | 'list'
  | 'text';

export interface ConfigListOption {
  value: string;
  label: string;
}

export interface ConfigOption {
  var: string;
  input_type: ConfigInputType;
  /** PoB2 Config page section name (e.g. `"When In Combat"`). */
  section?: string;
  label?: string;
  tooltip?: string;
  visibility?: ConfigVisibility;
  /** Options for `list` type; `default.index` is 1-based. */
  list_options?: ConfigListOption[];
  default?: { index?: number; state_bool?: boolean; state_number?: number; placeholder_number?: number } | number | boolean | string;
}

/** Catalog predicate families are extensible; unknown families stay visible. */
export interface ConfigVisibility {
  [family: string]: string[] | undefined;
  if_skill?: string[];
  if_skill_type?: string[];
  if_cond?: string[];
  if_enemy_cond?: string[];
  if_minion_cond?: string[];
  if_stat?: string[];
  if_flag?: string[];
  if_option?: string[];
}

export interface ConfigContext {
  main_skill: string | null;
  skill_types: string[];
  /** Null or absent means the engine cannot evaluate this visibility rule. */
  predicates: Record<string, Record<string, boolean | null>>;
}

export interface ConfigCatalogFile {
  options: ConfigOption[];
}

// ---------------------------------------------------------------------------
// skill_metrics_json
// ---------------------------------------------------------------------------

export interface SkillMetricsEntry {
  /** Original socket group index (0-based). */
  group_index: number;
  /** PoB's 1-based ordinal among all active effects, including granted effects. */
  active_skill_index: number;
  skill_id: string;
  status: 'calculated' | 'unavailable';
  metrics: {
    hit_dps: number;
    dot_dps: number;
    combined_dps: number;
    average_hit: number;
    action_rate: number;
    mana_cost: number;
    life_cost: number;
    cooldown: number;
  } | null;
  unmodelled_meta_trigger?: UnmodelledMetaTrigger;
  unsupported_modifiers: string[];
  error: string | null;
}

export interface SkillMetricsResponse {
  skills: SkillMetricsEntry[];
  item_errors: SlotIssue[];
}

export type TreeAllocationMode = 0 | 1 | 2;
export type TreeEditAction = 'allocate' | 'refund' | 'promote';
export interface TreeAllocation { shared: number[]; exclusive: [number[], number[]] }
export interface TreeEditResult {
  allocation: TreeAllocation;
  added: number[];
  removed: number[];
  promoted: number[];
  path: number[];
  points_used: number;
  points_max: number;
  weapon_points_used: [number, number];
  weapon_points_max: number;
}
export interface TreeEditResponse {
  valid: boolean;
  reason: string | null;
  result: TreeEditResult | null;
  current_points?: Pick<TreeEditResult, 'points_used' | 'points_max' | 'weapon_points_used'> | null;
}

/** Conversion is a detached preview; the caller must explicitly apply its code. */
export interface TreeConversionNode {
  id: number;
  name: string;
  stats: string[];
  reason: 'missing' | 'changed_identity' | 'socket_missing_or_changed' | 'disconnected' | null;
  jewel_item_id: number | null;
}
export interface TreeConversionResponse {
  code: string;
  specs: {
    index: number;
    title: string;
    from_version: string;
    to_version: string;
    kept: number[];
    dropped: TreeConversionNode[];
    disconnected: TreeConversionNode[];
    disconnected_by_set: [number[], number[], number[]];
    allocation: TreeAllocation;
  }[];
}

export interface PartyMember {
  id: string;
  name: string;
  enabled: boolean;
  blocks: Record<string, string>;
  warnings: string[];
}
export interface Party { members: PartyMember[]; exports: Record<string, string> }
