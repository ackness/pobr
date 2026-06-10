# M5c 实施蓝图：物品编辑态 + 树字段

> 撰写：2026-06-11 · 基线分支：`m0/data-foundation`（M0 W1/W2/W4 已合并；W3「calc 常量切 RuntimeConstants/RuleSet 注入」进行中，本蓝图按**注入管道已存在**假设编写）
> 输入：21-roadmap M5(c) · 20-target-architecture（P13/P15/P16/P2/P9）· 16-items.md · 17-passive-tree.md
> 读者：实施 agent。本文自包含——只读本蓝图 + 代码即可开工，无需回读 roadmap/审计。
> 行号声明：vendor 行号已于 2026-06-11 对当前 vendor 检出实地抽查验证（CheckModLineVariant/calcLocal/applyRange/WeaponSet 注入/ReplaceNode/ScaleAddMod 等）；pobr 行号对当前 HEAD（e3a1a91）。

---

## 0. 阶段定位与范围

### 0.1 范围（roadmap M5(c) 原文）

> **(c) 物品编辑态 + 树**：pobr-item 落地（variant 门控/applyRange + `mod_scalability.json`/`uniques.json`/`runes.json`/`catalysts.json`/武器局部 mod 结构化结算/Weapon2 局部词条）；树字段消费（is_attribute/options/isSwitchable ReplaceNode/WeaponSet 条件/`node_effect.rs` 节点效果缩放管线/Grants Skill）。

覆盖缺口：**16-G1**（variant 门控）、**16-G2**（applyRange/ModScalability）、**16-G3**（武器局部 mod）、16 号缺口 5/6/7/9/15（Weapon2、runes、catalyst、uniques、pobr-item 空置）、**17-G1**（武器组）、**17-G2**（节点效果缩放管线）、**17-G3**（isSwitchable ReplaceNode）、17 号缺口 4/5/6（is_attribute 计数、环形半径、Grants Skill）。

### 0.2 不在范围（边界，禁止顺手做）

- `BaseItemDef` 补列（spirit/block_chance/reload 等，16-G4）→ **M1 落库、M2/M4 消费**。本阶段武器局部结算只对既有 `WeaponBaseStats`/`ArmourBaseStats` 字段工作。
- statdesc 渲染链路（15-G1）→ **M5b**。本阶段任何需要"stat_id → 英文词条文本"渲染的路线一律绕开（见 §3.D 数据源裁决）。
- flask/charm/jewel 物品类别建模（16 号缺口 12）、物品需求计算（缺口 13）、corrupted 状态建模（缺口 11）→ 后续阶段，仅在 schema 设计上不堵死。
- MH/OH 双 pass（12-G1）→ **M4**（归因 RFC 前置）。本阶段只做 Weapon2 **局部词条隔离**，不做副手 DPS。
- 树分配/寻路引擎（17 号缺口 7）、树版本迁移（缺口 8）→ **M7**。
- mod_parser 六表数据化 → **M6**。本阶段对 mod_parser 只做**新增词条模式**（半径/效果系），不动架构。

### 0.3 与 M0 的衔接

| M0 已交付 | 本阶段如何消费 |
|---|---|
| 三层目录 `data/4.5.0.3.4/{base,overlay,generated}` + manifest v2 | 新表按层落位：vendor 抽取 → `overlay/`，.dat 再生 → `base/` |
| `pobr-gamedata` overlay merge 引擎（`overlay.rs`，key 级覆盖/冲突报错） | 树字段若走 overlay 路线（§3.D 路线 B）直接复用 |
| `sync-pob-catalog extract-lua`（luajit 执行 vendor 序列化，`extract_lua.rs` + 内嵌 bootstrap lua，byte-stable） | mod_scalability/catalysts/runes/uniques 四表抽取扩域复用同一骨架 |
| `RuleSet` 聚合骨架（`gamedata/src/ruleset.rs`，字段 Option 占位） | 新增 `item_rules`/`local_mods` 域；W3 完成后注入链路即通 |
| `jewel_radii.json`（base/，含 inner/outer 环形档） | Track E 环形半径消费 |
| CI 防线：`devs/scripts/regen-check.sh` + pobr-data 禁内嵌大数组 lint | 新表全部纳入重生 byte-diff；新增 Rust 侧不得内嵌数据表 |
| `local_mods.json` / `high_precision_mods.json`（M0 任务清单内） | **实查未落地**（`overlay/` 仅 `skill_overrides.json`）。本蓝图按 Track C / Track E 自带兜底处理（见 §3.C1、§3.E2 与 §7 开放问题 1） |

---

## 1. 现状坐标（开工前先读这些）

### 1.1 pobr 侧

| 文件 | 关键位置 | 现状要点 |
|---|---|---|
| `crates/pobr-data/src/item.rs` | `Item` 46-61、`EquipmentSlot` 64-94 | Item 仅三段 `Vec<String>` 词条 + rolled_defence；无 variant/catalyst 字段；10 槽无 Flask/Charm/Jewel |
| `crates/pobr-core/src/item_text.rs`（551 行） | `is_xml_metadata_line` 248-267、`strip_pob_annotations` 423-503、`classify_mod_lines` 529-550 | `Variant:`/`Selected Variant:`/`Catalyst:` 行当元数据**丢弃**；`{variant:N}`/`{range:x}` 标注**剥离丢语义**；`Quality (X Modifiers):` 行穿透成 Unsupported 噪声 |
| `crates/pobr-core/src/item.rs`（151 行） | `ingest_item` | 三段文本 → parse_mod → 带 SourceId 的 Modifier；品质刻意不在此建模（正确，保持） |
| `crates/pobr-core/src/mod_parser.rs`（1536 行） | `Bonded:` 前缀 72-97 | 无 "Effect of Small/Notable Passive Skills in Radius"、"grant nothing"、"upgrades radius to"、"Grants Skill:" 模式 |
| `crates/pobr-build/src/calc_orchestrator.rs`（2662 行） | add_item 局部剔除 373-419（显式 `slot == Weapon1` 分支）；`weapon_contribution` 1087-1146（仅物理 adds/inc/攻速三种局部）；`is_weapon_local_mod` 1333-1339（文本枚举）；`parse_local_defence_inc/flat` 1342-1404；`resolve_passive_nodes` 1746-1760；`radius_jewel_grant_texts` 1810-1885（计数段 1857-1868：`PassiveNodeKind::Normal` 一律计入 smalls，无 isAttribute 排除） | 武器元素 adds/局部暴击/局部元素 inc **泄漏为全局**；Weapon2 局部词条不剔除 |
| `crates/pobr-build/src/xml_build.rs`（1220 行） | `parse_passive_nodes` 315-360、AttributeOverride 365-400、`slot_from_pob_name` 724-740、useSecondWeaponSet 643 | 无 `<WeaponSet1/2 nodes>` 解析；item 文本块经 `parse_pob_xml_item` 进入 |
| `crates/pobr-data/src/passive_tree.rs` | `PassiveTreeSpec` 38-47 | 仅 allocated_nodes/mastery_effects/attribute_overrides 三字段，无 alloc_mode |
| `crates/pobr-data/src/catalog/tree.rs` | `PassiveNodeDef` 36-74 | 无 is_attribute/options/is_switchable/unlock_constraint/classes_start |
| `crates/pobr-tree/src/node.rs`（118 行） | `collect_allocated_mods` 38-87、`rewrite_attribute_choice` 106-118 | 属性三选一已有**文本判别**实现（`+N to any Attribute` 改写） |
| `crates/pobr-tree/src/radius_jewel.rs`（137 行） | 四档常量、`JewelRadius` | 仅 outer 圆盘，无 inner 环；未消费 `base/jewel_radii.json` |
| `tools/pobr-data-adapter/src/tree.rs`（199 行） | `RawNode` 57-87 | 只取 5 个 is* 布尔；isAttribute/options/isSwitchable 不读取 |
| `tools/sync-pob-catalog/src/extract_lua.rs` | BOOTSTRAP_LUA 内嵌、`resolve_luajit`、JSONL→Rust 排序序列化 | extract-lua 扩域的模板 |
| `crates/pobr-gamedata/src/{overlay.rs,ruleset.rs,domains/}` | merge 引擎 + RuleSet 骨架 | 新 loader 按 `domains/*.rs` 一表一文件惯例加 |
| `crates/pobr-item/src/lib.rs` | 650B 占位 | 本阶段落地主体 |

