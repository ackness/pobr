/**
 * pobr-wasm JSON 契约类型（与 `apps/pobr-wasm/src/build_api.rs` 一一对应）。
 *
 * 契约冻结：Rust 侧 `apps/pobr-wasm/tests/contract_golden.rs` 钉住 JSON 键集合；
 * 改这里必须同步改 Rust DTO 与 golden，反之亦然。
 */

/**
 * JSON 契约版本，与 Rust 侧 `pobr_wasm::SCHEMA_VERSION` 配对。
 * 任何破坏性形状变更两侧同时 +1；boot 时握手校验（见 wasmBackend.ts）。
 */
export const EXPECTED_SCHEMA_VERSION = 8;

// ---------------------------------------------------------------------------
// 错误契约（所有接口 Err 侧；解析入口见 ./error.ts::parseApiError）
// ---------------------------------------------------------------------------

export type ApiErrorCode = 'not_initialized' | 'bad_request' | 'decode_error' | 'internal';

/** wasm Err 侧 JSON：`{code, message, slot?}`。 */
export interface ApiErrorJson {
  code: ApiErrorCode;
  message: string;
  /** 出错槽位（装备槽 id / Flask·Charm 槽名），供局部提示。 */
  slot?: string;
}

/** 单件来源文本解析失败的降级记录（该件被跳过，其余照算）。 */
export interface SlotIssue {
  /** 装备槽 id / `Flask N`·`Charm N` / `Jewel@<socket_node>`。 */
  slot: string;
  message: string;
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
  /** 属性小点三选一（node skill id → str/dex/int）。 */
  attribute_choices: Record<string, AttributeChoice>;
}

export interface SlotItemJson {
  slot: string;
  text: string;
}

// ---------------------------------------------------------------------------
// classify_item_lines_json（物品文本 → 逐行类别，Items 面板上色）
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

/** 单条物品展示行：`text` 已剥标注，`kind` 决定前端上色。 */
export interface ItemLineJson {
  text: string;
  kind: ItemLineKind;
  /** 词缀档位（1 = 同池最强；仅 explicit 行且后端反查命中时给出）。 */
  tier?: number;
  /** 该基底可掷的同池总档数（与 tier 成对出现）。 */
  tier_total?: number;
  /** 词缀性质（与 tier 成对出现）。 */
  affix?: 'prefix' | 'suffix';
}

export interface SocketJewelJson {
  /** 珠宝插槽的树节点 skill id。 */
  socket_node: number;
  text: string;
}

export interface ItemsJson {
  equipped: SlotItemJson[];
  jewels: string[];
  /** 树插槽珠宝（天赋树页可编辑）。 */
  socket_jewels: SocketJewelJson[];
  flasks: SlotItemJson[];
  /** 超越肢体（槽 `arm1`/`arm2`/`leg1`/`leg2`）：与装备并列的一组栏位，词条为全局词条。 */
  limbs: SlotItemJson[];
}

export interface GemJson {
  stat_set_index?: number | null;
  /** Raw PoB name for an unresolved slot; display/export metadata, never calculation identity. */
  name_spec?: string | null;
  /** Present and true only for a corrupted gem. */
  corrupted?: boolean;
  skill_id: string;
  level: number;
  quality: number;
}

export interface SocketGroupJson {
  weapon_set?: 1 | 2 | null;
  slot: string | null;
  enabled: boolean;
  /** PoB's 1-based selection among this group's active skills. */
  main_active_skill?: number | null;
  /** PoB 装备授予技能来源；手动组为 null。 */
  source: string | null;
  active_skill_id: string | null;
  gems: GemJson[];
}

export type ConfigInputValue = boolean | number | string;

export interface WeaponSwap {
  active: 1 | 2;
  alternate_items: SlotItemInput[];
  exclusive_nodes: [number[], number[]];
}

export interface CustomModifierBlock {
  title: string;
  enabled: boolean;
  text: string;
}

