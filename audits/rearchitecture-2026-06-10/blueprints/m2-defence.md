# M2 防御机制实施蓝图（blueprints/m2-defence）

> 撰写：2026-06-11 · 依据：21-roadmap M2 节 + 13-defence.md 审计 + 20-target-architecture 裁决（P9/P11/P17）
> 本文自包含：实施 agent 只读本蓝图 + 代码即可开工，不需要再读 roadmap。
> 体量：~4 人周；并行形态：1 个串行契约批（W0）+ 5 个并行 track（A–E）+ 1 个串行收口 track（F）。

---

## 0. 阶段定位与硬性约束

**目标**：补齐防御侧三大结构缺失（扣池状态机 / keystone 开关 / taken-as 管线），EHP 口径切 PoB2（裁决 P11），防御 parity 51% → **≥80%@5%**（ninja_parity 18-build）。

**对应缺口**（13-defence.md 编号）：13-G1（taken-as）、13-G2（扣池状态机）、13-G3（MoM/EB/bypass）、13-G4（EHP 口径）、13-G5（max-hit 池扩展层）、13-G6（资源转换矩阵 + keystone）、13-G7（ArmourAppliesTo 百分比）、13-G8（Block）、13-G9（Evade 四分型）、13-G10（Deflection）、13-G11（Spirit 池/预留 efficiency）、13-G12（Stun）、13-G13（抗性细节，选做）、13-G14（Ward）、13-G15（Recoup 基数，部分）、13-G16（CI 接线）、16-G4（部分：block_chance/spirit 消费侧）。

**硬性约束**（违反即 review 打回）：

1. **P17——禁止顺手改归因结构**。本阶段不引入 pass 子图、combineStat 合并节点等任何 TraceGraph 模型扩展（那是 M4 归因 RFC 的事）。允许沿用现有 `TraceOperation`（Aggregate/Mitigate/Clamp）在输出端挂节点（如现 `ehp.rs::calc_ehp_traced` 的做法），禁止新增 `TraceOperation` 变体、禁止改 `SourceId`/`AttributionReport` 结构。
2. **calc 纯函数约定**：扣池状态机封装为局部纯函数 `fn reduce_pools(pools, hit, ctx) -> PoolsAfter`——输入输出皆值类型，**不写 `Env`**、不持共享可变状态；对 `Env` 的写入仍集中在 `perform.rs`。
3. **执行纪律**（roadmap §0 原文）：
   - 「**搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。」
   - 「行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。」
   - 「ninja_parity 18-build 零回归——防御 51% / 进攻 24%（@5% 容差）为底线不得倒退。」
4. **注入管道假设**：W3（M0 收尾，与本阶段并发）正把 calc 硬编码常量切到注入的 `RuntimeConstants`。本蓝图按「注入管道已存在」写：`CalcConfig.constants: RuntimeConstants`（`crates/pobr-core/src/config.rs:38`，`pobr_data::catalog::runtime::RuntimeConstants`，访问器 `.game()/.character()/.monster()`）。**M2 全部新机制的常量一律走 `cfg.constants.*`，不得新增 Rust 字面量魔数**。若开工时 W3 形态有变（如改注入 `RuleSet` 聚合包），仅替换访问表达式，公式不变。
5. **依赖方向不变**：不新增 crate；pobr-core 零 I/O；数据加载只在 pobr-gamedata、注入只在 pobr-build。

---

## 1. 现状坐标速查（实施前先读这些代码）

### 1.1 pobr 侧

| 坐标 | 内容 |
|---|---|
| `crates/pobr-core/src/calc/perform.rs:163-380` | `fill_mechanics`：防御机制统一写 OutputTable 的地方。**L217 `chaos_inoculation: false` 即 13-G16 的写死点**；L291-298 block 仅 Σ BASE clamp（读 `cfg.constants.game().block_chance_cap`）；L265-278 预留（无 efficiency/multiplier）；L216-231 `EhpOptions` 装配（`armour_applies_to_element: [bool;3]` 即 13-G7 的错误模型） |
| `crates/pobr-core/src/calc/ehp.rs`（全文 353 行） | 现行 EHP：per-type max hit（物理自洽迭代 L121-143，数学上等价 PoB2 无转换 quadratic）+ `total_ehp = 各类型 max hit 取 min`（L292-306，**与 PoB2 口径不同**，13-G4）。`EhpOptions` L190-211 |
| `crates/pobr-core/src/calc/defence.rs` | `calc_defence` L91-121（armour/evasion/ES + 被击中率）；`scaled_defence_stat` L205-235（per-slot 聚合，**无任何转换分支**，13-G6 插入点）；`defence_scaling_names` L240+；`es_to_mana_rate` L191-203（resourceList ES→Mana 的孤例实现，转换矩阵可参考其语义）；`calc_avoidance` L371-540（avoid 族 + **stun 的 ES>0 即 ×0.5 错误条件**在尾段）；`taken_mult_for_type*` L538-655；`enemy_crit_effect` L739 |
| `crates/pobr-core/src/calc/survivability.rs` | `reservation(pool, flat, percent)` L34-47（13-G11 缺 efficiency/multiplier 的位置）；leech/recoup/charge 在后段 |
| `crates/pobr-core/src/calc/skill_mechanics.rs:658-689` | `calc_spirit_reservation`：spirit 预留路径**已有** ReservationMultiplier more；缺池本值与 efficiency |
| `crates/pobr-core/src/calc/output.rs:4-145` | `OutputTable` 全部字段。现有：`total_ehp`/`*_max_hit`/`block_chance`/`avoid_*`/`taken_multi_*`/`spirit_reserved`。**缺**：spirit 池、block max/effect、deflect、evade 分型、stun、life_recoverable 等（W0 统一补） |
| `crates/pobr-core/src/calc/damage.rs:680` | 进攻侧 `apply_shift`（转换归一化骨架，Track B 的 shiftTable 参考同构实现，不直接复用——防御侧语义独立） |
| `crates/pobr-core/src/mod_parser.rs` | 已解析：`ChaosInoculation` flag（~L527）、`EnergyShieldConvertToMana`（L516/535）。**缺**：MoM/EB/taken-as/guard/ward bypass/aegis/block/deflect/spirit/efficiency 全族词条（W0 统一补） |
| `crates/pobr-core/src/rules/`（mod.rs + registry.rs） | M0 已落 handler 注册表骨架；`keystone_registry.rs` **不存在**（Track C 新建） |
| `crates/pobr-data/src/catalog/items.rs:55-61` | `ArmourBaseStats { armour, evasion, energy_shield, ward }`——**无 `block_chance`、无 `spirit`**（Track D 扩展） |
| `crates/pobr-data/src/catalog/runtime.rs:36` | `RuntimeConstants`（W3 注入包）；`catalog/game_constants.rs` 为 schema |
| `data/4.5.0.3.4/base/game_constants.json` | 已含 stun 全套（`stun_base_mult`/light/heavy 系列）、`deflection_chance_cap`/`deflect_effect`/`evade_chance_cap`/`block_chance_cap`/`resist_floor` 等。**缺** `ehp_calc_max_damage`/`ehp_calc_max_iterations`/`ehp_calc_speed_up`/`normal_enemy_dps_mult`（W0 补，来源 Data.lua:228/235/237/239） |
| `data/4.5.0.3.4/base/monster_scaling.json` | 已含 `damage` 百级表（敌人进伤默认值的数据源） |
| `data/4.5.0.3.4/base/enemy_presets.json` | 已含 `ehp_base_damage_mult: 1.5` + per-tier `dps_mult`（Track F 进伤装配直接消费） |
| `crates/pobr-build/src/calc_orchestrator.rs:364-460, 781-930` | 装备护甲件 per-slot 防御底值注入（armour/evasion/ES，含品质/局部 inc/per-level）。Track D 的 block/spirit 注入加在同一段 |
| `crates/pobr-build/tests/ninja_parity.rs` | parity 门禁：`defensive_rows` L89-132（**当前仅 8 列**：Life/Mana/ES/Armour/Evasion/三抗）；基线 `BASELINE_DEF_HIT5=111 / DEF_HIT10=117 / OFF_HIT5=23 / OFF_HIT10=31`（L270-273）。golden（`examples/demo-bd-test/builds/*/meta.json::player_stats`）**已含** `PhysicalMaximumHitTaken`/`Fire|Cold|Lightning|ChaosMaximumHitTaken`/`TotalEHP`/`EffectiveBlockChance`/`EffectiveSpellBlockChance`/`Spirit`/`SpiritUnreserved`/`EvadeChance`/`MeleeEvadeChance`/`Projectile|Spell|SpellProjectileEvadeChance`/`DeflectChance`/`DeflectionRating`/`PhysicalDamageReduction`/`LifeUnreserved`/`ManaUnreserved`/`EnergyShieldRecoveryCap`/`LifeRecoverable` 等键——M2 全部新产出都有黄金参照 |

