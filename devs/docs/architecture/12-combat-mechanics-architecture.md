# 战斗机制处理架构 (Combat Mechanics Architecture)

> 本文是把 `agent-docs/` 里那些 PoE2 细分机制（暴击 / 伤害缩放 / 异常 / 减益 / 恢复充能 / 主动防御 / 技能功能 / 命中与敌人 / 触发 / 召唤物）**落进 `pobr-core` 的工程契约**：研究 PoB2 (`CalcOffence/Defence/Setup/Perform.lua`) 怎么实现，再适配到 pobr 现有的 `ModDb` / `CalcConfig` / `Env` / `TraceGraph` 原语。
>
> **分工**：游戏事实（数值、公式、来源）以 `agent-docs/*` 为准；通用计算原语（`StatBoundary` / `SkillUseTime` / `DamageComponent` / `AilmentInstance`）见 [`08-mechanics-primitives.md`](08-mechanics-primitives.md)；本文聚焦**机制如何在引擎里组织与编排**，以及现状到目标的**分阶段路线**。本文档描述目标设计，多数尚未实现。

---

## 1. 结论

PoB2 的机制处理范式可以一句话概括：**一切皆 Modifier，特殊行为由 Flag 驱动，敌人是另一套 ModDB，输出附带 breakdown**。pobr 的 `ModDb`（`Base/Inc/More/Flag/Override/List` + `db.flag()` + `contributions/sum_traced`）已经具备承接这套范式的全部基础原语——差距不在原语，而在**覆盖面**与**计算编排**。

因此本文不新发明聚合机制，而是规定：

1. **每个机制都翻译成「稳定 ModName + 聚合语义 + 触发它的 Flag/Tag」**，而不是写死成公式分支。
2. **敌人侧机制（暴击弱点 / 曝光 / 诅咒 / 破甲 / 受到增伤）走 `enemy.mod_db`**，由进攻计算读取，对应 PoB2 的 `enemyDB:Sum(...)`。
3. **有效暴击 / 伤害 / 异常 / 防御 是固定顺序的管线**，每一步都把贡献写进 `TraceGraph`（这是 pobr 相对 PoB 的核心增量）。
4. **召唤物是独立 Actor**（独立 `mod_db`），玩家修饰词只通过受控通道传递。

---

## 2. PoB2 的机制处理范式（适配目标）

PoB2 把所有机制统一压进 modifier 存储，计算层只做"查询 + 组合"：

| PoB2 概念 | 含义 | pobr 对应 |
|-----------|------|-----------|
| `skillModList:Sum("BASE"/"INC", cfg, name)` | 加法桶聚合 | `ModDb::sum(ModType::Base/Inc, cfg, names)` |
| `skillModList:More(cfg, name)` | 连乘 `Π(1+v/100)` | `ModDb::more(cfg, names)` |
| `skillModList:Flag(cfg, "CritChanceLucky")` | 布尔行为开关 | `ModDb::flag(cfg, ModName::from("CritChanceLucky"))` |
| `skillModList:Override(cfg, name)` | 后写覆盖 | `ModDb::override_(cfg, name)` |
| `enemyDB:Sum("BASE", nil, "SelfCritChance")` | 敌人侧修饰词 | `enemy.mod_db.sum(...)`（**待接线**） |
| `cfg` = `{ flags, keywordFlags, skillName, ... }` | 适用性上下文 | `CalcConfig{ flags, keyword_flags, skill_types, damage_type, conditions, multipliers }` |
| `output.X` + `breakdown.X` | 结果 + 推导步骤 | `OutputTable` + `BreakdownTable` / `TraceGraph` |
| `env.player` / `env.enemy` / `env.minion` | 多 Actor | `Env{ player, enemy }`（**minion 待加**） |
| `data.misc.*` 常量 | 游戏常量 | `pobr-data` `GameConstants`（**待补**） |

**关键观察**：PoB2 里没有"暴击模块/异常模块"这种强类型边界——它靠 ModName 命名空间 + Flag 区分。pobr 可以更类型化（用枚举/结构体包裹 instance），但**底层仍应是 modifier 查询**，避免把数值写死进 Rust 分支，否则会丢失归因与可配置性。

---

## 3. pobr 现状与差距

### 3.1 已实现（`pobr-core`）

