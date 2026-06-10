# 天赋树

> 领域：PassiveTree / PassiveSpec / 树数据管线
> 审计日期：2026-06-10
> 注：上一轮审计 `audits/pob2-parity-2026-06-09/FINDINGS.md` 完全未覆盖天赋树领域（其 6 个子系统为 mod 聚合/setup/perform/伤害/异常/防御），本报告与之无重复。

## PoB2 代码结构

PoB2 天赋树分四层（自下而上：数据 → 静态树 → Build 状态 → 计算接入）：

| 层 | 文件 | 体量 | 职责 |
|---|---|---|---|
| 数据层 | `src/TreeData/0_5/tree.lua` | 2.4MB，纯生成数据 | nodes / groups / constants / classes 全量树数据 |
| 数据生成 | `Export/Scripts/passivetree.lua` | 1323 行，离线脚本 | 从 .dat 表生成 tree.lua |
| 静态树层 | `Classes/PassiveTree.lua` | ~30KB | 加载 tree.lua + 运行时派生（索引/邻接/坐标/半径预计算/ProcessStats） |
| Build 状态层 | `Classes/PassiveSpec.lua` | 85KB | XML Load/Save、分配引擎、寻路、依赖重建、版本迁移、URL 编解码 |
| 计算接入层 | `Modules/CalcSetup.lua` | buildModListForNode/buildModListForNodeList（L126-325） | 节点 ModList → 计算环境（缩放/条件/珠宝/授予技能） |

### 数据层：`TreeData/0_5/tree.lua`

由 `Export/Scripts/passivetree.lua` 离线从 .dat 生成。nodes 以 skill id 为键，关键字段：

- 基础：stats / connections / group / orbit / orbitIndex、isKeystone / isNotable / isJewelSocket / isAscendancyStart
- **isAttribute**（293 处，已核实）+ options 三选一子节点 —— 属性小点
- **isSwitchable**（78 处，已核实）+ options —— 按职业/飞升改写节点
- **unlockConstraint**（200 处）—— 寻路解锁约束
- isMultipleChoice（20）/ isMultipleChoiceOption、isFreeAllocate（3）、applyToArmour（12，Smith of Kitava）、classesStart
- groups 坐标、constants（orbitRadii / skillsPerOrbit）、classes（含 integerId / ascendancies internalId）

生成规则（passivetree.lua，行号已核实）：

| .dat 来源 | 生成产物 | 位置 |
|---|---|---|
| passiveskills 行 → describeStats | 词条文本 stats | — |
| classpassiveskilloverrides / ascendancypassiveskilloverrides | isSwitchable.options（飞升替换表 ascedancyReplacements 来自 ascendancy.dat 的 Replace 列 L573） | L941-1040 |
| ConstraintNode | unlockConstraint | L808-822 |
| Attribute=true | isAttribute + 三选一 options | L873-885 |
| GrantedSkill | "Grants Skill: X" 词条 | L887-911 |
| PassivePointsGranted / WeaponPointsGranted / MultipleChoice / FreeAllocate / ApplyToArmour? | 对应标记 | L913-1064 |

### 静态树层：`Classes/PassiveTree.lua`

- 职业映射表 classNameMap / classIntegerIdMap / internalAscendNameMap（L88-116）
- 节点 type 归类与 keystoneMap / notableMap / sockets（L190-263）
- connections → linkedId 双向邻接（L282-322）
- orbit 坐标公式 `x = group.x + sin(angle) * orbitRadius`（ProcessNode L524-530）
- 每个 socket 与 keystone 的 nodesInRadius 预计算，按 `innerSquared <= d² <= outerSquared` **环形**判定（L346，已核实，排除 mastery/proxy）
- ClassStart 邻接节点注入 `Condition:ConnectedTo<Class>Start` FLAG（L383-391）
- ProcessStats：stats 文本逐行解析为 ModList（含 `\n` 拆行 + 解析失败时多行拼接重试，L421-498）；Keystone 额外造 keystoneMod LIST（L495-497）；isSwitchable / isAttribute 的 options 子节点也各自 ProcessStats（L546-566）

### Build 状态层：`Classes/PassiveSpec.lua`