### 1.2 vendor PoB2 侧（`vendor/PathOfBuilding-PoE2/src/`，行号已验证）

| 文件 | 函数/数据 | 行号 |
|---|---|---|
| `Classes/Item.lua`（2123 行） | catalystList/catalystTags 手工表 | 14-29 |
| | getCatalystScalar | 33-58 |
| | ParseRaw（状态行 409-420；`Quality (X Modifiers)` 524-530；`Rune:` 544-555；Variant/Selected/Alt 571-636；Prefix/Suffix 引用 643-656；Catalyst 676-683；标注剥离 708-734；分桶 958-969；rune 组合逆推 1011-1155；affixLimit 1178-1219） | 294-1253 |
| | NormaliseQuality | 1255-1263 |
| | GetModSpawnWeight | 1265-1282 |
| | **BuildRaw（逆向序列化，往返契约权威）** | 1284-1483 |
| | UpdateRunes | 1491-1549 |
| | Craft | 1552-1613 |
| | **CheckModLineVariant（variant 门控权威）** | 1615-1623 |
| | **calcLocal（局部性判定权威："mod name + flag 精确匹配且无 tag"）** | 1655-1682 |
| | BuildModListForSlotNum（武器局部 1739-1786：WeaponRange/ReloadTime 1739-1743、五系 adds 1745-1771、局部暴击 1772、Accuracy/LifeOnHit/Leech per-hand 条件 1776-1786；护甲 1794-1830；槽位标签 `{SlotName}`/`{Hand}` 1701-1707） | 1685-1933 |
| | BuildModList（variant 门控调用 1965-1966；applyRange 调用 1973-1989；ExtraSkill→grantedSkills 2017-2029、2049-2068；slotModList 2112-2122） | 1936-2124 |
| `Modules/ItemTools.lua` | **itemLib.applyRange**（range 取值 + modScalability 消费 + catalyst scalar + corruptedRange；format 枚举 172-260） | 77-326 |
| `Data/ModScalability.lua`（1.3M，15037 行） | `["词条#模板"] = { {isScalable, formats?}, ... }` 每数值槽一项 | 全文件 |
| `Data/ModRunes.lua`（165K） | `["符文名"] = { ["槽类型"] = { type, 词条行…, statOrder, tradeHashes, rank } }` | 全文件 |
| `Data/Uniques/*.lua`（28 文件 + Special/） | raw 文本块数组（含 Variant/Source/League 行） | 全部 |
| `Modules/CalcSetup.lua` | buildModListForNode/ForNodeList：keystone 去重 130-136；radius 珠宝两轮 141-164；HasNoEffect 清空 166-168；PassiveSkillEffect 缩放 170-177；HasOtherEffect 替换 186-191；ExtraSkill 193-202；**WeaponSet 条件注入/移除 208-228**；Jewel 局部缩放收集/应用 230-274（含 isAttribute 排除）；Hulking Form 聚合 287-290 | 126-325 |
| `Classes/ModStore.lua` | **ScaleAddMod（缩放取整语义：highPrecision 查表 → `floor(v*scale*10^p)/10^p`，否则 `modf(round(v*scale,2))` 取整数部分；`+Levels` 用 floor）** | 45-80 |
| `Classes/PassiveSpec.lua` | `<WeaponSetN nodes>` 解析 | 137-146 |
| | ImportFromNodeList 设 allocMode | 355、366 |
| | CanPathThroughAllocMode（三套寻路图，M7 参考） | 824-828 |
| | **isSwitchable ReplaceNode（`options[curClassName] or options[curAscendClassName]`）** | 1251-1260 |
| | SwitchAttributeNode | 2469-2480 |
| `Classes/PassiveTree.lua` | options 子节点 metatable 继承（缺 stats 时回退原节点）+ ProcessStats | 546-566 |
| | nodesInRadius 环形判定 `innerSquared <= d² <= outerSquared` | 346 |
| `Export/Scripts/passivetree.lua` | classpassiveskilloverrides → isSwitchable.options（按职业名键控） | 941-971 |
| | ascendancypassiveskilloverrides + ascendancy.dat Replace 列 → 飞升替换 | 973-1040（替换表构建 516/572-579） |
| | Attribute 三选一 options | 873-885 |
| | GrantedSkill → "Grants Skill: X" 词条 | 887-911 |
| | ConstraintNode → unlockConstraint | 808-822 |
| `TreeData/0_5/tree.lua`（2.4M） | isAttribute 293 处（样例 24719：options[1..3] = Str/Dex/Int 子节点，各带 stats）；isSwitchable 78 处（样例 23607：options 按职业/替换飞升名键控，**stats 可缺省=继承原节点**）；unlockConstraint 200 处 | 数据源 |
| `Modules/ModParser.lua` | radius 珠宝词条语义（Small 要求 `node.type=="Normal" and not node.isAttribute` 6855-6857） | 6839-6905 |
| | "upgrades radius to" → timeLostJewelRadiusOverride | 5481-5482 |

### 1.3 数据现状

- `data/4.5.0.3.4/base/`：base_items/mods/passive_tree(_meta)/granted_effect* 等 19 表 + M0 九张常量表（含 `jewel_radii.json`）。
- `data/4.5.0.3.4/overlay/`：仅 `skill_overrides.json`。**无 local_mods/high_precision/mod_scalability/catalysts/runes/uniques**。
- `pipeline/config.json`：**无** ClassPassiveSkillOverrides / AscendancyPassiveSkillOverrides / PassiveSkills 表。
- GGG 树导出原始 `pipeline/tree/data.json` 为 gitignore 不在仓库，**是否携带 isAttribute/options/isSwitchable 未核实**（审计同样未核实）——见 §3.D1 决策梯。

---

## 2. 预备工作项 WI-0（串行，所有 track 之前）

**目标**：把 `calc_orchestrator.rs`（2662 行）中本阶段三个 track 都要写的两段抽成独立模块，消除大文件并行冲突。**纯搬迁 commit，golden diff = 0**。

- 新建 `crates/pobr-build/src/item_local.rs`：搬入 `weapon_contribution`/`non_weapon_attack_contribution`/`unarmed_contribution`/`is_weapon_local_mod`/`weapon_local_phys_adds`/`weapon_local_phys_inc`/`weapon_local_attack_speed`/`parse_local_defence_inc`/`parse_local_defence_flat`/`item_local_defence_*`/`parse_has_per_level_defence`/`item_rolled_defence`/`defence_base_modifiers` 及 add_item 路径的局部剔除闭包（373-419 段抽函数）。
- 新建 `crates/pobr-build/src/passive_inject.rs`：搬入 `resolve_passive_nodes`/`radius_jewel_grant_texts`/`parse_jewel_radius`/`parse_grant_line`/`GrantTargetKind` 及 orchestrator 中树词条注入调用段。
- `lib.rs` 增 `mod` 声明；函数可见性 `pub(crate)`。
- **门禁**：`cargo test --workspace` 全绿 + golden_regression/ninja_parity **逐值不变**；单独 PR 立即合并，各 track 之后 rebase。
- 预估：0.5-1 人日，纯移动 ~800 行。

---

## 3. 工作项分解

### Track A：物品编辑态核心（pobr-item 落地 + variant 门控 + BuildRaw）

#### WI-A1 Item 头部行解析 + variant 门控（接口先行，最早合并）

