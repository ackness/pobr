//! Mageblood legacies 应用（vendor `CalcPerform.lua:66-142` legacies 表 +
//! `:1502-1528` 应用逻辑，行号实读）。
//!
//! Mageblood 腰带的每个选中变体渲染成一行 `Legacy of <X>` 词条，解析层经
//! `special:mageblood_legacy` handler 产出 `LegacyOf<X>` BASE 1 + `MagebloodEquipped`
//! FLAG（vendor `ModParser.lua:5554-5557`）；implicit「All Mage's Legacies have N%
//! increased effect per duplicate...」→ `MagesLegacyEffect` INC N（special DSL）。
//! 本阶段把这些标记词条聚合成真实的护甲/闪避/抗性等 mod。
//!
//! vendor 逻辑（`:1502-1528`）：
//! - `MagebloodEquipped` flag 未置 → 空转；
//! - 逐 legacy 名 `Sum("BASE", LegacyOf<X>)` = 该变体装备份数（stacks）；
//! - `totalDuplicates = Σ max(stacks-1, 0)`（每个变体超出 1 份的部分）；
//! - `globalEffect = 1 + totalDuplicates × Sum("INC", MagesLegacyEffect)/100`；
//! - 每个**在场**的 legacy 把它的效果**应用一次**（不乘 stacks——duplicate 只放大
//!   globalEffect），值 = `floor(globalEffect × base)`，source = "Mageblood"。
//!
//! 空转兼容：无 `MagebloodEquipped` / 无任何 `LegacyOf*` 时零写入。幂等：已注入
//! （同一 Env 重复 perform）时按 source 判定跳过。

use pobr_data::prelude::*;

use super::Env;
use crate::Modifier;