- Init 复制树 + 版本迁移 convert（同 id 异名节点入 ignoredNodes，L45-67）
- Load/Save XML：nodes / masteryEffects / Sockets / WeaponSet1-2 / Overrides→AttributeOverride。`<WeaponSetN nodes>` 解析在 L137-146（已核实）——`nodes` 属性含**全部**已分配节点（含武器组点），WeaponSet 子元素只标记归属，ImportFromNodeList L355/366 据此设 `node.allocMode`
- ImportFromNodeList（mastery 校验、hashOverrides 先 ReplaceNode、weaponSets 给节点 allocMode，L319-373）
- GGG / poeplanner URL 编解码（L375-615）
- 职业选择 SelectClass / SelectAscendClass / SelectSecondaryAscendClass（L616-709）
- 分配引擎 AllocNode / DeallocNode / GetAllocationPath / GetEffectiveAllocationPath / CanPathThroughAllocMode（L824-828 已核实：普通 / 武器组1 / 武器组2 三种 allocMode 独立寻路图；多选一互斥；路径上属性点默认 Strength）
- 核心 **BuildAllDependsAndPaths**（L1220-1798）：
  - isSwitchable 按当前职业/飞升 ReplaceNode（L1251-1260，已核实）
  - hashOverrides 应用（L1337-1339）
  - conqueredBy 永恒珠宝节点改写（PoE2 大半注释，abyss 分支生效）
  - mastery 选择效果落到 node.sd（L1538-1561）
  - allocatedSmithBodyArmourNodeCount 统计（L1566-1568）
  - FindStartFromNode DFS 判连通 + depends 重建 + intuitive-leap 类珠宝豁免 + 孤儿剪枝
  - BuildPathFromNode BFS 重建 pathDist（L1061-1124，规则：不穿起始点、不跨飞升、不穿 mastery、unlockConstraint 须满足）
- SwitchAttributeNode 属性三选一 → hashOverrides（L2469-2480）
- cluster jewel 子图（PoE1 遗留，0_5 无 expansionJewel）

### 计算接入层：`Modules/CalcSetup.lua` buildModListForNode / buildModListForNodeList（L126-325，已核实）

1. keystone 走 keystoneMod 去重（L130-136）
2. radius 珠宝两轮 func（Other → Threshold/Self/SelfUnalloc）、Time-Lost 珠宝按 small/attribute/notable 三类缓存 ModList（L141-164）
3. PassiveSkillHasNoEffect / AllocatedPassiveSkillHasNoEffect 清空（L166-168）
4. PassiveSkillEffect 缩放（L170-177）
5. PassiveSkillHasOtherEffect → NodeModifier 替换（L186-191）
6. 树授予技能 ExtraSkill → node.grantedSkills（L193-202）
7. 武器组节点 mod 注入/改写 `Condition:WeaponSet1/2`（L208-228，已核实——普通节点上反向移除该条件）
8. JewelSmall/NotablePassiveSkillEffect 局部效果（L230-258 收集 + L260-274 应用，含 isAttribute 排除）与 SmallPassiveSkillEffect（Hulking Form）全局缩放（buildModListForNodeList L287-290 聚合）

配套词条语义：

- radius 珠宝词条在 `Modules/ModParser.lua` L6839-6905（已核实："increased Effect of Small/Notable Passive Skills in Radius"、"X Passive Skills in Radius also grant"——Small 明确要求 `node.type=="Normal" and not node.isAttribute`、"grant nothing" FLAG、Transformed → PassiveSkillHasOtherEffect 等）
- Time-Lost 腐化词条 "upgrades radius to medium/large" → timeLostJewelRadiusOverride（ModParser.lua:5481-5482，已核实）
- 半径表为版本化数据 `Modules/Data.lua` L597-613 `jewelRadii["0_1"]`（4 个标准 disc 档 + 8 个 inner>0 的 Variable 环形档，已核实）+ setJewelRadiiGlobally（L565-595，inner/outer 各乘 PassiveTreeJewelDistanceMultiplier 的平方）

## pobr 实现现状

pobr 侧分三处：

### crates/pobr-tree（4 个文件，约 16KB）