- **目标**：消灭 16-G1——多 variant unique 词条全量叠加的数值爆炸；同时为 Track B 备好 catalyst 字段、消除 `Quality (X Modifiers):` 噪声（16 号缺口 7 第一半）。
- **涉及文件**：`crates/pobr-data/src/item.rs`（Item 扩字段）、`crates/pobr-core/src/item_text.rs`、`crates/pobr-core/src/item.rs`（ingest 不变，确认归因）。
- **vendor 参照**：Item.lua:571-636（Variant/Selected Variant/Has Alt Variant 行解析）、1615-1623（CheckModLineVariant）、1965-1966（门控调用点）、524-530（`Quality %(%a+ Modifiers%)` 正则）、676-683（Catalyst/CatalystQuality 行）。
- **设计**：
  - `Item` 新增字段（REAL 类型直改，全部带默认值，跨 crate 构造点同步）：
    ```rust
    pub selected_variant: Option<u8>,        // Selected Variant: N
    pub alt_variants: Vec<u8>,               // Alt Variant 1..5 选择（最多 5 个）
    pub catalyst: Option<u8>,                // Catalyst id（对照 catalysts.json）
    pub catalyst_quality: u8,                // CatalystQuality / Quality (X Modifiers)
    ```
  - `item_text.rs`：`Variant:` 行只收集 variant 名列表（用于编辑态，calc 路径可丢）；`Selected Variant:`/`Has Alt Variant`/`Catalyst:`/`CatalystQuality:` 行解析入字段；词条行在 `strip_pob_annotations` **之前**先提取 `{variant:N1,N2,...}` 前缀为行级 variant 集合，按 CheckModLineVariant 等价规则门控——不匹配 `selected_variant`/任一 alt variant 的行**直接不进入三段文本**（calc 视图）。
  - `Quality (Defence Modifiers): +20%` 类行：识别为 catalyst quality 元数据（描述词 → catalyst id 映射查 Track B 的 catalysts.json；B 未合并前先存 `catalyst_quality` + 描述词字符串暂存字段或直接丢描述词，行不再落 explicit）。
- **测试**：单测——多 variant 块（Atziri's Splendour 三 variant 样例手工构造）选 variant 后仅对应行入段；`Selected Variant` 缺省时行为对照 PoB2（variantList 为 nil 的行恒过）；`Quality (X Modifiers)` 行不再出现在 modifier_texts；ninja 18-build golden **逐值不变**（ninja 样本无 variant 行）。
- **预估**：1.5 人日（~250 行 + 测试）。

#### WI-A2 pobr-item::ItemDraft（全保真编辑态草稿）

- **目标**：P16 落地——编辑态半边归 pobr-item。`ItemDraft` 保留 ParseRaw 等价的**行级结构**（calc 视图丢弃的信息这里全要）。
- **涉及文件**：`crates/pobr-item/src/{lib.rs,draft.rs,parse.rs}`（新）、`crates/pobr-item/Cargo.toml`（已依赖 pobr-data/pobr-core，足够）。
- **vendor 参照**：Item.lua ParseRaw 294-1253 全函数（GAME/WIKI 状态机可简化为 PoB XML + 剪贴板两种现有入口的超集）；分桶 958-969。
- **设计**：
  ```rust
  pub struct ModLineDraft {
      pub raw: String,                  // 原始行（含全部 {} 标注）
      pub text: String,                 // 剥标注后的干净文本
      pub variants: Option<Vec<u8>>,    // {variant:1,2}
      pub range: Option<f64>,           // {range:0.5}
      pub corrupted_range: Option<f64>, // {corruptedRange:x}
      pub crafted: bool, pub rune: bool, pub enchant: bool, pub fractured: bool,
      pub custom_tags: Vec<String>,     // {tags:...} 等未建模标注原样保留
      pub bucket: LineBucket,           // Implicit | Explicit | Enchant | Rune
  }
  pub struct ItemDraft {
      pub header: …(rarity/title/base/unique_id/item_level/quality/level_req/sockets 原文),
      pub variants: Vec<String>,        // Variant: 行名列表
      pub selected_variant: Option<u8>, pub alt_variants: Vec<u8>,
      pub catalyst: Option<u8>, pub catalyst_quality: u8,
      pub prefixes: Vec<AffixRef>, pub suffixes: Vec<AffixRef>, // Prefix:/Suffix: modId+range（Craft 输入）
      pub runes: Vec<String>,           // Rune: 行
      pub radius_label: Option<String>, pub limited_to: Option<String>,
      pub lines: Vec<ModLineDraft>,
      pub states: ItemStates,           // corrupted/mirrored（先布尔，缺口 11 后续扩）
  }
  impl ItemDraft {
      pub fn parse(raw: &str) -> Result<Self, DraftError>;       // 解析不丢信息
      pub fn to_calc_item(&self, rules: Option<&ItemRules>) -> pobr_data::Item;  // 门控+applyRange 具体化 → calc 视图
  }
  ```
  - `to_calc_item` 与 pobr-core `parse_pob_xml_item` 共享门控/取值逻辑——共享函数放 pobr-core（`item_text::gate_variant_line`、`apply_range::*`），pobr-item 只做编排，**不复制规则**。
- **测试**：解析-降级（draft.to_calc_item == parse_pob_xml_item 同输入产物，等价性测试，覆盖 ninja 18-build 全部 `<Item>` 块）；不支持标注的行 raw 原样保留（lossless 断言）。
- **预估**：2.5 人日（~600 行）。

#### WI-A3 BuildRaw 序列化 + 往返 golden fixture（R9 缓解主体）

- **目标**：P16 验收契约——"BuildRaw 往返等价 + golden fixture（编辑态无 parity 可依）"。
- **涉及文件**：`crates/pobr-item/src/build_raw.rs`（新）、`crates/pobr-item/tests/build_raw_roundtrip.rs`（新）、fixture 用 `examples/demo-bd-test/raw/` 既有样本 + 新增 `crates/pobr-item/tests/fixtures/`。
- **vendor 参照**：Item.lua BuildRaw 1284-1483（行序、标注重建、`Implicits: N` 头计数、variant 标注 `{variant:…}` 输出格式）。
- **设计**：`ItemDraft::build_raw() -> String`。往返契约分两档：
  1. **强契约（门禁）**：`parse(build_raw(parse(x))) == parse(x)`（语义不动点，全样本）；
  2. **字节契约（报表）**：`build_raw(parse(x))` 与 x 规范化（行 trim、空行折叠）后 byte-diff，差异行单列报表——PoB2 自身 BuildRaw 也不保证 byte-stable，字节档不作硬门禁但报表入 CI 工件，趋零是 M6 前目标。
- **fixture 计划**：ninja 18-build XML 内嵌全部 `<Item>` 块（约 200+ 件）+ vendor `Data/Uniques/` 抽样 ≥50 块（覆盖 Variant/range/League 行；含 body.lua 的 Atziri's Splendour 多 variant 件）落 golden 文件；`cargo test -p pobr-item --test build_raw_roundtrip`。
- **预估**：2 人日（~300 行 + harness）。

#### WI-A4 uniques.json 消费 + Craft 最小闭环（可裁剪，排最后）

- **目标**：编辑态可从 uniques DB 选件实例化 ItemDraft（variant 列表/range 区间自动补全）；`Prefix:`/`Suffix:`（modId+range）经 mods.json 重建词条（Craft 等价最小子集：重建+statOrder 同序合并，不做词缀池查询 UI 语义）。
- **涉及文件**：`crates/pobr-item/src/{uniques.rs,craft.rs}`（新）。
- **vendor 参照**：Item.lua Craft 1552-1613、GetModSpawnWeight 1265-1282（本阶段只实现 weightKey×tags 匹配函数，供测试断言，不接 UI）；Data.lua:1053-1060（uniques 加载）。
- **依赖**：Track B 的 `uniques.json`/`runes.json` 落库；mods.json 现有 `name`/stats 列（**rendered_lines 缺失** → Craft 重建词条文本暂走 stat_id+min/max 的近似渲染或直接保留 modId 引用 + XML 已展开行兜底，完整渲染待 M5b statdesc；在代码中以 TODO(M5b) 标注）。
- **测试**：unique 实例化后 variant 行集合与 vendor raw 块一致（抽样 20 件）；Craft 重建与 XML 已展开行 diff 报表。
- **预估**：2 人日。**若工期紧，本 WI 整体后置不阻塞阶段验收**。

