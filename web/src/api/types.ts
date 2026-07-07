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

export interface TreeJson {
  allocated_nodes: number[];
  tree_version: string | null;
}

export interface SlotItemJson {
  slot: string;
  text: string;
}

export interface ItemsJson {
  equipped: SlotItemJson[];
  jewels: string[];
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

/** `pob_code` 与 `character` 至少给一个（无 code = PoB2 新建 build 语义）。 */
export interface CalculateBuildRequest {
  pob_code?: string;
  character?: CharacterOverride;
  /** 整份替换已加点集合（交互加点）。 */
  allocated_nodes?: number[];
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

export interface CalculateBuildResponse {
  stats: DisplayStatValue[];
  unsupported_modifiers: string[];
  /** 键 = 聚合 ModName（Life / EnergyShield / FireResist / ...）。 */
  breakdowns: Record<string, Breakdown>;
}

// ---------------------------------------------------------------------------
// attribution_json
// ---------------------------------------------------------------------------

export interface AttributionRequest {
  pob_code?: string;
  /** display_catalog 字段 id；缺省 TotalDPS / Life / TotalEHP。 */
  fields?: string[];
  character?: CharacterOverride;
  allocated_nodes?: number[];
  main_socket_group?: number;
  mode_effective?: boolean;
  enemy_tier?: EnemyTier;
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