- `tree.rs`：PassiveTree = HashMap<skill, PassiveNodeDef> + 可注入坐标表，提供 node 查询 / nodes_in_radius（纯欧氏距离**圆盘**）
- `node.rs`：collect_allocated_mods——JewelSocket 节点 stats 整体 gating、Mastery 节点按 MasterySelection 文本注入否则跳过、stats 按 `\n` 拆行、属性小点 `+N to any Attribute` 按 AttributeChoice 文本改写为具体属性（对应 PoB2 SwitchAttributeNode 的文本侧等价实现，已核实）
- `radius_jewel.rs`：四档半径常量（outer×1.2，转录自 Data.lua jewelRadii["0_1"] 标准档）+ `JewelRadius::Custom(f64)` 自定义 outer + compute_radius_jewel_effect（**仅 outer 圆盘，无 inner 环**，已核实）

### 数据管线

- `tools/pobr-data-adapter/src/tree.rs`：消费 grindinggear/poe2-skilltree-export data.json，RawNode 只取 skill/id/name/stats/group/orbit/orbitIndex/out/ascendancyId + 五个 is*（Notable/Keystone/Mastery/JewelSocket/AscendancyStart）布尔（已核实，L58-87）→ passive_tree.json（isSwitchable/options 在产出 JSON 中 0 命中，已核实）+ passive_tree_meta.json（职业基础三维 + 飞升名）
- `tree_coords.rs`：从 vendor tree.lua 抽 groups/orbit 常量，按 PoB2 公式回填 x/y
- `PassiveNodeDef`（catalog.rs:385-423）仅含上述字段——无 isAttribute / options / isSwitchable / unlockConstraint / isMultipleChoice / isFreeAllocate / applyToArmour / classesStart（已核实）
- pipeline/config.json 未包含 passiveskills / classpassiveskilloverrides / ascendancypassiveskilloverrides 等 .dat 表（已核实，grep 零命中）

### 接入层

- `pobr-build/xml_build.rs`：解析 `<Spec nodes>`、`<Overrides><AttributeOverride strNodes/dexNodes/intNodes>`、`<Sockets><Socket>` 树珠宝与 `Radius:` 行；全仓 grep "WeaponSet" 仅命中 ItemSet 的 useSecondWeaponSet（已核实，无 `<WeaponSet1/2 nodes>` 解析）
- `pobr-data/passive_tree.rs::PassiveTreeSpec` 仅 allocated_nodes / mastery_effects / attribute_overrides 三字段，无 alloc_mode（已核实）
- `calc_orchestrator.rs`：resolve_passive_nodes → collect_allocated_mods → ingest_passive_nodes（按 ascendancy_id 区分归因 SourceKind）；radius_jewel_grant_texts（L1818-1890，已核实）把 "X Passive Skills in Radius also grant" 按半径内已分配 Notable/Normal 计数展开为 N 份全局词条——其中 PassiveNodeKind::Normal 一律计入 smalls，无 isAttribute 排除

### 覆盖度小结

已通：已分配节点词条收集 + source 归因 + 属性三选一 + 基础 "also grant" 半径珠宝。

缺失：分配/寻路/依赖引擎、节点效果缩放管线（全仓 grep PassiveSkillEffect / HasNoEffect / "Effect of Small" 零命中，已核实）、武器组节点条件、switchable 节点改写、树授予技能（grep "Grants Skill" Rust 侧零命中，而 data JSON 含 52 条，已核实）、环形/可变半径、版本迁移。