- `ModDb` 全套聚合：`sum / more / flag / override_ / list / contributions / sum_traced / filtered`。
- `Modifier{ name, mod_type, value, source, origin, flags, keyword_flags, tags }`，`matches(cfg)` / `effective_number(cfg)`（含 `Multiplier` tag）。
- `CalcConfig` 上下文 + `CalcConfig::attack()` 预设。
- `Env{ player, enemy, cfg }`、`Actor{ mod_db, level, base, output, breakdown }`（**enemy.mod_db 尚未参与计算**）。
- `calc/offence.rs`：最小 DPS 管线（生命/魔力/三抗 + capped/overcap、`DamageComponent` 分类型击中、暴击、命中率、action rate、DPS）+ 归因版 `calculate_minimal_traced`。
- `calc/damage.rs`：`DamageComponent` 分类型 `base × (1+Σinc/100) × Πmore`。
- `TraceGraph` source-level 归因。

### 3.2 差距矩阵（机制族 → 现状）

| 机制族 | agent-docs | 现状 | 主要缺口 |
|--------|-----------|------|----------|
| 暴击 | `critical-hits.md` | base×inc×more，**爆伤写死 `(150+Base)/100`** | PoE2 基础爆伤应为 **+100%→2.0**（现为 PoE1 的 1.5）；缺命中降级、幸运、分岔、必然、爆伤 inc/more、敌方 SelfCrit* |
| 伤害缩放 | `damage-scaling.md` | 分类型 inc/more | 缺转换链/双重 dip、gain-as-extra、added effectiveness、幸运伤害、双/三倍、Overwhelm/穿透/曝光、Hit/DoT 拆分 |
| 异常状态 | `ailments.md` | 无 | 异常/姿态阈值、DoT 公式、积累型、叠层、`AilmentInstance` |
| 减益/控制 | `debuffs.md` | 无 | 敌人 modDB 全链：诅咒/曝光(取最强)/凋萎/破甲/受到增伤 |
| 恢复/充能/增益 | `recovery-charges-buffs.md` | 无 | 充能乘数、偷取/再生/Recoup、BuffEffect、Spirit 保留 |
| 主动/进阶防御 | `active-defences.md` | 三抗 capped | 格挡(post-mitigation)、规避、taken 乘数、Max Hit/EHP；**不实现法术压制** |
| 技能功能 | `skill-mechanics.md` | action rate 简化 | `SkillUseTime`、AoE √area、投射物、冷却/消耗、SkillType 驱动适用性 |
| 命中与敌人 | `accuracy-and-enemy.md` | `hit_chance(evasion, accuracy)` 占位 | PoE2 进攻命中公式、法术必中、怪物等级缩放表、Boss 四档、`enemy.mod_db` 配置 |
| 触发 | `triggers.md` | 无 | 触发速率上限、能量、轮转 |
| 召唤物 | `minions.md` | 无 | 独立 Actor、player→minion 传递通道 |

---

## 4. 机制处理架构（核心设计）

### 4.1 统一抽象：稳定 ModName + Flag 驱动

新机制落地的第一步永远是**登记稳定标识**，而不是写计算分支：

- **数值机制** → 新 `ModName`（如 `CriticalStrikeMultiplier` 的 `Inc/More`、`<Type>Penetration`、`AilmentMagnitude`、`Leech*`、`PowerCharge` 计数）。建议在 `pobr-data` 建一个 `ModName` 常量清单（对照 PoB2 `SkillStatMap.lua`），避免散落的 `ModName::from("...")` 字符串字面量漂移。
- **行为机制** → `ModType::Flag` 的 ModName，计算层用 `db.flag(cfg, name)` 读取。直接对应 PoB2 的 `:Flag(...)`：

  | Flag (ModName) | 行为 | 读取处 |
  |----------------|------|--------|
  | `CritChanceLucky` | 暴击几率 `1-(1-c)²` | 暴击管线 |
  | `BifurcateCrit` | 暴击再 `1-(1-c)²` + 额外爆伤 | 暴击管线 |
  | `InevitableCriticalHits` | 暴击置 100% + less 爆伤 | 暴击管线 |
  | `NoCritMultiplier` | 爆伤 = 1 | 暴击管线 |
  | `CannotBeEvaded` | 命中率 = 100% | 命中管线 |
  | `Condition:ArmourFullyBroken` 等 | 条件态 | `ModTag::Condition` |

- **适用性** → `ModFlags` / `KeywordFlags` / `ModTag`（`DamageType` / `SkillTypes` / `Condition` / `Multiplier`）。这套已经在 `Modifier::matches` 工作，新机制复用即可。

