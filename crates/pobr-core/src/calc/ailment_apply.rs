//! 非伤害异常施加闭环（M3 T4 D1，env_finalize 阶段 7；蓝图 m3-orchestration.md §7.1）。
//!
//! 对照 vendor `CalcPerform.lua:3076-3180`「Calculate maximum and apply the strongest
//! non-damaging ailments」：把 Chill/Shock 的来源词条折算成 `Current<X>` 强度并写入
//! enemy db，使感电增伤经既有 `DamageTaken` 消费链（offence.rs
//! `enemy_damage_multiplier`，`mode_effective` 门控）自动进入有效 DPS。
//!
//! ## 公式（CalcPerform.lua 行号为 vendor 0.18.0 实读）
//!
//! - **来源聚合**（:3128-3151）：enemy db `<X>Val`（config 敌人状态项）∪ player db
//!   `<X>Base`/`<X>Override`/`<X>Minimum`（BASE 型词条，等价 vendor
//!   `modDB:Tabulate("BASE", …)`）。Base/Minimum 来源乘 magnitude =
//!   `calcLib.mod(skill, Enemy<X>Magnitude, AilmentMagnitude) ×
//!    calcLib.mod(enemyDB, Self<X>Magnitude, AilmentMagnitude)`（:3144-3146；
//!   Override 来源不乘）。`override = max(各来源 effect, Σ Minimum)`（:3147-3150）。
//! - **Maximum**（:3153-3163）：`<X>Max` Override 优先；否则
//!   `non_damaging_ailments.json 的 max + Σ <X>Max BASE`（vendor 按 activeSkillList
//!   逐技能取最大，pobr 无 per-skill modList，统一读 player db，见「已知差异」）。
//! - **Current**（:3164）：`floor(min(max(override, Σ <X>Val), Maximum) × 10^prec) /
//!   10^prec`。`prec` 来自 `non_damaging_ailments.json`（Chill/Shock 均为 0），即整数
//!   floor——不引入新魔数（数据通道见下）。
//! - **写 enemy db**（:3078-3124 + :3165-3168）：
//!   - Shock → `DamageTaken INC Current {Condition:Shocked}`（:3120）；
//!   - Chill → `ActionSpeed INC -Current {Condition:Chilled}`（:3089）+ Bonechill 分支
//!     `ColdDamageTaken INC Current {Condition:Chilled}`（:3092-3094）；
//!   - Override 来源 → 置 `Condition:Shocked/Chilled`（:3136-3138）；
//!   - 施加后置 `Condition:Already<cond>`（带 `{Condition:<cond>}` tag，:3168，防
//!     minion 二次施加）。
//! - **`Multiplier:ChillEffect/ShockEffect` 增量更新**（:3172-3180）：现有 Σ BASE <
//!   Current 时补差值。
//!
//! ## 常量数据通道（禁新魔数）
//!
//! - Chill max = `cfg.constants.game().chill_max_effect`（`base/game_constants.json`
//!   注入，= `non_damaging_ailments.json` Chill.max = 50）；
//! - Shock max = `pobr_data::monster::SHOCK_MAX_EFFECT`（既有 Rust 准源，=
//!   `non_damaging_ailments.json` Shock.max = 100；该域尚未进 `RuntimeConstants`
//!   注入包，接入属 runtime.rs/RuleSet 扩列，归 T4 后续/M5 数据化清单）；
//! - precision：`non_damaging_ailments.json` Chill/Shock 均为 0 → 整数 `floor`，
//!   同上暂无注入通道（数据若改非 0 需先扩 RuntimeConstants）。
//!
//! ## 与 `fill_ailments`（calc/ailment.rs，offence 之后）的职责切分
//!
//! 本阶段**只消费 `<X>Val/Base/Override/Minimum` 词条与 config**（与 PoB2 同——
//! vendor 该段也不依赖本次 perform 的 DPS），产物是 enemy db 的 debuff mod；
//! `fill_ailments` 是**面板口径**的 magnitude 估算（冷伤命中/阈值 → `chill_effect`、
//! `shock_effect` 输出字段），只写 `OutputTable` 不写 enemy db——两者无重复施加。
//!
//! ## 已知差异（M3 范围声明）
//!
//! - `ChillCanStack`/`ShockCanStack` 叠层分支（:3084-3088 / :3105-3112）不实现
//!   （按 build 命中再补）；
//! - `ChillEffectIncDamageTaken`（Asphyxia's Wrath，:3095-3097）不实现；
//! - Bonechill 的 `hasGuaranteedBonechill`（skill data `supportBonechill` +
//!   ChillingArea 等通道，:1089-1180）以「存在 `ChillOverride` 来源」近似；
//!   `HasBonechill` 取 player db flag（当前无产出方，分支休眠）；
//! - Maximum 的 per-skill `baseSkillModList` 取最大（:3155-3159）退化为 player db
//!   统一聚合（pobr 无 per-skill modList 通道）；
//! - vendor 把 Base/Minimum 折算值写回 `modDB:NewMod(<X>Override, …)`（:3147）——
//!   该写回仅服务 PoB UI/config 联动，pobr 无消费者，不写；
//! - enemy `ActionSpeed` 的消费点 M3 缺（敌方出手速度只影响 EHP 估算）；
//! - vendor 外层 gate 第二操作数因 Lua `0` 为 truthy 恒真（:3128-3129），等效行为是
//!   「无来源时写零值 mod + Already flag」；pobr 用显式存在性 gate（enemy `<X>Val`
//!   聚合 > 0 或 player 存在 Base/Override/Minimum 词条），保证空转兼容
//!   （无来源时 env/db 逐 bit 不变）。
//! - 条件桥接：vendor 的 `{Condition:<cond>}` tag 查 enemyDB.conditions；pobr 条件
//!   统一走 `cfg.conditions`（单命名空间），故施加时若 enemy db 持有
//!   `Condition:<cond>` flag（或本次有 Override 来源）则回填 `cfg.conditions`，
//!   使写入的 debuff mod 在 offence 聚合时命中。

