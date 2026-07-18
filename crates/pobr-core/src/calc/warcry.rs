//! Warcry uptime 机器（存量 #9）：非主技能的 warcry 按「賦能攻擊次數 / 主技能出手
//! 速率 / (冷却 + 喊叫时间)」折算 uptime，再把 warcry 的进攻性效果按 uptime 缩放
//! 注入主技能聚合。
//!
//! 逐段对照 vendor：
//! - **WarcryPower / 賦能次数**：`CalcPerform.lua:2116-2142`（Warcry buff 分支）——
//!   `warcryPower = Override("WarcryPower") or max(Sum(BASE)×(1+Sum(INC)/100),
//!   Sum(BASE,"MinimumWarcryPower"))`（:2120）；`baseEmpowers =
//!   floor(min(power, Sum("WarcryPowerCap")) / Sum("WarcryPowerPer"))`（:2121-2123）
//!   `+ Sum("<Name>EmpoweredAttacks")`（:2125）；`totalEmpowers =
//!   (base + Sum("ExtraEmpoweredAttacks")) × More("ExtraEmpoweredAttacks")`
//!   （:2127-2129）→ 发布 `Num<Name>Empowers`（:2130）。
//! - **冷却**：`calcSkillCooldown`（CalcOffence.lua:325-348）——`cooldown =
//!   Override("CooldownRecovery") or (skillData.cooldown + Sum(BASE)) /
//!   max(0, (1+Sum(INC)/100)×More)`，storedUses ≤ 1 且无 AdditionalCooldownUses
//!   时向上取整到服务器帧（:338-346）。
//! - **喊叫时间**：`calcWarcryCastTime`（CalcOffence.lua:350-359）——
//!   `1 / min((1/Sum(BASE,"WarcryCastTime")) × mod("WarcrySpeed") ×
//!   actionSpeedMod, ServerTickRate)`；`InstantWarcry` flag → 0。
//! - **uptime**：CalcOffence.lua:3229-3237（Infernal 分支，Ancestral/Intimidating/
//!   Rallying 同构）——`baseUptimeRatio = min((NumEmpowers / Speed) /
//!   (cooldown + castTime), 1) × 100`；`UptimeRatio = min(100, baseUptimeRatio ×
//!   storedUses)`。注：vendor `:3236` 的 `storedUses or 0 + Sum(...)` 受 Lua 优先级
//!   影响实为 `storedUses or (0 + Sum)`，本实现按意图语义
//!   `storedUses + Sum("AdditionalCooldownUses")`（与 scaled_damage.rs `:3845`
//!   笔误处置同一先例；两 fixture 均 storedUses=1、Additional=0，取值相同）。
//! - **Infernal 消费点**：`CalcPerform.lua:1362-1366` 把 warcry skillModList 的
//!   `InfernalExtraFireDamageMultiplier` 发布为玩家 `InfernalExtraFireDamage`；
//!   `CalcOffence.lua:3251-3254` 注入 `DamageGainAsFire BASE gain×uptime/100`
//!   （ModFlag.Melee，source "Uptime Scaled Infernal Cry"）。本实现把两步折叠：
//!   直接从 spec 的 skill-local mods（+玩家 db）求和后按 uptime 注入。
//!
//! 门控（vendor CalcOffence.lua:3203-3205/:3229）：`env.mode_buffs`；主技能非
//! NeverExertable/Triggered/OtherThingUsesSkill/Retaliation；Infernal 消费点要求
//! 主技能带 SkillType.Melee。
//!
//! ponytail: 本模块当前只接 Infernal 的 DamageGainAsFire 消费点（fixture 中唯一
//! 消费者）；Intimidating/Rallying/Seismic 的 exert 伤害乘区（OffensiveWarcryEffect
//! 族）待有消费 build 时按同一 spec/uptime 底座补。

use pobr_data::prelude::*;
use pobr_data::skill::SkillTypes;

use crate::{CalcConfig, ModDb, Modifier};

use super::env::Env;
use super::offence::MinimalInput;