### Track B：数值具体化数据链（applyRange 引擎 + 四张 overlay 表）

#### WI-B1 extract-lua 扩域：mod_scalability / catalysts / runes / uniques

- **目标**：四表入 `overlay/`（P13：luajit 执行 vendor 序列化，产物 byte-stable、头部记 vendor commit、禁手改）。
- **涉及文件**：`tools/sync-pob-catalog/src/extract_lua.rs`（扩 domain 枚举与分发）、新增 bootstrap：`extract_mod_scalability.lua`/`extract_catalysts.lua`/`extract_runes.lua`/`extract_uniques.lua`、`tools/sync-pob-catalog/src/main.rs`（子命令参数）。
- **vendor 源与抽取方式**：
  - `Data/ModScalability.lua`、`Data/ModRunes.lua`、`Data/Uniques/*.lua`：纯 `return {...}` / 文本数组，luajit `dofile` 直接执行序列化（同 skill_overrides 模式）。
  - catalystList/catalystDescriptorList/catalystTags 嵌在 `Classes/Item.lua:14-29` **逻辑文件内**，无法 dofile——bootstrap 用 lua 从源码截取这三个 `local xxx = {...}` 表字面量后 `loadstring` 执行（表字面量自身是合法 lua；比正则啃可靠且 drift 可检）。
- **新增 JSON 表 / schema**（schema 文件见 WI-B2）：

| 表 | 路径 | 形态 |
|---|---|---|
| `mod_scalability.json` | overlay/ | `[{ template: "#-化词条文本", slots: [{ is_scalable: bool, formats: ["divide_by_one_hundred", …] }] }]`（vendor 15037 行 ≈ 3-4MB JSON，注意 R6） |
| `catalysts.json` | overlay/ | `[{ id: 1-based, name, descriptor: "Life"…, tags: ["life"…] }]`（12 条） |
| `runes.json` | overlay/ | `[{ name, slots: { "<slot_class>": { kind: "Rune"\|"SoulCore", lines: [..], rank: [..], stat_order: [..] } } }]` |
| `uniques.json` | overlay/ | P15 双层：`[{ name, base, raw: "原始文本块", league?, source?, variants: ["…"], upgrade? }]` + 预解析索引最小化（base/variants 即可，词条模板行解析由 pobr-item 运行时做） |

- **测试**：重跑 byte-diff=0（纳入 regen-check 思路的 overlay drift 检查）；抽样断言（如 ModScalability `"# Armour per 2 Strength"` 条目、Hayoxi's Soul Core helmet 行、catalysts 第 3 条 = Carapace/Defence/{defences,armour,evasion,energyshield}）。
- **预估**：2 人日。

#### WI-B2 catalog schema + gamedata loader + RuleSet 注入域

- **目标**：四表的强类型 schema 与懒加载域；RuleSet 聚合出 `ItemRules` 供 pobr-build 注入 pobr-core（P9）。
- **涉及文件**：`crates/pobr-data/src/catalog/item_overlay.rs`（新：`ModScalabilityDef`/`CatalystDef`/`RuneDef`/`UniqueDef`）、`catalog/mod.rs`（re-export，**本文件冲突仲裁人 = Track B**）、`crates/pobr-gamedata/src/domains/{mod_scalability,catalysts,runes,uniques}.rs`（新）、`domains/mod.rs`、`ruleset.rs`（`RuleSet.item_rules: Option<ItemRules>`；`ItemRules { scalability: HashMap<String, …>, catalysts: Vec<CatalystDef> }`——runes/uniques 编辑态用，不进 ItemRules，按需单独加载）、`manifest.json` overlay 段登记。
- **约束**：新字段一律 `#[serde(default)]`/Option（R7）；pobr-data 零内嵌数据 lint 不得触发。
- **测试**：gamedata 加载单测（真实 data 目录冒烟 + 缺表容忍）；merge 引擎对 overlay 新表的 key 冲突报错路径复用既有单测模式。
- **预估**：1 人日（~400 行）。

#### WI-B3 pobr-core::apply_range 取值引擎

- **目标**：消灭 16-G2——`{range:0.5}+(40-50) to maximum Life` 类词条静默丢失。
- **涉及文件**：`crates/pobr-core/src/apply_range.rs`（新，纯函数零 I/O）。
- **vendor 参照**：ItemTools.lua:77-326 逐段对照——`(min-max)` 取值 `value = min + range*(max-min)`（77-90，含负号翻转与 antonym 处理）；`#`-模板化反查 modScalability；per-slot `isScalable=false` 时回退原始中值；formats 枚举换算（172-260：divide_by_one_hundred / per_minute_to_per_second / 30 余种，**只实现 data 中实际出现的格式集**，载入期对未知 format 告警）；catalyst scalar（getCatalystScalar 33-58：`(100+quality)/100`，按 catalystTags ∩ mod tags 匹配）；corruptedRange 同路径。
- **签名**：`pub fn apply_range(line: &str, range: f64, scalability: Option<&ScalabilityTable>, value_scalar: f64) -> String`。**无表降级**：朴素线性取值不丢词条（审计修复方向原文），结果打 `approx` 标记进 unsupported 报表而非丢弃。
- **测试**：与 PoB2 对拍——离线 luajit 跑 `itemLib.applyRange`（复用 extract-lua 引导）对 ≥200 条抽样（覆盖每种 format 至少 1 条 + 负值 + 多数值槽）生成期望值 fixture，Rust 侧逐条断言；精度边界（highPrecision 类词条）单测。
- **预估**：1.5 人日（~300 行）。

#### WI-B4 解析链接线（XML 导入路径 range/catalyst 具体化）

- **目标**：XML 导入的 range 词条经 applyRange 具体化后进 calc；catalyst scalar 生效。
- **涉及文件**：`crates/pobr-core/src/item_text.rs`（行处理插入 applyRange 调用——**依赖 WI-A1 已合并**，改动限 classify_mod_lines 前的行变换函数）、`parse_pob_xml_item`/`parse_item_text` 签名扩 `ctx: Option<&ItemRules>`、`crates/pobr-build/src/xml_build.rs`（item 解析调用点传 ctx——**限定只动 item 解析函数**，xml_build 主责在 Track F，见归属表）、`calc_orchestrator.rs`（BuildData/RuleSet 透传，W3 注入管道）。
- **测试**：含 `{range:0.5}` 词条的手工 XML fixture 端到端数值断言；ninja 18-build golden **逐值不变**（ninja 样本无 range 行）。
- **预估**：1 人日。

### Track C：武器/护甲局部 mod 结构化结算 + Weapon2

#### WI-C1 local_mods.json 落表（M0 未交付的兜底）

- **目标**：局部词条 ModName 白名单数据化（P3 的 M0 部分，实查未落地，本 track 自带；若开工时 M0 收尾已落表则跳过本 WI 直接消费）。
- **涉及文件**：`tools/sync-pob-catalog/src/local_mods.rs`（新子命令 `gen-local-mods`：权威清单内嵌于**工具**、产物可重复生成——该表性质是 Item.lua calcLocal 语义的人工归纳，无 vendor 单表可 dofile）、`crates/pobr-data/src/catalog/local_mods.rs`（新 `LocalModDef`）、`crates/pobr-gamedata/src/domains/local_mods.rs`、`data/4.5.0.3.4/overlay/local_mods.json`。
- **schema**：
  ```json
  { "name": "PhysicalDamage", "mod_types": ["INC"], "domain": "weapon",
    "weapon_data_key": "phys_inc", "verified": true }
  ```
  首批条目对照 Item.lua:1739-1786（武器：PhysicalDamage INC、五系 `<Type>Min/Max` BASE、CritChance BASE/INC、ElementalDamage INC、Speed INC、WeaponRange、Accuracy/LifeOnHit/Leech per-hand 系）+ 1794-1830（护甲：Armour/Evasion/EnergyShield/Ward flat+inc、Defences INC、BlockChance、MovementPenalty——后两者数据列属 M1/M2，条目先入表标 `consumer: "m2"`）。