pobr 定位是"从 build code 导入后计算"，故 PoB2 的交互式分配引擎部分缺失属暂时可接受的范围裁剪，但其中数个缺失直接影响计算数值（见下）。

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|---|---|---|---|---|---|
| 1 | 武器组（WeaponSet1/2）天赋节点未解析、节点词条无 WeaponSet 条件 | 🔴 high | missing | PassiveSpec.lua:137-146 / 355,366 + CalcSetup.lua:208-228 + PassiveSpec.lua:824-828 | xml_build.rs（无解析）；PassiveTreeSpec 无 alloc_mode | 两套武器组天赋同时永久生效，恒高估 |
| 2 | 节点效果缩放管线整体缺失（PassiveSkillEffect / HasNoEffect / Jewel*PassiveSkillEffect / Time-Lost 主词条） | 🔴 high | missing | CalcSetup.lua:126-290 + ModParser.lua:6839-6854 / 6894-6903 | 无（grep 零命中） | Time-Lost 珠宝主词条被丢弃，半径内全部天赋数值偏低 |
| 3 | isSwitchable 节点按职业/飞升改写（ReplaceNode）完全缺失 | 🔴 high | missing | PassiveSpec.lua:1251-1260 + passivetree.lua:941-1040 / 573；tree.lua 78 处 | catalog/adapter/JSON/pipeline 四处皆缺 | 数据与逻辑两侧全无，受影响飞升节点词条算错 |
| 4 | radius 珠宝 also-grant 把属性小点错误计入 Small 计数 | 🟡 medium | incorrect | ModParser.lua:6855-6857 + passivetree.lua:873-885；tree.lua 293 处 isAttribute | calc_orchestrator.rs:1864-1874 + adapter tree.rs:58-87 | 每个属性小点多发一份授予词条，恒高估 |
| 5 | 环形（inner/outer）与可变半径档、"upgrades radius to" 覆盖未支持 | 🟡 medium | partial | Data.lua:597-613 / 586-587 + PassiveTree.lua:346 + ModParser.lua:5481-5482 | radius_jewel.rs（仅 outer 圆盘）+ parse_jewel_radius 回退 Large | 环形/Variable 档/升档珠宝节点集合算错 |
| 6 | 树节点授予技能（Grants Skill: X → ExtraSkill）未接入 | 🟡 medium | missing | CalcSetup.lua:193-202 + passivetree.lua:887-911 | 无（Rust 侧零命中；data JSON 含 52 条） | 飞升关键节点授予的技能完全缺席计算 |
| 7 | 分配/寻路/依赖引擎（AllocNode / BuildAllDependsAndPaths / BFS）整体未实现 | 🟡 medium | design | PassiveSpec.lua:1220-1798 / 1061-1124 / 1021-1047 / 830-913 / 915-987 | pobr-tree 仅 mod 收集与半径几何 | 路线图缺口而非 bug，PoBR 替换 PoB2 业务逻辑目标下需立项 |
| 8 | 树版本迁移与 treeVersion 多版本选择缺失 | 🟡 medium | missing | PassiveSpec.lua:36-67 / 1532-1536 / 319-324 / Load L161-176 | xml_build.rs（treeVersion 解析忽略）；data/ 仅单版本 | 旧版本 build code 静默按当前树解释，节点语义变更时算错无提示 |
| 9 | keystoneMod 机制缺失：物品授予基石与树上基石的去重通道不存在 | 🟢 low | missing | PassiveTree.lua:495-497 + CalcSetup.lua:130-136 | pobr-tree/node.rs（直接解析 stats） | 物品授予基石路径未实现时为 latent，将来双算风险 |
| 10 | masteryEffects XML 属性未解析（0.5 暂无实际 mastery 效果，latent） | 🟢 low | partial | PassiveSpec.lua:192-197 / 1538-1561；0_5 树 masteryEffects 出现 0 次 | xml_build.rs:21（注释列为未做切片）；node.rs 已备 gating | 当前零数值影响；注意 id vs 文本索引差异 |
| 11 | unlockConstraint / isMultipleChoice / isFreeAllocate / applyToArmour 未入 JSON schema | 🟢 low | missing | passivetree.lua:808-822 / 1049-1064 + PassiveSpec.lua:858-874 / 943-958 / 990-1016 / 1566-1568 | catalog.rs:385-423 无对应字段 | applyToArmour 是 Smith of Kitava 机制的计算输入，缺数据则无法实现 |
| 12 | ClassStart 邻接节点 Condition:ConnectedTo\<Class\>Start FLAG 未注入 | 🟢 low | missing | PassiveTree.lua:383-391 + PassiveSpec.lua:1820-1828 | 无 | 0.5 是否有消费者未确认，记录备查 |

## 缺口详述

### 1. 🔴 武器组（WeaponSet1/2）天赋节点未解析、节点词条无 WeaponSet 条件

【已核查成立】PoB2 的 Spec XML `nodes` 属性包含全部已分配节点（**含武器组点**，Load L188-191 全量入 hashList），`<WeaponSet1/2 nodes>` 子元素仅标记哪些属于武器组；计算时武器组节点的 mod 带 `Condition:WeaponSet1/2`，仅在对应武器组激活时生效（含珠宝 mod 按所在 socket 的 allocMode 附加条件，CalcSetup L224-228；普通节点上反向移除该条件）。寻路侧 CanPathThroughAllocMode（L824-828）维护普通/武器组1/武器组2 三套独立寻路图。

