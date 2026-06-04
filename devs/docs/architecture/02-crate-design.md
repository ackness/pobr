# Crate 拆分设计

---

## 1. 总体结构

```
pobr/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── pobr-data/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── item.rs           # Item, ItemBase, ItemRarity
│   │       ├── gem.rs            # Gem, SkillEffect, SupportEffect
│   │       ├── skill.rs          # SkillDefinition, SkillTypes, SkillFlags
│   │       ├── passive_tree.rs   # NodeData, NodeId, PassiveTreeSpec
│   │       ├── modifier.rs       # ModName, ModType, ModFlags (定义层)
│   │       ├── stat.rs           # StatId, StatDescription
│   │       ├── game_data.rs      # GameData, 全局常量
│   │       └── constants.rs
│   │
│   ├── pobr-i18n/
│   │   ├── Cargo.toml
│   │   ├── locales/
│   │   │   ├── en-US/
│   │   │   │   ├── ui.toml
│   │   │   │   ├── stats.toml
│   │   │   │   └── errors.toml
│   │   │   └── zh-TW/
│   │   │       ├── ui.toml
│   │   │       ├── stats.toml
│   │   │       └── errors.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── language.rs       # LanguageId, fallback chain
│   │       ├── translator.rs     # Translator, TranslationKey
│   │       ├── stat_text.rs      # StatId <-> 本地化文本映射
│   │       └── errors.rs
│   │
│   ├── pobr-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── mod_parser.rs
│   │       ├── mod_cache.rs
│   │       ├── mod_db/
│   │       ├── modifier/
│   │       ├── calc/
│   │       └── config.rs
│   │
│   ├── pobr-tree/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tree.rs
│   │       ├── pathfinding.rs
│   │       ├── radius_jewel.rs
│   │       ├── timeless_jewel.rs
│   │       └── node.rs
│   │
│   ├── pobr-item/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── raw_text.rs
│   │       ├── sections.rs
│   │       ├── crafter.rs
│   │       ├── affix.rs
│   │       ├── item_builder.rs
│   │       └── errors.rs
│   │
│   ├── pobr-build/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── build.rs
│   │       ├── config.rs
│   │       ├── build_code.rs
│   │       ├── import_detect.rs
│   │       ├── import_export.rs
│   │       ├── calc_orchestrator.rs
│   │       ├── calc_cache.rs
│   │       └── snapshot.rs
│   │
│   └── pobr-trade/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── api.rs
│           ├── query.rs
│           └── types.rs
│
├── apps/
│   ├── pobr-cli/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── pobr-desktop/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── pobr-wasm/
│       ├── Cargo.toml
│       └── src/lib.rs
│
├── tools/
│   ├── export-poe-data/          # 数据导出/转换工具
│   ├── gen-mod-cache/            # Modifier cache 生成与回归检查
│   └── lint-i18n/                # 语言包完整性检查
│
├── fixtures/
│   ├── pob-codes/
│   ├── raw-items/
│   └── builds/
│
└── devs/
    └── docs/
```

---

## 2. pobr-data 详细设计

### 2.1 职责

- **纯数据定义**，零业务逻辑
- 所有 crate 的公共依赖
- 游戏版本相关常量和枚举

### 2.2 核心模块

```rust
// src/lib.rs
pub mod item;
pub mod gem;
pub mod skill;
pub mod passive_tree;
pub mod modifier;      // 只有定义，没有解析逻辑
pub mod stat;
pub mod game_data;
pub mod constants;

// re-exports
pub use constants::*;
```

### 2.3 关键类型定义

```rust
// src/item.rs
pub struct Item {
    pub id: String,
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub mods: Vec<String>,        // 原始文本，待解析
    pub implicits: Vec<String>,
    pub explicits: Vec<String>,
    pub quality: u8,
    pub item_level: u8,
    pub requirements: Requirements,
    pub sockets: Vec<Socket>,
}

// src/gem.rs
pub struct Gem {
    pub id: String,
    pub granted_effect_id: GrantedEffectId,
    pub level: u8,
    pub quality: u8,
    pub is_support: bool,
}

pub struct SkillEffect {
    pub id: GrantedEffectId,
    pub skill_types: SkillTypes,
    pub base_flags: SkillFlags,
    pub levels: Vec<SkillLevelData>,
    pub base_stats: Vec<(StatId, f64)>,
}

// src/modifier.rs (定义层)
pub type ModName = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModType {
    Base, Inc, More, Flag, Override, List,
}

#[repr(u64)]
pub enum ModFlag {
    Attack = 1 << 0, Spell = 1 << 1, Melee = 1 << 2,
    Projectile = 1 << 3, Area = 1 << 4,
}

// src/constants.rs
pub enum DamageType {
    Physical, Fire, Cold, Lightning, Chaos,
}

pub enum ClassId {
    Marauder, Duelist, Ranger, Shadow, Witch, Templar, Scion,
}
```