- **测试**：表逐条单测（条目数、关键条目存在性）；重生 byte-diff=0。
- **预估**：0.5 人日。

#### WI-C2 结构化局部判定 + 双跑切换

- **目标**：消灭 `is_weapon_local_mod` 文本枚举（16 审计"pobr 自身混合点"），改 PoB2 结构化规则：词条 parse_mod 产物 **name ∈ local_mods 白名单 ∧ flags 精确匹配 ∧ 无 tag** 即局部（calcLocal:1655-1682 等价）。
- **涉及文件**：`crates/pobr-build/src/item_local.rs`（WI-0 产物，本 track 独占）。
- **设计**：新函数 `classify_local_mods(item, rules: &LocalModRules) -> LocalSplit { local: Vec<ParsedLocal>, global: Vec<String> }`；旧文本枚举保留为 `legacy_` 前缀。**双跑对照**（roadmap §0 执行纪律原文："核心改动……feature-gated 双跑对照，diff 报告干净后才删旧码"）：测试 harness 对 ninja 18-build 全部装备词条跑新旧两版分类，输出 diff 报表；预期差异 = 新版**多**识别的元素 adds/局部暴击/元素 inc（这正是 16-G3 修复），每条差异附 Item.lua 行号依据后，行为切换作为**独立 commit** 更新 baseline。
- **测试**：双跑 diff 报表测试 + 分类单测（带 tag 的同名词条不局部、flag 不匹配不局部——对照 calcLocal "if and only if" 语义）。
- **预估**：1.5 人日。

#### WI-C3 武器局部覆盖扩展：元素 adds / 局部暴击 / LocalElementalDamage 入 weaponData 乘区

- **目标**：消灭 16-G3 三类分叉（元素 adds 泄漏法术、局部元素 inc 错入全局加法桶、局部暴击混桶）。
- **涉及文件**：`crates/pobr-build/src/item_local.rs`（`WeaponContribution` 扩字段：五系 min/max、crit_chance_flat/inc、elemental_inc；`weapon_contribution` 消费 WI-C2 的 ParsedLocal）。
- **vendor 参照**：Item.lua:1745-1772（五系 adds 入 weaponData、局部暴击、LocalElementalDamage 作为武器基底独立乘区）；CalcOffence 武器 source 消费口径（与现有 phys 路径同构扩展）。
- **测试**：带元素伤害武器 fixture：攻击技能吃到元素 adds、**法术技能不吃**（泄漏专项断言）；局部 `% increased Elemental Damage` 与全局同词条数值分离断言；ninja_parity 数值变化逐条核对（行为 commit + baseline 独立 commit，附 PoB2 oracle 中间值）。
- **预估**：1.5 人日。

#### WI-C4 Weapon2 局部词条隔离

- **目标**：16 号缺口 5 的 M5c 子集——Weapon2 物品的局部词条不再按全局注入（恒高估方向）；副手 DPS 模型本身留 M4。
- **涉及文件**：`item_local.rs`（add_item 剔除路径去掉 `slot == Weapon1` 限定，按 WI-C2 分类对 Weapon1/Weapon2 同规则剔除）+ `WeaponContribution` 调用点保持只取 Weapon1（M4 双 pass 接口预留：函数签名收 `slot` 参数）。
- **测试**：Weapon2 带局部物理 inc 的 fixture：该词条既不进全局也不影响主手 DPS（当前口径），全局词条仍正常注入。
- **预估**：0.5 人日。

### Track D：树字段数据落库 + ReplaceNode + Grants Skill

#### WI-D1 数据源裁决 + 字段落库（搬迁不变式 commit）

- **目标**：`passive_tree.json` 补 is_attribute（293 节点）/options/is_switchable（78 节点）/unlock_constraint（200 处）/is_multiple_choice/is_free_allocate/apply_to_armour/classes_start 列。
- **数据源决策梯**（开工第一件事，半天内裁决并记录到 PR 描述）：
  1. 下载 GGG `grindinggear/poe2-skilltree-export` data.json，核查是否携带上述字段。**携带** → 路线 A：adapter（`tools/pobr-data-adapter/src/tree.rs` RawNode 扩字段）直读 → 字段进 `base/passive_tree.json`。
  2. **不携带** → 路线 B：`sync-pob-catalog extract-lua` 新增 `extract_tree_fields.lua`，dofile `vendor/.../TreeData/0_5/tree.lua`（纯数据，2.4M，luajit 可执行）抽取上述字段 → `overlay/passive_tree_fields.json`（按 skill id 键控的字段补丁），由 pobr-gamedata 既有 overlay merge 引擎合入。**路线 B 的关键优势**：options 的 stats 是 tree.lua 中已渲染的英文词条文本，**绕开 M5b statdesc 依赖**。
  3. 无论走哪条：`pipeline/config.json` 登记 `ClassPassiveSkillOverrides` / `AscendancyPassiveSkillOverrides`（列：OriginalNode/SwitchedNode/Character 或 Ascendancy 键，对照 passivetree.lua:941-1040 的 GetRow 用法）与 `Ascendancy`（Replace 列，passivetree.lua:572-579）。该 .dat 通道本阶段**仅登记下载 + 外键完整性对账**（与 tree.lua 抽取值 diff 报表），不做 describeStats 渲染（M5b 后才可作为 base 层正式来源，version-bump-drill 的长期路径）。
- **schema**（`crates/pobr-data/src/catalog/tree.rs`，全部 `#[serde(default)]`）：
  ```rust
  pub is_attribute: bool,
  pub is_switchable: bool,
  /// isSwitchable：键 = 职业名或替换后飞升名；isAttribute：键 = "1"/"2"/"3"。
  /// name/stats 缺省 = 继承原节点（PoB2 metatable __index 语义，PassiveTree.lua:546-566）。
  pub options: Vec<NodeOption>,
  pub unlock_constraint: Option<UnlockConstraint>,
  pub is_multiple_choice: bool, pub is_multiple_choice_option: bool,
  pub is_free_allocate: bool, pub apply_to_armour: bool,
  pub classes_start: Vec<String>,
  // NodeOption { pub key: String, pub name: Option<String>, pub stats: Vec<String>,
  //              pub replaced_node_id: Option<u32> }
  // UnlockConstraint：对照 passivetree.lua:808-822 实际结构定型
  ```
- **涉及文件**：`catalog/tree.rs`、`tools/pobr-data-adapter/src/tree.rs`（路线 A）或 `tools/sync-pob-catalog/src/extract_lua.rs`+新 bootstrap（路线 B）、`pipeline/config.json`、`data/4.5.0.3.4/{base|overlay}/passive_tree*.json` 重生、manifest。
- **门禁**：**搬迁不变式**——本 WI 仅落数据，无消费侧改动，parity **逐值不变**；字段计数断言（is_attribute=293、is_switchable=78、unlock_constraint=200，对 vendor tree.lua 计数）。
- **预估**：2 人日。

#### WI-D2 isSwitchable ReplaceNode 消费（17-G3 逻辑侧）

- **目标**：受影响职业/飞升的 build，switchable 节点按 options 替换 name/stats 后再注入。
- **涉及文件**：`crates/pobr-tree/src/node.rs`（`collect_allocated_mods` 签名扩 `ctx: &TreeContext { class_name, ascendancy_name }`，节点查 options：`options[class_name] 否则 options[ascendancy_name]`，命中且有 stats 则替换 stats，无 stats 继承原节点——对照 PassiveSpec.lua:1251-1260 + PassiveTree.lua:546-566）；`crates/pobr-build/src/passive_inject.rs` 调用点传 `build.character` 的 class/ascendancy（**按 §4.4 接力顺序在 Track F 合并后改**）。
- **测试**：Lich→Abyssal Lich 类 fixture（无 stats 的 option = 词条不变）；有 stats 替换的 fixture（从落库数据中筛一个带 stats 的 option 节点做断言）；非受影响职业 build 逐值不变。
- **预估**：1 人日。