/// 一个 warcry 主动技能的注入规格（编排层 `warcry_skill_specs` 构造，
/// `session::add_warcry_skill` 写入 `Env::warcry_skills`）。
#[derive(Debug, Clone)]
pub struct WarcrySpec {
    /// warcry 键名（vendor `buff.name:gsub(" Cry",""):gsub("'s",""):gsub(" ","")`，
    /// CalcPerform.lua:2124——"Infernal Cry" → `Infernal`；`<Name>EmpoweredAttacks`
    /// 求和键用）。
    pub name: String,
    /// 授予效果 id（去重 + 归因）。
    pub skill_id: String,
    /// 技能基础冷却（秒，granted_effect_levels `cooldown_ms`；vendor
    /// `skillData.cooldown`）。
    pub cooldown_base_s: f64,
    /// `skillData.storedUses`（granted_effect_levels `stored_uses`，缺省 1）。
    pub stored_uses: f64,
    /// warcry 技能自身的类型位（skill-local/全局词条按 warcry 域匹配用——
    /// 对位 vendor 的 per-skill `skillCfg`）。
    pub skill_types: SkillTypes,
    /// skill-local mod 列表 = 自身 statmap 产物（WarcryPowerPer/Cap、
    /// InfernalExtraFireDamageMultiplier 等）+ 组内兼容 support 载荷
    /// （如 Cooldown Recovery II 的 CooldownRecovery INC 30）+
    /// `WarcryCastTime BASE`（效果 cast_time，对位 vendor skillModList "Base" 条）。
    pub mods: Vec<Modifier>,
}

/// spec-local 求和（vendor skillModList 的 local 段；global 段由玩家 db 单独求和后相加）。
fn local_sum(mods: &[Modifier], cfg: &CalcConfig, mod_type: ModType, name: &str) -> f64 {
    mods.iter()
        .filter(|m| m.mod_type == mod_type && m.name.as_str() == name && m.matches(cfg))
        .filter_map(|m| m.effective_number(cfg))
        .sum()
}

/// spec-local MORE 连乘（`Π(1+v/100)`）。
fn local_more(mods: &[Modifier], cfg: &CalcConfig, name: &str) -> f64 {
    mods.iter()
        .filter(|m| m.mod_type == ModType::More && m.name.as_str() == name && m.matches(cfg))
        .filter_map(|m| m.effective_number(cfg))
        .fold(1.0, |acc, v| acc * (1.0 + v / 100.0))
}

/// skill-local + 玩家 db 的合并 BASE/INC 求和（vendor skillModList 链上溯 modDB）。
fn scoped_sum(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, ty: ModType, name: &str) -> f64 {
    db.sum(ty, cfg, &[ModName::from(name)]) + local_sum(&spec.mods, cfg, ty, name)
}

fn scoped_more(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, name: &str) -> f64 {
    db.more(cfg, &[ModName::from(name)]) * local_more(&spec.mods, cfg, name)
}

/// 賦能攻擊次数（`Num<Name>Empowers`，CalcPerform.lua:2116-2130）。
fn total_empowers(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig) -> f64 {
    // :2120 —— WarcryPower：config OVERRIDE（multiplierWarcryPower）优先，否则
    // BASE×(1+INC/100) 与 MinimumWarcryPower 取大（BASE 20 来自敌人档位预设
    // player_mods，Boss/Pinnacle/Uber 共通，ConfigOptions.lua:2007）。
    let warcry_power = db
        .override_(cfg, ModName::from("WarcryPower"))
        .unwrap_or_else(|| {
            let base = db.sum(ModType::Base, cfg, &[ModName::from("WarcryPower")]);
            let inc = db.sum(ModType::Inc, cfg, &[ModName::from("WarcryPower")]);
            let min = db.sum(ModType::Base, cfg, &[ModName::from("MinimumWarcryPower")]);
            (base * (1.0 + inc / 100.0)).max(min)
        });
    // :2121-2123 —— per/cap 是 skillModList 侧 stat（Infernal 常量 stat 10/50）。
    let power_cap = scoped_sum(db, spec, cfg, ModType::Base, "WarcryPowerCap");
    let power_per = scoped_sum(db, spec, cfg, ModType::Base, "WarcryPowerPer");
    let mut base_empowers = if power_per > 0.0 {
        (warcry_power.min(power_cap) / power_per).floor()
    } else {
        0.0
    };
    // :2125 —— `<Name>EmpoweredAttacks`（主技能 cfg 求和；fixture 无来源，恒 0）。
    base_empowers += scoped_sum(
        db,
        spec,
        cfg,
        ModType::Base,
        &format!("{}EmpoweredAttacks", spec.name),
    );
    if base_empowers <= 0.0 {
        // :2126 —— base 为 0 时 vendor 不发布 Num<Name>Empowers（Sum 取 0）。
        return 0.0;
    }
    // :2127-2129。
    let extra = scoped_sum(db, spec, cfg, ModType::Base, "ExtraEmpoweredAttacks");
    let mult = scoped_more(db, spec, cfg, "ExtraEmpoweredAttacks");
    (base_empowers + extra) * mult
}