**影响**：pobr 把 nodes 全量当普通节点注入，等于两套武器组的天赋全部同时计入且无条件生效——对任何使用 0.5『武器组技能点』机制的 build 直接多算词条，方向恒为高估。

**修复方向**：
1. `xml_build.rs` 解析 `<WeaponSet1/2 nodes>` 子元素；
2. `PassiveTreeSpec` 增加节点 → alloc_mode（0/1/2）映射；
3. 词条注入时给 alloc_mode≠0 的节点 mod 附加 `Condition:WeaponSet1/2`，并与既有的 useSecondWeaponSet 配置联动决定激活组。

### 2. 🔴 节点效果缩放管线整体缺失

【已核查成立】PoE2 Time-Lost 珠宝的**主词条**就是『半径内小天赋/大天赋效果提高 X%』，pobr 目前只实现了次要的 also-grant 行，主词条被 mod_parser 归 Unsupported 丢弃；Hulking Form 等 SmallPassiveSkillEffect 全局缩放、『半径内大天赋不提供任何加成』（grant nothing → PassiveSkillHasNoEffect 清空整个节点 ModList）同样缺失。

**影响**：效果缩放是乘在整个节点 ModList 上的（ScaleAddList），带 Time-Lost 珠宝的 build 半径内全部天赋数值偏低，影响面大。

**修复方向**：在 pobr 的节点词条收集与注入之间加一层"节点级效果管线"，按 PoB2 顺序实现：HasNoEffect 清空 → PassiveSkillEffect 缩放 → 二轮珠宝 func → HasOtherEffect 替换 → 局部 Jewel*Effect 缩放（含 isAttribute/ascendancy 排除）→ 全局 SmallPassiveSkillEffect 聚合；mod_parser 需先支持 "increased Effect of Small/Notable Passive Skills in Radius" 等模式。

### 3. 🔴 isSwitchable 节点按职业/飞升改写（ReplaceNode）完全缺失

【已核查成立】0.5 树上 78 个节点对特定职业/飞升会被替换为另一套名称/词条，PoB2 在依赖重建时用 options 做 ReplaceNode（PassiveSpec.lua:1251-1260）。数据由 export 脚本从 classpassiveskilloverrides / ascendancypassiveskilloverrides .dat 生成（passivetree.lua:941-1040，飞升替换表来自 ascendancy.dat Replace 列 L573）。

**影响**：pobr 既没有该数据（adapter 不读取、产出 JSON 无此字段），也没有改写逻辑——受影响飞升的 build 这些节点词条直接算错（用了原版词条）。

**修复方向**：先确认上游 grindinggear/poe2-skilltree-export data.json 是否携带该字段（源文件不在仓库内，未能核实）；若不带，需把 classpassiveskilloverrides / ascendancypassiveskilloverrides .dat 表纳入 pipeline。schema 侧 `PassiveNodeDef` 增 `is_switchable` + `options`（含替换后 name/stats 与适用职业/飞升键）；逻辑侧在 resolve_passive_nodes 阶段按 build 的职业/飞升做节点替换。

### 4. 🟡 radius 珠宝 also-grant 把属性小点错误计入 Small 计数

【已核查成立】PoB2 在 "X Passive Skills in Radius also grant" 的 func 中明确要求 `node.type=="Normal" and not node.isAttribute` 才算 Small（ModParser.lua:6855-6857）；0.5 树有 293 个属性小点（kind=normal）。pobr 因无 isAttribute 信息，把它们一并计数（calc_orchestrator.rs:1864-1874：PassiveNodeKind::Normal 一律 +1）。

**影响**：珠宝半径内每个已分配属性小点多发一份授予词条，方向恒为高估。

**修复方向**（两条路径）：
- 稳妥方案：JSON schema 增加 `is_attribute` 字段（需 adapter 读取上游字段或扩 .dat 管线）；
- 止血方案：沿用 `pobr-tree/node.rs` 已有的文本判别（属性小点 stats 恒为 `+N to any Attribute` 模式），在计数处排除——无需动数据管线即可先行修复。

### 5. 🟡 环形（inner/outer）与可变半径档、Time-Lost『upgrades radius to』覆盖未支持