#### WI-D3 树授予技能 Grants Skill → 技能列表（17 号缺口 6）

- **目标**：`Grants Skill: <X>` 节点词条（data 中 52 条）不再 Unsupported 丢弃，注入技能列表参与计算（CalcSetup.lua:193-202 等价）。
- **涉及文件**：`crates/pobr-tree/src/node.rs`（collect 时识别 `Grants Skill:` 行，从 modifier_texts 摘出，输出 `AllocatedNodeMods.granted_skills: Vec<String>`）、`crates/pobr-build/src/passive_inject.rs` + `calc_orchestrator.rs` 技能解析区（granted skill 名 → skill_gems/granted_effects 查表 → 走 `skill_source::ingest_gem` 同通道注入，SourceId 归因到节点）。
- **设计取舍**：识别在 pobr-tree 文本层做（不进 mod_parser——PoB2 在 ProcessStats 后由 CalcSetup 处理 ExtraSkill mod，pobr 等价点是 collect 层；mod_parser 留 M6 再统一）。技能名查不到 granted_effect 时记入 unsupported 报表，不报错。
- **测试**：飞升授予技能 fixture（如举盾/召唤系节点）：技能出现在技能列表且有非零输出；52 条覆盖率报表（命中/未命中分布）。
- **预估**：1.5 人日。
- **顺位**：在 §4.4 接力中排 D2 之后；若与 M5a（minion build 接线）发生技能编排区冲突，以先合并者为准、后者 rebase。

#### WI-D4 属性小点 options 正式口径（可裁剪）

- **目标**：属性三选一从文本判别（`rewrite_attribute_choice`）切到 options 子节点 stats 正式口径（is_attribute + options["1".."3"]）。
- **涉及文件**：`crates/pobr-tree/src/node.rs`。
- **测试**：与现文本路径等价性断言（293 节点全量：两种口径产出相同词条）→ 等价确认后删文本判别。
- **预估**：0.5 人日。等价不成立时保留文本路径并记录差异，**不阻塞**。

### Track E：节点效果缩放管线 + 半径完善

#### WI-E1 mod_parser 半径/效果词条模式

- **目标**：Time-Lost 主词条等不再 Unsupported：`N% increased Effect of Small (and Notable) Passive Skills in Radius`、`Notable Passive Skills in Radius grant nothing`、`Passive Skills in Radius also count gaining/…`（按需）、`upgrades radius to <tier>`（ModParser.lua:5481-5482）、`N% increased Effect of Small Passive Skills`（Hulking Form 全局）。
- **涉及文件**：`crates/pobr-core/src/mod_parser.rs`（本 track 独占）；产出 ModName 约定：`JewelSmallPassiveSkillEffect`/`JewelNotablePassiveSkillEffect`/`PassiveSkillHasNoEffect`（FLAG，限定 radius 语境 tag）/`SmallPassiveSkillEffect`/`TimeLostJewelRadiusOverride`（LIST/Override），与 PoB2 ModParser.lua:6839-6905 命名对齐。
- **测试**：每模式 ≥2 条真实词条文本（从 vendor Uniques jewel 文件抓）解析断言。
- **预估**：1.5 人日。

#### WI-E2 pobr-tree::node_effect.rs 缩放管线（17-G2 主体）

- **目标**：节点级效果管线，应用顺序严格对照 CalcSetup.lua buildModListForNode：**HasNoEffect 清空（166-168）→ PassiveSkillEffect 缩放（170-177）→ radius 珠宝二轮改写（141-164 框架，本阶段实现 inc-effect/grant-nothing 两类）→ HasOtherEffect 替换（186-191，按需后置）→ 局部 Jewel\*Effect 缩放（230-274，含 **isAttribute 与 ascendancy 排除**）→ 全局 SmallPassiveSkillEffect 聚合（287-290）**。
- **涉及文件**：`crates/pobr-tree/src/node_effect.rs`（新）、`crates/pobr-core/src/passive.rs`（ingest_passive_nodes 收 per-node directive 应用值缩放）、`crates/pobr-build/src/passive_inject.rs`（接线，按接力顺序最后改）。
- **设计**：
  ```rust
  pub struct NodeEffectDirective { pub node: NodeId, pub scalar: f64, pub suppress: bool }
  pub fn compute_node_effects(allocated, jewels, tree, ctx) -> Vec<NodeEffectDirective>
  ```
  缩放在 **Modifier 值层**应用（文本→parse→value×scalar），取整对照 ModStore.lua ScaleAddMod:45-80：highPrecision 命中 → `floor(v*scale*10^p)/10^p`；否则 `trunc(round(v*scale, 2))`；`+N to Level of …` 用 floor。`high_precision_mods.json`：**总架构评审裁决——该表由 M4 W-A2 正式落表（唯一生产点），本 WI 经 RuleSet 注入只消费不自建**；若开工实查仍缺失（M4 偏差），先实现 defaultHighPrecision=2 语义并登记阻塞项回 M4，不在本阶段建表（§7 开放问题 1）。
- **测试**：顺序单测（清空先于缩放——构造同节点同时命中两词条的用例）；Time-Lost 珠宝 fixture：半径内节点词条数值 = 原值×(1+effect%)，半径外不变；归因断言（缩放后 Modifier 仍归因到原节点 + 珠宝来源进 TraceGraph）。
- **预估**：2 人日。

#### WI-E3 环形半径 + jewel_radii.json 消费 + 半径升档

- **目标**：17 号缺口 5——`JewelRadius` 改 `{inner, outer}` 语义、`nodes_in_radius` 按 `inner² <= d² <= outer²` 判定（PassiveTree.lua:346）；半径档从 `base/jewel_radii.json`（M0 已落，含 8 个 inner>0 档与 1.2 距离乘数）读取，删 `radius_jewel.rs` 四档硬编码常量（**搬迁不变式**：标准四档数值不变则 parity 不变）；`upgrades radius to` 覆盖接 WI-E1 的解析产物。
- **涉及文件**：`crates/pobr-tree/src/{radius_jewel.rs,tree.rs}`、`crates/pobr-build/src/passive_inject.rs`（parse_jewel_radius 改查表）。
- **测试**：环形档 fixture（inner 内节点被排除）；Variable 档与升档珠宝节点集合断言；既有 radius 测试逐值不变。
- **预估**：1 人日。

#### WI-E4 radius also-grant 计数排除属性小点（17 号缺口 4，roadmap 专项回归点）

- **目标**：`X Passive Skills in Radius also grant` 的 Small 计数排除 `is_attribute` 节点（ModParser.lua:6855-6857：`node.type=="Normal" and not node.isAttribute`）。
- **涉及文件**：`crates/pobr-build/src/passive_inject.rs` 计数段（现 calc_orchestrator 1857-1868 搬迁后位置）。
- **兜底契约**：WI-D1 合并前用文本判别（`stats == ["+5 to any Attribute"]` 模式，pobr-tree node.rs 已有同款判别逻辑可复用）先行落地；D1 合并后切 `def.is_attribute` 并保留文本判别为 debug 断言（两口径不一致即 panic-in-test）。
- **测试**：**radius 珠宝 attribute 误计数专项回归**（roadmap M5 验收点名）：构造半径内含属性小点的 fixture，断言 grant 份数不含属性小点；对带 also-grant 珠宝的 ninja build 重算并按行为 commit 纪律更新 baseline。
- **预估**：0.5 人日。

### Track F：WeaponSet 条件 gating（17-G1）

#### WI-F1 XML 解析 + PassiveTreeSpec.alloc_mode