### 1.2 vendor PoB2 侧（`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`，4285 行，本地完整可读；下列行号 2026-06-11 逐段核实）

| 行号 | 内容 |
|---|---|
| :48-54 | `deflectChance(deflection, accuracy)` = `100 - (acc/(acc+deflection×0.12)×150 - 50)`，cap `DeflectionChanceCap`(95) |
| :356-417 | `applyDmgTakenConversion`：`<Src>DamageTakenAs<Dst>`/`<Src>DamageFromHitsTakenAs<Dst>`/elemental 变体 BASE 求和成 shiftTable；源类型保留 `max(1-total/100,0)`；每个目标类型独立过 taken inc/more、flat DR、`ArmourAppliesTo<X>` 合成 effArmour、抗性，最后求和 |
| :422-455 | `takenHitFromDamage(rawDamage, damageType, actor)`：按 `actor.damageShiftTable[damageType]` 遍历转换后类型 → `EffectiveAppliedArmour` DR + flat DR − overwhelm（clamp `DamageReductionMax`）× `ResistTakenHitMulti` + `takenFlat`，再 × `AfterReductionTakenHitMulti` |
| :456-459 | `calcLifeHitPoolWithLossPrevention(life, maxLife, lossPrev, lossBelowHalfPrev)`：above-half/below-half 分段除法 |
| :461-678 | **`reducePoolsByDamage(poolTable, damageTable, actor)` 扣池状态机**（Track A 的逐行对照基准）：① allies（frostShield/spectres/totems/vaalRejuvenationTotems/radianceSentinel/soulLink，各 `{remaining, percent}` 比例先扣，**不计入 recoupable**）→ ② per-type damageRemainder 记入 `damageTakenThatCanBeRecouped` → ③ aegis（per-type → sharedElemental(仅元素) → shared）→ ④ guard（per-type 与 shared 各按 `AbsorbRate%` 比例吸收）→ ⑤ ward（`×(1−WardBypass/100)`；`WardNotBreak` 时返还）→（以上正序遍历伤害类型）→ ⑥ **逆序**遍历类型：ES（chaos `esDamageTypeMultiplier=2` 除非 `ChaosNotDoubleESDamage`；per-type `esBypass`；`EternalLife` 分支 :588-594；EB 嵌套时 `MoMEBPool` 公式 :597-603）→ ⑦ MoM（`MoMEffect=min(shared+perType,100)/100`，`MoMPool=min(lifeHitPool/(1−MoM)−lifeHitPool, mana)`）→ ⑧ loss-prevention（`preventedLifeLoss`/`LifeLossBelowHalfPrevented` 分段，溢出记 overkill）→ ⑨ life → overkill。返回完整 `PoolsAfter`（各池余量 + recoupable + LifeLossLostOverTime + overkill + hitPoolRemaining） |
| :806-808, :1235-1237 | `Unbreakable && IronReflexes` → Body Armour evasion 基底 ×2 |
| :961-1058 | Block：`BlockChanceMax = Override \|\| (BaseBlockChanceMax + BlockChanceMax)` cap `BlockChanceCap`(90)；盾基底 = `Weapon 2/3 armourData.BlockChance`（:975-979）；`(base + ΣBASE BlockChance) × calcLib.mod(inc/more)` 后 min cap；Projectile/Spell/SpellProjectile 分型；`SpellBlockChanceIsBlockChance` 等 flag；lucky/unlucky 幂与 `EffectiveBlockChance` 在后段（~:1030-1058）；格挡回复 :1901-1914 |
| :1150-1290 | 六槽位 armourData 逐槽聚合 + keystone flag：`DoubleBodyArmourDefence`（ward/ES/armour/evasion 皆 ×2）、`EnergyShieldToWard`（ES 的 inc 借给 ward、ES 本体不再聚合）、`Unbreakable`（armour ×2）、`ConvertBodyArmourArmourEvasionToWard` |
| :1301-1390 | **resourceList 五元转换矩阵**（Track C 基准）：`{Armour, Evasion, EnergyShield, Life, Mana}`；每对 `<Src>ConvertTo<Dst>` BASE（cap 100，总和 >100 归一化）+ `<Src>GainAs<Dst>`（不减源）；defence 源按 per-slot 转移（目标也是 defence 则进目标的 slot 桶，否则进 globalBase），非 defence 源走 global `ceil`；最后 per-slot × `(global_inc+slot_inc, more)` 聚合 + `modsTotal`；非 defence 目标以 `NewMod("Extra"+name)` 注入 |
| :818-941 | 抗性层：`<X>MaxResConvertTo<Y>`、Melding（`ElementalResistMaxIsHighestResistMax` Override）、Dot/totem 变体、INC 乘区、floor −200（13-G13，选做参照） |
| :1396-1466 | Evade：`Melee/Projectile/Spell/SpellProjectileEvasion` 四分型独立 inc；`EvadeChance = 100 − (monsterHitChance − BASE EvadeChance) × enemy HitChance 乘区`，`EvadeChanceMax`/`EvadeChanceCap=95`、`CannotEvade`/`AlwaysEvade`/`UnluckyEvade` |
| :1487-1506 | `DeflectionRating = BASE + Evasion/Armour GainAsDeflection`；:1491 `DeflectChance = deflectChance(rating, enemyAccuracy)`；`DeflectEffect` 基础 40 |
| :73-126, :172-350 | `doActorLifeManaSpirit`（Life/Mana/Spirit 统一 `base×(1−conv)+extra ×inc×more` + Override，CI→Life=1）；`doActorLifeManaSpiritReservation`（`ReservationMultiplier` more-floor4、`ReservationEfficiency` inc/more **除法**、`ExtraXReserved`、BloodMagic） |
| :2015-2037 | 四分型 `NotHitChance = 100 − (1−Evade)(1−Dodge)(1−Avoid)`（`EHPUnluckyWorstOf` 平方/四次方）；`ConfiguredNotHitChance` 按 damageCategoryConfig 选取 |
| :2040-2110 | 敌人进伤装配：`enemy<X>Damage = configInput \|\| configPlaceholder`（placeholder 默认 = `monsterDamageTable[lv] × 1.5 × normalEnemyDPSMult`，chaos 再 ÷2.5——ConfigOptions.lua:1975-1996）+ enemyDB Min/Max 词条 + EnemyCritEffect + 敌方 Conversion |
| :2247-2300 | taken 乘区矩阵：hit/Attack/Spell/Reflect/Dot 五口径 `<X>TakenHitMulti`/`AfterReductionTakenHitMulti`（suppress×deflect 折入后者 :2434） |
| :2525-2643 | Stun：`StunThreshold`（基 Life，可 `StunThresholdBasedOnEnergyShieldInsteadOfLife`/Mana/CI 前 Life + `AddESToStunThreshold`）；:2554-2557 `ES > totalTakenHit && not EnergyShieldProtectsMana` 才 ×0.5；`SelfStunChance = StunBaseMult(200) × 有效伤/阈值`（物理 ×0.25 加权）；时长按 `ServerTickRate` 上取整 |
| :2707-2723 | per-type `<X>EnergyShieldBypass`（Override 或 BASE；`UnblockedDamageDoesBypassES`→100；clamp 0-100；`MinimumBypass`） |
| :2726-2820 | MoM/EB 池整备：`sharedMindOverMatter = min(ΣDamageTakenFromManaBeforeLife, 100)`；EB（`EnergyShieldProtectsMana` flag）时 mana 池先被 ES 按 bypass 嵌套保护（`manaProtected` 公式）；`poolProtected = sourcePool/(rate)×(1−rate)`；per-type `<X>DamageTakenFromManaBeforeLife` 同构 |
| :2825-2890 | Guard 池整备：`sharedGuardAbsorbRate`(cap 100)/`GuardAbsorbLimit`，per-type 同构，poolProtected 同公式 |
| :2979-3145 | **`numberOfHitsToDie(DamageIn)`**（Track F 基准）：池快照（allies/aegis/guard/ward/ES=EnergyShieldRecoveryCap/Mana=ManaUnreserved/Life=LifeRecoverable）→ while Life>0 循环调 `reducePoolsByDamage` → `GainWhenHit` 每击间恢复 → 递归加速（`ehpCalcSpeedUp=8`，loss-prevention 时 `LimitEHPSpeedup`→4）→ overkill 折算小数击数；上限 `ehpCalcMaxDamage=1e8`/`ehpCalcMaxIterationsToCalc=50`（Data.lua:235-239）；`WardNotBreak && damage < Ward` → ∞ |
| :3153 | `output.NumberOfDamagingHits = numberOfHitsToDie(DamageIn)` |
| :3246-3247 | `ConfiguredDamageChance = blockEffect × suppressionEffect × deflectMulti × (1 − ConfiguredNotHitChance/100)`；`NumberOfMitigatedDamagingHits = NumberOfDamagingHits / ConfiguredDamageChance` |
| :3322 | `TotalEHP = TotalNumberOfHits × totalEnemyDamageIn` |
| :3540-3601, :3643-3656 | max hit：TotalHitPool 依次叠 ward（bypass poolProtected）/aegis/guard/allies；armour 分支解二次方程（a=ArmourRatio×convMulti×(1−flatDR+overwhelm)）；多转换分支 `useConversionSmoothing` |
| ModParser.lua:415-418, :2389, :2439 | MoM 词条文本（`damage is taken from mana before life` → `DamageTakenFromManaBeforeLife`，elemental/per-type 变体）；EB 文本 `energy shield protects mana instead of life` → flag `EnergyShieldProtectsMana`（Track W0 词条参照） |