export interface BuildJson {
  custom_modifier_blocks?: CustomModifierBlock[];
  weapon_swap?: WeaponSwap | null;
  character: CharacterJson;
  tree: TreeJson;
  items: ItemsJson;
  socket_groups: SocketGroupJson[];
  /** 0-based 主技能组下标；null = 未指定。 */
  main_socket_group: number | null;
  config_inputs: Record<string, ConfigInputValue>;
  /** `<Notes>` 自由文本（PoB 笔记页）。 */
  notes: string | null;
  /** 成组切换清单（PoB2 loadout）；单套 build 恒为一条 Default。 */
  loadouts: LoadoutJson[];
  /** 当前对应的 loadout 下标；无法判定为 null。 */
  active_loadout: number | null;
}

export interface SelectedLoadoutJson extends BuildJson {
  /** Build code with the returned loadout selected; use as the next export base. */
  code: string;
}

/**
 * 一个可切换的 loadout —— PoB2 把天赋树 / 装备 / 技能按各自 `title` 的命名约定
 * 绑成一组（`Build.lua::SyncLoadouts`），因此零格式扩展、与 PoB2 双向兼容。
 *
 * 建组方式：各类 title 取同名，或用花括号标识（`"升级期 {lvl30}"`，`{a,b}` 表示
 * 一套属多组）。某类只有一套时该类不参与绑定，对应字段为 null。
 */
export interface LoadoutJson {
  name: string;
  /** 1-based 文档序，回传 `switchLoadout` 用。 */
  tree: number;
  item: number | null;
  skill: number | null;
}

// ---------------------------------------------------------------------------
// calculate_build_json
// ---------------------------------------------------------------------------

export type EnemyTier = 'none' | 'boss' | 'pinnacle' | 'uber';

/** 角色身份覆盖（白手起 build 必填 class_name；导入后可改等级/升华）。 */
export interface CharacterOverride {
  level?: number;
  class_name?: string;
  ascendancy_name?: string;
}

/** 手动技能组宝石条目（gem id 由后端按 skill_id 反查）。 */
export interface GemInput {
  stat_set_index?: number | null;
  /** Raw PoB name for an unresolved slot; display/export metadata, never calculation identity. */
  name_spec?: string | null;
  /** Whether the gem is corrupted ("all Corrupted Skill Gems" modifiers test it); omitted means not. */
  corrupted?: boolean;
  skill_id: string;
  level: number;
  quality: number;
}

/** 手动技能组（整份替换 build 的 socket_groups）。 */
export interface SocketGroupInput {
  weapon_set?: 1 | 2 | null;
  slot?: string | null;
  enabled: boolean;
  /** PoB's 1-based selection among this group's active skills. */
  main_active_skill?: number | null;
  /** decode 后透传的装备授予技能来源；手动组省略。 */
  source?: string | null;
  gems: GemInput[];
}

/** 手动装备（PoB 原始文本块；整份替换装备槽）。 */
export interface SlotItemInput {
  slot: string;
  text: string;
}

/** 手动树插槽珠宝（插槽已加点才生效；范围珠宝 grant 行走几何展开）。 */
export interface JewelInput {
  socket_node: number;
  text: string;
}

/** 宝石目录条目（gem_catalog_json）。 */
export interface GemCatalogEntry {
  skill_id: string;
  /** Secondary effects supplied by this gem; display aliases, not picker entries. */
  additional_skill_ids?: string[];
  /** Canonical ordinal contributors: primary then granted active effects, never stat-set aliases. */
  active_effect_ids: string[];
  name: string;
  /** 繁中名（数据包 i18n 边车）。 */
  name_zh_tw: string | null;
  /** 简中名（国服词典转录边车，Phase 7.2）。 */
  name_zh_cn: string | null;
  /** 宝石颜色（红/绿/蓝 = str/dex/int；分类筛选用）。 */
  colour: 'str' | 'dex' | 'int' | null;
  is_support: boolean;
  /** 血脉（Lineage）特殊辅助宝石（徽标 + 优化器候选筛选）。 */
  is_lineage: boolean;
  /**
   * 技能标签（升序去重）。主动宝石＝granted effect 的 `skill_types`；辅助宝石＝
   * `require_skill_types`（能辅助什么），已滤掉 AND/OR/NOT 逻辑算子。
   * 含大量内部机制词，展示前过 `lib/gemTags` 的白名单。
   */
  tags: string[];
}

