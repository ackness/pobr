//! 技能 DoT（damage over time）计算模块——M4-T4 W-D1（蓝图
//! `audits/rearchitecture-2026-06-10/blueprints/m4-offence-deep.md` §2-T4）。
//!
//! vendor 参照 `CalcOffence.lua`（逐行亲验）：
//! - dotCfg（`:5832-5856`）：`flags = ModFlag.Dot | skillCfg.flags`，再按
//!   dotIs* 五布尔**剥**对应位（无该布尔则去掉 Area/Projectile/Spell/Attack/Hit
//!   位）；`keywordFlags &= ~KeywordFlag.Hit`（`:5838`）。dotIs* 数据来源两路
//!   合一为 `DotIs<X>` FLAG modifier：
//!   1. stat 驱动（`skill_stat_map` skill_data kind，如
//!      `spell_damage_modifiers_apply_to_skill_dot` → dotIsSpell）——
//!      `rules::stat_map_engine::collect_skill_data` 译出；
//!   2. statSet baseMods 直挂布尔（[`pobr_data::catalog::DotFlags`]，4.5.0.3.4
//!      vendor 全量仅 TornadoShot 一处）——编排层按选中 statSet 注入。
//!      未注入（数据缺失/保守默认）= 全剥，对齐蓝图 §5 风险行的保守口径。
//! - 逐类型（`:5870-5929`）：`baseVal = skillData[type.."Dot"]`（canDeal 门控，
//!   W-C3 同源 flag 口径）；`total = baseVal × (1+inc/100) × more ×
//!   (1 + (Override(DotMultiplier) or Sum(DotMultiplier)+Sum(<type>DotMultiplier))/100)
//!   × aura × effMult`；`TotalDotInstance` 累计 clamp `DotDpsCap`。
//!   `<Type>Dot` BASE 经 statmap `base_<type>_damage_to_deal_per_minute / 60`
//!   注入 ModDb（W-D1 数据通道）。aura 因子：pobr 无 aura-DoT 消费 build，
//!   按 1.0 跳过（TODO(M5+ aura-dot)：接 `AuraEffect × Magnitude`）。
//! - `DotCanStack`（`:5931`）：`TotalDot = min(instance × speed × Duration ×
//!   dpsMultiplier × quantityMultiplier, DotDpsCap)`；速率按 keywordFlags
//!   Mine/Trap 换 `MineLayingSpeed/TrapThrowingSpeed`——pobr 无图腾/陷阱吞吐
//!   （12-G11，M4 不做），KeywordFlags 暂无 Mine/Trap 位，留分支注释 + 退
//!   Speed。`duration == 0`（编排层未接 skill duration）时保守退化为
//!   instance（不放大）。dotIsBurningGround/CausticGround/CorruptingBlood
//!   三分支（`:5947-5967`）依赖 ground-dot 旗标数据，未建模——走 else 分支
//!   `TotalDot = TotalDotInstance`（`:5971-5972`）。
//! - 末端合并族（`:6093-6234`）：`WithDotDPS = baseDPS + TotalDot`（仅
//!   skillFlags.dot 时）；`TotalDotDPS = Σ(TotalDot + poison + caustic +
//!   ignite + burning + bleed + corrupting + decay)` clamp `DotDpsCap`
//!   （pobr 现值 = 异常侧 bleed/poison/ignite 三项，caustic/burning ground 与
//!   decay 未建模恒 0）；`CombinedDPS = baseDPS + TotalDotDPS`（culling /
//!   reservation 乘子未建模，按 1.0——vendor `:6238-6241`）。
//!
//! 字段命名契约（蓝图 §3.3 条目 6，合入 display_catalog 后不再改）：
//! `skill_dot_instance` / `skill_total_dot` / `total_dot_dps` /
//! `with_dot_dps` / `combined_dps`。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::damage::{DAMAGE_TYPES, aggregate_inc_more};
use super::output::OutputTable;
use super::round;
use super::scaled_damage::dps_end_factors;