> 充能等"计数型"机制对应 PoB2 的 `Multiplier:PowerCharge`：在 pobr 里就是 `CalcConfig.multipliers["PowerCharge"]`，词条用 `ModTag::Multiplier{ var:"PowerCharge", limit }` 引用——**已支持**，只差填充与封顶。

### 4.2 敌人 modDB / 双 Actor（最高优先的接线）

PoE2 大量进攻机制其实是**敌人身上的修饰词**。`Env` 已持有 `enemy: Actor`，但 `calc/offence.rs` 完全没读 `enemy.mod_db`。补这条线是解锁暴击弱点 / 曝光 / 诅咒 / 破甲 / 受到增伤的前提：

```rust
// 暴击几率基础 = 自身 + 敌人 SelfCritChance（暴击弱点等加在基础上）
let base = db.sum(Base, cfg, &[CriticalStrikeChance])
         + enemy.mod_db.sum(Base, cfg, &[SelfCritChance]);
// 敌人受到的增伤 / 抗性 / 护甲 / 破甲，都从 enemy.mod_db 读取
```

设计要点：
- **`enemy.mod_db` 由 `setup_env` 注入**：敌人等级缩放（`accuracy-and-enemy.md` 的怪物表）、Boss 四档加成、玩家施加的诅咒/曝光/破甲/凋萎（来自玩家技能/光环），都在 setup 阶段写进 `enemy.mod_db`。
- **曝光是特例**：多来源**取最强一份**（PoB2 `ExposureMin` 逻辑），不能用 `sum`。需要一个 `db.min_of` / 在 setup 阶段归约后只留最强值。
- **归因**：敌人侧贡献用 `SourceKind::EnemyConfig` / 玩家施加的 debuff 用其原 `SourceId`，保证 TraceGraph 能区分"敌人天生抗性"与"我方曝光"。

### 4.3 有效暴击管线（替换当前实现）

当前 `offence.rs` 的暴击是 `base×inc×more` + 写死 1.5 爆伤。目标管线（严格对齐 `critical-hits.md` 的 PoB2 段，顺序不可乱）：

```text
1. crit_chance = (base + enemy.SelfCritChance + Σbase) × (1 + Σinc/100) × Πmore
                 clamp 到 [0, CritChanceCap(默认100)]
2. 若 mode_effective:  crit_chance ×= hit_chance        // 闪避/命中二次检定降级
3. 若 Flag(CritChanceLucky):     crit_chance = 1-(1-c)²
4. 若 Flag(BifurcateCrit):       crit_chance = 1-(1-c)²  // 记 PreBifurcate 供爆伤用
5. 若 Flag(InevitableCriticalHits): crit_chance = 100，并追加 less 爆伤(几何级数)
6. crit_mult = 1 + extra,   extra = (BASE_BONUS + Σ"CriticalStrikeMultiplier"Base)/100
                                    × (1+Σinc/100) × Πmore
   其中 BASE_BONUS = 100 (PoE2 玩家/召唤物，= +100% → 2.0)   // ← 修正：现为 150(PoE1)
   + 敌方 SelfCritMultiplier、+ 分岔"两次都暴击"额外一份爆伤
7. crit_effect = (1 - c) + c × crit_mult     // 用于平均 DPS
```

**立即可做的正确性修正**：`crit_multiplier = (150.0 + Base)/100.0` 改为以 `GameConstants::PLAYER_BASE_CRIT_DAMAGE_BONUS = 100` 计算 `1 + (100 + Base)/100`（PoE2 默认 2.0）。这是与"项目主要针对 PoE2"一致的明确偏差修复。

签名草图：

```rust
struct CritOutcome { chance: f64, multiplier: f64, effect: f64 }
fn resolve_crit(player: &ModDb, enemy: &ModDb, cfg: &CalcConfig,
                hit_chance: f64, base_crit: f64, mode_effective: bool) -> CritOutcome;
```

### 4.4 伤害管线扩展

在现有 `calculate_components` 基础上补齐 `08-mechanics-primitives.md` §2.3 的 9 步（详见 `damage-scaling.md`）：