---

## 2. 工作分解（W0 → A–E 并行 → F 收口）

### W0：契约先行批（串行，先于一切合并；1 人 ~3-4 天）

W0 的意义：把全部 track 都要碰的三个共享面（mod_parser 词条 / OutputTable 字段 / pool 类型契约）一次性锁定，使 A–E 可以零冲突并行。**W0 全部是行为中性的纯增量**（新词条解析出的 ModName 此时无消费者；新字段默认 0），合并后 ninja_parity 必须逐值不变。

#### W0.1 mod_parser 防御词条批

- **目标**：把 M2 全部新机制的词条→ModName 解析一次性落地（数据面先行，消费侧由各 track 接）。
- **涉及文件**：`crates/pobr-core/src/mod_parser.rs`（独占）+ 其单测。
- **vendor 参照**：ModParser.lua nameList/specialList 防御段——:415-418（MoM 族）、:2389（all-damage MoM 100）、:2439（EB flag）、:2519-2544（ArmourAppliesTo 三变体：`N% of armour applies to <X>` → BASE；`instead of physical` → 额外 `flag("ArmourDoesNotApplyToPhysicalDamageTaken")`；`also applies` → 仅 BASE）；Data/ModCache.lua 中 `GuardAbsorbRate/GuardAbsorbLimit`、`<X>EnergyShieldBypass`、`WardBypass`、`<X>Aegis`/`sharedAegis`、`LifeLossBelowHalfPrevented`/`LifeLossPrevented`、`<Src>DamageTakenAs<Dst>` 族、`BlockChance`/`SpellBlockChance`/`BaseBlockChanceMax`/`BlockChanceMax`/`BlockEffect`/`DamageTakenOnBlock`、`DeflectionRating`/`DeflectEffect`/GainAsDeflection、`ReservationEfficiency`、`Spirit`、`<X>ConvertTo<Y>`/`<X>GainAs<Y>`（五元防御资源）、`EvadeChance`(BASE)/`EvadeChanceMax`、`StunThreshold`/`AvoidStun` 补充、`DamageTakenFromManaBeforeLife` per-type 族。
- **清单产出**：在 mod_parser 单测里建一张「M2 词条覆盖表」测试（每个文本模式 → 期望 ModName/ModType/值），作为各 track 的接口契约文档。
- **测试**：每模式至少一条解析单测（AAA）；跑 `cargo test -p pobr-core` 全绿；ninja_parity 基线逐值不变（新 ModName 无消费者）。
- **规模**：~35-45 个文本模式，400-600 行（含测试）。
- **注意**：这是「按 parity 需要在现有 Rust parser 上补词条」（P3 裁决允许的 M0–M5 节奏），不做六表数据化（那是 M6）。

#### W0.2 OutputTable / display_catalog 字段批

- **目标**：一次性补齐 M2 全部输出字段，锁定 `output.rs`，避免 5 个 track 同文件冲突。
- **涉及文件**：`crates/pobr-core/src/calc/output.rs`、`crates/pobr-core/src/display_catalog.rs`（新字段标 `ParityStatus`，未接线前为 Planned）。
- **新增字段**（默认 0/中性，golden key 对照见 §1.1 末行）：
  - Spirit：`spirit`、`spirit_unreserved`
  - Block：`block_chance_max`、`spell_block_chance_max`、`effective_block_chance`、`effective_spell_block_chance`、`block_effect`（承伤比例）
  - Deflection：`deflection_rating`、`deflect_chance`
  - Evade 分型：`evade_chance`（综合）、`melee_evade_chance`、`projectile_evade_chance`、`spell_evade_chance`、`spell_projectile_evade_chance`
  - Stun：`stun_threshold`、`self_stun_chance`、`stun_duration`
  - Ward：`ward`
  - 池口径：`life_recoverable`、`energy_shield_recovery_cap`、`physical_damage_reduction`（面板 DR%）
  - EHP 新口径：`number_of_damaging_hits`、`number_of_mitigated_hits`、`total_ehp_lowest_max_hit`（旧口径改名保留为附加指标；`total_ehp` 字段在 Track F 切换语义）
- **测试**：`Default` 中性值单测；display_catalog 条目数断言更新。
- **规模**：~150 行。

#### W0.3 pool_damage 类型契约（空实现）

- **目标**：锁定 Track A 与 Track F 之间的接口，使 F 可以在 A 完成前按契约写编排框架。
- **涉及文件**：新建 `crates/pobr-core/src/calc/pool_damage.rs`（类型 + 签名 + `todo!`/恒等空实现）+ `calc/mod.rs` 挂载。
- **契约**（与 CalcDefence.lua:461-678 的输入输出一一对应）：