/// 技能 DoT 计算结果（OutputTable `// === M4-T4 ===` 区块的来源值）。
///
/// 全零 = 中性（无技能 DoT 且无异常 DoT）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillDotOutput {
    /// 单实例技能 DoT DPS（PoB2 `TotalDotInstance`，clamp `DotDpsCap`）。
    pub skill_dot_instance: f64,
    /// 可叠加实例累计后的技能 DoT DPS（PoB2 `TotalDot`；不可叠加时 = instance）。
    pub skill_total_dot: f64,
    /// 全部 DoT 来源合计 DPS（PoB2 `TotalDotDPS` = 技能 dot + poison/caustic/
    /// ignite/burning/bleed/corrupting/decay，clamp `DotDpsCap`）。
    pub total_dot_dps: f64,
    /// 击中 DPS + DoT（PoB2 `WithDotDPS`；无技能 DoT 时维持 0 中性）。
    pub with_dot_dps: f64,
    /// 综合 DPS（PoB2 `CombinedDPS`）。
    pub combined_dps: f64,
}

/// 技能 DoT 计算的面板输入（perform fill 段从既有 [`OutputTable`] 读出）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillDotInputs {
    /// 技能有效速率（actions/s，服务器帧 cap 后；PoB2 `output.Speed`——
    /// DotCanStack 分支的实例生成速率）。
    pub speed: f64,
    /// 技能持续时间（秒；PoB2 `output.Duration`）。0 = 编排层未接 duration，
    /// DotCanStack 分支保守退化为单实例。
    pub duration: f64,
    /// 击中 DPS（PoB2 `baseDPS = output.TotalDPS`，合并族基底）。
    pub base_dps: f64,
    /// 流血 DPS（PoB2 `TotalBleedDPS or BleedDPS`——叠层值优先）。
    pub bleed_dps: f64,
    /// 中毒 DPS（同上口径）。
    pub poison_dps: f64,
    /// 点燃 DPS（同上口径）。
    pub ignite_dps: f64,
}

/// dotIs* 五布尔（dotCfg flag 保留/剥除开关；vendor skillData `dotIs*`）。
///
/// 数据通道见模块文档——两路最终都落 `DotIs<X>` FLAG modifier，本结构从
/// ModDb 读出（缺省全 false = 全剥，保守）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DotIsFlags {
    pub area: bool,
    pub projectile: bool,
    pub spell: bool,
    pub attack: bool,
    pub hit: bool,
}

impl DotIsFlags {
    /// 从 ModDb 读取 `DotIs*` FLAG（stat 驱动 + statSet 旗标注入的合一消费点）。
    pub fn from_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        Self {
            area: db.flag(cfg, ModName::from("DotIsArea")),
            projectile: db.flag(cfg, ModName::from("DotIsProjectile")),
            spell: db.flag(cfg, ModName::from("DotIsSpell")),
            attack: db.flag(cfg, ModName::from("DotIsAttack")),
            hit: db.flag(cfg, ModName::from("DotIsHit")),
        }
    }
}

/// 构造 dotCfg（PoB2 `CalcOffence.lua:5832-5856`）：置 Dot 位 + 按 dotIs* 剥
/// Area/Projectile/Spell/Attack/Hit 位 + keyword 去 Hit。
///
/// （历史：实施期按 `modflags-pob2` feature 双态编写；M4-i1 已切换 PoB2 全位表
/// 常驻并删除该 feature，合并适配时去门控——Dot/Hit 位逻辑常驻，与 vendor
/// 位表逐位一致。）
pub fn dot_config(cfg: &CalcConfig, dot_is: DotIsFlags) -> CalcConfig {
    let mut flags = cfg.flags | ModFlags::DOT;
    if !dot_is.area {
        flags = flags.without(ModFlags::AREA);
    }
    if !dot_is.projectile {
        flags = flags.without(ModFlags::PROJECTILE);
    }
    if !dot_is.spell {
        flags = flags.without(ModFlags::SPELL);
    }
    if !dot_is.attack {
        flags = flags.without(ModFlags::ATTACK);
    }
    if !dot_is.hit {
        flags = flags.without(ModFlags::HIT);
    }
    let keyword_flags = cfg.keyword_flags.without(KeywordFlags::HIT);
    cfg.clone()
        .with_flags(flags)
        .with_keyword_flags(keyword_flags)
}