- **转换链**：固定 `Phys→Light→Cold→Fire→Chaos`；技能转换先于全局；`>100%` 归一；**increased 沿途类型标签累积（双重 dip）**——这要求 `DamageComponent` 携带"沿途经过的类型集合"用于 inc 匹配。
- **gain-as-extra**：额外伤害包，不扣减来源、不归一。
- **added effectiveness**：只乘外部 flat，不乘技能自带 base（对应 `AddedDamage` MORE）。
- **hit 结果层**：lucky/unlucky 伤害、double/triple（含 on-crit）。
- **减伤层**：抗性（已部分）+ 穿透 `max(resist-pen, minPen)` + 护甲 + Overwhelm(`EnemyPhysicalDamageReduction`)。
- **Hit vs DoT 拆分**：`DamageComponent` 增加 `kind: Hit/Dot` 与 `source`（见 08 的 `DamageSource/DamageKind`），异常从指定 hit 分量派生。

### 4.5 异常 / Debuff 实例

按 08 的 `AilmentInstance` / `DebuffInstance` 落地，数据公式以 `ailments.md` 为准：

- **阈值**：`EnemyAilmentThreshold`（伤害型）与 `PoiseThreshold`（冰冻/电击/重眩晕）双表，从 `enemy.mod_db` / 怪物表读取。
- **派生**：伤害型(点燃/流血/中毒) → DoT dps + duration；积累型(冰冻/电击) → buildup。
- **叠层**：`CanStack` / `StackPotential` / 最高实例。
- **玩家施加在敌人的 debuff**（凋萎/破甲/曝光/暴击弱点）→ 写入 `enemy.mod_db`（§4.2），与"敌人受到的异常"是同一存储。

### 4.6 恢复 / 充能 / 增益

- **充能**：`CalcConfig.multipliers["PowerCharge"/"FrenzyCharge"/"EnduranceCharge"]` + 封顶（默认 3）；PoE2 充能**无固有属性**，仅供 `per X charge` 词条引用。
- **偷取 / 再生 / Recoup**：防御侧输出，按 0.5.0 重制（单实例、速率/单实例上限）；属 `calc/defence.rs` 扩展。
- **BuffEffect**：统一 `BuffEffectOnSelf` 乘区作用于 buff 提供的 modifier。
- **Spirit 保留**：`pobr-build` 编排（技能保留 spirit），`pobr-core` 只暴露保留效率聚合。

### 4.7 防御层

- **格挡 = post-mitigation 期望乘子**：`(1 - blockChance)` 放在减伤之后；不要按攻击/法术二分（PoE2 已统一，见 `block.md`）。
- **规避 (Avoidance)**：异常/眩晕/伤害规避几率，按"不被影响几率"乘法叠加。
- **taken 乘数**：`(1+Σinc/100)×Πmore`，区分 WhenHit/OverTime。
- **Max Hit / EHP**：按伤害类型分别求解（`active-defences.md` / `recovery-charges-buffs.md`）。
- **不实现法术压制 / 偏转**（PoE2 已移除）。

### 4.8 技能功能

- `SkillUseTime`（08 §3.2）替换当前 `base_action_rate × speed`。
- **AoE**：半径 `floor(baseRadius × √areaMod)`。
- **投射物**：数量 + 行为优先级 `Split→Pierce→Fork→Chain`（用 flag + count ModName）。
- **冷却/消耗**：恢复速率作除数 + 储存次数；消耗/保留分桶。
- **SkillType 驱动适用性**：技能的 `SkillTypes` 灌进 `CalcConfig.skill_types`，modifier 的 `ModTag::SkillTypes` 自动匹配——**机制已就位**，只差 gem 数据填充。

### 4.9 触发 / 召唤物

- **召唤物 = 独立 Actor**（新增 `Env.minions: Vec<Actor>`，各自 `mod_db`）。怪物式等级缩放建基础属性；**player→minion 仅三通道**：`MinionModifier`（包裹内层 mod 注入）、盟友 buff（`BuffEffectOnSelf`）、属性灌注 flag。内禀默认必中、爆伤 +70。
- **触发**：触发速率上限 `1/(ceil(cd × ServerTickRate)/ServerTickRate)`，`ServerTickRate≈30.3`；能量/轮转属高级，初版可只建模速率上限。

### 4.10 归因贯穿

所有新机制都必须把贡献写入 `TraceGraph`（沿用 `sum_traced` / `contributions` / `more_factor_traced` 模式）：敌人配置 → `SourceKind::EnemyConfig`；玩家 debuff/光环 → 其原始 `SourceId`；充能/buff → 对应来源。**没有 trace 的机制视为未完成**——这是 pobr 的差异化价值。

---

## 5. 计算编排顺序（perform 管线）

对齐 PoB2 `CalcSetup → CalcPerform → CalcOffence/Defence`，pobr 目标编排：

