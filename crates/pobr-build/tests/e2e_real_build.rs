//! 端到端集成测试：真实 PoB2 Ninja build code → 完整计算。
//!
//! 目标：验证 `decode → parse_build → calculate_with_data` 管线端到端可以跑通，关键
//! 输出在合理范围内（不要求与 PoB2 数值逐字对齐）。
//!
//! 与早期版本不同：本测试**直接使用生产解析器** [`pobr_build::parse_build_from_code`]，
//! 不再在测试内手写 XML 抽取。这同时把生产 XML→Build 解析路径纳入端到端回归。
//!
//! 装配范围（由 `parse_build` 完成）：
//! - 角色身份（等级 / 职业 / 升华）：完整解析。
//! - 天赋树节点：`<Tree activeSpec>` 选中 `<Spec nodes>`。
//! - 技能宝石组：`<Skills activeSkillSet>` 选中 `<SkillSet>` 下每个 `<Skill>`。
//! - 装备：`<Item>` 文本块经 `parse_pob_xml_item` 解析，`<ItemSet>` 槽位映射到 `EquipmentSlot`。
//! - 角色基础（CharacterBase）：通过 BuildData 职业属性表注入（`inject_character_base`）。
//! - 敌人：Pinnacle level 80（有效 DPS 口径）。
//!
//! 缺口（记录为 deferred，不阻塞 e2e 通过）：
//! - 宝石 → modifier 文本管线尚未导出（见 calc_orchestrator 模块文档）→ 宝石自身
//!   不贡献 modifier，但 source 归因注册正常。
//! - Charm / Flask / Ring 3 等 `EquipmentSlot` 枚举外槽位忽略（不进入当前计算）。
//! - Mastery 效果（masteryEffects）暂未解析 → Mastery 节点词条不进入计算。

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::PassiveTreeSpec;
use pobr_gamedata::{GameData, repo_data_root};

// ── fixtures ────────────────────────────────────────────────────────────────

const DEADEYE_CODE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

const MARTIAL_ARTIST_CODE: &str =
    include_str!("../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

// ── 数据加载（每套测试共用同一 GameData + BuildData）────────────────────────

fn load_game_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("load BuildData from repo data")
}

// ── 测试主体 ─────────────────────────────────────────────────────────────────

/// Deadeye build：端到端完整管线（含装备 / 天赋树 / 宝石 / CharacterBase / 敌人）。
#[test]
fn deadeye_e2e_full_pipeline_succeeds() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye build");

    // 断言：生产解析器还原了装备 / 天赋树 / 宝石组。
    assert!(
        !build.items.is_empty(),
        "expected items from deadeye build, got none"
    );
    assert!(
        !build.tree.allocated_nodes.is_empty(),
        "expected passive nodes from deadeye build, got none"
    );
    assert!(
        !build.socket_groups.is_empty(),
        "expected socket groups from deadeye build, got none"
    );
    // 角色身份解析正确。
    assert_eq!(build.character.level, 98);
    assert_eq!(build.character.class_name, "Ranger");
    assert!(build.character.ascendancy_name.contains("Deadeye"));

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    };

    let out = calculate_with_data(&build, &build_data, &opts)
        .expect("calculate_with_data should succeed for deadeye build");

    // ── 合理性断言 ──────────────────────────────────────────────────────────
    // CharacterBase(level 98 Ranger: 28 + 12*98 + 2*7 = 1218) 来自职业属性表。
    // 装备 / 天赋节点应进一步抬升 life。保守断言：life > CharacterBase 基础值。
    let ranger_char_base_life = 28.0 + 12.0 * 98.0 + 2.0 * 7.0;
    assert!(
        out.life > ranger_char_base_life,
        "life ({}) should exceed CharacterBase alone ({}); items/passives should add more",
        out.life,
        ranger_char_base_life
    );
    assert!(out.life.is_finite(), "life must be finite");

    // 有限值断言：防御字段。
    assert!(out.mana.is_finite(), "mana must be finite");
    assert!(out.armour.is_finite(), "armour must be finite");
    assert!(out.evasion.is_finite(), "evasion must be finite");
    assert!(
        out.energy_shield.is_finite(),
        "energy_shield must be finite"
    );

    // 命中率在有效口径下应在 (0, 1] 内。
    assert!(
        out.hit_chance >= 0.0 && out.hit_chance <= 1.0,
        "hit_chance out of range: {}",
        out.hit_chance
    );

    // 抗性应在合理范围内（百分比口径：最低无限制负数，最高 90 硬上限）。
    for (label, res) in [
        ("fire", out.fire_resistance),
        ("cold", out.cold_resistance),
        ("lightning", out.lightning_resistance),
    ] {
        assert!(
            res.is_finite() && (-200.0..=90.0).contains(&res),
            "{label}_resistance out of range: {res}"
        );
    }
}