/// `DamageType` → `<Type>Dot` 的 KeywordFlag 位（vendor `KeywordFlag[type.."Dot"]`，
/// `:5872`）。
fn dot_keyword(damage_type: DamageType) -> KeywordFlags {
    match damage_type {
        DamageType::Physical => KeywordFlags::PHYSICAL_DOT,
        DamageType::Lightning => KeywordFlags::LIGHTNING_DOT,
        DamageType::Cold => KeywordFlags::COLD_DOT,
        DamageType::Fire => KeywordFlags::FIRE_DOT,
        DamageType::Chaos => KeywordFlags::CHAOS_DOT,
    }
}

/// `DamageType` → ModName 前缀。
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// 技能 DoT 的敌方 effMult（vendor `:5878-5890`）：
/// `(1 - resist/100) × (1 + takenInc/100) × takenMore`。
///
/// - taken 链 = `DamageTaken` / `DamageTakenOverTime` / `<Type>DamageTaken` /
///   `<Type>DamageTakenOverTime`（元素加 `ElementalDamageTaken`）；
/// - 物理走 `min(enemy PhysicalDamageReduction, EnemyPhysicalDamageReductionCap)`
///   下限 0（`:5882`，与异常侧"物理抗性=0"口径不同——技能物理 DoT 吃敌方
///   面板物理减伤）；其余类型读 `<Type>Resist` clamp 抗性区间；
/// - 仅 `mode_effective` 生效（面板口径恒 1.0）。
fn skill_dot_eff_mult(enemy: &ModDb, taken_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    if !taken_cfg.mode_effective {
        return 1.0;
    }
    let type_cfg = taken_cfg.clone().with_damage_type(damage_type);
    let prefix = type_prefix(damage_type);
    let mut taken_names = vec![
        ModName::from("DamageTaken"),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{prefix}DamageTaken")),
        ModName::from(format!("{prefix}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        taken_names.push(ModName::from("ElementalDamageTaken"));
    }
    let taken_inc = enemy.sum(ModType::Inc, &type_cfg, &taken_names);
    let taken_more = enemy.more(&type_cfg, &taken_names);

    let resist = if damage_type == DamageType::Physical {
        enemy
            .sum(
                ModType::Base,
                &type_cfg,
                &[ModName::from("PhysicalDamageReduction")],
            )
            .clamp(
                0.0,
                type_cfg
                    .constants
                    .monster()
                    .maximum_physical_damage_reduction_pct,
            )
    } else {
        enemy
            .sum(
                ModType::Base,
                &type_cfg,
                &[ModName::from(format!("{prefix}Resist"))],
            )
            .clamp(
                type_cfg.constants.game().resist_floor,
                pobr_data::monster::ENEMY_MAX_RESIST,
            )
    };
    (1.0 - resist / 100.0) * (1.0 + taken_inc / 100.0) * taken_more
}

/// 技能 DoT 主计算（vendor `CalcOffence.lua:5831-5973` + `:6093-6234` 末端合并）。
///
/// `db`/`enemy` = 玩家/敌方 ModDb；`cfg` = 主技能 skillCfg；`inputs` 为面板既有
/// 输出（speed/duration/hit DPS/异常 DoT 三值）。纯函数、无副作用——perform fill
/// 段经 [`fill_skill_dot`] 落表。
pub fn calc_skill_dot(
    db: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    inputs: &SkillDotInputs,
) -> SkillDotOutput {
    let cap = cfg.constants.game().dot_dps_cap;
    let dot_is = DotIsFlags::from_db(db, cfg);
    let dot_cfg = dot_config(cfg, dot_is);
    // spell_damage_modifiers_apply_to_skill_dot 不作用于敌方 taken（`:5859-5862`）：
    // dotIsSpell 时 taken 侧仍剥 Spell 位。
    let taken_cfg = if dot_is.spell {
        dot_cfg
            .clone()
            .with_flags(dot_cfg.flags.without(ModFlags::SPELL))
    } else {
        dot_cfg.clone()
    };

    let deal_no_damage = db.flag(&dot_cfg, ModName::from("DealNoDamage"));
    let mut instance = 0.0_f64;
    let mut dot_active = false;
    for damage_type in DAMAGE_TYPES {
        let prefix = type_prefix(damage_type);
        let dot_type_cfg = dot_cfg
            .clone()
            .with_damage_type(damage_type)
            .with_keyword_flags(dot_cfg.keyword_flags | dot_keyword(damage_type));
        // canDeal 门控（W-C3 契约 3 同源 flag：`DealNoDamage` / `DealNo<Type>`）。
        let can_deal =
            !deal_no_damage && !db.flag(&dot_type_cfg, ModName::from(format!("DealNo{prefix}")));
        if !can_deal {
            continue;
        }
        let base_val = db.sum(
            ModType::Base,
            &dot_type_cfg,
            &[ModName::from(format!("{prefix}Dot"))],
        );
        if base_val <= 0.0 {
            continue;
        }
        dot_active = true;
        let eff_mult = skill_dot_eff_mult(enemy, &taken_cfg, damage_type);
        // inc/more 桶 = `Damage` + `<Type>Damage`（元素加 `ElementalDamage`）+
        // 按 dotCfg flag 选入的类别桶（SpellDamage/AreaDamage…）——pobr 用
        // ModName 桶表达 vendor 的 flag 匹配语义（`damage::aggregate_inc_more`，
        // hit 管线同款聚合，flag 剥除后类别桶自然脱落）。
        let (inc, more) = aggregate_inc_more(db, &dot_type_cfg, damage_type);
        // DotMultiplier：Override 优先，否则 Sum(DotMultiplier) + Sum(<Type>DotMultiplier)
        // （`:5897`）。
        let mult = db
            .override_(&dot_type_cfg, ModName::from("DotMultiplier"))
            .unwrap_or_else(|| {
                db.sum(
                    ModType::Base,
                    &dot_type_cfg,
                    &[ModName::from("DotMultiplier")],
                ) + db.sum(
                    ModType::Base,
                    &dot_type_cfg,
                    &[ModName::from(format!("{prefix}DotMultiplier"))],
                )
            });
        // aura 因子（`:5898`）：aura-DoT 未建模，按 1.0（TODO(M5+ aura-dot)）。
        let total = base_val * (1.0 + inc / 100.0) * more * (1.0 + mult / 100.0) * eff_mult;
        instance = (instance + total).min(cap);
    }

    // TotalDot（`:5931-5973`）：DotCanStack 走 instance × speed × duration ×
    // dpsMultiplier × quantityMultiplier；ground-dot 三旗标未建模；其余 = instance。
    let total_dot = if db.flag(cfg, ModName::from("DotCanStack")) && inputs.duration > 0.0 {
        // 速率分支（`:5934-5940`）：keywordFlags 带 Mine/Trap 时换
        // MineLayingSpeed/TrapThrowingSpeed——pobr 无图腾/陷阱吞吐（12-G11，
        // M4 不做）且 KeywordFlags 暂无 Mine/Trap 位，恒走 Speed。
        let speed = inputs.speed;
        let end = dps_end_factors(db, cfg, None);
        (instance * speed * inputs.duration * end.dps_multiplier * end.quantity_multiplier).min(cap)
    } else {
        instance
    };

    // 末端合并族（`:6093-6234`）：TotalDotDPS 恒计算（异常 DoT 不依赖技能 dot），
    // WithDotDPS 仅技能 dot 在场时（vendor `if skillFlags.dot`）。
    let total_dot_dps =
        (total_dot + inputs.poison_dps + inputs.ignite_dps + inputs.bleed_dps).min(cap);
    let with_dot_dps = if dot_active {
        inputs.base_dps + total_dot
    } else {
        0.0
    };
    let combined_dps = inputs.base_dps + total_dot_dps;

    SkillDotOutput {
        skill_dot_instance: round(instance),
        skill_total_dot: round(total_dot),
        total_dot_dps: round(total_dot_dps),
        with_dot_dps: round(with_dot_dps),
        combined_dps: round(combined_dps),
    }
}

/// 把技能 DoT 结果写入 [`OutputTable`] 的 M4-T4 契约字段（纯字段搬运，零计算）。
///
/// 调用点：`perform.rs` fill 段 `fill_skill_dot_stage`（函数级新增，蓝图 §3.2
/// 共享文件规则），在 `fill_ailments` 之后（消费异常 DoT 现值）。
pub fn fill_skill_dot(output: &mut OutputTable, dot: &SkillDotOutput) {
    output.skill_dot_instance = dot.skill_dot_instance;
    output.skill_total_dot = dot.skill_total_dot;
    output.total_dot_dps = dot.total_dot_dps;
    output.with_dot_dps = dot.with_dot_dps;
    output.combined_dps = dot.combined_dps;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    fn db_with(mods: Vec<Modifier>) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(mods);
        db
    }

    /// 中性不变式：空 db + 全零输入 → 输出全零（非 DoT build 的契约字段保持中性）。
    #[test]
    fn neutral_without_dot_sources() {
        let db = ModDb::new();
        let enemy = ModDb::new();
        let cfg = CalcConfig::attack();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(out, SkillDotOutput::default(), "无 DoT 来源必须全零中性");
    }

    /// 契约字段搬运：五个值各自落到同名 OutputTable 字段。
    #[test]
    fn fill_transfers_all_contract_fields() {
        let dot = SkillDotOutput {
            skill_dot_instance: 1.0,
            skill_total_dot: 2.0,
            total_dot_dps: 3.0,
            with_dot_dps: 4.0,
            combined_dps: 5.0,
        };
        let mut out = OutputTable::default();
        fill_skill_dot(&mut out, &dot);
        assert_eq!(out.skill_dot_instance, 1.0);
        assert_eq!(out.skill_total_dot, 2.0);
        assert_eq!(out.total_dot_dps, 3.0);
        assert_eq!(out.with_dot_dps, 4.0);
        assert_eq!(out.combined_dps, 5.0);
    }

    /// 基线公式：base × (1+inc) × more × (1+DotMultiplier) （面板口径 effMult=1）。
    #[test]
    fn basic_dot_pipeline() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("ChaosDamage", ModType::Inc, 50.0),
            Modifier::number("Damage", ModType::More, 30.0),
            Modifier::number("DotMultiplier", ModType::Base, 20.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // 100 × 1.5 × 1.3 × 1.2 = 234
        assert!((out.skill_dot_instance - 234.0).abs() < 1e-9, "{out:?}");
        assert_eq!(
            out.skill_total_dot, out.skill_dot_instance,
            "非叠加 = instance"
        );
        assert_eq!(out.total_dot_dps, 234.0);
        assert_eq!(out.with_dot_dps, 234.0, "baseDPS=0 时 WithDot = TotalDot");
    }

    /// DotMultiplier 的 Override 优先级：Override 在场时 Sum 双通道全部失效（:5897）。
    #[test]
    fn dot_multiplier_override_priority() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("DotMultiplier", ModType::Base, 50.0),
            Modifier::number("ChaosDotMultiplier", ModType::Base, 25.0),
            Modifier::number("DotMultiplier", ModType::Override, 10.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // Override(10) 优先：100 × 1.10 = 110（非 100 × (1 + (50+25)/100)）。
        assert!((out.skill_dot_instance - 110.0).abs() < 1e-9, "{out:?}");
    }

    /// 无 Override 时 DotMultiplier = Sum(DotMultiplier) + Sum(<Type>DotMultiplier)。
    #[test]
    fn dot_multiplier_sum_includes_typed_channel() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("DotMultiplier", ModType::Base, 50.0),
            Modifier::number("ChaosDotMultiplier", ModType::Base, 25.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!((out.skill_dot_instance - 175.0).abs() < 1e-9, "{out:?}");
    }

    /// dotIs* 剥 flag 回归：非 area DoT（无 DotIsArea）不吃 `increased Area Damage`；
    /// 注入 DotIsArea FLAG 后保留 Area 位、AreaDamage 桶生效。
    #[test]
    fn dot_is_area_gates_area_damage_bucket() {
        let base = vec![
            Modifier::number("FireDot", ModType::Base, 100.0),
            Modifier::number("AreaDamage", ModType::Inc, 100.0),
        ];
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell().with_flags(ModFlags::SPELL | ModFlags::AREA);

        let db = db_with(base.clone());
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!(
            (out.skill_dot_instance - 100.0).abs() < 1e-9,
            "无 dotIsArea：Area 位被剥，AreaDamage 不得作用于 DoT：{out:?}"
        );

        let mut with_flag = base;
        with_flag.push(Modifier::flag("DotIsArea"));
        let db = db_with(with_flag);
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!(
            (out.skill_dot_instance - 200.0).abs() < 1e-9,
            "DotIsArea 在场：保留 Area 位，AreaDamage 生效：{out:?}"
        );
    }

    /// canDeal 门控：`DealNoChaos` 清零混沌 DoT 基值（W-C3 契约 3 同源 flag）。
    #[test]
    fn deal_no_type_gates_dot_base() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DealNoChaos"),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(
            out.skill_dot_instance, 0.0,
            "DealNoChaos 必须门控掉混沌 DoT"
        );
    }

    /// TotalDotInstance / TotalDotDPS 的 DotDpsCap clamp。
    #[test]
    fn dot_dps_cap_clamps_instance_and_merge() {
        let db = db_with(vec![Modifier::number(
            "FireDot",
            ModType::Base,
            1.0e12, // 远超 cap
        )]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let cap = cfg.constants.game().dot_dps_cap;
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(out.skill_dot_instance, cap, "instance clamp DotDpsCap");
        assert_eq!(out.total_dot_dps, cap, "TotalDotDPS clamp DotDpsCap");
    }

    /// DotCanStack：TotalDot = instance × speed × duration ×
    /// dpsMultiplier × quantityMultiplier（duration 已接入时）。
    #[test]
    fn dot_can_stack_scales_by_speed_duration() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DotCanStack"),
            Modifier::number("QuantityMultiplier", ModType::Base, 2.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            speed: 2.0,
            duration: 3.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        // 100 × 2 × 3 × 1 × 2 = 1200
        assert!((out.skill_total_dot - 1200.0).abs() < 1e-9, "{out:?}");
    }

    /// DotCanStack 但 duration 未接（=0）：保守退化为单实例（不放大）。
    #[test]
    fn dot_can_stack_without_duration_degenerates_to_instance() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DotCanStack"),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            speed: 5.0,
            duration: 0.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(out.skill_total_dot, 100.0, "duration 缺失必须保守退化");
    }

    /// effMult（mode_effective）：敌方抗性 + DamageTakenOverTime 链作用于技能 DoT。
    #[test]
    fn eff_mult_applies_enemy_resist_and_taken() {
        let db = db_with(vec![Modifier::number("ChaosDot", ModType::Base, 100.0)]);
        let enemy = db_with(vec![
            Modifier::number("ChaosResist", ModType::Base, 50.0),
            Modifier::number("DamageTakenOverTime", ModType::Inc, 20.0),
        ]);
        let cfg = CalcConfig::spell().with_mode_effective(true);
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // 100 × (1-0.5) × 1.2 = 60
        assert!((out.skill_dot_instance - 60.0).abs() < 1e-9, "{out:?}");
    }

    /// 末端合并族：TotalDotDPS = 技能 dot + 异常三值；CombinedDPS = baseDPS +
    /// TotalDotDPS；WithDotDPS = baseDPS + TotalDot。
    #[test]
    fn merge_family_sums_skill_and_ailment_dots() {
        let db = db_with(vec![Modifier::number("FireDot", ModType::Base, 100.0)]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            base_dps: 1000.0,
            bleed_dps: 10.0,
            poison_dps: 20.0,
            ignite_dps: 30.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(out.total_dot_dps, 160.0);
        assert_eq!(out.with_dot_dps, 1100.0);
        assert_eq!(out.combined_dps, 1160.0);
    }

    /// 无技能 dot 但有异常 dot：WithDotDPS 维持 0（vendor 仅 skillFlags.dot 置），
    /// TotalDotDPS / CombinedDPS 仍合并异常侧。
    #[test]
    fn ailment_only_keeps_with_dot_neutral() {
        let db = ModDb::new();
        let enemy = ModDb::new();
        let cfg = CalcConfig::attack();
        let inputs = SkillDotInputs {
            base_dps: 500.0,
            ignite_dps: 80.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(out.skill_dot_instance, 0.0);
        assert_eq!(out.with_dot_dps, 0.0, "无技能 dot：WithDotDPS 维持中性");
        assert_eq!(out.total_dot_dps, 80.0);
        assert_eq!(out.combined_dps, 580.0);
    }
}