【已核查成立】pobr 的半径模型是纯圆盘且只认 Small/Medium/Large/Very Large 四个 label，未识别一律回退 Large；PoE2 数据里存在 8 个 inner>0 的 Variable 环形档（Data.lua:597-613），半径判定是 `innerSquared <= d² <= outerSquared` 环形（PassiveTree.lua:346），且 Time-Lost 珠宝腐化词条可把半径升档（ModParser.lua:5481-5482 → timeLostJewelRadiusOverride）。

**影响**：遇到这些珠宝时受影响节点集合算错（环内圈节点被误计入 / 升档未生效）。

**修复方向**：`JewelRadius` 改为 `{ inner, outer }` 语义并在 nodes_in_radius 加 inner 环判定；半径表本身是版本化游戏数据，应迁出为 `jewel_radii.json`（见下节）；mod_parser 增加 "upgrades radius to" 模式并在珠宝解析时应用覆盖。

### 6. 🟡 树节点授予技能（Grants Skill: X → ExtraSkill）未接入

【已核查成立】飞升关键节点普遍通过树授予主动/被动技能（data/4.5.0.3.4/passive_tree.json 中实测 52 条 "Grants Skill" 词条），PoB2 解析为 ExtraSkill 并进入技能列表参与计算（CalcSetup.lua:193-202）。

**影响**：pobr 的 mod_parser 把 "Grants Skill: \<underline\>{X}" 归 Unsupported 丢弃，相应技能及其 buff/伤害贡献完全缺席。

**修复方向**：mod_parser 识别该模式 → 产出 ExtraSkill 类 mod；calc 编排侧把树授予技能注入技能列表（与宝石 ingest 共用 skill_source 通道），并保留 SourceId 归因到对应天赋节点。

### 7. 🟡 分配/寻路/依赖引擎整体未实现

pobr 当前定位『导入 build code 后计算』可以不验证连通性，故对 parity 数值影响小；但 PoBR 目标是替换 PoB2 业务逻辑——交互式点树/最短路径预览/孤儿剪枝/intuitive-leap 类豁免/unlockConstraint 寻路约束这一整层尚无 Rust 对应。

**修复方向**：属路线图缺口而非 bug，建议显式立项。connections 邻接数据已在 JSON 中具备，可直接复用；实现时需带上三套 allocMode 寻路图（与缺口 1 联动）、多选一互斥、unlockConstraint 约束（与缺口 11 数据联动）。

### 8. 🟡 树版本迁移与 treeVersion 多版本选择缺失

PoB2 打开旧版本 build 时按 treeVersion 加载对应 TreeData 并做 convert（同 skill id 但改名的节点入 ignoredNodes 不迁移，PassiveSpec.lua:36-67、1532-1536；Load L161-176 还有 legacyClassIdMap/classInternalId 映射）。pobr 解码任意版本 build code 都按当前 data/<版本> 树解释节点 id。

**影响**：旧码遇到节点语义变更会静默算错且无提示。

**修复方向**：至少应解析 treeVersion 属性并在与当前数据版本不匹配时给出警示/拒绝；完整方案是 data/ 多版本目录 + 迁移规则（同 id 异名剔除）。

## 数据 vs 逻辑切分建议

### PoB2 现状：数据与逻辑如何混在一起

天赋树是 PoB2 里数据/逻辑分离做得**相对最好**的领域——`TreeData/0_5/tree.lua`（2.4MB）本身就是 export 脚本从 .dat 离线生成的纯数据文件，角色等价于 pobr 的 `passive_tree.json`。但仍有四类"数据"散落在 Lua 逻辑里：

1. **jewelRadii 半径表**（`Modules/Data.lua:597-613`）——按树版本键控（"0_1"）、含 inner/outer 环形档（8 个 Variable 档 inner>0，已核实），是版本化游戏数据却写死在模块里；距离乘数 1.2 又埋在 GameConstants。
2. **运行时派生索引**（`PassiveTree.lua`：节点 type 归类、keystoneMap、nodesInRadius 预计算、坐标推导）——这是"可由数据确定性派生的缓存"，归逻辑层，但其输入字段必须全部入数据。
3. **版本补丁常量**（`PassiveSpec.lua` 硬编码的 legacyClassIdMap、abyss 永恒珠宝改写常量等）。
4. **radius 珠宝词条语义函数**（ModParser.lua 6839-6905 的 func 闭包）——文本模式 → 行为映射，本质是"词条语义表"，PoB2 用 Lua 闭包表达，难以整体 JSON 化，但其中的节点类型过滤规则（Small 排除 isAttribute 等）是可表驱动的数据。