use pobr_data::monster::SHOCK_MAX_EFFECT;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, ModTag, Modifier};

use super::Env;

/// 单个非伤害异常的施加规格（vendor `ailments` 表行，CalcPerform.lua:3078-3125）。
struct AilmentSpec {
    /// vendor canonical 异常名（词条名前缀）："Chill" / "Shock"。
    name: &'static str,
    /// enemy 条件名（`Condition:<X>`）："Chilled" / "Shocked"。
    condition: &'static str,
}

const CHILL: AilmentSpec = AilmentSpec {
    name: "Chill",
    condition: "Chilled",
};
const SHOCK: AilmentSpec = AilmentSpec {
    name: "Shock",
    condition: "Shocked",
};

impl AilmentSpec {
    /// `non_damaging_ailments.json` 的 max（数据通道见模块文档「常量数据通道」）。
    fn data_max(&self, cfg: &CalcConfig) -> f64 {
        match self.name {
            "Chill" => cfg.constants.game().chill_max_effect,
            _ => SHOCK_MAX_EFFECT,
        }
    }
}

/// env_finalize 阶段 7 入口：施加 Chill/Shock，再做 `Multiplier:ChillEffect/ShockEffect`
/// 增量更新（CalcPerform.lua:3127-3180）。无任何来源词条时逐 bit 空转。
pub fn apply_nondamaging_ailments(env: &mut Env) {
    let current_chill = apply_ailment(env, &CHILL);
    let current_shock = apply_ailment(env, &SHOCK);
    // vendor :3173-3180：output.Current<X> 高于现有 multiplier 时补差值（增量更新，
    // 供 `per ChillEffect/ShockEffect` 类词条引用）。未施加（Current=0）时恒空转。
    update_effect_multiplier(env, &CHILL, current_chill);
    update_effect_multiplier(env, &SHOCK, current_shock);
}

/// player db 来源词条的种类（vendor Tabulate 的三个名字）。
enum SourceKind3 {
    Base,
    Override,
    Minimum,
}