/// 单条 legacy 效果：(stat 名, mod 类型, base 值)。
type LegacyEffect = (&'static str, ModType, f64);
/// 一个 legacy：(`LegacyOf<X>` 名, 效果列表)。
type LegacyDef = (&'static str, &'static [LegacyEffect]);

/// vendor `legacies` 表逐条镜像（`CalcPerform.lua:66-142`）。
const LEGACIES: &[LegacyDef] = &[
    ("LegacyOfAmethyst", &[("ChaosResist", ModType::Base, 45.0)]),
    ("LegacyOfBasalt", &[("Armour", ModType::Inc, 150.0)]),
    (
        "LegacyOfBismuth",
        &[("ElementalResist", ModType::Base, 45.0)],
    ),
    // vendor stat `CritChance` → PoBR 消费名 `CriticalStrikeChance`（calc::crit 读后者；
    // 与 special_mod::translate_vendor_name 同口径。裸注入不过 parser 翻译，故表内直用
    // PoBR 规范名，否则落死桶——同 Virtuous Barrier Life→MaximumLife 教训）。
    (
        "LegacyOfDiamond",
        &[("CriticalStrikeChance", ModType::Inc, 75.0)],
    ),
    ("LegacyOfGold", &[("LootRarity", ModType::Inc, 45.0)]),
    ("LegacyOfGranite", &[("Armour", ModType::Base, 2000.0)]),
    ("LegacyOfJade", &[("Evasion", ModType::Base, 2000.0)]),
    (
        "LegacyOfQuicksilver",
        &[("MovementSpeed", ModType::Inc, 30.0)],
    ),
    (
        "LegacyOfRuby",
        &[
            ("FireResist", ModType::Base, 60.0),
            ("FireResistMax", ModType::Base, 5.0),
        ],
    ),
    (
        "LegacyOfSapphire",
        &[
            ("ColdResist", ModType::Base, 60.0),
            ("ColdResistMax", ModType::Base, 5.0),
        ],
    ),
    (
        "LegacyOfSilver",
        &[
            // vendor 裸 `Speed`（通用行动速度）→ PoBR 速度桶名 `SkillSpeed`（攻/施法通吃，
            // SPEED_BUCKET；同 CritChance 死桶类，translate_vendor_name 裸 Speed→SkillSpeed）。
            ("SkillSpeed", ModType::Inc, 30.0),
            ("WarcrySpeed", ModType::Inc, 30.0),
            ("TotemPlacementSpeed", ModType::Inc, 30.0),
        ],
    ),
    ("LegacyOfStibnite", &[("Evasion", ModType::Inc, 150.0)]),
    ("LegacyOfSulphur", &[("Damage", ModType::Inc, 60.0)]),
    (
        "LegacyOfTopaz",
        &[
            ("LightningResist", ModType::Base, 60.0),
            ("LightningResistMax", ModType::Base, 5.0),
        ],
    ),
];

/// 注入 mod 的 source 串（vendor `NewMod(..., "Mageblood")`）——兼作幂等判据。
const MAGEBLOOD_SOURCE: &str = "Mageblood";

/// env_finalize 阶段：应用 Mageblood legacies（vendor `CalcPerform.lua:1502-1528`）。
pub fn apply_mageblood_legacies(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    if !db.flag(cfg, ModName::from("MagebloodEquipped")) {
        return;
    }
    // 幂等护栏：本 Env 已注入过（同一 Env 重复 perform）。
    if db
        .iter_mods()
        .any(|m| m.source.as_deref() == Some(MAGEBLOOD_SOURCE))
    {
        return;
    }

    // 逐 legacy 统计 stacks（`Sum("BASE", LegacyOf<X>)` = 装备份数）。
    let found: Vec<(f64, &'static [LegacyEffect])> = LEGACIES
        .iter()
        .filter_map(|(name, effects)| {
            let stacks = db.sum(ModType::Base, cfg, &[ModName::from(*name)]);
            (stacks > 0.0).then_some((stacks, *effects))
        })
        .collect();
    if found.is_empty() {
        return;
    }

    let total_duplicates: f64 = found
        .iter()
        .map(|(stacks, _)| (stacks - 1.0).max(0.0))
        .sum();
    let effect_per_dupe = db.sum(ModType::Inc, cfg, &[ModName::from("MagesLegacyEffect")]);
    let global_effect = 1.0 + total_duplicates * (effect_per_dupe / 100.0);

    let source_id = SourceId::new(SourceKind::Item, "mageblood");
    let mut injected: Vec<Modifier> = Vec::new();
    for (_, effects) in &found {
        for (stat, mod_type, value) in *effects {
            let scaled = (global_effect * value).floor();
            let mut origin = ModifierSource::new(source_id.clone());
            origin.raw_text = Some(MAGEBLOOD_SOURCE.to_string());
            injected.push(
                Modifier::number(*stat, *mod_type, scaled)
                    .with_source(MAGEBLOOD_SOURCE)
                    .with_origin(origin),
            );
        }
    }
    env.player.mod_db.add_list(injected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// 装 Mageblood + 若干 LegacyOf 标记词条，跑聚合后返回 Env。
    fn run(legacy_stacks: &[(&str, f64)], effect_per_dupe: f64) -> Env {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player.mod_db.add_mod(Modifier::flag("MagebloodEquipped"));
        for (name, stacks) in legacy_stacks {
            for _ in 0..(*stacks as usize) {
                player
                    .mod_db
                    .add_mod(Modifier::number(*name, ModType::Base, 1.0));
            }
        }
        if effect_per_dupe > 0.0 {
            player.mod_db.add_mod(Modifier::number(
                "MagesLegacyEffect",
                ModType::Inc,
                effect_per_dupe,
            ));
        }
        let mut env = Env::new(player);
        apply_mageblood_legacies(&mut env);
        env
    }

    fn base(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
    }
    fn inc(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from(name)])
    }

    #[test]
    fn no_flag_no_effect() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        apply_mageblood_legacies(&mut env);
        assert_eq!(base(&env, "Armour"), 0.0);
    }

    /// ice-shot 档：Jade + Stibnite 无重复 → globalEffect=1，Evasion BASE 2000 + INC 150。
    #[test]
    fn distinct_legacies_no_duplicate_bonus() {
        let env = run(&[("LegacyOfJade", 1.0), ("LegacyOfStibnite", 1.0)], 0.0);
        assert_eq!(base(&env, "Evasion"), 2000.0);
        assert_eq!(inc(&env, "Evasion"), 150.0);
    }

    /// titan 档：Silver×2 → totalDuplicates=1，globalEffect=1.46；Basalt Armour INC
    /// floor(1.46×150)=219（钉住 vendor `defenceModList` 观测值）。
    #[test]
    fn duplicate_scales_global_effect() {
        let env = run(
            &[
                ("LegacyOfBasalt", 1.0),
                ("LegacyOfSilver", 2.0),
                ("LegacyOfQuicksilver", 1.0),
            ],
            46.0,
        );
        assert_eq!(inc(&env, "Armour"), 219.0); // floor(1.46 × 150)
        assert_eq!(inc(&env, "MovementSpeed"), (1.46 * 30.0f64).floor()); // 43
        // Silver 只应用一次（duplicate 只放大 globalEffect，不叠份数）。
        assert_eq!(inc(&env, "SkillSpeed"), (1.46 * 30.0f64).floor()); // 43, 非 86
    }

    /// 幂等：重复 perform 不重复注入。
    #[test]
    fn idempotent_reapply() {
        let mut env = run(&[("LegacyOfGranite", 1.0)], 0.0);
        assert_eq!(base(&env, "Armour"), 2000.0);
        apply_mageblood_legacies(&mut env);
        assert_eq!(base(&env, "Armour"), 2000.0);
    }
}