```rust
/// 五类型伤害向量（taken 之后、入池之前）。
pub struct TypedDamage { pub physical: f64, pub fire: f64, pub cold: f64, pub lightning: f64, pub chaos: f64 }

/// 盟友先扣层（frost shield / spectre / totem / soul link …）。
pub struct AllyLayer { pub id: &'static str, pub remaining: f64, pub mitigation_pct: f64 }

/// 扣池前的全部池快照（对应 PoB2 poolTable；构造一次、循环中按值传递）。
pub struct PoolState {
    pub allies: Vec<AllyLayer>,
    pub aegis_shared: f64, pub aegis_shared_elemental: f64, pub aegis_by_type: [f64; 5],
    pub guard_shared: f64, pub guard_shared_rate: f64, pub guard_by_type: [f64; 5], pub guard_rate_by_type: [f64; 5],
    pub ward: f64,
    pub energy_shield: f64, pub mana: f64, pub life: f64,
    pub life_loss_lost_over_time: f64, pub life_below_half_loss_lost_over_time: f64,
}

/// 不随单次击中变化的上下文（flag/比例从 ModDb 读出后固化，状态机本体不读 ModDb）。
pub struct PoolCtx {
    pub max_life: f64,
    pub es_bypass_by_type: [f64; 5],          // 0-100
    pub mom_shared: f64, pub mom_by_type: [f64; 5], // 0-100
    pub ward_bypass: f64,
    pub eternal_life: bool, pub eb: bool /* EnergyShieldProtectsMana */,
    pub chaos_not_double_es: bool, pub ward_not_break: bool,
    pub prevented_life_loss: f64, pub life_loss_below_half_prevented: f64,
}

pub struct PoolsAfter {
    pub pools: PoolState,                       // 扣减后的池
    pub recoupable_by_type: [f64; 5],
    pub overkill: f64,
    pub hit_pool_remaining: f64,
    pub resources_lost: /* 每类型每层扣量，breakdown 用 */ Vec<(DamageType, &'static str, f64)>,
}

/// 扣池状态机（纯函数；顺序固定 allies→aegis→guard→ward→ES(bypass)→MoM→loss-prevention→life→overkill）。
pub fn reduce_pools(pools: &PoolState, hit: &TypedDamage, ctx: &PoolCtx) -> PoolsAfter;

/// X-protects-Y 通用原语：poolProtected = source/(rate)×(1−rate)（rate∈(0,1]；rate≥1 → ∞ 保护）。
/// MoM / Guard / Ward bypass / SoulLink / EB 嵌套全部复用本公式（CalcDefence.lua:2746/2837/3546-3550）。
pub fn pool_protected(source_pool: f64, rate_fraction: f64) -> f64;

/// 分段生命命中池（CalcDefence.lua:456-459）。
pub fn life_hit_pool_with_loss_prevention(life: f64, max_life: f64, loss_prev_pct: f64, below_half_prev_pct: f64) -> f64;
```

- **测试**：契约编译 + `pool_protected`/`life_hit_pool_with_loss_prevention` 的公式单测（这两个小函数 W0 即可实现并锁数值）。
- **规模**：~200 行。

#### W0.4 game_constants 增四常量

- **目标**：`ehp_calc_max_damage=100000000` / `ehp_calc_max_iterations=50` / `ehp_calc_speed_up=8` / `normal_enemy_dps_mult=0.227272…(1/4.4)` 入 `game` 段。
- **涉及文件**：`crates/pobr-data/src/catalog/game_constants.rs`（`GameMechanicsConstantsDef` 加字段，`#[serde(default)]` + fallback 默认值=同值）、`data/4.5.0.3.4/base/game_constants.json`、`crates/pobr-gamedata/tests/load_game_constants.rs`（逐值锁定）。
- **vendor 参照**：Modules/Data.lua:228（normalEnemyDPSMult）/:235/:237/:239。
- **纪律**：沿用 M0-W2 九表落库通道（搬迁不变式：此 commit 无任何 calc 消费，parity 逐值不变）。注：`enemy_presets.json` 已有 `ehp_base_damage_mult=1.5` 与 per-tier `dps_mult`，不重复。
- **规模**：~60 行。

**W0 验收**：fmt/clippy/test 全绿；ninja_parity 四个 BASELINE 数字逐值不变；一个 commit 一个子项（W0.1-W0.4 共 4 个 commit）。

---

### Track A：扣池状态机 + 池整备（13-G2 / 13-G3 / 13-G5；1 人 ~5 天）

- **目标**：实现 `reduce_pools` 全状态机与「池整备」（从 ModDb 读出 bypass/MoM/guard/aegis 等构造 `PoolCtx`/`PoolState`），并以 poolProtected 原语扩展 max-hit 的 TotalHitPool。
- **涉及文件**（独占写）：
  - `crates/pobr-core/src/calc/pool_damage.rs`（W0.3 契约的实现主体）
  - 新建 `crates/pobr-core/src/calc/pool_setup.rs`：`fn build_pool_ctx(db, cfg, output_like) -> PoolCtx` + `fn build_pool_state(...) -> PoolState`（ES bypass per-type :2707-2723、MoM shared/per-type :2726-2820、Guard :2825-2890、Aegis、allies 占位——本阶段玩家无 frost shield 等来源时为空 Vec，结构保留）
  - `crates/pobr-core/tests/pool_damage.rs`（新集成测试）
- **不碰** perform.rs（A 是纯函数库，消费者是 Track F）。
- **vendor 参照**：CalcDefence.lua:461-678 逐分支（§1.2 已列顺序与公式）；:2707-2820（bypass/MoM/EB 整备）；:3540-3601（max-hit 池扩展层——以 `pool_protected` 把 ward/aegis/guard 折进 TotalHitPool，供 F 的 max hit 重算）。
- **实现要点**：
  - 伤害类型遍历顺序：前半段（allies→aegis→guard→ward）正序，ES 及之后**逆序**（:578 `for i=#dmgTypeList,1,-1`，dmgTypeList = Physical,Lightning,Cold,Fire,Chaos——逆序即 Chaos 先）。
  - chaos 对 ES 双倍（`esDamageTypeMultiplier=2`）；`EternalLife` 与 EB 两条 ES 分支互斥，公式照抄 :585-603。
  - MoM 池 `MoMPool = min(lifeHitPool/(1−MoM)−lifeHitPool, mana)`，`lifeHitPool` 用 `life_hit_pool_with_loss_prevention`。
  - 全程 f64 值语义、无 `&mut Env`；`PoolState` 按值进出（Clone 开销可忽略，EHP 循环 ≤50 次迭代）。
- **测试与 fixture 计划**：
  - 单元：每层独立 fixture（纯 life；life+ES；life+ES+chaos 双倍；bypass 30%；MoM 30%；MoM 100%；EB+bypass 嵌套；guard 20%/limit；ward+bypass；EternalLife；loss-prevention above/below half；overkill 折算）。期望值**手算自 PoB2 公式并在测试注释标注 CalcDefence.lua 行号**。
  - 对拍：挑 2 个 ninja build（MoM 系如 sorceress-stormweaver-comet、CI 系 witch-abyssal-lich）用 pob2-oracle 跑中间值（`sharedMoMHitPool`/`<X>EnergyShieldBypass`）比对池整备输出。
- **门禁**：本 track 合并时 parity 逐值不变（无 perform 接线，零行为影响）；fixture 全绿。
- **规模**：~700-900 行（含测试）。

### Track B：taken-as 管线 + effectiveAppliedArmour（13-G1 / 13-G7；1 人 ~5 天）

- **目标**：防御侧 `<Src>DamageTakenAs<Dst>` shift 矩阵 + `ArmourAppliesTo<X>` 从 `[bool;3]` 改百分比模型 + `takenHitFromDamage` 等价入口。
- **涉及文件**（独占写）：
  - 新建 `crates/pobr-core/src/calc/taken.rs`：
    - `fn damage_shift_table(db, cfg) -> [[f64;5];5]`（:356-365 的 BASE 求和 + 源保留 `max(1−total,0)`；elemental 变体并入）
    - `fn effective_applied_armour(db, cfg, armour, evasion, es, dtype) -> f64`（:386-396：`Armour×pct/100×(1+ArmourDefense) + Evasion×pct + ES×pct`；物理隐式 BASE 100 由调用方注入，对应 :1862-1863 `NewMod("ArmourAppliesToPhysicalDamageTaken","BASE",100)`——在本函数内以「物理且无 `ArmourDoesNotApplyToPhysicalDamageTaken` flag 时 +100」实现，**不写 ModDb**）
    - `fn taken_hit_from_damage(raw, dtype, mit: &MitigationCtx) -> (f64, [f64;5])`（:422-455 等价：per 转换类型 `armourReductionF(effArmour, dmg)` + flat DR − overwhelm（clamp DR max）× resist taken multi + takenFlat，× AfterReductionTakenHitMulti）
  - `crates/pobr-core/src/calc/perform.rs`：替换 L216-231 `EhpOptions.armour_applies_to_element` 装配段——改为构造 `MitigationCtx`（13-G7 行为修复，独立 commit，附 ModParser.lua:2519-2544 + CalcDefence.lua:2336-2362 依据）。**perform.rs 仅此一段，见 §3 合并顺序**。
  - `crates/pobr-core/tests/taken_as.rs`（新）。
