# 物品系统

> 领域范围：Item.lua 解析语义 / ItemTools / Bases / Uniques / ModItem 等物品数据文件 / pobr-item 职责边界。
> 审计日期：2026-06-10。本文档基于逐条核查后的领域审计报告整理（核查说明见附录）。

## PoB2 代码结构

PoB2 物品系统 = **1 个大类 + 1 个工具模块 + 一组纯数据文件**。

```
vendor/PathOfBuilding-PoE2/src/
├── Classes/Item.lua            (2123 行) 物品全生命周期：解析/装配/局部结算/Craft/序列化
├── Modules/ItemTools.lua       (375 行)  applyRange：range 词条取值 + ModScalability 消费
├── Modules/Data.lua            :619-629 itemMods 组装；:1053-1060 uniques 加载；:657 itemTagSpecial（手工数据）
└── Data/
    ├── Bases/*.lua             (28 文件, ~640K) itemBases：基底属性
    ├── Uniques/*.lua           (28 文件 + Special/, ~130K) raw 文本块数组（含 Variant/Source/League）
    ├── ModItem.lua             (1M)   词缀库 itemMods
    ├── ModItemExclusive.lua    (1.8M) 专属词缀
    ├── ModCorrupted/ModJewel/ModFlask/ModCharm/ModVeiled (~430K)
    ├── ModRunes.lua            (165K) 符文/魂核 × 槽类型 → 词条+rank
    └── ModScalability.lua      (1.3M) 词条文本 → 每数值槽缩放性/格式
```

### Classes/Item.lua 关键函数

| 函数 | 行号 | 职责 |
|---|---|---|
| `ParseRaw` | 294-1253 | raw 文本 → 结构化 Item。状态机（FINDIMPLICIT/IMPLICIT/EXPLICIT）区分 GAME/WIKI 模式；头部元数据（Rarity/名称/基底 760-801、Item Level、Quality、催化剂品质 `Quality (X Modifiers)` 524-530、Sockets、`Rune:` 行 544-555、Jewel Radius/Limited、Variant/Selected Variant + 5 个 Alt Variant 571-636、Prefix/Suffix crafted 词缀引用 643-656、Catalyst 676-683）；状态行（Corrupted/Twice Corrupted/Mirrored/Sanctified/Desecrated 409-420）；词条行剥离 `{variant:N}`/`{tags:}`/`{range:x}`/`{corruptedRange:x}` 等标注（708-734），按 rune→enchant→implicit→explicit 分桶（958-969）；高级复制格式反查词缀库（428-482、859-896）；rune 组合逆推（1011-1155）；affixLimit（1178-1219）；`NormaliseQuality`（1255-1263） |
| `BuildModList` | 1936-2124 | 词条 modList 装配。`CheckModLineVariant`（1615-1623）按 variant 门控每行；range 词条经 `itemLib.applyRange` 重算（1973-1989）；`ExtraSkill` → grantedSkills（2017-2029）+ 按 gem 等级反算需求（2049-2068）；属性需求转换（2082-2100）；按槽位生成 slotModList[1/2] 或 modList（2112-2122）；socketedJewelEffect / socketedSoulCoreEffect 系数（2111、2123） |
| `BuildModListForSlotNum` + `calcLocal` | 1685-1933 / 1655-1682 | **局部 mod 结算核心**。calcLocal 把"无 tag、flag 精确匹配"的 mod 移除并就地结算。武器：局部物理 inc×quality、LocalElementalDamage、五系 Min/Max adds 入 weaponData（1745-1771）、局部暴击（1772）、WeaponRange/ReloadTime（1739-1743）、Accuracy/LifeOnHit/Leech 转 per-hand Condition（1776-1786）；护甲：armour/evasion/ES/ward + 三种混合双属性 flat+inc+quality 公式（1794-1823）、PerLevel 变体、BlockChance（1825-1827）、MovementPenalty → MovementSpeed 负 mod（1828-1830）；flask/charm（1834-1882）；jewel/clusterJewel/Grand Spectrum（1883-1931）。`{SlotName}`/`{Hand}`/`{OtherSlotNum}` 标签替换实现双手/双持槽位语义（1701-1707） |
| `Craft` | 1552-1613 | 由 prefixes/suffixes（modId+range）从词缀库重建 explicitModLines，statOrder 同序合并，催化剂 scalar 入 applyRange |
| `GetModSpawnWeight` | 1265-1282 | base.tags × weightKey/weightVal 判词缀可生成性 |
| `BuildRaw` | 1284-1483 | 逆向序列化为 raw 文本（Build Code XML 内嵌格式即此） |
| `getCatalystScalar` | 33-58 | 催化剂 id × mod.modTags 匹配 → (100+quality)/100 |