### 2.4 GameData 加载

> ⚠️ **此设计已被取代（superseded）**。下方 `include_bytes!` + bincode 编译期内联到
> `pobr-data` 的方案**未被采用**。实际实现把数据加载拆为独立的 `pobr-gamedata` loader
> crate（运行时读 `data/<poe_version>/` 的 JSON），`pobr-data` 维持零 I/O，只保留入库
> JSON 的 schema 定义（`catalog.rs`）。详见下方 **§2.5 数据层实现现状**。保留此小节仅作
> 历史设计记录。

```rust
// src/game_data.rs（未采用的早期设计）
use once_cell::sync::Lazy;

pub struct GameData {
    pub skills: HashMap<GrantedEffectId, SkillDefinition>,
    pub items: HashMap<ItemBaseId, ItemBase>,
    pub uniques: HashMap<String, UniqueItem>,
    pub stat_descriptions: HashMap<StatId, StatDescription>,
    pub version: DataVersion,
}

pub static GAME_DATA: Lazy<GameData> = Lazy::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/game_data.bin"));
    bincode::deserialize(bytes).expect("Failed to deserialize game data")
});
```

### 2.5 数据层实现现状（pobr-data-adapter + pobr-gamedata）

> 本小节描述**已落地**的数据管线（截至 PoE2 patch `4.5.0.3.4`），与目标架构其余尚未实现
> 的 crate 不同——这部分是当前可运行的事实。

**管线全貌**：

```
GGG .dat 原始导出（pathofexile-dat 产物）
    │
    ▼  tools/pobr-data-adapter（离线工具，解析外键 / 反范式化 / 过滤占位 / 按 id 排序）
    │
data/<poe_version>/                      # PoBR 自有最小 JSON（schema = pobr-data::catalog）
    ├── manifest.json                    # DataManifest 信封：schema_version / poe_version / languages / domains
    ├── base_items.json                  # BaseItemDef[]
    ├── stats.json                       # StatDef[]
    ├── mods.json                        # ModDef[]（Stat 外键已解析、掷值区间已合并）
    ├── skill_gems.json                  # SkillGemDef[]
    ├── granted_effects.json             # GrantedEffectDef[]
    └── i18n/zh-TW/{base_items,mods,skills}.json   # id → 本地化名称 边车（英文为 canonical）
    │
    ▼  crates/pobr-gamedata（运行时 loader，唯一持有文件 I/O 的层）
    │
pobr_data::catalog 类型 → 上层计算
```

**职责边界**：

| 组件 | 类型 | 职责 |
|------|------|------|
| `pobr-data::catalog`（`catalog.rs`） | schema 定义 | PoBR 自有最小 JSON 的强类型 schema：`DataManifest` / `BaseItemDef` / `StatDef` / `ModDef`（+ `ModStat`）/ `SkillGemDef` / `GrantedEffectDef`。与 GGG 原始列名 / PoB 生成 Lua 解耦，稳定字符串 ID，版本可钉、diff 友好。零 I/O。`CATALOG_SCHEMA_VERSION` 标记结构不兼容变更 |
| `tools/pobr-data-adapter` | 离线工具 | `.dat 导出 → data/<ver>/*.json`。解析整型外键为稳定字符串 ID（ItemClass / Tags / Stat1..4 / BaseItemType / ActiveSkill 等）、过滤开发占位（`[DNT-UNUSED]` 类）、反范式化、按 id 排序输出。同时生成 zh-TW i18n 边车 |
| `crates/pobr-gamedata` | 运行时 loader | `GameData::new(version_dir)` 指向某版本目录，按域懒加载（`base_items()` / `stats()` / `mods()` / `skill_gems()` / `granted_effects()`）+ 语言边车（`base_item_names(lang)` 等）。`repo_data_root()` 给出仓库内置 `data/` 根，供测试与默认加载 |

**关键约定**：

