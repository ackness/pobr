//! Integration test for loading + merging `overlay/skill_overrides.json`
//! (real repo data).
//!
//! Migration invariant: `base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` are **pure adapter output**;
//! vendor values the `.dat` can't provide are merged in from the overlay
//! at load time — this test locks "plain base + the value in effect after
//! merge" value-equal to the historical hand-patched base (aggregate row
//! counts + representative skill point values).
//!
//! Channel narrowing: critChance / attackSpeedMultiplier were switched to
//! reading the `.dat` table columns directly into base/
//! (GrantedEffectStatSetsPerLevel's two crit columns /
//! GrantedEffectsPerLevel's `AttackSpeedMultiplier`), switched over after
//! the historical overlay values (3911 + 3578 entries) were verified
//! value-equal; the overlay sidecar now only has base_multiplier
//! (per-level) + skill_attack_speed_more (a statSet constant). Reading the
//! table column directly **adds** values for monster/non-vendor skills
//! beyond vendor's coverage (crit +2180 rows, attspd +678 rows) — for
//! skills vendor already covers, there are zero additions and zero drift
//! (T4.3 verified).

use pobr_gamedata::GameData;

fn repo_game_data() -> GameData {
    GameData::new(pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version()))
}

/// The overlay document loads, and has already been narrowed to two stat
/// kinds (crit/attspd switched to reading the table column directly, and
/// must no longer appear in the sidecar — their appearance would mean the
/// extract script regressed).
#[test]
fn loads_skill_overrides_overlay() {
    let overrides = repo_game_data()
        .skill_overrides()
        .expect("加载 skill_overrides overlay")
        .expect("仓库数据包应包含 overlay/skill_overrides.json");
    let has = |stat: &str| overrides.overrides.iter().any(|o| o.stat == stat);
    assert!(has("base_multiplier"));
    assert!(has("skill_attack_speed_more"));
    assert!(has("dot_is_area"), "M4-T4 W-D1 dotIs* 抽取段不得回退");
    assert!(!has("crit_chance"), "crit 已表列直读，不得回流 overlay");
    assert!(
        !has("attack_speed_multiplier"),
        "attspd 已表列直读，不得回流 overlay"
    );
}

/// Aggregate row counts after the level-domain merge.
///
/// crit / attspd: the historical vendor-covered values (3911 / 3578
/// entries) are kept value-for-value, and reading the table column
/// directly adds values for monster/non-vendor skills → totals 6091 / 4256;
/// base_multiplier still goes through the overlay merge (6821 unchanged).
/// Level-field-family override counts: CostMultiplier≠100 = 542 /
/// Reservation≠0 = 3166 / EffectOnPlayer≠100 = 169 / StoredUses≠0 = 6721.
#[test]
fn merged_levels_match_historical_coverage() {
    let levels = repo_game_data()
        .granted_effect_levels()
        .expect("加载 + merge granted_effect_levels");

    let count = |f: fn(&pobr_data::catalog::SkillLevelDef) -> bool| {
        levels.values().flatten().filter(|r| f(r)).count()
    };
    // After the 4.5.4.3 (0.5.4b) data upgrade, crit/base_multiplier/stored_uses
    // grew along with newly added skill level rows (6091→6294 / 6821→6850 /
    // 6721→6734); the other coverage counts are unchanged.
    assert_eq!(count(|r| r.crit_chance.is_some()), 6294);
    assert_eq!(count(|r| r.attack_speed_multiplier.is_some()), 4256);
    assert_eq!(count(|r| r.base_multiplier.is_some()), 6850);
    // The level-field family (read directly by the adapter, trivial values
    // normalized to None).
    assert_eq!(count(|r| r.mana_multiplier.is_some()), 542);
    assert_eq!(count(|r| r.spirit_reservation_flat.is_some()), 3166);
    assert_eq!(count(|r| r.reservation_multiplier.is_some()), 169);
    assert_eq!(count(|r| r.stored_uses.is_some()), 6734);
    // level_requirement: PoE2's `.dat` has no such column (the real source
    // table isn't downloadable), so it's always None, and stored as such.
    assert_eq!(count(|r| r.level_requirement.is_some()), 0);
}

