/**
 * pobr-wasm JSON 契约类型（与 `apps/pobr-wasm/src/build_api.rs` 一一对应）。
 *
 * 契约冻结：Rust 侧 `apps/pobr-wasm/tests/contract_golden.rs` 钉住 JSON 键集合；
 * 改这里必须同步改 Rust DTO 与 golden，反之亦然。
 */

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
}

export interface GemJson {
  skill_id: string;
  level: number;
  quality: number;
}

export interface SocketGroupJson {
  slot: string | null;
  enabled: boolean;
  /** PoB 装备授予技能来源；手动组为 null。 */
  source: string | null;
  active_skill_id: string | null;
  gems: GemJson[];
}

export type ConfigInputValue = boolean | number | string;

export interface BuildJson {
  character: CharacterJson;
  tree: TreeJson;
  items: ItemsJson;
  socket_groups: SocketGroupJson[];
  /** 0-based 主技能组下标；null = 未指定。 */
  main_socket_group: number | null;
  config_inputs: Record<string, ConfigInputValue>;
  /** `<Notes>` 自由文本（PoB 笔记页）。 */
  notes: string | null;
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
  skill_id: string;
  level: number;
  quality: number;
}

/** 手动技能组（整份替换 build 的 socket_groups）。 */
export interface SocketGroupInput {
  slot?: string | null;
  enabled: boolean;
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
  /** 对请求物品基底适用的效果词条行（无物品上下文或不适用为空）。 */
  lines: string[];
}

/** `pob_code` 与 `character` 至少给一个（无 code = PoB2 新建 build 语义）。 */
export interface CalculateBuildRequest {
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
  /** 整份替换激活态药剂/护符（槽名 `Flask 1/2`、`Charm 1..3`）。 */
  flasks?: SlotItemInput[];
  /** 整份替换树插槽珠宝。 */
  jewels?: JewelInput[];
  main_socket_group?: number;
  mode_effective?: boolean;
  enemy_tier?: EnemyTier;
  extra_modifiers?: string[];
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
  value: number | null;
  source_text: string | null;
  origin_kind: string | null;
  origin_id: string | null;
  slot: string | null;
}

export interface Breakdown {
  base_total: number;
  inc_total: number;
  mods: BreakdownMod[];
}

/** 主技能击中伤害的单类型分量（非暴击腿、玩家侧敌减伤前；占比用 avg）。 */
export interface HitDamagePart {
  /** Physical / Fire / Cold / Lightning / Chaos。 */
  damage_type: string;
  min: number;
  max: number;
  avg: number;
}

/** 主技能身份 + 伤害分解（PoB2 左侧栏 Main Skill 的对应物；每次重算刷新）。 */
export interface MainSkillInfo {
  /** 选中技能组（0-based，与请求 socket_groups 对齐）。 */
  group_index: number;
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
  unsupported_modifiers: string[];
  /** 键 = 聚合 ModName（Life / EnergyShield / FireResist / ...）。 */
  breakdowns: Record<string, Breakdown>;
  /** 主技能分解（null = 无可解析的伤害主技能）。 */
  main_skill: MainSkillInfo | null;
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

export type AttributionSourceKind = 'item' | 'socket_group' | 'flask';

export interface AttributionEntry {
  kind: AttributionSourceKind;
  /** 装备槽 id / 技能组下标（字符串）/ 药剂槽名。 */
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
  default?: { index?: number } | number | boolean | string;
}

export interface ConfigCatalogFile {
  options: ConfigOption[];
}

// ---------------------------------------------------------------------------
// full_dps_json
// ---------------------------------------------------------------------------

export interface SkillDpsEntry {
  /** 技能组下标（0-based，与 socket_groups 对齐）。 */
  group_index: number;
  /** 该组主动技能的授予效果 id。 */
  skill_id: string;
  dps: number;
}

export interface FullDpsResponse {
  /** 全部启用伤害技能组的 CombinedDPS 之和。 */
  full_dps: number;
  per_skill: SkillDpsEntry[];
}