### Modules/ItemTools.lua

`itemLib.applyRange`（77-326）：把 `(min-max)` range 词条按 range∈[0,1] 取值，查 **data.modScalability**（精确到每个数值槽的 isScalable + 30 余种 divide_by_X/per_minute 格式）做精度/单位换算，再叠 catalyst scalar 与 corruptedRange；找不到 scalability 数据时回退 highPrecisionMods 旧路径。

### 数据文件 schema 要点

- `Data/Bases/*.lua`：`itemBases[名称] = {type/subType/quality(上限)/socketLimit/spirit/charmLimit/tags/implicit(文本)/weapon{PhysicalMin/Max,CritChanceBase,AttackRateBase,Range,ReloadTimeBase}/armour{Armour,Evasion,EnergyShield,Ward,BlockChance,MovementPenalty}/flask/charm/req}`
- `Data/Uniques/*.lua`：unique = raw 文本块数组（含 Variant/Source/League 行），运行时走同一 ParseRaw
- `Data/ModItem.lua` 等：`itemMods[域][modId] = {type=Prefix/Suffix, affix=显示名, 文本行..., statOrder, level, group, weightKey/weightVal, modTags, tradeHashes}`
- `Data/ModRunes.lua`：按槽类型（weapon/armour/caster/具体类型）给出符文/魂核词条+rank
- `Data/ModScalability.lua`：词条文本 → 每数值槽缩放性/格式

**数据流**：raw 文本（剪贴板/XML/Uniques DB）→ ParseRaw → modLines（分桶+标注）→ BuildModList（variant 门控 + applyRange + 局部 calcLocal）→ slotModList/weaponData/armourData/flaskData/jewelData → CalcSetup 装配进角色 modDB。

## pobr 实现现状

pobr 侧物品链路分散在三处，覆盖"**游戏导出的具体数值物品**"这一子集；PoB2 自建/合成/范围词条物品基本未覆盖：

1. **`crates/pobr-core/src/item_text.rs`**（约 550 行）：raw 文本 → `pobr_data::Item`。支持剪贴板格式（`--------` 分段）与 PoB XML 内嵌格式两套入口（`parse_item_text` / `parse_pob_xml_item`）；解析 Rarity/基底名/Quality/`Implicits: N` 计数/`Armour:` 等 rolled 防御行；`strip_pob_annotations` 剥离 `{key:value}`、`(augmented)`、`(tier:N)`、`[..]` 标注（对照 Item.lua:708-734）；`{crafted}`/`{enchant}`/`{rune}` 前缀行归 enchant 段。**只做分段切分**，所有 `{variant:}`/`{range:}`/`{tags:}` 标注一律剥离丢弃，无 variant 门控、无 range 取值。

2. **`crates/pobr-core/src/item.rs`**（151 行）：`ingest_item` 把三段词条逐行喂 `parse_mod`，产出带 `SourceId(item.<slot>.<section>)` 归因的 Modifier；品质刻意不在此建模（模块文档详细记录了 PoB2 逐属性 base 缩放语义，正确）。另外 `mod_parser.rs:72-97` 已实现 PoB2 的 `Bonded:` 前缀语义（递归解析 + 挂 `Condition:CanUseBondedModifiers` tag）与激活源 flag。

3. **`crates/pobr-build/src/calc_orchestrator.rs`**：局部 mod 的"穷人版 calcLocal"——`weapon_contribution`（约 1102-1146）只算主手：基底物理×(1+quality)×局部 phys inc + 局部 adds phys、局部攻速、基底暴击注入；`is_weapon_local_mod`（1339-1344）仅识别 3 种局部词条文本并仅对 Weapon1 从全局剔除（add_item 路径 382-392）；护甲件 `item_rolled_defence`（842 起）优先用 rolled 行，否则基底+局部 flat/inc×quality（含 PerLevel，对照 Item.lua:1817-1822 公式正确）。

4. **数据**：`data/4.5.0.3.4/base_items.json`（schema = `catalog.rs::BaseItemDef`）含 weapon{physical_min/max,speed_ms,crit_chance,range} 与 armour{armour,evasion,energy_shield,ward}；`mods.json` 含 stats(stat_id,min,max)/tags/generation_type/level/name(词缀显示名)。**无** uniques、runes、mod scalability、flask/charm 基底块、statOrder/spawn weight/渲染后英文词条文本。

5. **`crates/pobr-item`**：纯占位（lib.rs 仅 650B 文档），职责边界悬而未决。