- **目标**：`<Spec>` 下 `<WeaponSet1 nodes="…"/>`/`<WeaponSet2 nodes="…"/>` 解析（PassiveSpec.lua:137-146：`nodes` 属性含**全部**已分配节点，WeaponSet 子元素只标记归属）。
- **涉及文件**：`crates/pobr-data/src/passive_tree.rs`（`PassiveTreeSpec` 增 `alloc_modes: HashMap<NodeId, u8>`，缺席=0=普通）、`crates/pobr-build/src/xml_build.rs`（parse_passive_nodes 同处扫 WeaponSet 子元素）。
- **roadmap 措辞校准**：roadmap/架构表把 "weapon_set" 列在 passive_tree.json 扩展里；实际语义是 **build 状态**（PoB2 存于 Spec XML 而非树数据），本蓝图按 build 状态实现，树静态 JSON 不加列（偏差已记 §7 开放问题 3）。
- **测试**：双武器组 build XML fixture 解析断言（节点全量 + alloc_mode 标记正确）。
- **预估**：1 人日。

#### WI-F2 Condition:WeaponSet1/2 注入 + 激活组联动

- **目标**：alloc_mode≠0 节点的全部 mod 附 `Condition:WeaponSet{N}` tag；普通节点 mod 上若词条自带该条件则按 CalcSetup.lua:208-228 语义改写/移除；激活武器组（ItemSet `useSecondWeaponSet`，xml_build:643 已解析）→ 设 `Condition:WeaponSet1` 或 `WeaponSet2` 为 true（CalcConfig conditions）。
- **涉及文件**：`crates/pobr-build/src/passive_inject.rs`（注入点——接力首位）、`calc_orchestrator.rs` config 条件设置处（一行级）。
- **测试**：fixture——武器组 2 节点词条仅在 useSecondWeaponSet=true 时生效；两组词条不再同时生效（恒高估修复，方向断言）；ninja build（多数无武器组点）逐值不变；含武器组点的 ninja build 数值下降需附 PoB2 对拍、baseline 独立 commit。
- **预估**：1 人日。

---

## 4. 并行 track 切分

### 4.1 总览

| Track | 主题 | WI | 规模 | 可与谁并行 |
|---|---|---|---|---|
| WI-0 | orchestrator 拆分（搬迁） | — | 0.5-1 人日 | **串行最先** |
| A | 物品编辑态 + variant + BuildRaw | A1→A2→A3→A4 | ~8 人日 | B/C/D/E/F |
| B | applyRange + 四张 overlay 表 | B1/B2/B3 并行，B4 依赖 A1 | ~5.5 人日 | A/C/D/E/F |
| C | 武器局部结构化 + Weapon2 | C1→C2→C3→C4 | ~4 人日 | A/B/D/E/F（依赖 WI-0） |
| D | 树字段落库 + ReplaceNode + Grants Skill | D1→D2/D3/D4 | ~5 人日 | A/B/C（passive_inject 接力见 4.4） |
| E | node_effect 管线 + 半径 | E1/E3 并行→E2→E4 | ~5 人日 | A/B/C（接力最后） |
| F | WeaponSet gating | F1→F2 | ~2 人日 | A/B/C/D 前段（接力首位） |

合计 ~30 人日；3-5 agent 并行约 1.5-2 周。**裁剪优先级**（工期压缩时砍序）：A4 → D4 → E2 的 HasOtherEffect 子项 → C4。16-G1/G2/G3、17-G1/G2/G3 六个 high 缺口对应的 WI（A1/A3、B1-B4、C2/C3、F1/F2、E2、D1/D2）不可裁。

### 4.2 文件归属表（独占写；未列文件默认冻结，确需改先在 PR 里声明）

| 文件/目录 | 归属 | 备注 |
|---|---|---|
| `crates/pobr-item/**` | **A** | |
| `crates/pobr-core/src/item_text.rs` | **A** | B4 的 applyRange 插入点：A1 合并后由 B 改"行变换"单一函数，A 复核 |
| `crates/pobr-data/src/item.rs` | **A** | catalyst 字段 A1 一并加（B 的需求前置吸收） |
| `crates/pobr-core/src/apply_range.rs`（新） | **B** | |
| `crates/pobr-data/src/catalog/item_overlay.rs`（新） | **B** | |
| `crates/pobr-data/src/catalog/local_mods.rs`（新） | **C** | |
| `crates/pobr-data/src/catalog/tree.rs` | **D** | |
| `crates/pobr-data/src/catalog/mod.rs` | 共享 | re-export 各加各行；**冲突仲裁人 B** |
| `crates/pobr-gamedata/src/domains/<新表>.rs` | 各 track 一表一文件 | `domains/mod.rs`/`lib.rs`/`ruleset.rs` **主责 B**；C/D 在 B 的 ruleset 扩展合并后 rebase 加自己字段 |
| `tools/sync-pob-catalog/src/extract_lua.rs` + `extract_*.lua` | **B** | D 路线 B 的 `extract_tree_fields.lua` 例外归 D（文件独立，分发枚举行冲突 trivial，仲裁人 B） |
| `tools/sync-pob-catalog/src/local_mods.rs`（新） | **C** | main.rs 子命令注册冲突仲裁人 B |
| `tools/pobr-data-adapter/src/tree.rs`、`pipeline/config.json` | **D** | |
| `crates/pobr-build/src/item_local.rs`（WI-0 新拆） | **C** | |
| `crates/pobr-build/src/passive_inject.rs`（WI-0 新拆） | **接力：F → D → E** | 见 4.4 |
| `crates/pobr-build/src/calc_orchestrator.rs` 残余主体 | 冻结 | 确需改动按 F→D→E 同序；D3 的技能解析区改动单独小 commit |
| `crates/pobr-build/src/xml_build.rs` | **F** | B4 限定只动 item 解析调用点函数 |
| `crates/pobr-data/src/passive_tree.rs` | **F** | |
| `crates/pobr-tree/src/node.rs` | **D** | |
| `crates/pobr-tree/src/{node_effect.rs(新),radius_jewel.rs,tree.rs}` | **E** | |
| `crates/pobr-core/src/mod_parser.rs` | **E** | |
| `crates/pobr-core/src/passive.rs` | **E** | |
| `data/4.5.0.3.4/` 新 overlay 文件 | 各自新文件无冲突 | `manifest.json` 冲突仲裁人 B |

### 4.3 接口契约（track 间唯一握手面）

1. **A1 契约**（A→B/C）：`pobr_data::Item` 新字段名与语义如 §3.A1 所列；`item_text` 暴露 `pub(crate) fn gate_variant_line(...)`；A1 是第一个合并的功能 PR。
2. **B2 契约**（B→A/C/核心）：`RuleSet.item_rules: Option<ItemRules>`；`ItemRules { scalability, catalysts }`；`apply_range(line, range, scalability, value_scalar) -> String`（None 表时朴素线性）。pobr-item 与 xml 导入共用同一函数，**禁止两处实现**。
3. **C1 契约**（C 内部 + 未来 M2 消费）：`LocalModDef` schema 如 §3.C1；`RuleSet.local_mods`。
4. **D1 契约**（D→E）：`PassiveNodeDef.is_attribute` 等字段如 §3.D1；E 在 D1 合并前以文本判别兜底，合并后切字段（两口径并行断言一个版本）。
5. **D2 契约**（D→编排）：`collect_allocated_mods(spec, nodes, ctx: &TreeContext)`；`AllocatedNodeMods` 增 `granted_skills: Vec<String>`。
6. **E2 契约**（E→core）：`NodeEffectDirective`；`ingest_passive_nodes(..., directives: &[NodeEffectDirective])`，缩放取整语义对照 ScaleAddMod。
7. **F1 契约**（F→D/E）：`PassiveTreeSpec.alloc_modes: HashMap<NodeId, u8>`（0=普通）；条件名常量 `"WeaponSet1"/"WeaponSet2"`。

### 4.4 必须串行的先后序

```
WI-0（拆分，golden diff=0）
  ├─→ Track C 全部
  ├─→ passive_inject.rs 接力：F2 → D2/D3 → E2/E4（每棒合并后下一棒 rebase）
A1 ─→ B4（XML 接线）/ A2
B2(ruleset 扩展) ─→ C1 的 RuleSet 字段 / E2 的 high_precision 域
D1 ─→ D2/D3/D4 消费、E4 的字段口径切换（E4 兜底版不等 D1）
B1(uniques/runes 表) ─→ A4
```