- **vendor 参照**：CalcDefence.lua:356-455、:2336-2362（EffectiveAppliedArmour 合成）、:1862-1863；ModParser.lua:2519-2544（三变体解析语义，词条由 W0.1 提供）。
- **过渡语义**：在 Track F 接线前，现行 `ehp.rs` 的 `armour_applies_to_element: [bool;3]` 路径保持原行为（旧 max-hit 口径继续用它）；`taken.rs` 的新模型只被新增测试与 F 消费。**B 对 perform 的 `[bool;3]` 替换 commit 是行为修复**：「also applies / N% applies」build 的物理 max hit 修正，须附 baseline 审查。
- **测试与 fixture**：shift 矩阵归一化（>100% 截断到源 0 保留）、`Lightning Coil 型 50% phys taken as lightning` 端到端 fixture（物理 max hit 显著提高、电 max hit 受抗性制约）、`50% of armour applies to fire` 部分适用 fixture、`instead of physical` flag fixture（物理护甲清零仅此变体）。
- **门禁**：除 13-G7 修复 commit 外 parity 逐值不变；修复 commit 附 PoB2 行号与受影响 build 列表。
- **规模**：~500-650 行。

### Track C：keystone_registry + 防御资源转换矩阵（13-G6 / 13-G16；1 人 ~5-6 天）

- **目标**：建 `rules/keystone_registry.rs`（数据 flag → 有限稳定分支的集中开关层），消灭 perform 写死 false 的 CI，补防御五元 ConvertTo 矩阵与翻倍 flag。
- **涉及文件**（独占写）：
  - 新建 `crates/pobr-core/src/rules/keystone_registry.rs` + `rules/mod.rs` 挂载：

```rust
/// 防御 keystone 开关快照（一次性从 ModDb 读出，calc 各处只读本结构，不再散读 flag）。
pub struct DefenceKeystones {
    pub chaos_inoculation: bool,        // mod_parser 已解析（~L527）
    pub eldritch_battery_es_to_mana: bool, // EnergyShieldConvertToMana >= 100（既有 es_to_mana_rate 语义并入）
    pub energy_shield_protects_mana: bool, // EB flag（W0.1 新词条）
    pub eternal_life: bool,
    pub iron_reflexes: bool,            // EvasionGainAsArmour 100 的数据展开仍走转换矩阵；flag 仅供 Unbreakable 联动
    pub unbreakable: bool,
    pub double_body_armour_defence: bool,
    pub energy_shield_to_ward: bool,
    pub ward_not_break: bool,
    pub blood_magic: bool,              // 预留（M3 接 reservation）
}
impl DefenceKeystones { pub fn from_db(db: &ModDb, cfg: &CalcConfig) -> Self { … } }
```

  - `crates/pobr-core/src/calc/defence.rs`：`scaled_defence_stat` 区段（L205-260）插入五元转换矩阵 `apply_resource_conversion`——在 per-slot 聚合之后、全局 inc/more 之前，对照 :1301-1390（`ConvertTo` cap100+归一化、`GainAs` 不减源、defence↔非 defence 的 slot/global 分流、非 defence 目标以 `ExtraLife`/`ExtraMana` BASE 注入语义返回给 perform）；翻倍 flag（Unbreakable/DoubleBodyArmourDefence/EnergyShieldToWard/Unbreakable×IronReflexes）作用在 Body Armour 槽基底（:1150-1290）——现有 `es_to_mana_rate`（defence.rs:191-203）并入矩阵实现后删除旧函数。
  - `crates/pobr-core/src/calc/perform.rs`：**仅两处**——L217 `chaos_inoculation: false` → `keystones.chaos_inoculation`（13-G16，一行接线）；`fill_mechanics` 开头构造 `DefenceKeystones` 并传给相关调用。
  - `crates/pobr-core/tests/keystone_defence.rs`（新）。
- **vendor 参照**：CalcDefence.lua:1301-1390（矩阵）、:1150-1290 / :806-808（翻倍 flag）、:85/:120-123/:2537-2539（CI）；keystone 词条文本在树/ModCache（解析已就位或 W0.1 补）。
- **数据 vs 逻辑切分**（13-defence §5 结论）：开关是数据（树词条→flag）、行为是逻辑（本注册表的有限分支）。**不做** per-unique 硬编码；矩阵词条名走 W0.1 解析表。
- **commit 切分**（关键纪律）：
  1. `keystone_registry.rs` + 测试（无消费者，parity 不变）；
  2. **CI 接线一行**（行为修复，独立 commit：witch-abyssal-lich 等 CI build 的 chaos max hit → ∞、EHP 改走 ES 池；附 CalcDefence.lua:2537-2539 依据 + baseline 审查）；
  3. 转换矩阵 + 翻倍 flag（行为修复，独立 commit：Unbreakable/IronReflexes build 修正；矩阵无词条的 build 逐值不变——用 golden_regression 证明）。
- **测试与 fixture**：矩阵单测（单向转换/超 100 归一化/GainAs 叠加/defence→非 defence）；Unbreakable 身甲翻倍 fixture；IronReflexes（`EvasionGainAsArmour` 数据词条）端到端；CI build EHP fixture。
- **门禁**：commit 1 parity 逐值不变；commit 2/3 baseline 更新独立审查。
- **规模**：~600-800 行。

### Track D：Block / Spirit / Ward / Deflection 面板族 + 预留 efficiency（13-G8 / 13-G10 / 13-G11 / 13-G14 / 16-G4 部分；1 人 ~6-7 天）

- **目标**：补防御面板四个缺失子系统的「基底数据 → 聚合 → OutputTable」全链路，以及 ReservationEfficiency。
- **涉及文件**（独占写）：
  - **数据三件**（16-G4 的 M2 部分——M1 的 adapter 扩展聚焦 gem/skill 链路，**不含** block/spirit，故 M2 自带）：
    - `crates/pobr-data/src/catalog/items.rs`：`ArmourBaseStats` 加 `block_chance: Option<f64>` 与 `movement_penalty: Option<f64>`（serde default，schema 兼容 R7）；`BaseItemDef` 加 `spirit: Option<u32>`。
    - `pipeline/config.json` + `tools/pobr-data-adapter/src/main.rs`：增 `ShieldTypes` 表下载与按 BaseItemType join（参照现有 ArmourTypes 处理 L184-270）；spirit 列源待定（见 §6 开放问题 2）——**兜底方案**：`tools/sync-pob-catalog extract-lua` 从 `vendor/.../Data/Bases/{shield,sceptre,…}.lua` 抽 `armour.BlockChance`/`spirit` 落 `overlay/base_item_overrides.json`（M0 的 skill_overrides 通道同构），由 gamedata overlay merge 进 BaseItemDef。两条路线取先通者，门禁同（regen-check byte-diff）。
    - `data/4.5.0.3.4/`：重生产物（独立「搬迁/数据」commit，calc 不消费时 parity 不变）。
  - `crates/pobr-build/src/calc_orchestrator.rs`：装备注入段（L781-930 邻域）把盾 `block_chance` 注入 `ShieldBlockChance` BASE（槽位 Weapon 2/3）、`spirit` 注入 `Spirit` BASE。
  - 新建 `crates/pobr-core/src/calc/defence_panels.rs`（避免与 C/E 抢 defence.rs）：
    - `calc_block(db, cfg, shield_base, constants) -> BlockResult`：BlockChanceMax 体系（`Override || BaseBlockChanceMax+BlockChanceMax` cap `block_chance_cap`）、`(shield_base + ΣBASE) × (1+inc)` cap、Projectile/Spell/SpellProjectile 分型、`EffectiveBlockChance`（lucky/unlucky 幂 :1030-1058）、`BlockEffect`→承伤折减系数（对照 :961-1058）
    - `calc_ward(db, cfg, slot_bases) -> f64`（per-slot 聚合 + `EnergyShieldToWard` 由 Track C 的 keystone 结构传入；:1144-1273）
    - `calc_deflection(db, cfg, armour, evasion, enemy_accuracy, constants) -> (rating, chance)`（:48-54、:1487-1506；常量 `deflection_chance_cap`/`deflect_effect` 已在 game_constants）
    - `calc_spirit_pool(db, cfg, base_spirit) -> f64`（与 Life/Mana 同构：base+BASE × inc × more + Override，:73-126）
  - `crates/pobr-core/src/calc/survivability.rs`：`reservation` 扩签名加 `efficiency_inc/more`（除法语义 :172-350）+ multiplier 推广到 Life/Mana 路径；`skill_mechanics.rs:658-689` spirit 预留补 efficiency。**与 M1-T4.5 的衔接**：技能侧 spirit 预留聚合（spirit_reservation_flat × ReservationMultiplier → `spirit_reserved`）由 M1 交付，本 track 只补 efficiency、池本值（calc_spirit_pool）与 `spirit`/`spirit_unreserved` 字段（W0.2）；若与 M1 并行执行，这两个文件以 M1 先合并、M2-D rebase 为序。
  - `crates/pobr-core/src/calc/perform.rs`：新增 `fill_defence_panels(env, &keystones)` 子函数一处调用（见 §3 合并协议）。
  - `crates/pobr-core/tests/defence_panels.rs`（新）。