/// `calcSkillCooldown`（CalcOffence.lua:325-348）的 warcry 取值路径。
/// 未建模：Temporalis 冷却注入（:328/:332-337）——fixture 无来源。
fn actual_cooldown(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, tick_s: f64) -> f64 {
    let name = "CooldownRecovery";
    let added = scoped_sum(db, spec, cfg, ModType::Base, name);
    let base = spec.cooldown_base_s + added;
    let recovery = (1.0 + scoped_sum(db, spec, cfg, ModType::Inc, name) / 100.0)
        * scoped_more(db, spec, cfg, name);
    let cooldown = match db.override_(cfg, ModName::from(name)) {
        Some(v) => v,
        None => base / recovery.max(0.0),
    };
    // :338-346 —— 可存多次使用时不取整到服务器帧。
    let extra_uses = scoped_sum(db, spec, cfg, ModType::Base, "AdditionalCooldownUses");
    if spec.stored_uses > 1.0 || extra_uses > 0.0 {
        cooldown
    } else {
        (cooldown / tick_s).ceil() * tick_s
    }
}

/// `calcWarcryCastTime`（CalcOffence.lua:350-359）。
/// 未建模：`SupportedByAutoexertion`（:355 后半）——fixture 无来源。
fn warcry_cast_time(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, tick_s: f64) -> f64 {
    if db.flag(cfg, ModName::from("InstantWarcry")) {
        return 0.0;
    }
    let base = scoped_sum(db, spec, cfg, ModType::Base, "WarcryCastTime");
    if base <= 0.0 {
        return 0.0; // 无 cast time 数据（防御：除零）。
    }
    // 只求和 "WarcrySpeed"（:352）。「N% increased Skill Speed」文本在 parser 侧
    // 扇出成 {SkillSpeed, WarcrySpeed, TotemPlacementSpeed}（vendor ModParser.lua:770
    // 同构；extract_parser_rules 的 curated 覆盖，存量 #9 起对齐），statmap 的
    // `skill_speed_+%` 条目与 mageblood.rs 的手工扇出本就双名——三通道口径一致，
    // 此处单名求和无双计。
    let speed_mod = (1.0 + scoped_sum(db, spec, cfg, ModType::Inc, "WarcrySpeed") / 100.0)
        * scoped_more(db, spec, cfg, "WarcrySpeed");
    // vendor calcs.actionSpeedMod(actor)：与 offence 主链同名 ActionSpeed 乘区。
    let action_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
    let action_speed =
        (1.0 + db.sum(ModType::Inc, cfg, &action_names) / 100.0) * db.more(cfg, &action_names);
    let rate = ((1.0 / base) * speed_mod * action_speed).min(1.0 / tick_s);
    1.0 / rate
}

/// 单个 warcry 的 uptime（百分比 0..=100，CalcOffence.lua:3231-3237）。
fn uptime_ratio(
    db: &ModDb,
    spec: &WarcrySpec,
    scope_cfg: &CalcConfig,
    main_cfg: &CalcConfig,
    speed: f64,
) -> f64 {
    if speed <= 0.0 {
        return 0.0;
    }
    // 賦能次数按主技能 cfg 求和（vendor :2125/:3234 读 env.player.mainSkill.skillCfg /
    // env.modDB）；per/cap 等 skill-local 常量不带条件，两 cfg 等价。
    let empowers = total_empowers(db, spec, main_cfg);
    let tick_s = main_cfg.constants.game().server_tick_seconds;
    let cooldown = actual_cooldown(db, spec, scope_cfg, tick_s);
    let cast_time = warcry_cast_time(db, spec, scope_cfg, tick_s);
    if cooldown + cast_time <= 0.0 {
        return 0.0;
    }
    let base_ratio = ((empowers / speed) / (cooldown + cast_time)).min(1.0) * 100.0;
    // :3236 —— storedUses 意图语义（模块 doc 的 Lua 优先级说明）。
    let stored =
        spec.stored_uses + scoped_sum(db, spec, scope_cfg, ModType::Base, "AdditionalCooldownUses");
    (base_ratio * stored).min(100.0)
}