/// 施加单个异常，返回 `Current<X>`（未施加 → 0.0）。对照 CalcPerform.lua:3127-3168。
fn apply_ailment(env: &mut Env, spec: &AilmentSpec) -> f64 {
    // cfg 快照：读阶段统一口径；写阶段末尾才回填 cfg.conditions。
    let cfg = env.cfg.clone();
    let player = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;

    let val_name = ModName::from(format!("{}Val", spec.name));
    let enemy_val = enemy.sum(ModType::Base, &cfg, std::slice::from_ref(&val_name));

    // vendor Tabulate("BASE", nil, <X>Base, <X>Override, <X>Minimum)（:3131）。
    let sources = collect_player_sources(player, &cfg, spec);

    // 存在性 gate（vendor :3128-3129 的语义化，见模块文档「已知差异」末两条）。
    if enemy_val <= 0.0 && sources.is_empty() {
        return 0.0;
    }
    // `Condition:Already<cond>` 防重复施加（:3130）。
    let already_name = ModName::from(format!("Condition:Already{}", spec.condition));
    if enemy.flag(&cfg, already_name.clone()) {
        return 0.0;
    }

    // magnitude（仅 Base/Minimum 来源乘，:3144-3146）：skill 侧 × enemy 侧，
    // calcLib.mod = (1 + Σinc/100) × Πmore。
    let magnitude = ailment_magnitude(player, enemy, &cfg, spec);

    let mut strongest = 0.0_f64;
    let mut minimum = 0.0_f64;
    let mut override_seen = false;
    for (kind, raw) in &sources {
        let effect = match kind {
            SourceKind3::Override => {
                override_seen = true;
                *raw
            }
            SourceKind3::Base => raw * magnitude,
            SourceKind3::Minimum => {
                let scaled = raw * magnitude;
                minimum += scaled;
                scaled
            }
        };
        strongest = strongest.max(effect);
    }
    // `override = m_max(m_max(override, effect), minimum)` 逐次取 max 的序无关等价形
    // （minimum 单调不减，末次包含全量）。
    strongest = strongest.max(minimum);

    // Maximum<X>：Override 优先，否则 数据 max + Σ <X>Max BASE（:3153-3163）。
    let max_name = ModName::from(format!("{}Max", spec.name));
    let maximum = player.override_(&cfg, max_name.clone()).unwrap_or_else(|| {
        spec.data_max(&cfg) + player.sum(ModType::Base, &cfg, std::slice::from_ref(&max_name))
    });

    // Current<X> = floor(min(max(override, Σ Val), Maximum) × 10^prec)/10^prec（:3164）；
    // prec=0（non_damaging_ailments.json Chill/Shock）→ 整数 floor。
    let current = strongest.max(enemy_val).min(maximum).floor();

    // 条件桥接判定须在写入前读取（override 来源 / enemy db 既有条件 flag）。
    let cond_flag_name = ModName::from(format!("Condition:{}", spec.condition));
    let condition_active = override_seen || enemy.flag(&cfg, cond_flag_name.clone());

    // Bonechill 分支判定（Chill 专属，:3092-3094；近似口径见模块文档「已知差异」）。
    let bonechill = spec.name == "Chill"
        && player.flag(&cfg, ModName::from("HasBonechill"))
        && (enemy_val > 0.0 || override_seen);

    // ---- 写阶段（读借用结束后统一落 enemy db / cfg.conditions）----
    let origin_id = format!("ailment.{}", spec.name.to_lowercase());
    let origin = || ModifierSource::new(SourceId::new(SourceKind::Derived, origin_id.clone()));
    let cond_tag = || ModTag::condition(spec.condition, false);

    let mut new_mods: Vec<Modifier> = Vec::new();
    // Override 来源 → 置 enemy `Condition:<cond>`（:3136-3138；合并为单条 flag）。
    if override_seen {
        new_mods.push(
            Modifier::flag(cond_flag_name)
                .with_source(spec.name)
                .with_origin(origin()),
        );
    }
    match spec.name {
        // Shock → DamageTaken INC Current {Condition:Shocked}（:3120）。
        "Shock" => new_mods.push(
            Modifier::number(ModName::from("DamageTaken"), ModType::Inc, current)
                .with_tag(cond_tag())
                .with_source(spec.name)
                .with_origin(origin()),
        ),
        // Chill → ActionSpeed INC -Current {Condition:Chilled}（:3089）+ Bonechill。
        _ => {
            new_mods.push(
                Modifier::number(ModName::from("ActionSpeed"), ModType::Inc, -current)
                    .with_tag(cond_tag())
                    .with_source(spec.name)
                    .with_origin(origin()),
            );
            if bonechill {
                new_mods.push(
                    Modifier::number(ModName::from("ColdDamageTaken"), ModType::Inc, current)
                        .with_tag(cond_tag())
                        .with_source("Bonechill")
                        .with_origin(origin()),
                );
            }
        }
    }
    // `Condition:Already<cond>` {Condition:<cond>}（:3168，防 minion 双重施加）。
    new_mods.push(
        Modifier::flag(already_name)
            .with_tag(cond_tag())
            .with_source(spec.name)
            .with_origin(origin()),
    );
    env.enemy.mod_db.add_list(new_mods);

    // 条件桥接（pobr 单条件命名空间，见模块文档）：使上面带 {Condition:<cond>} tag 的
    // debuff mod 在 offence/defence 聚合时命中。
    if condition_active {
        env.cfg.conditions.insert(spec.condition.to_string(), true);
    }

    current
}