```text
setup_env(build, game_data) ->
  1. 建 player.mod_db（装备/天赋/宝石/buff/充能/配置 → modifier）
  2. 建 enemy.mod_db（怪物等级缩放表 + Boss 四档 + 玩家施加的诅咒/曝光/破甲/凋萎）
  3. 建 minions[*].mod_db（基础缩放 + MinionModifier 注入）
  4. 解析 CalcConfig（flags/keyword/skill_types/conditions/multipliers，含充能层数）

perform(env) ->
  for actor in [player, minions*]:
    offence:  damage 管线 → 有效暴击 → 平均击中 → 异常派生 → DPS（× hit_chance / 敌人减伤，mode_effective）
    defence:  pools → 减伤 → 格挡 → 规避 → 恢复 → Max Hit / EHP
  汇总 OutputTable + BreakdownTable + TraceGraph
```

`mode_effective`（有效 DPS）vs 面板 DPS 的口径差异见 `accuracy-and-enemy.md`：面板把命中视 100%、不扣敌人减伤；有效乘命中率与敌人抗性/护甲/受伤。

---

## 6. 落地路线（分阶段）

每阶段都要：稳定 ModName/Flag 登记 → 计算接线 → `TraceGraph` 贯穿 → unit test + PoB2 golden fixture。

| 阶段 | 内容 | 关键交付 |
|------|------|----------|
| **P1 暴击对齐 PoE2** | 修正基础爆伤 100(→2.0)；爆伤 inc/more；有效暴击链(命中降级/幸运/分岔/必然) | `resolve_crit`，crit golden |
| **P2 敌人 modDB 接线** | `setup_env` 注入敌人；offence 读取 `enemy.mod_db`；曝光取最强 | 暴击弱点/曝光/受伤 debuff 生效 |
| **P3 异常状态** | `AilmentInstance` + 阈值表 + DoT/积累 | bleed/ignite/poison/freeze/shock |
| **P4 伤害管线** | 转换/双重dip、gain-as-extra、added eff、lucky、double/triple、穿透/Overwhelm | damage golden |
| **P5 恢复/充能/增益** | 充能乘数封顶、偷取/再生/Recoup、BuffEffect | defence 扩展 |
| **P6 技能功能** | `SkillUseTime`、AoE、投射物、冷却/消耗 | 替换简化 action rate |
| **P7 触发/召唤物** | 独立 minion Actor + 传递通道；触发速率上限 | 多 Actor 编排 |

> P1/P2 是地基（敌人侧 + 暴击口径），应最先做；其余可按 Build 覆盖需求穿插。

---

## 7. 测试与 parity

- **per-mechanic unit test**：每个 Flag/公式一个最小用例（参照 08 的"第一批测试"风格）。
- **PoB2 golden fixture**：用同一 Build 在 PoB2 跑出 `output.X`，与 pobr `OutputTable` 比对（`10-pob-parity-and-attribution.md` 的 parity matrix）。
- **breakdown 断言**：关键输出的 `TraceGraph` 必须能回溯到预期 `SourceId`（暴击弱点、曝光、充能等）。
- **PoE2 优先**：与 PoB2 中 PoE1 遗留数据冲突时以一手 PoE2 数据为准（如基础爆伤 100、`Bosses.lua` 遗留、Cluster Jewel 等已在 agent-docs 标注）。

---

## 8. 资料来源

- 游戏机制事实：`agent-docs/`（`critical-hits.md` / `damage-scaling.md` / `ailments.md` / `debuffs.md` / `recovery-charges-buffs.md` / `active-defences.md` / `skill-mechanics.md` / `accuracy-and-enemy.md` / `triggers.md` / `minions.md` / `block.md`）。
- 计算原语：[`08-mechanics-primitives.md`](08-mechanics-primitives.md)；玩家可见输出：[`09-player-facing-calculation.md`](09-player-facing-calculation.md)；parity 与归因：[`10-pob-parity-and-attribution.md`](10-pob-parity-and-attribution.md)。
- PoB2 实现：`vendor/PathOfBuilding-PoE2/src/Data/`（常量/词条映射，本地）；`src/Modules/CalcOffence|Defence|Setup|Perform.lua`（计算公式，本地为部分检出不含 Modules，需 `gh api .../contents/src/Modules/<f>.lua --jq '.content' | base64 -d` 取远程）。
- 现有 pobr 实现：`crates/pobr-core/src/{mod_db,modifier,config,calc/*}.rs`。
</content>
</invoke>