/** 节点威力条目（node_power_json；PoB2 热力图语义）。 */
export interface NodePowerEntry {
  /** 节点 skill id。 */
  skill: number;
  /** 单点试加后目标属性的增量（可为负）。 */
  delta: number;
  /** 距已加点前沿的 BFS 步数（1 = 相邻）。 */
  distance: number;
}

export interface NodePowerResponse {
  /** 基线属性值。 */
  base: number;
  entries: NodePowerEntry[];
}

// ---------------------------------------------------------------------------
// optimize_variants_json（通用变体评估：寻优框架的计算面）
//
// 分工契约：Rust 只算变体 → 属性值；打分/约束/排序在 `lib/optimize.ts`。
// ---------------------------------------------------------------------------

/** 属性约束（display_catalog 字段 id + 上下界，逐字段可缺省）。 */
export interface StatConstraint {
  stat: string;
  min?: number;
  max?: number;
}

/** 向技能组追加宝石（宝石组合寻优通道）。 */
export interface AddGemsInput {
  group_index: number;
  gems: GemInput[];
}

/** 单个变体：各通道可任意叠加；全空 = 基线复算。 */
export interface VariantInput {
  /** 回显标签（结果行展示用）。 */
  label?: string;
  add_gems?: AddGemsInput;
  /** 覆盖装备槽（text 为空 = 摘下该槽）。 */
  set_items?: SlotItemInput[];
  /** Whole-list replacements, preserving the ordinary edit/import validation. */
  jewels?: JewelInput[];
  flasks?: SlotItemInput[];
  socket_groups?: SocketGroupInput[];
  /** 追加加点（不验证连通性——假设性试算）。 */
  allocate_nodes?: number[];
  deallocate_nodes?: number[];
  /** 任意词条文本（「只要有词条就能算」的兜底通道）。 */
  extra_modifiers?: string[];
}

export interface OptimizeVariantsRequest {
  request: CalculateBuildRequest;
  /** 要收集的展示属性 id（未知 id 记 0）。 */
  stats: string[];
  variants: VariantInput[];
  /** 缺省 true；分批调用时后续批关掉省一次基线计算。 */
  include_baseline?: boolean;
}

export interface VariantStats {
  /** 请求内下标（对回变体定义）。 */
  index: number;
  label: string | null;
  /** 计算失败时为空表并给出 error。 */
  stats: Record<string, number>;
  error: string | null;
  unsupported?: string[];
}

export interface OptimizeVariantsResponse {
  baseline: Record<string, number> | null;
  variants: VariantStats[];
}

/** 词条 → 官方 trade2 stat 映射条目（overlay/trade_stat_map.json，静态资产直读）。 */
export interface TradeStatEntry {
  /** 官方 stat id（如 `explicit.stat_3372524247`）。 */
  id: string;
  /** 官方展示文案。 */
  text: string;
}