- **vendor 参照**：见上各函数标注行号；Data/Bases/shield.lua（`BlockChance=26…`）、sceptre.lua（`spirit=100`）为数值参照。
- **测试与 fixture**：盾 build（warrior-titan-shield-wall / warrior-smith-of-kitava 两个 ninja build 即现成 fixture，golden 有 `EffectiveBlockChance`）；spirit golden（多数 build 有 `Spirit=100`）；deflect golden（`DeflectChance`/`DeflectionRating` 多为 0，verify 零值不误报）；ReservationEfficiency 单测。
- **commit 切分**：数据落库（parity 不变）→ 注入+聚合接线（行为，按子系统拆 commit：block / spirit / ward / deflection / efficiency 各一）。
- **门禁**：每个行为 commit 跑 ninja_parity，`Spirit`/`EffectiveBlockChance` 等新列尚未进 defensive_rows（W2/F 才扩列），故以专项断言（fixture 内直接对 golden 值断言 @5%）+ 旧基线不倒退双保险。
- **规模**：~900-1200 行（数据+代码+测试）。

### Track E：Evade 四分型 + Stun 体系 + 抗性细节（13-G9 / 13-G12 / 13-G13 选做；1 人 ~4-5 天）

- **目标**：把单值 evade 拆四分型并补 cap/flag；实现 StunThreshold/SelfStunChance/Duration 体系并修 ES 避晕条件；抗性细节按余力选做。
- **涉及文件**（独占写）：
  - `crates/pobr-core/src/calc/defence.rs` 的 **avoidance/evade 区段**（L350-540 及 `calc_avoidance` 尾段 stun 部分）：
    - `calc_evade_suite(db, cfg, evasion, enemy_accuracy, constants) -> EvadeSuite`：四分型（Melee/Projectile/Spell/SpellProjectile 各自独立 inc 乘区 :1396-1404）、`EvadeChance = 100 − (monsterHitChance − ΣBASE EvadeChance) × enemyHitMult`、`EvadeChanceMax`/cap `evade_chance_cap`(95)、`CannotEvade`/`AlwaysEvade` flag（:1421-1466）
    - 修 `avoid_stun` 的 ES 条件：`ES > totalTakenHit && !EnergyShieldProtectsMana` 才 ×0.5（:2554-2557）——`totalTakenHit` 在 F 接线前用单击参考伤害（与 EhpOptions reference_hit 同源）近似，F 接线后换真值；EB flag 从 Track C 的 `DefenceKeystones` 读（**接口依赖 C 的 commit 1**，纯类型依赖、可并行开发）
  - 新建 `crates/pobr-core/src/calc/stun.rs`：`StunThreshold`（基 Life/ES/Mana 词条切换 + AddESToStunThreshold）、`SelfStunChance = stun_base_mult × effHit/threshold`（物理 ×0.25 加权）、light/heavy 常量（game_constants 已有 `light_stun_*`/`heavy_stun_*` 全套）、时长按 `server_tick_seconds` 上取整（:2525-2643）
  - 选做（余力且不挤压 W2 窗口）：`offence.rs::resolve_resistance` 补 floor −200（`resist_floor` 常量已入库）与 INC 乘区（:819-941）——floor 是一行行为修复独立 commit。
  - `crates/pobr-core/src/calc/perform.rs`：新增 `fill_evade_stun(env, &keystones)` 一处调用。
  - `crates/pobr-core/tests/evade_stun.rs`（新）。
- **vendor 参照**：上列行号；常量对照 `data/4.5.0.3.4/base/game_constants.json`（已含 stun 全套与 evade cap，**勿再加常量**）。
- **测试与 fixture**：四分型 golden 对照（meta.json 有 `MeleeEvadeChance` 等全套键，huntress/ranger 系 build 是高 evasion 现成 fixture）；stun 阈值/几率单测（公式手算）；ES 避晕条件修复 fixture（修复前后对照）。
- **门禁**：evade 拆分与 stun 均为行为 commit（附行号依据）；旧 8 列基线不倒退。
- **规模**：~500-700 行。

### Track F：EHP 口径切换 + harness 扩列（13-G4 / 13-G5 / 13-G15 部分；串行收口，1 人 ~6-7 天；依赖 A、B 合并，读 C/D/E 的输出）

- **目标**：实现 `numberOfHitsToDie × 单击进伤` 的 PoB2 EHP 口径（P11），重算 max hit（池扩展层 + taken-as），接 not-hit/mitigation 概率层，扩 ninja_parity defensive_rows，完成阶段验收。
- **涉及文件**（独占写）：
  - `crates/pobr-core/src/calc/ehp.rs` **重构**：
    - 旧 `calc_ehp_with_opts` 的 lowest-max-hit 输出改挂 `total_ehp_lowest_max_hit`（保留为附加指标，roadmap 原文「旧 lowest-max-hit 保留为附加指标」）；
    - 新 `fn enemy_damage_in(monster_scaling, enemy_presets, level, tier, enemy_db) -> TypedDamage`：`monsterDamageTable[lv] × ehp_base_damage_mult(1.5) × tier dps_mult`，chaos ÷2.5（ConfigOptions.lua:1975-1996；数据全部已入库——monster_scaling.json `damage` 表 + enemy_presets.json）；per-type config 覆盖（`enemy<X>Damage` configInput）留 M3 config_interpreter，本阶段只用 placeholder 默认；
    - 新 `fn number_of_hits_to_die(damage_in: &TypedDamage, pools: &PoolState, ctx: &PoolCtx, constants) -> f64`：循环调 Track A 的 `reduce_pools`，含 GainWhenHit 恢复钩子（本阶段可置 0）、递归加速（`ehp_calc_speed_up`，loss-prevention 时 cap 4）、`ehp_calc_max_damage`/`max_iterations` 上限、overkill 小数折算（:2979-3145 逐行）；
    - 新 max hit：单击进伤经 Track B `taken_hit_from_damage` → TotalHitPool（life hit pool + MoM + ES(bypass) + ward/aegis/guard 的 poolProtected 扩展层，:3540-3601）；物理保留自洽迭代解（数学等价 quadratic，13-G5 修复方向认可）；
    - `not-hit/mitigation 层`：`ConfiguredNotHitChance`（四分型 NotHitChance 综合，读 Track E 输出 :2015-2037）、`ConfiguredDamageChance = blockEffect × deflectMulti × (1−notHit)`（PoE2 无 suppression，读 Track D 的 block_effect/deflect）、`NumberOfMitigatedDamagingHits`、`TotalEHP = mitigatedHits × totalEnemyDamageIn`（:3246-3247、:3322）。
  - `crates/pobr-core/src/calc/perform.rs`：`fill_mechanics` 的 EHP 段（L188-262）整体改写为新管线（构造 PoolState/PoolCtx → takenHit → hits-to-die → TotalEHP；`avoid_stun` 的 totalTakenHit 换真值）；recoup 基数换 `recoupable_by_type`（13-G15 部分，survivability 调用点参数替换）。
  - `crates/pobr-build/tests/ninja_parity.rs`：`defensive_rows` 扩列（新增：TotalEHP、Physical/Fire/Cold/Lightning/ChaosMaximumHitTaken、EffectiveBlockChance、EffectiveSpellBlockChance、Spirit、SpiritUnreserved、EvadeChance、MeleeEvadeChance、LifeUnreserved、ManaUnreserved、EnergyShieldRecoveryCap、PhysicalDamageReduction、DeflectChance——共 8→~24 列）+ `BASELINE_*` 重记。
  - `crates/pobr-core/tests/ehp_pob2.rs`（新）。
