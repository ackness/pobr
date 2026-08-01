//! env_finalize 阶段 3：flask/charm 词条合并端到端。
//!
//! 载荷构造已由 `item::ingest_flask_charm` 单测锚定（tests/item_source.rs），
//! 本文件验证**合并语义**（vendor CalcPerform.lua:1429-1663）：
//! `cfg.mode_combat` 门控整段、effect 乘区缩放（FlaskEffect/CharmEffect +
//! 载荷局部 LocalUtilityEffect）、charm limit（Override/Sum + cap 常量）、
//! mergeBuff 同 base 取最大、Using* 条件写回、幂等护栏。
//!
//! 关键不变式（ninja_parity 锚点）：`mode_combat` 默认 false → 载荷在不在
//! player db，输出逐值不变。

use pobr_core::calc::env_finalize::merge_flasks_charms;
use pobr_core::calc::{Actor, ActorBaseStats, Env, MinimalInput};
use pobr_core::item::{ItemIngest, ingest_flask_charm_with_ctx};

/// engine 版 flask/charm ingest（签名对齐历史 `ingest_flask_charm`）。
fn ingest_flask_charm(slot_name: &str, item: &Item) -> ItemIngest {
    ingest_flask_charm_with_ctx(slot_name, item, crate::support::ctx())
}
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

fn utility_item(base: &str, explicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Magic,
        quality: 0,
        corrupted: false,
        implicit_texts: Vec::new(),
        modifier_texts: explicits.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

/// 带载荷的 Env（mode_combat 由调用方设定）。
fn env_with_carriers(mode_combat: bool, carriers: Vec<Modifier>) -> Env {
    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    env.cfg = CalcConfig::attack()
        .with_damage_type(DamageType::Physical)
        .with_mode_combat(mode_combat);
    for c in carriers {
        env.player.mod_db.add_mod(c);
    }
    env
}

fn carrier_for(slot: &str, base: &str, explicits: &[&str]) -> Vec<Modifier> {
    ingest_flask_charm(slot, &utility_item(base, explicits)).modifiers
}

fn lightning_res(db: &ModDb, cfg: &CalcConfig) -> f64 {
    db.sum(ModType::Base, cfg, &[ModName::from("LightningResistance")])
}

/// 基线：charm 词条 + CharmLimit 1 → 合并进 player db 并吃 CharmEffect 乘区
/// （15 × 1.2 = 18，vendor ScaleAddMod 整数截断）；Using* 条件置位。
#[test]
fn charm_mods_merge_and_scale_with_charm_effect() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmEffect", ModType::Inc, 20.0));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        lightning_res(&env.player.mod_db, &env.cfg),
        18.0,
        "+15% 雷抗 × (1 + 20/100) = 18（整数截断）"
    );
    assert_eq!(env.cfg.conditions.get("UsingCharm"), Some(&true));
    assert_eq!(
        env.cfg.conditions.get("UsingSapphireCharmofLightning"),
        Some(&true)
    );
    // 归因穿透：合并产物可回溯 Flask 来源。
    let contributions = env.player.mod_db.contributions(
        ModType::Base,
        &env.cfg,
        &[ModName::from("LightningResistance")],
    );
    assert_eq!(
        contributions[0].origin.as_ref().unwrap().source_id,
        SourceId::new(SourceKind::Flask, "flask.charm1")
    );
}

/// flask 载荷：局部 `25% increased effect` 与全局 FlaskEffect INC 相加进乘区
/// （10 × (1 + (20+25)/100) = 14.5 → 整数截断 14）；UsingFlask 条件置位。
///
/// `Grants Onslaught during effect` **不产出 Onslaught flag**——PoB2 ModParser 对该行
/// 返回 unsupported（见 item.rs 注释 + item_source.rs 对应测试），PoBR 逐行一致。
#[test]
fn flask_local_and_global_effect_inc_stack_additively() {
    let mut carriers = carrier_for(
        "Flask 1",
        "Quicksilver Flask",
        &[
            "Grants Onslaught during effect",
            "25% increased effect",
            "10% increased Movement Speed during effect",
        ],
    );
    carriers.push(Modifier::number("FlaskEffect", ModType::Inc, 20.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("MovementSpeed")]),
        14.0,
        "10 × 1.45 = 14.5 → m_modf(round(…,2)) 截断 14"
    );
    assert!(
        !env.player.mod_db.flag(&env.cfg, ModName::from("Onslaught")),
        "Grants Onslaught during effect 与 PoB2 一致归 Unsupported，不置 Onslaught flag"
    );
    assert_eq!(env.cfg.conditions.get("UsingFlask"), Some(&true));
    assert_eq!(env.cfg.conditions.get("UsingQuicksilverFlask"), Some(&true));
}

/// mode_combat=false → 载荷存在与否输出逐值不变（ninja_parity 不变式的微观锚点）。
#[test]
fn mode_combat_false_keeps_everything_untouched() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(false, carriers);
    let mods_before = env.player.mod_db.iter_mods().count();
    let conditions_before = env.cfg.conditions.clone();

    merge_flasks_charms(&mut env);

    assert_eq!(env.player.mod_db.iter_mods().count(), mods_before);
    assert_eq!(env.cfg.conditions, conditions_before);
    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), 0.0);
}