/// 收集 player db 的 `<X>Base/<X>Override/<X>Minimum` BASE 型来源词条
/// （vendor `modDB:Tabulate("BASE", …)` 等价：逐条 `matches(cfg)` + Multiplier tag 求值）。
fn collect_player_sources(
    player: &ModDb,
    cfg: &CalcConfig,
    spec: &AilmentSpec,
) -> Vec<(SourceKind3, f64)> {
    let base_name = ModName::from(format!("{}Base", spec.name));
    let override_name = ModName::from(format!("{}Override", spec.name));
    let minimum_name = ModName::from(format!("{}Minimum", spec.name));
    player
        .iter_mods()
        .filter(|modifier| modifier.mod_type == ModType::Base && modifier.matches(cfg))
        .filter_map(|modifier| {
            let kind = if modifier.name == base_name {
                SourceKind3::Base
            } else if modifier.name == override_name {
                SourceKind3::Override
            } else if modifier.name == minimum_name {
                SourceKind3::Minimum
            } else {
                return None;
            };
            Some((kind, modifier.effective_number(cfg)?))
        })
        .collect()
}

/// magnitude 乘子（:3144-3146）：skill 侧 `Enemy<X>Magnitude`/`AilmentMagnitude`
/// （player db）× enemy 侧 `Self<X>Magnitude`/`AilmentMagnitude`（enemy db），
/// 各为 `(1 + Σinc/100) × Πmore`（calcLib.mod）。
fn ailment_magnitude(player: &ModDb, enemy: &ModDb, cfg: &CalcConfig, spec: &AilmentSpec) -> f64 {
    let skill_names = [
        ModName::from(format!("Enemy{}Magnitude", spec.name)),
        ModName::from("AilmentMagnitude"),
    ];
    let enemy_names = [
        ModName::from(format!("Self{}Magnitude", spec.name)),
        ModName::from("AilmentMagnitude"),
    ];
    let skill_side = (1.0 + player.sum(ModType::Inc, cfg, &skill_names) / 100.0)
        * player.more(cfg, &skill_names);
    let enemy_side =
        (1.0 + enemy.sum(ModType::Inc, cfg, &enemy_names) / 100.0) * enemy.more(cfg, &enemy_names);
    skill_side * enemy_side
}

/// `Multiplier:<X>Effect` 增量更新（:3173-3180）：现有 Σ BASE < Current 时补差值。
fn update_effect_multiplier(env: &mut Env, spec: &AilmentSpec, current: f64) {
    let name = ModName::from(format!("Multiplier:{}Effect", spec.name));
    let existing = env
        .enemy
        .mod_db
        .sum(ModType::Base, &env.cfg, std::slice::from_ref(&name));
    if existing < current {
        env.enemy.mod_db.add_mod(
            Modifier::number(name, ModType::Base, current - existing)
                .with_source(spec.name)
                .with_origin(ModifierSource::new(SourceId::new(
                    SourceKind::Derived,
                    format!("ailment.{}", spec.name.to_lowercase()),
                ))),
        );
    }
}