- **双跑纪律**（R2/R11 落点）：新旧口径**并存输出**（`total_ehp` 新语义 vs `total_ehp_lowest_max_hit` 旧值），切换 commit 前先出一份 18-build 新旧对照报告（`--nocapture` 跑 harness 打印）人工审查无量级异常，再切 `total_ehp` 字段语义。
- **commit 切分**（严格序）：
  1. ehp 新管线 + 新字段产出（`total_ehp` 仍旧口径，新值挂新字段）——parity 不变；
  2. 18-build 新旧对照报告 + 审查记录（commit message 附摘要）；
  3. **口径切换 + defensive_rows 扩列 + baseline 重记**（一个显式审查的独立 commit——roadmap 原文「EHP 口径切换的 baseline diff 显式审查」「baseline 更新独立 commit」）；
  4. recoup 基数替换（独立行为 commit）。
- **测试与 fixture**：MoM build（sorceress 系）、CI build（witch-abyssal-lich）、taken-as build（如有 Lightning Coil 系词条）、盾 block build（warrior 系）四类专项 fixture 对 golden `TotalEHP`/`*MaximumHitTaken` @5% 断言（roadmap 验收原文「MoM/CI/taken-as 类 fixture」）；`number_of_hits_to_die` 加速路径 vs 朴素逐击路径等价单测（小池暴力对照）。
- **门禁 = 阶段验收**：见 §4。
- **规模**：~900-1100 行。

---

## 3. 并行切分：文件归属表 / 接口契约 / 串并序

### 3.1 文件归属表（每文件唯一写者；「读」不限）

| 文件 | 归属 | 说明 |
|---|---|---|
| `pobr-core/src/mod_parser.rs` | **W0 owner** | W1 期间冻结；A–E 发现缺词条 → 提给 W0 owner 串行补（避免五方同改 61K 大文件） |
| `pobr-core/src/calc/output.rs`、`display_catalog.rs` | **W0 owner** | W0 后冻结；F 的字段语义切换是唯一例外（W2 串行期） |
| `pobr-core/src/calc/pool_damage.rs` | W0 定契约 → **A** 实现 | F 只读 |
| `pobr-core/src/calc/pool_setup.rs`（新） | **A** | |
| `pobr-core/src/calc/taken.rs`（新） | **B** | |
| `pobr-core/src/rules/keystone_registry.rs`（新）、`rules/mod.rs` | **C** | |
| `pobr-core/src/calc/defence.rs` | **C**（L84-260 聚合/转换段）+ **E**（L350-540 avoidance/evade 段） | 函数级分区不重叠；合并顺序 C 先 E 后；新函数各自追加在所属区段尾 |
| `pobr-core/src/calc/defence_panels.rs`（新） | **D** | |
| `pobr-core/src/calc/stun.rs`（新） | **E** | |
| `pobr-core/src/calc/survivability.rs`、`skill_mechanics.rs`（spirit 段） | **D** | |
| `pobr-core/src/calc/ehp.rs` | **F**（W2 才动） | W1 期间冻结 |
| `pobr-core/src/calc/perform.rs` | **集成共享**，见 3.2 协议 | |
| `pobr-data/src/catalog/items.rs`、adapter、`pipeline/config.json`、`data/<ver>/` | **D** | game_constants 例外归 W0 |
| `pobr-data/src/catalog/game_constants.rs` + 数据 + gamedata 测试 | **W0 owner** | |
| `pobr-build/src/calc_orchestrator.rs`（装备注入段） | **D** | |
| `pobr-build/tests/ninja_parity.rs` | **F** | W1 期间只读（基线数字任何人不得动） |
| 各 track 新测试文件 | 各自 | 命名互斥：`pool_damage.rs`/`taken_as.rs`/`keystone_defence.rs`/`defence_panels.rs`/`evade_stun.rs`/`ehp_pob2.rs` |

### 3.2 perform.rs 合并协议（最热共享文件）

- 每个 track **不在 perform.rs 内写逻辑**，只暴露自模块的 `fill_xxx(env, …)` 纯编排子函数 + 在 `fill_mechanics` 加**一行调用**（或替换一个既有装配块）。
- 改动点预登记：C（keystone 构造 + L217 一行）、B（L216-231 EhpOptions 装配块替换）、D（`fill_defence_panels` 一行）、E（`fill_evade_stun` 一行 + avoid_stun 调用点参数）、F（EHP 段 L188-262 整体改写，W2 串行期独占）。
- **合并顺序**：C → E → D → B →（W2）F。每次合并后下游 rebase；顺序依据：C 的 `DefenceKeystones` 是 E/D/B 的入参类型。

### 3.3 接口契约（track 间唯一耦合面）

1. `pool_damage::{PoolState, PoolCtx, TypedDamage, PoolsAfter, reduce_pools, pool_protected}`（W0.3 锁定）——A 实现，F 消费。
2. `keystone_registry::DefenceKeystones::from_db`（C 的 commit 1，开工 48h 内先合并类型）——E/D/B/F 以参数形式消费，**禁止**各 track 自行散读 keystone flag。
3. `taken::{damage_shift_table, taken_hit_from_damage, MitigationCtx}`——B 实现，F 消费。
4. W0.1 词条覆盖表（ModName 字符串即契约；各 track 查询用名以该测试为准）。
5. W0.2 OutputTable 字段名（D/E 写入、F 与 harness 读取）。

### 3.4 串并序总览

```
W0.1-W0.4（串行，单 worktree） ──┬─→ A（pool_damage/pool_setup）────┐
                                ├─→ B（taken.rs）                  │
                                ├─→ C（keystone+转换矩阵；commit1 先行）│→ F（EHP 口径+扩列+验收，串行）
                                ├─→ D（block/spirit/ward/deflect） │
                                └─→ E（evade/stun；类型依赖 C-1）   ┘
```

F 的硬依赖：A、B 全量 + C 的 commit 1/2；D/E 的输出经 OutputTable 字段解耦（F 开工时若 D/E 未全合，对应 mitigation 因子按字段默认 0→中性 1.0 处理，合入后自动生效——not-hit/block 层的 fixture 断言放在 D/E 合并之后跑）。

---

## 4. 门禁与验收

### 4.1 每 track 局部门禁（合并回集成分支的前置）