/// Deadeye build：分步验证各来源的贡献（基础 vs 天赋 vs 装备）。
#[test]
fn deadeye_e2e_contributions_are_additive() {
    let build_data = load_game_data();
    let full = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye");

    let base_opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };

    // 步骤 1：仅 CharacterBase（无装备 / 无天赋）。
    let build_base = pobr_build::Build::new().with_character(full.character.clone());
    let out_base =
        calculate_with_data(&build_base, &build_data, &base_opts).expect("base only calc");

    // 步骤 2：CharacterBase + 天赋树。
    let build_with_tree = build_base.clone().with_tree(PassiveTreeSpec {
        allocated_nodes: full.tree.allocated_nodes.clone(),
        ..Default::default()
    });
    let out_with_tree =
        calculate_with_data(&build_with_tree, &build_data, &base_opts).expect("with tree calc");

    // 步骤 3：完整 build（CharacterBase + 天赋树 + 装备 + 宝石）。
    let out_full = calculate_with_data(&full, &build_data, &base_opts).expect("full calc");

    // 断言：life 单调递增（CharacterBase ≤ +tree ≤ +items）。
    assert!(
        out_with_tree.life >= out_base.life,
        "adding passive tree should not decrease life: base={} tree={}",
        out_base.life,
        out_with_tree.life
    );
    assert!(
        out_full.life >= out_base.life,
        "full build life ({}) should be >= base only ({})",
        out_full.life,
        out_base.life
    );
}

/// Martial Artist build：非 Ranger 职业端到端管线。
#[test]
fn martial_artist_e2e_full_pipeline_succeeds() {
    let build_data = load_game_data();
    let build = parse_build_from_code(MARTIAL_ARTIST_CODE).expect("parse martial artist build");

    assert_eq!(build.character.class_name, "Monk");
    assert!(!build.items.is_empty(), "expected items");
    assert!(!build.tree.allocated_nodes.is_empty(), "expected nodes");

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let out = calculate_with_data(&build, &build_data, &opts)
        .expect("calculate_with_data should succeed for martial artist build");

    assert!(out.life.is_finite(), "life must be finite");
    assert!(out.mana.is_finite(), "mana must be finite");
    assert!(
        out.life > 0.0,
        "martial artist life should be > 0 (CharacterBase injected)"
    );
}

/// 确定性：相同输入两次调用结果完全一致。
#[test]
fn deadeye_e2e_is_deterministic() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse");

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let out1 = calculate_with_data(&build, &build_data, &opts).expect("first calc");
    let out2 = calculate_with_data(&build, &build_data, &opts).expect("second calc");

    assert_eq!(out1.life, out2.life, "life is not deterministic");
    assert_eq!(out1.mana, out2.mana, "mana is not deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_resistance is not deterministic"
    );
    assert_eq!(out1.dps, out2.dps, "dps is not deterministic");
}

/// 解析确定性：同一 build code 两次解析得到等价 Build（装备槽位→基底映射 / 节点 / 宝石组）。
#[test]
fn parse_build_is_deterministic() {
    let a = parse_build_from_code(DEADEYE_CODE).expect("parse a");
    let b = parse_build_from_code(DEADEYE_CODE).expect("parse b");

    assert_eq!(
        a.tree.allocated_nodes, b.tree.allocated_nodes,
        "allocated nodes differ"
    );
    assert_eq!(
        a.socket_groups, b.socket_groups,
        "socket groups differ (slot / enabled / gem ids)"
    );

    // 槽位→(基底, 词条) 映射逐项一致：证明 HashMap 遍历顺序不影响 item-slot 绑定。
    let mapping = |build: &pobr_build::Build| -> Vec<(String, String, Vec<String>)> {
        build
            .equipped_items()
            .into_iter()
            .map(|(slot, item)| {
                (
                    slot.id().to_string(),
                    item.base.to_string(),
                    item.modifier_texts.clone(),
                )
            })
            .collect()
    };
    assert_eq!(mapping(&a), mapping(&b), "slot→item mapping differs");
}

/// 有效口径 vs 面板口径：mode_effective=true 时命中率应 ≤ mode_effective=false（面板 = 100%）。
#[test]
fn deadeye_e2e_effective_dps_lower_hit_chance() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse");

    let panel_opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };
    let effective_opts = DataOrchestratorOptions {
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..panel_opts.clone()
    };

    let panel = calculate_with_data(&build, &build_data, &panel_opts).expect("panel calc");
    let effective =
        calculate_with_data(&build, &build_data, &effective_opts).expect("effective calc");

    // 面板口径命中率应 ≥ 有效口径（面板不计敌人闪避）。
    assert!(
        panel.hit_chance >= effective.hit_chance,
        "panel hit_chance ({}) should be >= effective ({})",
        panel.hit_chance,
        effective.hit_chance
    );
    assert!(
        effective.hit_chance >= 0.0 && effective.hit_chance <= 1.0,
        "effective hit_chance out of range: {}",
        effective.hit_chance
    );
}