### 应 JSON 化的"数据"

pobr 已正确落库：节点定义（skill/id/name/kind/stats/connections/orbit/坐标）、职业基础三维 + 飞升名。当前 catalog schema 还缺：

**① `PassiveNodeDef` 字段扩展**

| 新字段 | 类型建议 | 来源/用途 |
|---|---|---|
| `is_attribute` | bool | 293 节点；radius 计数正确性的前提（短期可用 stats 文本判别兜底） |
| `is_switchable` + `options` | bool + Vec<NodeOption{ class/ascendancy 键, name, stats }> | 属性三选一的三个子节点 + 按职业/飞升替换定义（含替换后 stats） |
| `unlock_constraint` | Option<{ ascendancy, nodes }> | 寻路解锁约束（200 处） |
| `is_multiple_choice` / `is_multiple_choice_option` | bool | 多选一互斥（20 处） |
| `is_free_allocate` | bool | 不计点数（3 处） |
| `apply_to_armour` | bool | Smith of Kitava 机制输入（12 处） |
| `classes_start` | Vec<String> | 出生点节点——pobr 目前完全没有 ClassStart 概念 |
| charm socket 标记 | bool | 备查 |

**② `passive_tree_meta.json` 扩展**：每职业 `start_node_id`、`integer_id`；飞升 `internal_id` + 各自 `start_node_id`——build code 新格式 classInternalId/ascendancyInternalId 解码必需。

**③ 新表 `jewel_radii.json`**：按树版本组织 `{ label, inner, outer }` + distance multiplier——把 radius_jewel.rs 里硬编码的四档常量与 1.2 乘数迁出为数据。

**④（未来）`mastery_effects.json`**：mastery 系统恢复时按 id → 效果建表。

**⑤ switchable 改写源数据**：若上游 grindinggear/poe2-skilltree-export data.json 不携带（未核实，源文件不在仓库），需把 classpassiveskilloverrides / ascendancypassiveskilloverrides / passiveskills 的 ConstraintNode 等 .dat 表纳入 pipeline/tables 下载清单（已核实 pipeline/config.json 当前没有这些表）——这是当前数据管线最大的输入缺口之一。

### 应留在框架的"逻辑"

- 坐标推导与 nodesInRadius 几何（pobr 已做，且选择运行时算而非预计算缓存，合理；但需补 inner 环判定）
- BFS/DFS 寻路、depends 重建、孤儿剪枝
- switchable/hashOverrides 的 ReplaceNode 语义
- **节点效果缩放管线**（PassiveSkillHasNoEffect 清空 → PassiveSkillEffect 缩放 → 二轮珠宝 → HasOtherEffect 替换 → 局部 Jewel*Effect 缩放的应用顺序）——这是 buildModListForNode 里最重要的纯逻辑，pobr 完全缺失
- WeaponSet 条件注入
- mastery/多选一互斥等分配规则
- 树版本迁移（同 id 异名剔除）

radius 珠宝词条语义建议拆两半：模式识别留在 mod_parser（逻辑），而"哪类节点受哪种词条影响"的过滤规则（Notable/Small/Attribute × also-grant/inc-effect/grant-nothing）可做成小型静态表，置于框架内但与 PoB2 ModParser 行为逐条对照锁定。

### 一句话评估

数据侧 pobr 的形态正确（JSON per version + 零 I/O 加载），但字段覆盖只有 PoB2 tree.lua 的约六成——缺的恰是 0.5 新机制字段（isAttribute / options / unlockConstraint / applyToArmour / 武器组）；逻辑侧只移植了"收集已分配节点词条"这一层，PoB2 在 CalcSetup.buildModListForNode 里的整个节点级效果管线（缩放/条件/授予技能/珠宝改写）尚未开工，是该领域 parity 的主战场。

## 附录：核查说明

核查范围：全部 3 条 high + 3 条 medium（isAttribute 计数 / 环形半径 / Grants Skill），共 6 条，全部逐行打开 vendor PoB2 Lua 与 pobr Rust 源码比对，并用 grep 全仓（crates/apps/tools）排除"在别处实现"的可能。上一轮审计 FINDINGS.md（audits/pob2-parity-2026-06-09）完全未覆盖天赋树领域，本报告与之无重复。