1. `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` 全绿；
2. ninja_parity 四个 `BASELINE_*` **不得倒退**（roadmap §0：「防御 51% / 进攻 24%（@5% 容差）为底线不得倒退」——对应当前基线常数 DEF_HIT5=111 / DEF_HIT10=117 / OFF_HIT5=23 / OFF_HIT10=31）；
3. 纯数据/纯库 commit（W0 全部、A 全部、C-1、D 数据落库）：**parity 逐值不变**——跑 `golden_regression` + ninja_parity 输出 diff 为零；
4. 行为修复 commit：message 附 CalcDefence.lua/ModParser.lua 行号依据（必要时 pob2-oracle 中间值），baseline 若上调则**独立 commit 显式审查**；
5. 涉及 data/ 的 commit：`devs/scripts/regen-check.sh` 绿（可再生性 byte-diff）；
6. 新机制必须带「PoB2 行号注释 + 手算期望值」的公式单测（本仓库既有惯例，见 ehp.rs/defence.rs 文档注释风格）。

### 4.2 阶段整体验收（F 的 commit 3 之后）

引 roadmap M2 节原文：「**防御 parity 51% → ≥80%@5%；MoM/CI/taken-as 类 fixture；EHP 口径切换的 baseline diff 显式审查**」。落成可执行口径：

1. defensive_rows 扩列（8→~24 列）后，ninja_parity 防御 hit5 比例 **≥80%**（分母 = 扩列后 golden 可比项总数；见 §6 开放问题 3 的裁决前提）；旧 8 列子集的命中数不低于 111（防止「扩列稀释」掩盖回退）；
2. 进攻 hit5 不低于现基线（M2 不动进攻，OFF_HIT5 ≥23 纯防回归）；
3. MoM / CI / taken-as / 盾 block 四类专项 fixture 全绿（@5% 对 golden `TotalEHP`/`*MaximumHitTaken`/`EffectiveBlockChance`）；
4. EHP 口径切换的 18-build 新旧对照报告入库（commit 3 message 或 `audits/` 附件），reviewer 签字式审查；
5. `cargo bench -p pobr-core --bench mod_db_bench` 无明显回退（EHP 循环引入 ≤50×5 次聚合查询，热路径在 perform 单次，预期无影响；若 harness 总时长 >2× 需查 reduce_pools 的分配）。

---

## 5. 风险与回退（风险登记簿在 M2 的落点）

| 风险 | 落点 | 缓解（已写入上文流程） | 回退 |
|---|---|---|---|
| **R2** 「理论正确」重构破坏隐藏补偿 | F 的 EHP 切换最大；B 的 [bool;3]→百分比次之 | 新旧口径并存双跑 + 18-build 对照报告先审后切；行为/搬迁分 commit | `total_ehp` 语义切换是单 commit，revert 即回旧口径（旧实现保留在 `total_ehp_lowest_max_hit` 路径，不删码） |
| **R11** 零回归 vs 提升的张力 | C-2（CI）、C-3（矩阵）、D 各子系统、E、F 都改输出 | 每个行为 commit 附一手依据 + 单独跑 harness；baseline 只升不降、升档独立 commit | 逐 commit revert 粒度 |
| 扣池状态机 vs 纯函数约定的张力（roadmap M2 风险原文） | Track A | 「状态机封装为局部纯函数 `fn reduce_pools(pools, hit) -> PoolsAfter`，不写 Env」——契约在 W0.3 以类型锁死；review checklist 检查 pool_damage.rs 无 `&mut Env`/无 ModDb 读取（整备与求值分离） | — |
| **P17** 双 pass 诱惑（roadmap 原文「禁止顺手改归因结构」） | F 实现 takenHit per-type 拆解时最易越界 | §0 约束 1；EHP 段 trace 仅沿用既有 Mitigate/Clamp 节点；review checklist 增「无新 TraceOperation/SourceKind」检查 | — |
| 多 track 共享 perform.rs/defence.rs 冲突 | C/D/E/B | §3.2 一行调用协议 + 预登记改动点 + 固定合并顺序 | 冲突时以归属表裁决，非 owner 改动回滚重提 |
| ShieldTypes/spirit 数据源不可得 | Track D | 双路线（.dat 优先、extract-lua overlay 兜底），蓝图已给兜底通道（M0 skill_overrides 同构） | block/spirit 基底走 overlay；不阻塞其余子系统 |
| harness 扩列稀释/夸大 parity 口径 | F | 双指标：扩列后 ≥80% **且** 旧 8 列子集 ≥111；扩列与口径切换同一显式审查 commit | baseline 常数 revert 即回旧口径 |
| W3 并发改 calc 常量签名 | 全 track | M2 只经 `cfg.constants` 访问；与 W3 的冲突面收敛到 config.rs（双方都不重写该文件结构）；开工时若 RuleSet 形态落地，做一次机械替换 | — |
| EHP 循环性能（递归加速实现错误 → 50 次迭代上限内不收敛） | F | 加速路径 vs 朴素路径等价单测；`ehp_calc_max_iterations` 硬上限保证终止 | 禁用加速（speed_up=1）仍正确，仅慢 |

---

## 6. 实施前仍需裁决的开放问题

1. **ShieldTypes .dat 表**：pipeline 下载索引是否含该表、Block 列确切列名未验证（当前 `pipeline/config.json` 无此表）。Track D 开工第一天验证；不可得即走 overlay 兜底（蓝图已给通道），无需回头改蓝图。
2. **spirit 基底的 .dat 来源列**：vendor `Data/Bases/sceptre.lua` 有 `spirit=100`，对应 .dat 列未确认（可能在 BaseItemTypes 扩展列或独立表）。同上双路线。
3. **parity ≥80% 的分母口径**：建议「扩列后口径 ≥80% + 旧 8 列子集 ≥111 双指标」（§4.2），与 roadmap 附 B 的 80% 目标语义需 reviewer 确认（扩列使分母从 ~144 增至 ~430，80% 比旧口径严格得多；若判定过严，备选口径=「max-hit/EHP 新列单独 ≥70% + 旧列 ≥90%」，裁决后只改 §4.2 数字不影响实施）。
4. **`total_ehp` 字段语义切换对下游消费方**（CLI `calculate` 输出 / wasm `calculate_json` / display_catalog）：是否需要一个过渡期双字段输出（`total_ehp` + `total_ehp_lowest_max_hit` 本蓝图已并存，问题仅在于对外文档与 i18n 文案何时切）。
5. **W3 收口形态**：若 `GameData::load_ruleset()` 在 M2 开工前从 Option 骨架升级为实包，C/D/E/F 的常量访问按其最终 API 微调（机械替换，不改公式）。

---

## 附：commit 计划摘要（供编排脚本排程）

| 序 | 内容 | 性质 | parity 预期 |
|---|---|---|---|
| W0.1-W0.4 | 词条/字段/契约/常量（4 commit） | 纯增量 | 逐值不变 |
| C-1 | keystone_registry 类型+测试 | 纯库 | 逐值不变 |
| A-1..n | reduce_pools 实现+整备+fixture | 纯库 | 逐值不变 |
| B-1 | taken.rs 实现+fixture | 纯库 | 逐值不变 |
| B-2 | perform 的 ArmourAppliesTo 模型替换 | **行为** | 局部修正，附依据 |
| C-2 | CI 一行接线 | **行为** | CI build 修正，baseline 审查 |
| C-3 | 转换矩阵+翻倍 flag 接线 | **行为** | keystone build 修正 |
| D-1 | block/spirit 数据落库（schema+adapter/overlay+data） | 数据 | 逐值不变 |
| D-2..6 | block/spirit/ward/deflect/efficiency 接线 | **行为**×5 | 各子系统修正 |
| E-1 | evade 四分型 | **行为** | evade build 修正 |
| E-2 | stun 体系 + ES 条件修复 | **行为** | 局部 |
| E-3（选做） | resist floor −200 / INC | **行为** | 局部 |
| F-1 | EHP 新管线并行产出（不切口径） | 纯增量 | 逐值不变 |
| F-2 | 18-build 新旧对照报告 | 文档 | — |
| F-3 | **口径切换 + defensive_rows 扩列 + baseline 重记** | **行为（显式审查）** | 防御 ≥80%@5% |
| F-4 | recoup 基数替换 | **行为** | 局部 |