- **英文为 canonical**：主数据文件存英文名，其它语言走 `i18n/<lang>/*.json` 边车（`id → 本地化名称`），与「计算用稳定 ID、显示走 i18n」一致（见 05-compatibility-and-i18n.md）。
- **I/O 收口**：仅 `pobr-gamedata` 持有 `fs` 读取；`pobr-data` / `pobr-core` 维持零 I/O，可测试 / 可并行 / 可 WASM 化。
- **已知缺口（结构进、语义待接）**：`SkillGems.GemEffects` FK 指向的 `GemEffects` 表当前 pipeline 未导出，故宝石 → 授予效果的直接连边暂缺；分等级缩放（`GrantedEffectsPerLevel` 的 cost / cooldown / attack time）尚未接入。武器 / 护甲数值（`WeaponTypes` / `ArmourTypes`）后续切片接入。详见 `catalog.rs` 内 TODO。

---

## 3. pobr-i18n 详细设计

### 3.1 职责

- 提供语言包加载、fallback、格式化和缺失翻译诊断。
- 初始支持 `en-US` 与 `zh-TW`。
- 计算内部使用稳定 ID；显示层通过 `pobr-i18n` 转换为用户语言。
- 保持语言包可扩展，新语言通过新增目录和 manifest 接入。

### 3.2 核心类型

```rust
// pobr-i18n/src/language.rs
pub struct LanguageId(String);

pub struct LanguagePack {
    pub id: LanguageId,
    pub display_name: String,
    pub fallback: Option<LanguageId>,
}

// pobr-i18n/src/translator.rs
pub struct Translator {
    active: LanguageId,
    fallback: LanguageId,
}

impl Translator {
    pub fn new(active: LanguageId) -> Result<Self, I18nError>;
    pub fn text(&self, key: &str) -> Cow<'_, str>;
    pub fn format(&self, key: &str, args: &[(&str, I18nValue)]) -> String;
}
```

### 3.3 文本边界

- UI 文本使用稳定 key：`items.import.title`, `build.copy_code`, `errors.build_code.invalid_base64`。
- 游戏数据文本使用稳定 ID：`StatId`, `SkillId`, `ItemBaseId`。
- 英文 Modifier 文本仍是 PoB 兼容解析的第一输入源。
- 繁体中文文本先用于显示；后续增加中文复制物品导入时，通过 `stat_text` 做反向映射。

---

## 4. pobr-core 详细设计

### 4.1 职责

- Modifier 解析、存储、聚合查询
- 所有伤害/防御/技能计算
- **零 I/O**，对外提供确定性计算入口
- 内部通过 `Env` / `CalculationSession` 承载阶段性可变状态，避免共享可变引用泄漏到上层

### 4.2 模块划分

```rust
// src/lib.rs
pub mod mod_parser;
pub mod mod_cache;
pub mod mod_db;
pub mod modifier;
pub mod calc;
pub mod config;

pub use calc::perform;
pub use mod_db::{ModDb, ModList};
```

### 4.3 mod_db 模块

```rust
// src/mod_db/mod_db.rs
use pobr_data::{ModName, ModType};
use crate::modifier::Modifier;
use crate::config::CalcConfig;

pub struct ModDb {
    mods: HashMap<ModName, Vec<Modifier>>,
    conditions: HashMap<String, bool>,
    multipliers: HashMap<String, i32>,
}

impl ModDb {
    pub fn new() -> Self;
    pub fn add_mod(&mut self, mod: Modifier);
    pub fn add_list(&mut self, mods: Vec<Modifier>);
    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64;
    pub fn more(&self, cfg: &CalcConfig, names: &[ModName]) -> f64;
    pub fn flag(&self, cfg: &CalcConfig, name: ModName) -> bool;
    pub fn override_(&self, cfg: &CalcConfig, name: ModName) -> Option<f64>;
}

// src/mod_db/mod_list.rs
pub struct ModList {
    db: ModDb,
    parent: Option<Box<ModList>>,
}

impl ModList {
    pub fn with_parent(parent: ModList) -> Self;
    pub fn add_mod(&mut self, mod: Modifier);
    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64;
}
```

### 4.4 modifier 模块

```rust
// src/modifier/modifier.rs
use pobr_data::{ModName, ModType, ModFlags, KeywordFlags};

pub struct Modifier {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: f64,
    pub source: String,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub tags: Vec<ModTag>,
}

pub enum ModTag {
    Condition { var: String },
    ActorCondition { actor: ActorType, var: String },
    Multiplier { var: String, limit: Option<i32>, global_limit: Option<i32> },
    PerStat { stat: String, div: Option<i32> },
    SkillType(SkillType),
    SkillName(String),
    GlobalEffect { effect_type: String, effect_name: String },
}

impl Modifier {
    pub fn matches(&self, cfg: &CalcConfig) -> bool;
}
```