/** 符文/魂核目录条目（rune_catalog_json）。 */
export interface RuneCatalogEntry {
  /** canonical 英文名（`Rune:` 行与 reforge 请求的键）。 */
  name: string;
  name_zh_tw: string | null;
  name_zh_cn: string | null;
  is_soul_core: boolean;
  kind?: 'Rune' | 'SoulCore' | 'Idol';
  required_level?: number;
  limit?: number;
  limit_id?: string;
  /** 对请求物品基底适用的效果词条行（无物品上下文或不适用为空）。 */
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

/** `pob_code` 与 `character` 至少给一个（无 code = PoB2 新建 build 语义）。 */
export interface CalculateBuildRequest {
  tree_version?: string | null;
  pob_code?: string;
  character?: CharacterOverride;
  /** 整份替换已加点集合（交互加点）。 */
  allocated_nodes?: number[];
  /** 整份替换属性小点三选一。 */
  attribute_choices?: Record<string, AttributeChoice>;
  /** 整份替换技能组（手动编辑）。 */
  socket_groups?: SocketGroupInput[];
  /** 整份替换装备槽（手动编辑）。 */
  items?: SlotItemInput[];
  /**
   * `active` 是这次计算使用的武器组（1/2），“使用武器组 1/2 时”的词条按它生效；
   * 另一组武器与武器组专属天赋只用于导出。省略时沿用构筑码的武器组（新构筑为 1）。
   */
  weapon_swap?: WeaponSwap | null;
  /** 整份替换激活态药剂/护符（槽名 `Flask 1/2`、`Charm 1..3`）。 */
  flasks?: SlotItemInput[];
  /** 整份替换超越肢体（槽 `arm1`/`arm2`/`leg1`/`leg2`）。 */
  limbs?: SlotItemInput[];
  /** 整份替换树插槽珠宝。 */
  jewels?: JewelInput[];
  main_socket_group?: number;
  mode_effective?: boolean;
  enemy_tier?: EnemyTier;
  extra_modifiers?: string[];
  custom_modifier_blocks?: CustomModifierBlock[];
  config_inputs?: Record<string, ConfigInputValue>;
}

/** display_catalog 类别（Rust `DisplayStatCategory` 的枚举名直出）。 */
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
  | 'Utility';

export interface DisplayStatValue {
  id: string;
  /** null = 计算侧标记为不可用（如 ChaosMaxHit 的 ∞）。 */
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
  | 'hit_interval'
  | 'stronger_hand'
  | 'stronger_share'
  | 'weaker_hand';

export interface FormulaStep {
  op: FormulaOp;
  /** Amount applied: percentage points (increased, converted, hit_chance, chance_to_hit, crit_hits, enemy_block, the bonus and penalty steps), the enemy's evasion (vs_evasion), a multiplier (more, action_speed, enemy_mitigation, uses_per_second, dps_multiplier, ailment_effect, ailment_rate), seconds (added_time), the limit, the number of rolls (lucky, bifurcate), the starting damage per second (skill_dot, hit_dps), the hit an ailment starts from (ailment_source), the percentage of it dealt per second (per_second), the number of instances (active_stacks), the chance in percent (chance_to_apply), a hand's own value (main_hand, the off_hand steps, stronger_hand, weaker_hand), the percentage of instances from the stronger hand (stronger_share), a multiplier (reload), the seconds before a target can be hit again (hit_interval) or the amount added. */
  operand: number;
  /** Running value after the step. */
  result: number;
}

export interface BreakdownFormula {
  steps: FormulaStep[];
  /** The displayed stat value the steps end at. */
  value: number;
}

export interface Breakdown {
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
  /** Most instances that deal damage at once; the `active_stacks` step uses the smaller of the two. */
  max_stacks: number;
  /** `source_hit` and `source_crit` per damage type that can apply the ailment; the rows add up to the two totals. */
  source: { damage_type: string; hit: number; crit: number }[];
  /** `duration` before modifiers, in seconds. */
  base_duration: number;
  /** Multiplier from how fast effects expire on the enemy; `duration` is `base_duration` scaled by `duration_mods`, times this. */
  enemy_duration: number;
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

/** 主技能击中伤害的单类型分量（非暴击腿、玩家侧敌减伤前；占比用 avg）。 */
export interface HitDamagePart {
  /** Physical / Fire / Cold / Lightning / Chaos。 */
  damage_type: string;
  min: number;
  max: number;
  avg: number;
}

/** Current inspected skill identity and damage breakdown, refreshed on calculation. */
export interface MainSkillInfo {
  /** 选中技能组（0-based，与请求 socket_groups 对齐）。 */
  group_index: number;
  /** PoB's 1-based ordinal among all active effects in the group. */
  active_skill_index: number;
  /** 该组主技能的授予效果 id。 */
  skill_id: string;
  hit_damage: HitDamagePart[];
  /** 击中 DPS（TotalDPS）。 */
  hit_dps: number;
  /** 持续伤害合计 DPS（TotalDotDPS）。 */
  dot_dps: number;
  /** 综合 DPS（CombinedDPS）。 */
  combined_dps: number;
}

export interface CalculateBuildResponse {
  stats: DisplayStatValue[];
  /**
   * Lines the calculation could not apply, so their effect is missing from the
   * numbers. An item's line is given as written; a passive node's as
   * `Tree:<node>: <line>`; a configuration line (custom modifier, quest reward)
   * as `Config: <line>`.
   */
  unsupported_modifiers: string[];
  /** 键 = 聚合 ModName（Life / EnergyShield / FireResist / ...）。 */
  breakdowns: Record<string, Breakdown>;
  /** 主技能分解（null = 无可解析的伤害主技能）。 */
  main_skill: MainSkillInfo | null;
  /** 单件装备/药剂/珠宝文本解析失败的降级记录（据 slot 标红；空 = 全部成功）。 */
  item_errors: SlotIssue[];
  tree_effects?: PassiveJewelState;
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
  /** 完整计算请求（基线；与 node_power / optimize_variants 同形状）。 */
  request: CalculateBuildRequest;
  /** display_catalog 字段 id；缺省 TotalDPS / Life / TotalEHP。 */
  fields?: string[];
}

export type AttributionSourceKind = 'item' | 'socket_group' | 'flask' | 'limb';

export interface AttributionEntry {
  kind: AttributionSourceKind;
  /** 装备槽 id / 技能组下标（字符串）/ 药剂槽名 / 肢体槽 id。 */
  id: string;
  /** 字段 → 边际贡献（baseline - 移除后值；正 = 增益）。 */
  deltas: Record<string, number>;
}

export interface AttributionResponse {
  baseline: Record<string, number>;
  entries: AttributionEntry[];
}

// ---------------------------------------------------------------------------
// 树静态数据（web/public/data/<version>/base/passive_tree.json 直接加载）
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
// 职业/升华元数据（passive_tree_meta.json，新建 build 选择器用）
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

// ---------------------------------------------------------------------------
// 天赋树美术边车（public/tree-art/<version>/manifest.json，由 build-tree-art.py
// 从 vendor PoB2 DDS 图集解码生成）。URL 已由 backend 解析为绝对路径。
// ---------------------------------------------------------------------------

/** 每种节点的外框图 URL（已点/未点两态；缺省则该态回退到另一态）。 */
export interface TreeArtFrame {
  unalloc?: string;
  alloc?: string;
}

export interface TreeArt {
  /** skill id（字符串）→ 节点内技能图标 URL。 */
  nodeIcons: Record<string, string>;
  /** 节点 kind → 外框图 URL 两态。 */
  frames: Record<string, TreeArtFrame>;
  /** 精通节点通用徽记 URL（专属图标在游戏客户端里，vendor 只有这个空白徽记）。 */
  masteryIcon?: string;
}

/** 职业/升华名的简中对照表（数据包 `i18n/zh-CN/classes.json`，英文名 → 简中名，如 `"Druid": "德鲁伊"`）。 */
export interface ClassNames {
  classes: Record<string, string>;
  ascendancies: Record<string, string>;
}

// ---------------------------------------------------------------------------
// 内置配置目录（overlay/config_options.json，静态资产直读——与计算侧
// config_interpreter 消费同一份数据）
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
  /** PoB2 Config 页分区名（如 "When In Combat"）。 */
  section?: string;
  label?: string;
  /** list 型的选项；default 的 {index} 是 1-based。 */
  list_options?: ConfigListOption[];
  default?: { index?: number; state_bool?: boolean; state_number?: number; placeholder_number?: number } | number | boolean | string;
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
  unsupported_modifiers: string[];
  error: string | null;
}

export interface SkillMetricsResponse {
  skills: SkillMetricsEntry[];
  item_errors: SlotIssue[];
}
