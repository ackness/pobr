# M4-T3 → T2 接线说明（乘区与 DPS 末端）

> T3 产出 = 独立模块 + 冻结签名（蓝图 `m4-offence-deep.md` §2-T3 / §3.3 契约 2/3）。
> **offence.rs / crit_pass 的全部接线由 T2 执行**（§3.2 文件归属）；本文是接线 diff 说明。
> T3 落地 commit：`feat(m4-t3): W-C4+W-C1 ...`（scaled_damage.rs）、`feat(m4-t3): W-C2+W-C3 ...`（damage.rs）。

## 1. 契约函数最终签名与口径

### 契约 2 — `scaled_damage_effect`（W-C1，`pobr-core/src/calc/scaled_damage.rs`）

```rust
pub fn scaled_damage_effect(
    db: &ModDb,          // 玩家（技能）modDB
    enemy_db: &ModDb,    // 敌方 modDB（SelfDouble/TripleDamageChance，仅 mode_effective）
    cfg: &CalcConfig,    // per-pass skillCfg（含 mode_effective）
    crit_chance: f64,    // ⚠ 分数 0..=1（= resolve_crit().chance；vendor CritChance/100）
) -> ScaledDamage { effect, double_chance, triple_chance }
```

- `effect` = `1 + DD/100 + 2×TD/100`（= vendor `output.ScaledDamageEffect`）。
- `double_chance` / `triple_chance` = **百分比 0..=100**（与 vendor `output.DoubleDamageChance`
  / `TripleDamageChance` 同位；`double_chance` 已做 Triple 抵扣）。
- vendor 对照 `CalcOffence.lua:3840-3861`。**口径决策**：vendor `:3845`（Triple 行）的
  `Sum(...) or 0 + (...)` 是 Lua 优先级笔误（实际丢掉敌方/OnCrit 两项），本实现按蓝图
  §2 W-C1 意图语义（与 Double 行 `:3849` 同构）。oracle 对拍用例不触发该差异
  （TripleDamageChanceOnCrit=0）。
- TODO 已登记（源码注释）：W-A3 globalLimit（DOUBLED form 词条，Sum 侧生效后本函数零改动）；
  Intimidate / `IntimidatingUpTimeRatio`（warcry M5+，`:3850-3854` 分支整段跳过）。

### 契约 3 — `apply_can_deal`（W-C3，`pobr-core/src/calc/damage.rs`）

```rust
pub fn apply_can_deal(components: &mut [DamageComponent], db: &ModDb, cfg: &CalcConfig)
```

- 蓝图契约写作 `&mut Vec<DamageComponent>`；落地为 `&mut [DamageComponent]`
  （clippy `ptr_arg`）。**调用形状不变**：`apply_can_deal(&mut components_vec, db, cfg)`
  经 deref 直接编译。
- 语义：`canDeal[type] = !Flag("DealNo"+Type) && !Flag("DealNoDamage")`
  （`CalcOffence.lua:2226-2230`）；不能造成的类型分量**就地清零**（min/max→0，分量保留，
  下游求和等价 vendor 跳过）。
- **顺序关键**：必须在转换链之后调用——清零的是转换后残留（Avatar of Fire：残留物理清零、
  已转火焰保留）。T4 的技能 DoT（`:5451`）与 T2 的 hit（`:3989`）/ ailment（`:4793`）共用。

### 附属（非冻结契约，T2/T4 消费）

```rust
// W-C2（damage.rs）
impl DamageComponent { pub fn avg_with_lucky(&self, lucky_chance: f64) -> f64 }  // p 分数 0..=1
pub fn lucky_hit_chance(db, cfg, damage_type, is_crit_pass: bool) -> f64          // 返回分数 0..=1

// W-C1 allMult 占位（scaled_damage.rs）
pub struct AllMultExtras { fist_of_war, ancestral_call, ancestral_empowerment,
                           ancestral_empowerment_combined, offensive_warcry }     // Default = 全 1.0
pub fn all_mult(scaled: &ScaledDamage, extras: &AllMultExtras) -> f64

// W-C4（scaled_damage.rs）
pub struct DpsEndFactors { dps_multiplier: f64, quantity_multiplier: f64 }
pub fn dps_end_factors(db, cfg, skill_dps_multiplier: Option<f64>) -> DpsEndFactors
```

全部经 `pobr_core::calc::{...}` re-export。

## 2. 接线点（T2 执行；vendor 行号为锚）

### W-C1 → crit_pass / all_mult（vendor `:4023-4032`）

- vendor 在**每个 hand pass** 内、crit 内层循环之前算一次 `ScaledDamageEffect`
  （`:3840-3861`，用该 pass 的 `output.CritChance`），随后 crit / non-crit 两腿共用：
  `allMult = ScaledDamageEffect × 五因子`；仅 crit 腿（pass==1）额外 `× CritMultiplier`。
- 接线：T2 在 hand pass 内调
  `let scaled = scaled_damage_effect(db, enemy_db, &pass_cfg, crit.chance);`
  再 `let mult = all_mult(&scaled, &AllMultExtras::default());`（M4 范围 extras 恒缺省）
  乘入 W-B3 的 `all_mult` 入参（替换现 stub 常量 1.0）。
- 注意 `crit_chance` 是**分数**（直接传 `resolve_crit(...).chance`，不要再 /100）。
- 现单 pass 模型（W-B3 之前）的等价写法：DD/TD 对两腿同乘 → `total_hit_avg ×= scaled.effect`
  即可（数学上与双腿各乘后 blend 相同）。