/// charm limit：CharmLimit 1 → 仅首个（插入序）charm 并入；CharmLimit Base 5
/// 被 cap 常量（charm_limit_cap = 3）截断；无 CharmLimit 词条 → charm 全不生效
/// （vendor :1589 `Override or Sum` 为 0）。
#[test]
fn charm_limit_caps_number_of_active_charms() {
    let two_charms = |limit: Option<f64>| {
        let mut carriers = carrier_for(
            "Charm 1",
            "Sapphire Charm of Lightning",
            &["+15% to Lightning Resistance"],
        );
        carriers.extend(carrier_for(
            "Charm 2",
            "Ruby Charm of Embers",
            &["+25% to Fire Resistance"],
        ));
        if let Some(v) = limit {
            carriers.push(Modifier::number("CharmLimit", ModType::Base, v));
        }
        let mut env = env_with_carriers(true, carriers);
        merge_flasks_charms(&mut env);
        (
            lightning_res(&env.player.mod_db, &env.cfg),
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("FireResistance")]),
        )
    };

    assert_eq!(two_charms(None), (0.0, 0.0), "无 CharmLimit → charm 不生效");
    assert_eq!(
        two_charms(Some(1.0)),
        (15.0, 0.0),
        "limit 1 → 仅首个 charm（插入序）"
    );
    assert_eq!(two_charms(Some(2.0)), (15.0, 25.0), "limit 2 → 两个都进");
    assert_eq!(
        two_charms(Some(5.0)),
        (15.0, 25.0),
        "Base 5 被 cap=3 截断后仍覆盖两个"
    );
}

/// mergeBuff（vendor :41-63）：同 base 同参数词条取最大不叠加；不同 base 各自生效。
#[test]
fn merge_buff_takes_max_for_same_base_same_params() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm",
        &["+15% to Lightning Resistance"],
    );
    carriers.extend(carrier_for(
        "Charm 2",
        "Sapphire Charm",
        &["+12% to Lightning Resistance"],
    ));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 3.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        lightning_res(&env.player.mod_db, &env.cfg),
        15.0,
        "同 base 同参数：取最大（15），不叠加（27）"
    );
}

/// 幂等：重复调用（同一 Env 多次 perform 场景）不重复并入。
#[test]
fn merge_is_idempotent_across_repeated_calls() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);
    let after_first = lightning_res(&env.player.mod_db, &env.cfg);
    merge_flasks_charms(&mut env);
    merge_flasks_charms(&mut env);

    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), after_first);
    assert_eq!(after_first, 15.0);
}

/// FlasksDoNotApplyToPlayer（vendor :1561）：flask buff 与条件整体不落 player；
/// charm 不受该 flag 影响。
#[test]
fn flasks_do_not_apply_to_player_flag_gates_flask_only() {
    let mut carriers = carrier_for(
        "Flask 1",
        "Quicksilver Flask",
        &["10% increased Movement Speed during effect"],
    );
    carriers.extend(carrier_for(
        "Charm 1",
        "Sapphire Charm",
        &["+15% to Lightning Resistance"],
    ));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    carriers.push(Modifier::flag("FlasksDoNotApplyToPlayer"));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("MovementSpeed")]),
        0.0
    );
    assert_eq!(env.cfg.conditions.get("UsingFlask"), None);
    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), 15.0);
    assert_eq!(env.cfg.conditions.get("UsingCharm"), Some(&true));
}

/// 端到端（session → perform）：charm 抗性词条经 mode_combat=true 进输出；
/// mode_combat=false 输出逐值持平基线（无 charm 的 session）。
#[test]
fn end_to_end_charm_resistance_reaches_output_under_mode_combat() {
    let input = MinimalInput {
        base_life: 1_000.0,
        ..Default::default()
    };
    let run = |mode_combat: bool, with_charm: bool| {
        let cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Physical)
            .with_mode_combat(mode_combat);
        let mut session = crate::support::session(input).with_config(cfg);
        if with_charm {
            let mut mods = carrier_for(
                "Charm 1",
                "Sapphire Charm of Lightning",
                &["+15% to Lightning Resistance"],
            );
            mods.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
            session.add_modifiers(mods);
        }
        session.perform_minimal().lightning_resistance
    };

    let baseline = run(false, false);
    assert_eq!(
        run(false, true),
        baseline,
        "mode_combat=false：载荷在场输出逐值不变"
    );
    assert_eq!(
        run(true, true),
        baseline + 15.0,
        "mode_combat=true：charm 雷抗进入输出"
    );
}

/// 端到端：charm 预算来源不再手搓 `CharmLimit` modifier，而是经 mod_parser
/// 解析腰带 implicit「Has 1 Charm Slot」（vendor ModParser.lua:5453）注入——预算
/// 解锁后 charm 抗性进入输出；无该词条（预算 0，:1589）时 charm 全不生效。
#[test]
fn end_to_end_belt_charm_slots_text_unlocks_charm_budget() {
    let input = MinimalInput {
        base_life: 1_000.0,
        ..Default::default()
    };
    let run = |with_belt_line: bool| {
        let cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Physical)
            .with_mode_combat(true);
        let mut session = crate::support::session(input).with_config(cfg);
        if with_belt_line {
            session
                .add_modifier_texts(["Has 1 Charm Slot"])
                .expect("parse belt implicit");
        }
        session.add_modifiers(carrier_for(
            "Charm 1",
            "Sapphire Charm of Lightning",
            &["+15% to Lightning Resistance"],
        ));
        session.perform_minimal().lightning_resistance
    };
    assert_eq!(
        run(true) - run(false),
        15.0,
        "「Has 1 Charm Slot」解析为 CharmLimit BASE → charm 预算解锁"
    );
}