ninja_parity 18 build（游戏导出、数值已具体化、无 range/variant/catalyst 行）下该子集够用——这解释了为何防御 51% parity 可达；但任何 PoB2 内手搓 build（unique DB 选件、Craft、range 词条）会大面积失真。

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|---|---|---|---|---|---|
| 1 | variant 词条无门控：多 variant unique 的所有变体词条全部注入 | 🔴 high | incorrect | Item.lua:1615 CheckModLineVariant + 1965-1966；ParseRaw:571-636 | item_text.rs:248-267 + strip_pob_annotations（423 起） | Variant 行当元数据丢弃、{variant:N} 连语义剥除，互斥词条全部叠加 |
| 2 | range 词条取值（applyRange + ModScalability）完全缺失 | 🔴 high | missing | ItemTools.lua:77-326；Item.lua:1969-1989；Data/ModScalability.lua (1.3M) | 无（全 workspace grep 零命中） | `{range:0.5}+(40-50)...` 类词条静默丢失 |
| 3 | 武器局部 mod 覆盖不全：元素 adds / 局部暴击 / LocalElementalDamage 泄漏为全局 | 🔴 high | partial | Item.lua:1745-1786 | calc_orchestrator.rs:1339-1344 + 1102-1146；mod_parser.rs:377-398 | 仅识别物理 inc/攻速/物理 adds 三种局部词条 |
| 4 | BaseItemDef schema 缺关键字段：spirit/socketLimit/quality 上限/BlockChance/MovementPenalty/ReloadTimeBase/subType/flask/charm | 🔴 high | partial | Data/Bases/*（sceptre.lua:8、shield.lua:14、crossbow.lua:11、body.lua:8-9）；Item.lua:1724-1830 | catalog.rs:33-88 | 盾基底格挡/权杖 Spirit/弩装填等纯数据缺口 |
| 5 | 双持/副手 weaponData 与 per-hand 槽位语义缺失 | 🟡 medium | missing | Item.lua:1685-1712 + 2112-2122 | calc_orchestrator.rs:382-392 + 1102-1146 | 局部剔除仅 Weapon1，无槽 2 模型 |
| 6 | 符文数据表（ModRunes）/UpdateRunes/SocketedSoulCoreEffect/rune 等级需求缺失（Bonded 词条语义已实现） | 🟡 medium | partial | Data/ModRunes.lua；Item.lua:544-555、1011-1155、1491-1549、2123 | item_text.rs strip_enchant_marker；mod_parser.rs:72-97（Bonded 已实现） | 依赖 XML 已展开的 {rune} 行兜底，无重算能力 |
| 7 | 催化剂链路断裂且 catalystQuality 行被当词条噪声 | 🟡 medium | missing | Item.lua:33-58、524-530、676-683、1977 | item_text.rs quality_from_line / is_metadata_line；tests/item_source.rs:299-314 defer 说明 | 'Quality (Defence Modifiers):' 穿透前缀匹配落入 Unsupported |
| 8 | 物品授予技能（Grants Skill / ExtraSkill → grantedSkills）未接入 | 🟡 medium | missing | Item.lua:2017-2068；Data/Bases/sceptre.lua、shield.lua | 无（grep 零命中，已复核） | 权杖召唤/盾系 build 技能入口断链 |
| 9 | Uniques 数据库未入库（data/ 无 uniques.json） | 🟡 medium | missing | Data/Uniques/*.lua；Data.lua:1053-1060 | 无 | 不影响 XML 导入，但 unique 选件/variant 补全/range 区间全依赖它 |
| 10 | 词缀库缺 crafting 维度：statOrder/group/spawn weight/渲染英文词条文本未导出（affix 显示名已有） | 🟡 medium | missing | Data/ModItem.lua；Item.lua:1552-1613、1265-1282、1178-1219、643-656 | catalog.rs:125-146 ModDef；item_text.rs is_xml_metadata_line | Craft/GetModSpawnWeight 无对应物 |
| 11 | corrupted/mirrored/sanctified/fractured/desecrated 状态未建模 | 🟢 low | missing | Item.lua:409-420、720-721、1255-1263 | pobr-data/src/item.rs Item struct；item_text.rs is_metadata_line | corruptedRange 缩放丢失，归因粒度受损 |
| 12 | flask/charm/jewel 物品类别未建模 | 🟡 medium | missing | Item.lua:1834-1931、828-843、556-570 | pobr-data/src/item.rs EquipmentSlot（仅 10 槽） | charm 常驻 buff/药剂参数/珠宝 Radius 均缺 |
| 13 | 物品属性/等级需求计算缺失（含转换 keystone） | 🟢 low | missing | Item.lua:2039-2100 | 无 | 不进 DPS/EHP，属规划器框架补全项 |
| 14 | 护甲局部词条小缺口：混合 flat 与 shield 局部格挡 inc | 🟢 low | partial | Item.lua:1795-1800、1825-1827 | calc_orchestrator.rs:1383-1395 / 1348-1366 | flat 侧不处理 'and'，盾局部格挡 inc 无人消费 |
| 15 | pobr-item crate 始终空置，物品编辑态职责无归属 | 🟡 medium | design | Classes/Item.lua 全文件 | crates/pobr-item/src/lib.rs（650B 占位） | 编辑态（Craft/BuildRaw/variant/range 选择）职责待落位 |

合计：🔴 high ×4，🟡 medium ×8，🟢 low ×3。

## 缺口详述

### 1. variant 词条无门控（🔴 high / incorrect）

【已核实】PoB2 每条词条可带 `{variant:N}` 标记，BuildModList 时只保留与 Selected Variant（含 Alt Variant 1-5）匹配的行（`CheckModLineVariant` 逐条门控，Item.lua:1615 + 1965-1966，代码与描述一致）。pobr 把 `Variant:`/`Selected Variant:` 行当元数据忽略（item_text.rs:248-267 `is_xml_metadata_line`）、把词条上的 `{variant:N}` 前缀直接剥除（`strip_pob_annotations`），结果一件多 variant unique 的全部互斥词条都会进 ModDb 重复叠加。

PoE2 Uniques DB 中 variant 物品真实存在且不少：Data/Uniques/body.lua 含 **124 处** `Variant:` 行（含 Atziri's Splendour、Blackbraid），helmet.lua 63 处、shield.lua 52 处。当前 ninja 样本（游戏导入、词条已具体化）恰好不触发所以 parity 未暴露；任何从 PoB2 unique DB 选件或含历史 variant 的导入 build 都会**数值爆炸**。

**修复方向**：item_text 保留每行的 variant 标注（line-level 元信息而非剥离）；Item 增加 `selected_variant` / `alt_variants` 字段；ingest 前做等价于 CheckModLineVariant 的门控过滤。

### 2. range 词条取值（applyRange + ModScalability）完全缺失（🔴 high / missing）

【已核实】PoB2 自建 build 中从 Uniques DB 添加的物品、crafted 物品的词条以 `{range:0.5}+(40-50) to maximum Life` 形式存在 XML 里，processModLine 检测到 `(min-max)` 模式即调 `applyRange` 按 range 比例取值（再叠催化剂 scalar / corruptedRange，按 modScalability 决定每个数值槽是否可缩放及精度/单位）。pobr 既不应用 range 也无 modScalability 数据（grep `applyRange`/`mod_scalability` 全 workspace 零命中；data/4.5.0.3.4/ 无对应文件），这类词条剥掉 `{range:0.5}` 后若仍含 `(40-50)` 字面区间，mod_parser 无对应解析路径 → Unsupported **静默丢失**——对非游戏导入的 build 是系统性数值缺失。

**修复方向**：这是"数据应 JSON 化"最典型的缺口——ModScalability 是纯数据（应转为 `mod_scalability.json`），applyRange 是少量逻辑（Rust 实现取值/精度算法）。无数据时可先实现"无 scalability 表的朴素线性取值"作为降级路径，至少不丢词条。

### 3. 武器局部 mod 覆盖不全（🔴 high / partial）

【已核实，表述微调】PoB2 中武器上的 `Adds N to M Fire/Cold/Lightning/Chaos Damage`、局部 `+N% to Critical Hit Chance` / `N% increased Critical Hit Chance`、局部 `N% increased Elemental Damage`（LocalElementalDamage）都经 calcLocal 入 weaponData（Item.lua:1745-1772），只对"用该武器的攻击"生效。pobr 只把物理 adds/inc 和攻速识别为局部（is_weapon_local_mod 仅 3 种文本），其余按全局词条 ingest，造成三类分叉：

1. 武器元素 adds 无 `to Attacks` 后缀 → mod_parser 不加 ATTACK flag（已核实 parse_added_damage_range:377-398 代码路径），作为无 flag 的 `<Type>DamageMin/Max` BASE 被 calc/damage.rs 对任意技能聚合——**会同时加成法术**（对纯攻击 build 主手伤害数值上近似，但跨技能/法术泄漏）；
2. 武器局部元素 inc 本应是武器基底独立乘区，pobr 落入全局加法 inc 桶；
3. 武器局部暴击与全局暴击混桶（weapon_contribution 只取 .dat 基底 crit，局部暴击词条不剔除）。

带元素伤害/局部暴击武器的 build DPS 口径与 PoB2 有**结构性分叉**。

**修复方向**：放弃文本枚举式 is_weapon_local_mod，改为 PoB2 的结构化判定（"mod name + flag 精确匹配且无 tag"即局部，见 calcLocal:1655-1682），局部候选名单数据化（见"数据 vs 逻辑切分"末节）。

### 4. BaseItemDef schema 缺关键字段（🔴 high / partial）

【已核实】所列字段在 PoB2 Bases 数据与消费侧均确认存在：sceptre.lua:8 `spirit=100`（21 处）、shield.lua:14 `armour={BlockChance=26, MovementPenalty=0.03}`（194 处）、crossbow.lua:11 `weapon={ReloadTimeBase=0.8/0.85}`（33 处）、body.lua:8-9 `subType='Armour', quality=20`；pobr catalog.rs:33-88 均无。直接计算后果：

- 盾牌基底格挡率（20-26%）完全不入 calc（`Block:` rolled 行也被 is_metadata_line 当元数据丢弃）；
- 权杖基底 100 Spirit 缺失（calc_orchestrator.rs:556 的 Spirit 只聚合词条 BASE）；
- 弩的装填时间不参与攻速（PoB2 ReloadTime 参与 weaponData）；
- 护甲移动惩罚 → MovementSpeed 负 mod 缺失；
- quality 上限 / NormaliseQuality 无法实现；
- subType 缺失导致 buckler/warstaff 等符文槽类型判定无依据。

**修复方向**：这些都是 .dat 已有的纯数据，属 **pipeline/adapter 一轮补列即可消除**的缺口（见切分建议第 1 条），随后在 orchestrator/defence 消费侧接线。

### 5. 双持/副手 weaponData 与 per-hand 槽位语义缺失（🟡 medium / missing）

【已核实】PoB2 每件武器为槽 1/2 各生成一份 modList（BuildModListForSlotNum，`{SlotName}`/`{Hand}`/`{OtherSlotNum}` 标签替换），SlotNumber/InSlot tag 过滤 + `Condition:MainHandAttack/OffHandAttack` 条件化，支撑双持/副手切换计算。pobr 的 add_item 路径中局部词条剔除明确只对 Weapon1 生效（代码有显式 `slot == EquipmentSlot::Weapon1` 分支），Weapon2 物品的局部词条不剔除、按全局注入；副手武器攻击/双持交替无模型。对当前以单武器 build 为主的 parity 影响有限，但属于**框架级缺位**。

**修复方向**：weapon_contribution 泛化为 per-hand；Modifier tag 体系补 SlotNumber/InSlot 等价物；Weapon2 局部剔除对齐。

### 6. ModRunes 数据表 / UpdateRunes / SocketedSoulCoreEffect / rune 等级需求缺失（🟡 medium / partial）

【部分修正】原报告称"Bonded: 魂核绑定词条无建模"**不实**——pobr 的 mod_parser.rs:72-97 已按 PoB2 ModParser 语义处理 `Bonded:` 前缀（递归解析剩余文本并挂 CanUseBondedModifiers 条件，激活源 flag 由编排层 calc_orchestrator.rs:574 接线），有注释明确对照 PoB2。仍然成立的子缺口：

1. 无 ModRunes 数据表 → 无法做"改符文重算"（UpdateRunes 语义，Item.lua:1491-1549），当前完全依赖 XML 里已展开的 `{rune}` 词条行兜底；
2. `SocketedSoulCoreEffect` 跨槽魂核加成系数（Item.lua:2123）无建模；
3. rune 等级对物品需求等级的抬升（Item.lua:546-555 据 ModRunes rank 反查）缺失；
4. 符文组合逆推（Item.lua:1011-1155）缺失。

**修复方向**：ModRunes 是纯数据，应入 JSON（`rune_id → {slot_type → [词条文本], rank}`），随后实现 UpdateRunes 等价逻辑。

### 7. 催化剂链路断裂（🟡 medium / missing）

【已核实】审计 02-05 已 defer 催化剂，但有两个补充事实：

1. 游戏导出剪贴板格式的 `Quality (Defence Modifiers): +20% (augmented)` 行既不被 quality_from_line 识别也不在 metadata 前缀表里（两处均按 `Quality:` 精确前缀匹配，带括号变体落空），会落进 explicit 词条段成为 **Unsupported 噪声**；
2. 实现前置依赖是 mods.json 携带 modTags（GGG tags 已有但需确认与 PoB2 modTags 对齐）+ Item 增加 catalyst/catalyst_quality 字段 + applyRange 的 scalar 通道——与 pobr 自己测试文件（tests/item_source.rs:299-314）里记录的 defer 依赖清单一致。

首饰催化剂影响 jewellery 词条数值最高 20%。

**修复方向**：先修 `Quality (X Modifiers)` 行的识别（消除噪声，正则参照 Item.lua:524-530 `Quality %(%a+ Modifiers%)`）；完整实现随 applyRange/scalar 通道一并落地。

### 8. 物品授予技能未接入（🟡 medium / missing）

【已核实】PoE2 权杖的召唤系技能、盾牌的举盾、大量 unique 的附加技能都走 `Grants Skill:` implicit → ExtraSkill mod → grantedSkills（Item.lua:2017-2029）→ 进入技能列表参与计算。pobr 完全没有这条链路（grep -i 'grants skill|granted_skill|ExtraSkill|extra_skill' 于 crates/apps/tools 全部 .rs 零命中，已复核）：sceptre 召唤 build、盾系 build 的核心技能来源缺位（召唤计算本身属另一域，但**物品侧的入口在这里断了**）。BaseItemDef.implicits 仅存 mod 稳定 ID，implicit 渲染文本（含 Grants Skill 行）未入库。

**修复方向**：mod_parser 识别 ExtraSkill 类词条 → Item/ingest 产出 granted_skills 列表 → 与 skill_source.rs::ingest_gem 链路对接；数据侧需 implicit 渲染文本或结构化 granted_skill 字段入库。

### 9. Uniques 数据库未入库（🟡 medium / missing）

【已核实】解析 XML 内嵌物品文本时不需要 uniques DB（文本自带词条），所以 ninja parity 不受影响；但"在 pobr 内新建/搜索/对比 unique、补全 variant 选项、unique 词条 range 区间"都依赖它。PoB2 的 Uniques 文件本质是"带模板语法的 raw 文本数组"。

**修复方向**：JSON 化建议保留原始文本块（PoB2 兼容要求）+ 预解析出 base/variants/词条行结构两层。注意它**部分是手工维护**（League/Source/Upgrade 注记），不是纯 .dat 可再生数据，需要独立的同步管线（从 PoB2 仓库抽取而非 GGG dat，参照 sync-pob-catalog 模式）。

### 10. 词缀库缺 crafting 维度（🟡 medium / missing）

【部分修正】原报告称 mods.json"无 affix 显示名"**不实**——catalog.rs ModDef 已有 `name: Option<String>`（英文 canonical 词缀名，如 'of the Brute'，i18n 走边车）。仍然成立的缺口：PoB2 crafted 物品在 XML 中同时存 `Prefix:`/`Suffix:`（modId+range）与展开后的词条行，pobr 靠后者兜底可算数值；但物品编辑（换词缀、查可用词缀池、tier 列表）、高级复制格式（`{ Prefix Modifier "..." }`）反查、statOrder 同序合并都不可能。mods.json 离 PoB2 itemMods 还差两块：

- "从 stat_id + StatDescriptions 渲染英文词条文本"——这是 pipeline 的 stat 描述渲染工作，**PoB2 导出器已做、pobr-data-adapter 未做的最大单项**；
- "spawn weight（weightKey/weightVal）、group、statOrder 列族"——.dat 既有列，补导出即可。

**修复方向**：见切分建议第 3 条；逻辑侧 Craft/GetModSpawnWeight 等价物归 pobr-item（见缺口 15）。

### 12. flask / charm / jewel 物品类别未建模（🟡 medium / missing）

【已核实】pobr EquipmentSlot 仅 Weapon1/2 + Helmet/Body/Gloves/Boots/Amulet/Ring1/2/Belt 共 10 槽，无 Flask/Charm/Jewel。PoE2 charm（护符）常驻触发增益、药剂恢复参数（duration/charges/recovery/effectInc，Item.lua:1834-1882）、珠宝 socket（pobr-tree 有范围珠宝 first pass 但 item 侧无 Radius/Limited 解析）都缺。charm 的 buff 词条（base.charm.buff）在 PoB2 直接从 Bases 数据生成 buffModLines 入 modDB——这又要求 BaseItemDef 携带 flask/charm 块（与缺口 4 联动）。对 EHP/恢复类 parity（防御侧剩余偏差）**可能是隐性贡献项**。

**修复方向**：EquipmentSlot 扩展 + BaseItemDef 补 flask/charm 块 + flaskData/charmData 等价结算；珠宝侧补 Radius/Limited 解析并与 pobr-tree 对接。

### 15. pobr-item crate 空置，编辑态职责无归属（🟡 medium / design）

PoB2 的 Item.lua 把解析/编辑/Craft/BuildRaw 全装在一个类里；pobr-item 自述与 pobr-core::item_text 边界未厘清，至今 650B 占位。建议边界：

- **pobr-core 保留**"文本 → Item → Modifier"的只读解析链（item_text + ingest_item，calc 依赖）；
- **pobr-item 承接** PoB2 Item.lua 的"编辑态"半边——CustomItem 草稿（prefixes/suffixes/range/variant/catalyst/rune 选择）、`Craft()` 等价的词缀重建、`BuildRaw()` 等价的序列化（保证与 PoB2 XML 往返兼容）、GetModSpawnWeight 词缀池查询；
- **applyRange**（依赖 mod_scalability JSON）解析与编辑两侧共用，可下沉 pobr-core 或独立小模块。

这样 calc 路径零新依赖，编辑功能不污染核心。

## 数据 vs 逻辑切分建议

### PoB2 在该领域的数据/逻辑混合现状

物品域是 PoB2 "数据即代码"最重的区域之一：src/Data/ 38MB 中物品相关 **≈ 6MB+**（ModItem 1M、ModItemExclusive 1.8M、ModScalability 1.3M、ModRunes 165K、ModCorrupted/ModVeiled/ModJewel/ModFlask/ModCharm 共 ~430K、Bases/ ~640K、Uniques/ ~130K），全部是 `-- automatically generated` 的 Lua 表或 raw 文本数组——**本质 100% 是数据**。混入逻辑的地方有三类：

1. Modules/Data.lua:619-629 的表组装与 633-642 的 Time-Lost jewel 词条文本改写（轻逻辑）；
2. Data.lua:657 起的 itemTagSpecial/itemTagSpecialExclusionPattern 是**手工维护**的数据（由 Item.lua getTagBasedModifiers 这个开发期工具半自动生成）；
3. Item.lua/ItemTools.lua 本体（~2500 行）是真逻辑，但其中 catalystList/catalystTags（Item.lua:14-29）、applyRange 的 30 余种 format 枚举映射（ItemTools.lua:172-260）、Tabula Rasa 等 hardcode hack（Item.lua:2077-2081）、Two-Toned Boots hack（587-598、786-800）其实是**嵌在逻辑里的数据**。

### 应 JSON 化的清单（按优先级）

| 优先级 | 数据文件 | 目标 schema 建议 | 来源 |
|---|---|---|---|
| 高 | **base_items.json 补列** | 增列 `sub_type`、`quality_max`、`socket_limit`、`spirit`、`charm_limit`、`implicit_text`（渲染文本或解析后 stat 行）、`weapon.reload_time_ms`、`armour.block_chance`、`armour.movement_penalty`、`flask{life,mana,duration,charges_max,charges_used,buff}`、`charm{duration,charges,buff}` | 纯 .dat 再生（pipeline 补列） |
| 高 | **mod_scalability.json** | 词条模板（#化文本）→ 每数值槽 `{is_scalable, format}` | 可从 PoB2 Data/ModScalability.lua 直接转换（其本身由 GGG StatDescriptions 生成） |
| 中 | **mods.json 增列** | 增列 `stat_order`、`group`、`weight_keys/weight_vals`、渲染后英文词条文本行（affix 显示名 `name` 已有） | weight/group/statOrder 是 .dat 既有列；词条文本需 pipeline 做 StatDescriptions 渲染（**最大单项**） |
| 中 | **runes.json** | `rune_id → {slot_class → [词条文本/stat 行], rank, is_soul_core}` | 对应 ModRunes.lua |
| 中 | **uniques.json** | 双层结构：原始 raw 文本块（保证 BuildRaw 往返/对比兼容）+ 预解析索引（base、variants、词条模板行） | **非 .dat 可全自动再生**（League/Source/variant 历史是社区手工维护），建议作 vendored 数据由 sync-pob-catalog 类工具从 PoB2 仓库抽取，而非 pipeline |
| 低 | **小数据表** | `catalysts.json`（id/名称/tags 矩阵，Item.lua:14-29）、itemTagSpecial/ExclusionPattern（手工数据 → JSON 并标注维护来源）、jewel radius 表、unique hardcode 特例表（Tabula Rasa 等 → 数据化为 per-unique override 而非代码分支） | Lua 内嵌数据抽取 |

### 应留在框架（Rust 逻辑）的部分

ParseRaw 状态机与分桶、CheckModLineVariant 门控、applyRange 的取值/精度算法（消费 mod_scalability 数据）、calcLocal 局部结算语义（武器/护甲/flask 公式）、Craft 词缀重建、BuildRaw 序列化、NormaliseQuality、GetModSpawnWeight 的 tags×weight 匹配算法、属性需求公式。这些在 PoB2 里已经基本是数据驱动的纯逻辑，Rust 化时无需夹带数据。

### pobr 当前混合点的自查

pobr 自身也有两处把数据写进了逻辑：calc_orchestrator.rs 的 `is_weapon_local_mod` / `parse_local_defence_*` 用**硬编码英文文本后缀**判定局部性——PoB2 的对应判定其实是"mod name + flag 精确匹配且无 tag"（calcLocal，Item.lua:1655-1682），是基于解析后 mod 结构的通用规则而非文本枚举。

**建议演进方向**：在 mod_parser 输出上标注 local-candidate（按 ModName 白名单，这个白名单本身可以是数据表 `local_mods.json`），让局部结算走结构化路径；否则每补一种局部词条都要改 orchestrator 代码，违背"框架稳定、数据迭代"的目标。

## 附录：核查说明

核查范围：全部 4 条 high（variant 门控、applyRange/ModScalability、武器局部 mod、BaseItemDef schema）+ 6 条可疑 medium/low（符文/Bonded、催化剂、Grants Skill、词缀库、双持、flask/charm/状态/局部 flat），逐条打开 PoB2 Lua 与 pobr Rust 源码比对，并对 pobr 侧用 grep 全局搜索（crates/apps/tools）防止"在别处实现"误判。

**查实成立、保留**：
1. variant 门控：Item.lua:1615 CheckModLineVariant + 1965 processModLine 与描述一致；pobr item_text.rs:248-267 把 Variant 行当元数据、strip_pob_annotations 剥 {variant:N}，全局 grep 'variant' 无任何门控实现；并实测 PoE2 Uniques DB 中 body.lua 有 124 处 Variant: 行、Atziri's Splendour/Blackbraid 均真实存在——条目成立且例证有效。
2. applyRange：ItemTools.lua:77-326 与 ModScalability.lua(1.3M) 属实；grep applyRange/mod_scalability 全 workspace 零命中，data/4.5.0.3.4/ 目录无对应 JSON。
3. 武器局部 mod：is_weapon_local_mod(1339-1344) 确实只识别 3 种文本；mod_parser.rs:377-398 确认无后缀的元素 adds 不带 ATTACK flag，calc/damage.rs 按 <Type>DamageMin/Max 无差别聚合 → 法术泄漏断言成立；对 detail 做了一处精化：主手攻击自身的 flat 元素 adds 数值上与 PoB2 近似（两边都进 base 桶），结构性分叉主要在泄漏/局部 inc 乘区/局部暴击三点。
4. BaseItemDef schema：catalog.rs:33-88 实读确认所列字段全缺；PoB2 侧 sceptre spirit=100、shield BlockChance=26+MovementPenalty、crossbow ReloadTimeBase、body subType/quality 全部 grep 实证；'Block:' 在 is_metadata_line 前缀表、orchestrator:556 Spirit 仅来自词条 base_sum 均属实。

**修正 2 条**：
- (a) 符文条目：原称"Bonded: 魂核绑定词条无建模"——不实。mod_parser.rs:72-97 已完整实现 Bonded 前缀（递归解析 + Condition:CanUseBondedModifiers tag）与激活源 flag，calc_orchestrator.rs:574 有接线注释。已改写标题与 detail，保留仍成立的 ModRunes 数据表/UpdateRunes/SocketedSoulCoreEffect/rune 等级需求四个子缺口，severity 维持 medium。
- (b) 词缀库条目：原称 mods.json"无 affix 显示名"——不实，catalog.rs ModDef 有 `name: Option<String>`（英文词缀名）。已从标题/detail/切分建议第 3 条中移除该项，保留 statOrder/group/weight/渲染文本缺口，severity 维持 medium。

**顺带核实未改动**：催化剂条目的两个新断言均成立（'Quality (Defence Modifiers):' 确实穿透 quality_from_line 与 is_metadata_line 的 'Quality:' 精确前缀；Item.lua:524-530 正则属实，且与 pobr 自带 defer 测试注释一致）；Grants Skill 全局 grep 零命中属实；双持条目核实 add_item 局部剔除有显式 `slot == Weapon1` 分支；EquipmentSlot 10 槽无 Flask/Charm/Jewel、Item struct 无状态字段、parse_local_defence_flat 单类型 match 均逐行读码确认。无条目需要删除或降级 severity；所有修正为描述级（2 条 medium 的部分断言），4 条 high 全部维持原级。