/// perform 阶段入口：主技能 hand pass 之前调用，把 uptime 缩放后的 warcry 进攻
/// 效果注入玩家 db（vendor 在 CalcOffence 伤害段之前写 skillModList，注入后
/// 击中与其派生 DoT（点燃）同吃该增益——smith dot 缺口 = 命中比平方的根源）。
///
/// 幂等：`env.warcry_gain_injected` 防重复注入（vendor 以 `InfernalActive` flag
/// 防重，CalcPerform.lua:1365）。
pub fn apply_warcry_uptime(env: &mut Env) {
    if env.warcry_skills.is_empty() || env.warcry_gain_injected {
        return;
    }
    // vendor CalcOffence.lua:3203 `if env.mode_buffs`。
    if !env.cfg.mode_buffs {
        return;
    }
    // :3205 —— 主技能可被赋能（NeverExertable/Triggered/OtherThingUsesSkill/
    // Retaliation 之外）。
    let excluded = [
        "NeverExertable",
        "Triggered",
        "OtherThingUsesSkill",
        "Retaliation",
    ]
    .iter()
    .filter_map(|n| SkillTypes::from_pob2_name(n))
    .fold(SkillTypes::NONE, |acc, t| acc | t);
    if env.cfg.skill_types.intersects(excluded) {
        return;
    }
    // 主技能 Speed（vendor globalOutput.Speed，:3235）：与 hand pass 主手域逐位
    // 一致地解析（双持时 vendor 逐 pass 计算 uptime——此处取主手，见 hand_scope doc）。
    let input = MinimalInput::from(env.player.base);
    let speed = match env
        .hand_sources
        .iter()
        .find(|h| matches!(h.label, crate::HandTag::MainHand | crate::HandTag::Single))
        .or(env.hand_sources.first())
    {
        Some(hand) => {
            let (cfg, input) = super::hand_pass::hand_scope(hand, &env.cfg, &input);
            super::offence::resolve_action_rate(&env.player.mod_db, &cfg, &input)
        }
        None => super::offence::resolve_action_rate(&env.player.mod_db, &env.cfg, &input),
    };

    let dbg = std::env::var("POBR_DBG_WARCRY").is_ok();
    let mut gain_mods: Vec<Modifier> = Vec::new();
    for spec in &env.warcry_skills {
        // per-skill scope cfg（vendor skillCfg）：warcry 自身类型位；flags/keyword
        // 清空（warcry 非攻击/法术，主技能的武器位不外溢到冷却/喊叫速度求和）。
        let scope_cfg = env
            .cfg
            .clone()
            .with_skill_types(spec.skill_types)
            .with_flags(ModFlags::NONE)
            .with_keyword_flags(KeywordFlags::NONE);
        let uptime = uptime_ratio(&env.player.mod_db, spec, &scope_cfg, &env.cfg, speed);
        if dbg {
            let tick_s = env.cfg.constants.game().server_tick_seconds;
            eprintln!(
                "[POBR_DBG_WARCRY] {} empowers={} cooldown={} castTime={} speed={speed} uptime={uptime} localMods={}",
                spec.skill_id,
                total_empowers(&env.player.mod_db, spec, &env.cfg),
                actual_cooldown(&env.player.mod_db, spec, &scope_cfg, tick_s),
                warcry_cast_time(&env.player.mod_db, spec, &scope_cfg, tick_s),
                spec.mods.len(),
            );
            for m in &spec.mods {
                eprintln!(
                    "[POBR_DBG_WARCRY]   local {:?} {:?} {:?}",
                    m.name, m.mod_type, m.value
                );
            }
            for m in env.player.mod_db.iter_mods() {
                if matches!(m.name.as_str(), "WarcrySpeed" | "SkillSpeed") && m.matches(&scope_cfg)
                {
                    eprintln!(
                        "[POBR_DBG_WARCRY]   db {:?} {:?} {:?} src={:?}",
                        m.name, m.mod_type, m.value, m.source
                    );
                }
            }
        }

        // Infernal 消费点（CalcOffence.lua:3229/:3251-3254）：主技能须带 Melee。
        let gain = scoped_sum(
            &env.player.mod_db,
            spec,
            &scope_cfg,
            ModType::Base,
            "InfernalExtraFireDamageMultiplier",
        );
        if gain > 0.0 && env.cfg.skill_types.intersects(SkillTypes::MELEE) {
            // :3253 —— `Condition:WarcryMaxHit`（config）时按满 uptime。
            let uptime_used = if env
                .player
                .mod_db
                .flag(&env.cfg, ModName::from("Condition:WarcryMaxHit"))
                || env.cfg.condition("WarcryMaxHit")
            {
                100.0
            } else {
                uptime
            };
            if uptime_used > 0.0 {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("warcry.{}.uptime_gain_as_fire", spec.skill_id),
                ))
                .with_raw_text(format!(
                    "Uptime Scaled Infernal Cry ({gain} x {uptime_used}%)"
                ));
                gain_mods.push(
                    Modifier::number(
                        "DamageGainAsFire",
                        ModType::Base,
                        gain * uptime_used / 100.0,
                    )
                    .with_flags(ModFlags::MELEE)
                    .with_source("Uptime Scaled Infernal Cry")
                    .with_origin(origin),
                );
            }
        }
    }
    env.player.mod_db.add_list(gain_mods);
    env.warcry_gain_injected = true;
}