### 4.5 calc 模块

```rust
// src/calc/mod.rs
pub mod env; pub mod actor; pub mod perform; pub mod setup;
pub mod offence; pub mod defence; pub mod active_skill;
pub mod triggers; pub mod mirages; pub mod breakdown;
pub use env::Env; pub use actor::Actor; pub use perform::perform;
```

### 4.6 环境 (env.rs)

```rust
pub struct Env<'a> {
    pub build: &'a Build,
    pub data: &'a GameData,
    pub config_input: &'a BuildConfig,
    pub mode: CalcMode,
    pub spec: &'a PassiveTreeSpec,
    pub override_params: Option<OverrideParams>,
    pub class_id: ClassId,
    pub player: Actor,
    pub enemy: Actor,
    pub item_mod_db: ModDb,
    pub enemy_level: u8,
    pub requirements: Requirements,
    pub radius_jewels: Vec<RadiusJewel>,
    pub extra_radius_nodes: Vec<NodeId>,
    pub granted_skills: Vec<GrantedSkill>,
    pub flasks: Vec<Flask>,
    pub tinctures: Vec<Tincture>,
    pub active_skills: Vec<ActiveSkill>,
    pub aux_skills: Vec<AuxSkill>,
    pub main_socket_group: Option<usize>,
}
```

### 4.7 Actor (actor.rs)

```rust
pub struct Actor {
    pub mod_db: ModDb,
    pub level: u8,
    pub output: OutputTable,
    pub breakdown: BreakdownTable,
    pub item_list: Vec<Item>,
    pub main_skill: Option<ActiveSkill>,
}
```

### 4.8 perform.rs

```rust
pub fn perform(env: &mut Env, skip_ehp: bool) -> Result<(), CalcError> {
    init_jewel_mods(env)?;
    init_minion_skills(env)?;
    apply_flask_effects(env)?;
    calc_actor_attribs_conditions(env, &mut env.player)?;
    calc_actor_life_mana(&mut env.player)?;
    calc_reservations(env)?;
    apply_buffs_and_debuffs(env)?;
    calc_charges(env)?;
    defence::calc_defence(env, &mut env.player)?;
    if !skip_ehp { defence::build_defence_estimations(env, &mut env.player)?; }
    triggers::calc_triggers(env, &mut env.player)?;
    mirages::calc_mirages(env)?;
    if let Some(ref main_skill) = env.player.main_skill {
        offence::calc_offence(env, &mut env.player, main_skill)?;
    }
    Ok(())
}
```

---

## 5. pobr-tree 详细设计

### 5.1 职责

- 天赋树拓扑结构
- 节点寻路算法
- 范围珠宝/永恒珠宝效果计算

### 5.2 关键类型

```rust
// src/tree.rs
pub struct PassiveTree {
    pub nodes: HashMap<NodeId, NodeData>,
    pub groups: HashMap<GroupId, NodeGroup>,
    pub root: NodeId,
    pub classes: Vec<ClassStart>,
    pub ascendancy_classes: Vec<AscendancyClass>,
    pub jewel_slots: Vec<NodeId>,
    pub mastery_nodes: Vec<NodeId>,
}

impl PassiveTree {
    pub fn from_json(data: &str) -> Result<Self, TreeError>;
    pub fn shortest_path(&self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>>;
    pub fn nodes_in_radius(&self, socket: NodeId, radius: f64) -> Vec<NodeId>;
    pub fn apply_radius_jewel(&mut self, socket: NodeId, jewel: &Jewel);
    pub fn apply_timeless_jewel(&mut self, socket: NodeId, seed: u32, jewel_type: TimelessType);
}
```

---

## 6. pobr-item 详细设计

### 6.1 职责

- 解析从游戏或 PoB 复制出来的 raw item text。
- 支持自定义物品创建、base item 选择、rarity、quality、socket、influence、prefix/suffix、crafted mod 和 roll 编辑。
- 保留原始文本区块，输出标准 `pobr-data::Item` 和可回放的 `ItemDraft`。
- 复用 `pobr-core::mod_parser` 解析英文 modifier 文本，避免在 item 层重复实现规则。

### 6.2 核心类型