---

## 5. 门禁与验收

### 5.1 通用门禁（roadmap §0 原文，每次合并回 master 适用）

> 1. `cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；
> 2. **ninja_parity 18-build 零回归**——防御 51% / 进攻 24%（@5% 容差）为底线不得倒退；
> 3. 涉及解析/数据的阶段：加 pob2-oracle 对拍或 generated 重生一致性校验。

注：M5c 实际开工时 baseline 应已到 M4 后水位（进攻 ≥70% / 防御 ≥85%），"零回归"以**当时已提交 baseline** 为准。

> **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
> 行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。

本阶段的搬迁不变式适用点：WI-0、WI-D1、WI-E3 的常量迁出、WI-C1 落表。行为 commit 适用点（每个都要 PoB2 依据 + baseline 独立 commit）：A1 variant 门控（ninja 预期 diff=0）、C2/C3 局部切换、E4 计数修复、F2 条件 gating、E2 缩放生效。

### 5.2 每 track 局部门禁

| Track | 局部门禁 |
|---|---|
| A | BuildRaw 往返强契约（语义不动点）全样本绿；字节档 diff 报表入 CI 工件；多 variant 门控单测；ninja golden 逐值不变 |
| B | extract 产物重跑 byte-diff=0；applyRange 对拍 PoB2 luajit 期望值 ≥200 条全过；未知 format 告警清单为空或逐条登记 |
| C | 新旧分类**双跑 diff 报表**干净（每条差异附 Item.lua 行号依据）后才删旧文本枚举；元素 adds 法术泄漏专项断言；`mod_db_bench` 无回归 |
| D | 字段计数断言（293/78/200）；搬迁 commit parity 逐值不变；ReplaceNode fixture；Grants Skill 52 条覆盖率报表 |
| E | 管线顺序单测；ScaleAddMod 取整对拍（oracle 抽样 ≥30 条）；**radius 珠宝 attribute 误计数专项回归**；环形档几何断言 |
| F | 双武器组 fixture（激活切换 → 输出变化、两组不同时生效）；无武器组 build 逐值不变 |

### 5.3 阶段整体验收（roadmap M5 验收门禁原文中归属 (c) 的部分）

> BuildRaw 往返等价 golden fixture（P16，编辑态无 parity 可依）；radius 珠宝 attribute 误计数专项回归；unsupported 词条率下降曲线纳入报表。

操作化：

1. `cargo test -p pobr-item --test build_raw_roundtrip` 进 CI 必跑集；
2. radius attribute 专项回归用例进 ninja_parity 套件旁挂；
3. unsupported 率报表：对 ninja 18-build 全语料统计 `ParseStatus::Unsupported` 行数，M5c 合并前后对比——variant 门控/range 具体化/半径词条/Grants Skill 四项应使该数**下降**，曲线数据落 `audits/` 或 CI 工件；
4. 含 range/variant/武器组的**新增 fixture build**（手工构造 + PoB2 实际导出各 ≥1）先建 baseline 后入门禁（"先建 baseline 后入门禁"纪律同 roadmap 召唤 build 条目）；
5. version-bump-drill 视角自查：本阶段新增的全部表（mod_scalability/catalysts/runes/uniques/local_mods/树字段）重生四步可重复、Rust 零改动（P18 抽查，不要求跑完整 drill——那是 M6）。

---

## 6. 风险与回退（风险登记簿在本阶段的落点）

| ID | 风险 | 本阶段落点 | 缓解/回退 |
|---|---|---|---|
| **R9** | pobr-item 编辑态无 parity 依据 | A2/A3/A4 全部 | BuildRaw 往返契约双档（语义档硬门禁、字节档报表）；ItemDraft→calc 视图与 core 解析等价性测试把编辑态锚定在已有 parity 的 calc 路径上。回退：A track 整体可回退不影响 calc（pobr-item 无下游依赖） |
| **R2** | 重构破坏隐藏补偿 | C2/C3 局部切换（现行 phys-only 口径可能在 ninja 上"碰巧对"）；E2 缩放 | 双跑对照 + 行为/搬迁分 commit + baseline 独立审查。回退：legacy_ 前缀旧路径在 diff 报告干净前不删，出回归一键切回 |
| **R3** | extract-lua 抽取正确性 / vendor 漂移 | B1 四表（尤其 1.3M ModScalability）、D1 路线 B | luajit 执行而非正则；重跑 byte-diff CI；applyRange 以 PoB2 运行时输出对拍为正确性标准（"不以源码读得对为标准"）；产物头部记 vendor commit |
| **R6** | 数据体积/性能 | mod_scalability.json ~3-4MB、uniques.json | 懒加载（gamedata 按域）；applyRange 仅在含 `(min-max)` 行时查表；模板查找用 HashMap 预索引；`mod_db_bench` 无回归门禁 |
| **R7** | schema 演化 | Item REAL 类型扩字段、PassiveNodeDef 扩列、PassiveTreeSpec 扩 alloc_modes | 新字段全 default/Option；manifest 按域记 schema 版本；loader 容忍缺表/缺字段（旧 data 可读） |
| 合并冲突（三线并行） | 6 track 同期 | calc_orchestrator/xml_build/catalog mod.rs | WI-0 预拆分；§4.2 归属表 + 仲裁人；passive_inject 接力序 F→D→E；每表独立文件 |
| 范围蔓延 | flask/charm、副手 DPS、statdesc 渲染的诱惑 | §0.2 边界清单写入各 PR checklist；TODO(M5b)/TODO(M4) 注释明确归属 |

---

## 7. 实施前开放问题（开工前需裁决）

1. **local_mods.json / high_precision_mods.json 归属（总架构评审已裁决）**：M0 任务清单内但实查未落地（overlay 仅 skill_overrides.json）。裁决：`local_mods.json` → 本蓝图 Track C（WI-C1，M6 只消费）；`high_precision_mods.json` → **M4 W-A2**（本蓝图 Track E 只消费，见 §3.E2）。若 M0 收尾 agent 已在并行落表，开工时对一次 schema 避免双轨。
2. **GGG poe2-skilltree-export data.json 是否携带 isAttribute/options/isSwitchable/unlockConstraint**：审计与本蓝图均未能核实（源文件 gitignore 不在仓库）。WI-D1 决策梯第一步即下载核查；路线 A（base 层）与路线 B（vendor tree.lua → overlay）产物 schema 相同，消费侧不受影响，但层归属与 version-bump-drill 路径不同，需在 D1 PR 中记录裁决。
3. **"weapon_set 列"语义偏差**：roadmap/20 号文档 §3.1 把 weapon_set 列在 passive_tree.json 数据列；实际是 build 状态（PoB2 存 Spec XML）。本蓝图按 `PassiveTreeSpec.alloc_modes` 实现、树 JSON 不加列——需确认接受该偏差（或 roadmap 勘误）。
4. **mods.json tags 与 PoB2 modTags 对齐度**：催化剂 scalar 匹配依赖 mod 的 tags（getCatalystScalar 按 catalystTags ∩ mod.modTags）。GGG Mods.dat 的 Tags 列已入库但与 PoB2 modTags 的语义对齐未验证——B4 接线前需抽样 20 条对照，若不对齐则催化剂匹配需要补一张映射边车。
5. **Craft 词条文本渲染依赖**：WI-A4 的词缀重建在 statdesc（M5b）落地前只能近似渲染或保留 modId 引用。若 M5b 先于 A4 完成则直接消费其 rendered_lines；两个 track 的时序由总编排决定。
6. **ScaleAddMod 取整的 oracle 对拍通道**：E2 需要对 PoB2 运行时取整行为抽样对拍，依赖 pob2-oracle 可驱动 ModStore（现有 oracle 以 calc 输出对拍为主）——若驱动成本高，退化为对 vendor 源码语义的单测复刻 + 整数值用例规避精度分歧。