#[cfg(test)]
mod tests {
    //! smith-of-kitava oracle 钉值链（tools/pob2-oracle，2026-07-17 复跑）：
    //! WarcryPower 20（Boss preset）→ empowers floor(min(20,50)/10)=2；
    //! cooldown 8/(1+(30-12+10)/100)=6.25 → 帧取整 6.27；castTime
    //! 1/((1/0.8)×1.47×1)=0.544218；Speed 1.512 →
    //! uptime (2/1.512)/6.814218=19.4116%；gain 62 → DamageGainAsFire 12.0352。

    use super::*;
    use crate::CalcConfig;

    fn smith_spec() -> WarcrySpec {
        WarcrySpec {
            name: "Infernal".into(),
            skill_id: "InfernalCryPlayer".into(),
            cooldown_base_s: 8.0,
            stored_uses: 1.0,
            skill_types: SkillTypes::WARCRY,
            mods: vec![
                Modifier::number("WarcryCastTime", ModType::Base, 0.8),
                Modifier::number("WarcryPowerPer", ModType::Base, 10.0),
                Modifier::number("WarcryPowerCap", ModType::Base, 50.0),
                Modifier::number("InfernalExtraFireDamageMultiplier", ModType::Base, 62.0),
                // Cooldown Recovery II support 载荷。
                Modifier::number("CooldownRecovery", ModType::Inc, 30.0),
            ],
        }
    }

    fn smith_db() -> ModDb {
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("WarcryPower", ModType::Base, 20.0).with_source("Boss"),
            Modifier::number("CooldownRecovery", ModType::Inc, -12.0).with_source("Quest"),
            Modifier::number("CooldownRecovery", ModType::Inc, 10.0).with_source("Rune"),
            // Mageblood 30 + 树上「Skill Speed」小点 17（parser 扇出成 WarcrySpeed）。
            Modifier::number("WarcrySpeed", ModType::Inc, 47.0),
        ]);
        db
    }

    #[test]
    fn smith_uptime_matches_oracle() {
        let db = smith_db();
        let spec = smith_spec();
        let cfg = CalcConfig::attack();
        let tick = cfg.constants.game().server_tick_seconds;

        assert_eq!(total_empowers(&db, &spec, &cfg), 2.0);

        let cd = actual_cooldown(&db, &spec, &cfg, tick);
        assert!((cd - 6.27).abs() < 1e-9, "cooldown {cd} != 6.27");

        let ct = warcry_cast_time(&db, &spec, &cfg, tick);
        assert!((ct - 0.5442176870748299).abs() < 1e-12, "castTime {ct}");

        let uptime = uptime_ratio(&db, &spec, &cfg, &cfg, 1.512);
        assert!(
            (uptime - 19.41163877).abs() < 1e-6,
            "uptime {uptime} != 19.41163877"
        );
        let gain = 62.0 * uptime / 100.0;
        assert!((gain - 12.03521604).abs() < 1e-6, "gain {gain}");
    }
}