```rust
pub enum ItemTextSection {
    Header,
    Requirements,
    Enchant,
    Implicit,
    Explicit,
    Crafted,
    Fractured,
    Veiled,
    Eldritch,
    Crucible,
    Flavour,
    Unknown(String),
}

pub struct RawItemParseResult {
    pub item: Item,
    pub sections: Vec<ItemTextBlock>,
    pub unsupported_lines: Vec<String>,
}

pub struct CustomItemDraft {
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub quality: u8,
    pub implicits: Vec<EditableMod>,
    pub explicits: Vec<EditableMod>,
    pub crafted: Vec<EditableMod>,
}

impl CustomItemDraft {
    pub fn to_item(&self) -> Result<Item, ItemError>;
}
```

### 6.3 兼容策略

- 英文 raw item text 是第一阶段必须兼容的输入。
- 繁体中文 raw item text 作为后续扩展，依赖 `pobr-i18n::stat_text` 的反向映射。
- 不支持的行保留在 `unsupported_lines`，UI 可提示但不阻塞保存草稿。
- 物品最终参与计算时只暴露 `Item` 和 mod 文本，计算核心不感知 crafting UI 状态。

---

## 7. pobr-build 详细设计

### 7.1 职责

- Build 状态管理（序列化/反序列化）
- 计算调用编排（带缓存）
- PoB Build Code 兼容导入/导出
- 快捷导入字符串识别：Build Code、XML、pobb.in/外链、raw item text

### 7.2 核心类型

```rust
// src/build.rs
pub struct Build {
    pub db_file_name: Option<std::path::PathBuf>,
    pub build_name: String,
    pub view_mode: ViewMode,
    pub character_level: u8,
    pub target_version: GameVersion,
    pub bandit: BanditChoice,
    pub pantheon: Pantheon,
    pub spec: PassiveTreeSpec,
    pub items: Vec<Item>,
    pub socket_groups: Vec<SocketGroup>,
    pub config: BuildConfig,
    pub mod_flag: bool,
    pub unsaved: bool,
}

impl Build {
    pub fn new() -> Self;
    pub fn from_xml(xml: &str) -> Result<Self, BuildError>;
    pub fn to_xml(&self) -> String;
    pub fn from_pob_code(code: &str) -> Result<Self, BuildError>;
    pub fn to_pob_code(&self) -> Result<String, BuildError>;
}
```

### 7.3 PoB Build Code 编解码

```rust
pub enum ImportKind {
    PobCode,
    Xml,
    PobbinUrl,
    RawItemText,
    Unknown,
}

pub fn detect_import(input: &str) -> ImportKind;
pub fn decode_pob_code(code: &str) -> Result<String, BuildCodeError>; // returns XML
pub fn encode_pob_code(xml: &str) -> Result<String, BuildCodeError>;
```

**格式契约**：

1. XML 序列化保持 PoB 兼容字段名。
2. 压缩使用 deflate/zlib 兼容实现。
3. Base64 使用 PoB 的 URL-safe 替换：`+` → `-`，`/` → `_`。
4. 语言选择不写入兼容 Build Code，显示语言由应用配置管理。

### 7.4 计算缓存

```rust
// src/calc_cache.rs
use pobr_core::calc::OutputTable;

pub struct CalcCache {
    // 以 Build 的 hash 为 key
    player_output: Option<OutputTable>,
    enemy_output: Option<OutputTable>,
    last_build_hash: u64,
}

impl CalcCache {
    pub fn compute_if_stale(&mut self, build: &Build) -> (&OutputTable, &OutputTable);
    pub fn invalidate(&mut self);
}
```

---

## 8. 其余 Crate

### 8.1 pobr-trade

```rust
// src/api.rs
pub struct TradeApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl TradeApiClient {
    pub async fn search(&self, league: &str, query: &TradeQuery) -> Result<SearchResult, TradeError>;
    pub async fn fetch(&self, league: &str, ids: &[String]) -> Result<Vec<TradeItem>, TradeError>;
}
```

### 8.2 apps

- `apps/pobr-cli`: CLI 入口（计算指定 Build Code、导入 raw item、导出 JSON）。
- `apps/pobr-desktop`: 桌面 GUI 入口（egui / iced）。
- `apps/pobr-wasm`: WASM 绑定入口（wasm-bindgen）。
- 剪贴板读取、文件读取、URL 下载、语言选择属于 app/CLI I/O 层。

---

## 9. Cargo.toml 配置