**查实成立·保留**：

1. **high·WeaponSet**：PoB2 三处引用全部属实——PassiveSpec.lua L137-146 解析 `<WeaponSetN nodes>`（报告原写 137-145，微调为 137-146）、ImportFromNodeList L355/366 设 node.allocMode、CalcSetup.lua L208-228 注入/改写 Condition:WeaponSet（原引 208-258，实际条件注入在 208-228，230-258 是 Jewel*Effect 收集，已在 ref 中拆清）、CanPathThroughAllocMode L824-828 原文吻合。关键断言验证：Load L188-191 确认 `nodes` 属性含全部节点（含武器组点），故 pobr 不解析 WeaponSet 元素的后果确为"两套同时生效、恒高估"而非漏算。pobr 侧 grep "WeaponSet" 全仓仅命中 ItemSet useSecondWeaponSet，PassiveTreeSpec 无 alloc_mode——缺失属实。
2. **high·节点效果缩放管线**：CalcSetup.lua L166-168（HasNoEffect 清空）/ L170-177（PassiveSkillEffect 缩放）/ L260-274（Jewel*Effect 局部缩放含 isAttribute 排除）/ L287-290（Hulking Form 聚合）逐行核实；ModParser.lua 6839-6905 的 Time-Lost 语义函数原文核实。pobr 侧 grep PassiveSkillEffect / HasNoEffect / "Effect of Small" 零命中——完全缺失属实。pob2_ref 行号范围由 126-277 扩正为 126-290（Hulking Form 聚合在 buildModListForNodeList）。
3. **high·isSwitchable**：PassiveSpec.lua ReplaceNode 实际在 L1251-1260（原写 1250-1259，偏 1 行，已修正）；export 脚本 L941-1040 核实（含 ascedancyReplacements 来自 ascendancy.dat Replace 列 L573）；tree.lua 78 处 isSwitchable 计数核实；pobr 侧 catalog / adapter / 产出 JSON（0 命中）/ pipeline/config.json（无相关 .dat 表）四处核实全缺。唯一修正：原断言"GGG data.json 不含"无法核实（上游源文件不在仓库），改为存疑表述，不影响 gap 成立（无论上游是否携带，pobr 数据+逻辑两侧皆缺）。
4. **medium·isAttribute 计入 Small**：ModParser.lua L6855-6857 原文核实（`node.type=="Normal" and not node.isAttribute`）；calc_orchestrator.rs L1864-1874 核实（PassiveNodeKind::Normal 一律 +1）；tree.lua 293 处 isAttribute 计数核实。修正一处过强断言："修复需先在 JSON schema 增加 is_attribute"——pobr-tree/node.rs 已有 `+N to any Attribute` 文本判别，可零数据改动先行止血，schema 字段为更稳方案，detail 已改写。
5. **medium·环形/可变半径**：Data.lua jewelRadii["0_1"] 实际在 L597-613（原写 595-611，已修正），8 个 inner>0 Variable 档核实；PassiveTree.lua:346 环形判定、ModParser.lua:5481-5482 upgrades-radius 均核实；radius_jewel.rs JewelRadius（含 Custom 也仅 outer 语义）核实。成立。
6. **medium·Grants Skill**：CalcSetup.lua grantedSkills 实际在 L193-202（原写 192-202，微调）；pobr Rust 侧零命中、data/4.5.0.3.4/passive_tree.json 实测 52 条 "Grants Skill"（原文"多条"，已补精确计数）。成立。

**未深查·原样保留**：design/low 条目（分配引擎、版本迁移、keystoneMod、mastery、unlockConstraint 群、ClassStart FLAG）仅做了引用可达性抽查（PassiveSpec Load L161-176、CalcSetup L130-136、catalog.rs L385-423 等顺带核实），未发现矛盾，severity 本就保守，维持原判；其中 unlockConstraint 条目的"不在 GGG data.json"同样改为"未核实"表述。

**结论**：无删除、无降级。6 条核查全部成立；修正内容限于：4 处行号校准、2 处"GGG 导出不含该字段"降为存疑表述、1 处修复路径断言放宽（isAttribute 可文本判别兜底）、1 处补精确计数（52 条 Grants Skill）。pob2_structure / pobr_status / data_logic_split 同步带入已核实的行号与计数标注。