/// Representative point values (compared line-for-line against vendor PoB2 Lua).
#[test]
fn merged_levels_spot_values() {
    let data = repo_game_data();
    let levels = data.granted_effect_levels().expect("加载等级域");

    // Flicker Strike: attackSpeedMultiplier -50 (same value at every level).
    let flicker = &levels["FlickerStrikePlayer"];
    assert!(
        flicker
            .iter()
            .all(|r| r.attack_speed_multiplier == Some(-50.0))
    );

    // Arc: critChance 9 (same value at every level).
    let arc = &levels["ArcPlayer"];
    assert!(arc.iter().all(|r| r.crit_chance == Some(9.0)));

    // RisenArbalestSnipe: vendor only has baseMultiplier 2.65 at L1 — the
    // per_level breakdown must not wrongly fill in the missing levels
    // (a regression anchor for the compression-condition fix).
    let snipe = &levels["RisenArbalestSnipe"];
    assert_eq!(snipe[0].base_multiplier, Some(2.65));
    assert!(snipe[1..].iter().all(|r| r.base_multiplier.is_none()));

    // Level-field-family point values (compared line-for-line against
    // vendor Data/Skills/*.lua):
    // Acrimony support: manaMultiplier 10 (sup_int.lua's `manaMultiplier = 10`).
    let acrimony = &levels["SupportAcrimonyPlayer"];
    assert_eq!(acrimony[0].mana_multiplier, Some(10.0));
    // Alchemist's Boon (a sustained effect): spiritReservationFlat 30.
    let boon = &levels["AlchemistsBoonPlayer"];
    assert_eq!(boon[0].spirit_reservation_flat, Some(30.0));
    // Impurity: manaMultiplier -100 + reservationMultiplier -100 (both
    // multipliers normalized while preserving sign).
    let impurity = &levels["ImpurityPlayer"];
    assert_eq!(impurity[0].mana_multiplier, Some(-100.0));
    assert_eq!(impurity[0].reservation_multiplier, Some(-100.0));

    // statSet-level: Flicker's inherent attack speed MORE 285 (a PoB2
    // baseMods constant).
    let sets = data.skill_stat_sets().expect("加载 stat-set 域");
    let flicker_set = sets
        .iter()
        .find(|s| s.effect_id == "FlickerStrikePlayer")
        .and_then(|def| def.sets.first())
        .expect("FlickerStrikePlayer 主 stat-set 存在");
    assert_eq!(flicker_set.skill_attack_speed_more, Some(285.0));

    //  dotIs* boolean — vendor's only full-data entry, TornadoShotPlayer's
    // statSets[2] ("Tornado")'s dotIsArea, matches TornadoShotNovaPlayer's
    // set by vendor index; the primary set keeps the conservative default
    // (unverified, all false).
    let tornado = sets
        .iter()
        .find(|s| s.effect_id == "TornadoShotPlayer")
        .expect("TornadoShotPlayer stat-set 存在");
    let nova = tornado
        .sets
        .iter()
        .find(|s| s.set_id == "TornadoShotNovaPlayer")
        .expect("Tornado 形态 set 存在");
    assert!(nova.dot_flags.area, "dotIsArea 应命中 vendor statSets[2]");
    assert!(nova.dot_flags.verified);
    assert!(
        tornado.sets[0].dot_flags.is_default(),
        "主 set（Impact）未核验，保守默认"
    );

    //  explodeCorpse boolean (vendor statSet baseMods, act_int.lua:5287;
    // CalcOffence.lua:2213's corpse-explosion base-damage gate) —
    // DetonateDeadPlayer's primary set matches; skills without this
    // baseMod keep the default false.
    let dd = sets
        .iter()
        .find(|s| s.effect_id == "DetonateDeadPlayer")
        .expect("DetonateDeadPlayer stat-set 存在");
    assert!(dd.sets[0].explode_corpse, "DD 主 set explodeCorpse");
    assert!(!flicker_set.explode_corpse, "无 baseMod 的技能缺省 false");
}

/// Plain base doesn't contain overlay-exclusive fields — ensures these
/// values only ever come from the merge, with no more hand-patched drift
/// (the semantic equivalent of a zero regen-check byte-diff). The
/// overlay-exclusive fields have since narrowed to base_multiplier (level
/// domain) + skill_attack_speed_more (statSet domain); crit / attspd are
/// already base fields read directly from the table columns, so they're
/// outside this assertion's scope.
#[test]
fn pure_base_has_no_overlay_fields() {
    // Builds a temp data directory without an overlay (base symlinked to
    // the repo's files, avoiding a large-file copy).
    let dir = std::env::temp_dir().join(format!(
        "pobr-gamedata-skill-overrides-pure-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("base")).unwrap();
    let repo_base = pobr_gamedata::repo_data_root()
        .join(pobr_gamedata::data_version())
        .join("base");
    for f in [
        "granted_effect_levels.json",
        "granted_effect_stat_sets.json",
    ] {
        std::os::unix::fs::symlink(repo_base.join(f), dir.join("base").join(f)).unwrap();
    }

    let data = GameData::new(&dir);
    assert!(
        data.skill_overrides().unwrap().is_none(),
        "无 overlay 文件时应返回 None（行为 = 纯 base）"
    );
    let levels = data.granted_effect_levels().expect("纯 base 可加载");
    assert!(
        levels
            .values()
            .flatten()
            .all(|r| r.base_multiplier.is_none()),
        "纯 base 不得含 base_multiplier（该列在 .dat 导出中不存在，值只来自 overlay merge）"
    );
    // The table-column-direct-read fields **should** exist in plain base
    // (unrelated to overlay) — anchors that the channel switchover can't
    // regress.
    assert!(
        levels.values().flatten().any(|r| r.crit_chance.is_some()),
        "crit_chance 是表列直读的 base 字段（M1-T4.3），纯 base 不得为空"
    );
    assert!(
        levels
            .values()
            .flatten()
            .any(|r| r.attack_speed_multiplier.is_some()),
        "attack_speed_multiplier 是表列直读的 base 字段（M1-T4.3），纯 base 不得为空"
    );
    let sets = data.skill_stat_sets().expect("纯 base stat-set 可加载");
    assert!(
        sets.iter()
            .flat_map(|s| &s.sets)
            .all(|s| s.skill_attack_speed_more.is_none())
    );
}