> ⚠️ **下方为目标架构配置；实际 `Cargo.toml` 与之有差异，以仓库为准。** 现状：
>
> - **workspace members（13 个）**：`pobr-data` / `pobr-core` / `pobr-gamedata`（见 §2.5，
>   有实质实现）；`pobr-i18n` / `pobr-tree` / `pobr-item` / `pobr-build` / `pobr-trade` +
>   `apps/{pobr-cli, pobr-desktop, pobr-wasm}`（**占位骨架**，最小可编译、仅挂项目内 path
>   依赖，外部依赖待实现时加）；`tools/{sync-pob-catalog, pobr-data-adapter}`。
> - **工具改名（职责已被现有工具承担）**：早期规划的 `tools/export-poe-data` 由
>   **`tools/pobr-data-adapter`** 实现（GGG `.dat` → 入库 JSON，见 §2.5）；
>   `tools/gen-mod-cache` 由 **`tools/sync-pob-catalog`**（catalog / parity 扫描）+
>   `pobr-core::mod_cache`（运行时解析缓存）共同承担；`tools/lint-i18n` 暂未实现
>   （依赖尚未落地的 i18n 数据，留作未来项）。
> - **依赖继承**：实际用 `version.workspace = true` / `edition.workspace = true`，
>   内部 crate 登记在 `workspace.dependencies`（path），下游写 `pobr-core.workspace = true`。
> - **workspace deps**：当前仅 `pobr-data` / `pobr-core` / `pobr-i18n` / `pobr-tree` /
>   `pobr-item` / `pobr-build` / `pobr-trade`（path）+ `regex` / `serde` / `serde_json`。
>   下方的 `bincode` / `once_cell` / `thiserror` / `rayon` / `quick-xml` 等尚未引入。

### Workspace Root

```toml
[workspace]
members = [
    "crates/pobr-data",
    "crates/pobr-core",
    "crates/pobr-gamedata",
    # 占位骨架（尚未实现）
    "crates/pobr-i18n",
    "crates/pobr-tree",
    "crates/pobr-item",
    "crates/pobr-build",
    "crates/pobr-trade",
    "apps/pobr-cli",
    "apps/pobr-desktop",
    "apps/pobr-wasm",
    # 工具
    "tools/sync-pob-catalog",
    "tools/pobr-data-adapter",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
pobr-data = { path = "crates/pobr-data" }
pobr-core = { path = "crates/pobr-core" }
pobr-i18n = { path = "crates/pobr-i18n" }
pobr-tree = { path = "crates/pobr-tree" }
pobr-item = { path = "crates/pobr-item" }
pobr-build = { path = "crates/pobr-build" }
pobr-trade = { path = "crates/pobr-trade" }
regex = "1.11"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# 目标架构还会用到（尚未引入）：bincode / once_cell / thiserror / rayon / quick-xml / base64 / flate2
```

### pobr-data/Cargo.toml

```toml
[package]
name = "pobr-data"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }

[build-dependencies]
# 用于编译时生成 game_data.bin
```

### pobr-core/Cargo.toml

```toml
[package]
name = "pobr-core"
version = "0.1.0"
edition = "2024"

[dependencies]
pobr-data = { path = "../pobr-data" }
serde = { workspace = true }
thiserror = { workspace = true }
rayon = { workspace = true, optional = true }

[features]
default = ["parallel"]
parallel = ["dep:rayon"]
```

### pobr-i18n/Cargo.toml

```toml
[package]
name = "pobr-i18n"
version = "0.1.0"
edition = "2024"

[dependencies]
pobr-data = { path = "../pobr-data" }
serde = { workspace = true }
thiserror = { workspace = true }
```

### pobr-tree/Cargo.toml

```toml
[package]
name = "pobr-tree"
version = "0.1.0"
edition = "2024"

[dependencies]
pobr-data = { path = "../pobr-data" }
serde = { workspace = true }
```

### pobr-item/Cargo.toml

```toml
[package]
name = "pobr-item"
version = "0.1.0"
edition = "2024"

[dependencies]
pobr-data = { path = "../pobr-data" }
pobr-core = { path = "../pobr-core" }
serde = { workspace = true }
thiserror = { workspace = true }
```

### pobr-build/Cargo.toml

```toml
[package]
name = "pobr-build"
version = "0.1.0"
edition = "2024"

[dependencies]
pobr-data = { path = "../pobr-data" }
pobr-core = { path = "../pobr-core" }
pobr-tree = { path = "../pobr-tree" }
pobr-item = { path = "../pobr-item" }
serde = { workspace = true }
quick-xml = { workspace = true }
base64 = "0.22"
flate2 = "1.0"
```