- 输出回填（如需 parity 面板）：`double_chance`/`triple_chance` 即 vendor
  `DoubleDamageChance`/`TripleDamageChance`（百分比）。

### W-C2 → crit_pass 内 hit avg（vendor `:4035-4046`）

- vendor 顺序：min/max 先 ×allMult（`:4033-4034`），**然后**按 (pass, damageType) 求 lucky
  几率、用 lucky 公式折 avg（`:4036-4046`）。
- 接线：在 crit_pass 内对每个分量
  `let p = lucky_hit_chance(db, &pass_cfg, comp.damage_type, is_crit_pass);`
  `let avg = comp.avg_with_lucky(p);`（替换 `comp.avg()` 的调用点）。
- 无 lucky 词条时 `p=0`、`avg_with_lucky(0) == avg()` 逐位一致（零回归锚点，已有单测钉住）。

### W-C3 → 转换链末尾（vendor 消费点 `:3989` hit / `:4793` ailment / `:5451` DoT）

- 接线（hit 侧）：`calculate_components(...)` 产出分量向量之后、求和/暴击 blend 之前：
  `apply_can_deal(&mut damage_components, db, cfg);`
  （offence.rs `calculate_minimal_vs_enemy` 中 `let damage_components = ...` 的下一行）。
- T4 DoT 侧：对 `skillData[type.."Dot"]` 基值逐类型同函数门控（构造单分量后调用，或按
  flag 直接判 `DealNo<Type>`——函数语义等价）。
- ailment 侧（T4 ailment.rs 归属）：magnitude 源分量同函数复用。

### W-C4 → DPS 末端（vendor `:3128-3130` / `:3863` / `:4407`）

- 接线点 = offence.rs `calculate_minimal_vs_enemy` 的
  `let dps = round(total_hit_avg_for_dps * action_rate * hit_chance);`（现 L302）一行带：

  ```rust
  let end = dps_end_factors(db, cfg, skill_dps_multiplier);
  let dps = round(total_hit_avg_for_dps * action_rate * hit_chance
      * end.dps_multiplier * end.quantity_multiplier);
  ```

  traced 路径（`total_dps_traced`）同口径补两个 Multiply 节点。
- `skill_dps_multiplier`：来自技能数据 `dpsMultiplier`（T4 给 catalog
  `SkillStatSetDef`/`SkillLevelDef` 加 `dps_multiplier: Option<f64>` + orchestrator 透传）。
  **T4 落地前传 `None`**（= vendor `skillData.dpsMultiplier or 1`，行为不变）。
- `"DPS"` ModName 的 inc/more 已在 `dps_multiplier` 内折算（`calcLib.mod`，
  CalcTools.lua:16-18），T2 勿重复消费。
- `QuantityMultiplier` 现实来源是 mirage 类编排注入（vendor CalcMirages `NewMod`）——
  普通 build 无词条 → floor 1.0，逐值不变。
- 技能 DoT 末端（W-D1 `:5931`）消费**同一对值**——T4 直接调 `dps_end_factors`。

## 3. 零回归论证（接线前）

- 模块未接线：本说明对应的两个 commit 不触碰 offence.rs/perform.rs/orchestrator，
  ninja_parity 逐值不变天然成立（workspace 1695 测试全绿）。
- 接线后零回归锚点（均有单测钉住）：无 DD/TD 词条 `effect == 1.0`；`avg_with_lucky(0) == avg()`；
  无 DealNo 旗标分量逐位不变；无 DPS/Quantity 词条且 `skill_dps_multiplier=None` 时两因子均 1.0。

## 4. oracle 对拍记录（复现步骤）

- `tools/pob2-oracle/oracle.lua` 已扩展（同 commit）：新增 dump `mainHandOutput` /
  `offHandOutput`（= `mainOutput.MainHand/OffHand` 标量；DD/TD/ScaledDamageEffect/per-pass
  CritChance 在此）与 `skillInfo.dpsMultiplier`。
- 用例：monk-martial-artist-flicker-strike 的 decoded.xml，在 Sapphire Ring（Item id 8）
  词条尾部加三行：
  `25% chance to deal Double Damage` / `10% chance to deal Triple Damage` /
  `Your Critical Hits have a 50% chance to deal Double Damage`，跑 `run.sh`。
- oracle（vendor 0.18.0，2026-06-12）：`CritChance=83.6784`、`DoubleDamageChance=60.15528`、
  `TripleDamageChance=10`、`ScaledDamageEffect=1.8015528`——Rust 侧
  `oracle_parity_flicker_strike_with_dd_mods` 逐位命中（1e-6）。
- DPS 末端恒等式（同 build 原始词条）：`TotalDPS = AverageDamage × Speed × 1 × 1` 闭合
  （`oracle_identity_dps_end_factors_default_to_one`）。

## 5. 禁动遵守

T3 两个 commit 仅改：`calc/scaled_damage.rs`（新）、`calc/damage.rs`、`calc/mod.rs`
（一行级 re-export）、`tests/{scaled_damage,lucky_can_deal}.rs`（新）、
`tools/pob2-oracle/oracle.lua`（纯增量诊断面，未列归属文件，特此说明）。
offence.rs / mod_db.rs / modifier.rs / trace.rs / trigger.rs / calc_orchestrator.rs /
skill_use_time.rs 零触碰。
