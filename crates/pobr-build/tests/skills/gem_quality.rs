//! M1-T1 gem quality 四层链路集成测试（18-G1 / 15-G5）。
//!
//! 覆盖：XML `<Gem quality>` 解析 → `BuildData::effect_stats` 品质段（trunc 语义）
//! → 双跑方向断言（stormweaver-comet 15×q20 fixture）。
//!
//! **oracle 对拍**：品质段 golden 值来自 PoB2 真实框架函数
//! `calcLib.buildSkillInstanceStats`（Modules/CalcTools.lua:138）在 vendor commit
//! `2df5a7433dd2f1609e2fad8a6c3c917f923fe34f` 下的输出差值（quality=Q − quality=0），
//! 跑法：`tools/pob2-oracle/quality_stats.lua`（2026-06-11 实跑记录）：
//!
//! ```json
//! {"skill":"CometPlayer","level":21,"quality":20,
//!  "quality_contribution":{"base_spell_%_chance_to_echo":10}}
//! {"skill":"SparkPlayer","level":21,"quality":20,
//!  "quality_contribution":{"base_projectile_speed_+%":30}}
//! {"skill":"ArcticArmourPlayer","level":19,"quality":20,
//!  "quality_contribution":{"maximum_number_of_arctic_armour_stationary_stacks":1}}
//! {"skill":"ArcticArmourPlayer","level":19,"quality":19,"quality_contribution":{}}
//! ```
//!
//! 注意 ArcticArmour q19 = 空（trunc(0.05×19)=trunc(0.95)=0）——PoB2 `math.modf`
//! 截断语义的真实数据实证，PoBR 必须逐值一致。

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::Path;

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData")
}

/// stormweaver-comet fixture（15 个 q20 宝石）的 build code。
fn stormweaver_code() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds/sorceress-stormweaver-comet/code.txt");
    std::fs::read_to_string(path).expect("read stormweaver code.txt")
}

fn opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// trunc 语义单测（蓝图 T1 门禁指定用例）：rate=0.55, q19 → trunc(10.45)=10；
/// 负斜率 toward zero（区别于 floor）；q0 → 空段。合成数据，不依赖真实表。
#[test]
fn quality_segment_truncates_toward_zero() {
    use pobr_data::catalog::QualityStat;
    let mut bd = BuildData::empty();
    bd.gem_quality_stats.insert(
        "SynthSkill".into(),
        vec![
            QualityStat {
                stat: "synth_pos".into(),
                per_quality_rate: 0.55,
            },
            QualityStat {
                stat: "synth_neg".into(),
                per_quality_rate: -0.55,
            },
        ],
    );

    let es = bd.effect_stats("SynthSkill", 20, 19, None);
    let get = |stat: &str| {
        es.quality
            .iter()
            .find(|s| s.stat == stat)
            .map(|s| s.value)
            .expect("stat present")
    };
    assert_eq!(get("synth_pos"), 10.0, "trunc(0.55×19)=trunc(10.45)=10");
    assert_eq!(
        get("synth_neg"),
        -10.0,
        "负斜率 toward zero：trunc(-10.45)=-10（floor 会得 -11）"
    );

    // 品质 0 → 空段；未知技能 → 空段。
    assert!(
        bd.effect_stats("SynthSkill", 20, 0, None)
            .quality
            .is_empty()
    );
    assert!(bd.effect_stats("NoSuch", 20, 20, None).quality.is_empty());
}

/// 真实数据 + oracle 对拍：q20 各效果的品质段与 PoB2 `buildSkillInstanceStats`
/// 差值逐值一致（golden 见模块 doc 的 oracle 实跑记录）。
#[test]
fn quality_segment_matches_pob2_oracle() {
    let bd = load_data();
    let seg = |skill: &str, level: u32, q: u32| -> Vec<(String, f64)> {
        bd.effect_stats(skill, level, q, None)
            .quality
            .iter()
            .filter(|s| s.value != 0.0)
            .map(|s| (s.stat.clone(), s.value))
            .collect()
    };

    assert_eq!(
        seg("CometPlayer", 21, 20),
        vec![("base_spell_%_chance_to_echo".to_string(), 10.0)]
    );
    assert_eq!(
        seg("SparkPlayer", 21, 20),
        vec![("base_projectile_speed_+%".to_string(), 30.0)]
    );
    assert_eq!(
        seg("ArcticArmourPlayer", 19, 20),
        vec![(
            "maximum_number_of_arctic_armour_stationary_stacks".to_string(),
            1.0
        )]
    );
    // trunc 实证（oracle：q19 贡献为空，trunc(0.95)=0）。
    assert_eq!(seg("ArcticArmourPlayer", 19, 19), vec![]);
    // support 效果：品质表无条目（PoB2 导出条件已跳过）→ 恒空段。
    assert_eq!(seg("SupportSpellEchoPlayer", 19, 20), vec![]);
}

/// q20 fixture（stormweaver-comet）端到端：XML quality 解析进 build 模型 +
/// 双跑方向断言（品质生效 vs 品质归零，DPS/防御不得下降）。
#[test]
fn stormweaver_comet_q20_fixture_dual_run() {
    let bd = load_data();
    let build = parse_build_from_code(stormweaver_code().trim()).expect("parse build");

    // XML 解析层：fixture 含 15 个 quality=20 宝石（decoded.xml 实数）。
    let q20_count: usize = build
        .socket_groups
        .iter()
        .flat_map(|g| g.gem_skills.iter())
        .filter(|g| g.quality == 20)
        .count();
    assert_eq!(
        q20_count, 15,
        "decoded.xml 的 15 个 q20 宝石应全部进入 build 模型"
    );

    // 双跑：品质归零副本（其余完全一致）。
    let mut stripped = build.clone();
    for group in &mut stripped.socket_groups {
        group.active_gem_quality = Some(0);
        for gem in &mut group.gem_skills {
            gem.quality = 0;
        }
    }

    let with_q = calculate_with_data(&build, &bd, &opts()).expect("calc with quality");
    let no_q = calculate_with_data(&stripped, &bd, &opts()).expect("calc without quality");

    assert!(with_q.dps.is_finite() && with_q.life.is_finite());
    // 方向断言：品质只会增益（本 build 的品质 stat 均为正向），DPS/防御不得下降。
    assert!(
        with_q.dps >= no_q.dps,
        "quality 生效后 DPS 不得下降：{} < {}",
        with_q.dps,
        no_q.dps
    );
    assert!(
        with_q.life >= no_q.life && with_q.energy_shield >= no_q.energy_shield,
        "quality 生效后防御不得下降"
    );
}
