//! Load tests for the data prerequisites: `overlay/minions.json` /
//! `overlay/spectres.json` / `overlay/granted_effect_minions.json` /
//! `overlay/mirage_configs.json`.
//!
//! Every sample assertion notes its vendor line-number source (commit
//! `2df5a74`, see `vendor/.pob2-version.txt`); the migration-invariant
//! check compares against `pobr_data::minion::minion_def_*`'s
//! hand-transcribed constants.

use pobr_data::catalog::actors::{LuaValueDef, MinionEntryDef, MinionsDef};
use pobr_data::minion::{
    MinionDef, minion_def_raging_spirit, minion_def_skeletal_storm_mage,
    minion_def_skeletal_warrior, minion_def_zombie,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

fn find<'a>(def: &'a MinionsDef, id: &str) -> &'a MinionEntryDef {
    def.minions
        .iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("entry {id} missing"))
}

/// Compares the extracted entry (overlay) against the hand-transcribed
/// constant (`pobr_data::minion`, transcribed 2025-06) **numeric fields**
/// value-for-value — the value-by-value check for the migration invariant.
///
/// Known discrepancies (vendor is authoritative, recorded here):
/// - The hand-transcribed `monster_tags` is a reduced subset (e.g. the
///   zombie's hand-transcription has 4 entries vs. vendor
///   Minions.lua:11's full 10) — not a numeric-convention issue, to be
///   reconciled with schema v2 when the hand-transcription is deleted at A6;
/// - The hand-transcription has no `attack_range`/`accuracy`/
///   `base_movement_speed`/`weapon_type1` (new columns in schema v2), so
///   they're outside this comparison's scope.
fn assert_matches_handwritten(entry: &MinionEntryDef, hand: &MinionDef) {
    assert_eq!(entry.name, hand.name, "{}: name", hand.id);
    assert_eq!(entry.life, hand.life, "{}: life", hand.id);
    assert_eq!(entry.damage, hand.damage, "{}: damage", hand.id);
    assert_eq!(
        entry.damage_spread, hand.damage_spread,
        "{}: damage_spread",
        hand.id
    );
    assert_eq!(
        entry.attack_time, hand.attack_time,
        "{}: attack_time",
        hand.id
    );
    assert_eq!(
        entry.crit_chance, hand.crit_chance,
        "{}: crit_chance",
        hand.id
    );
    // Optional-field default semantics: armour/evasion absent = 1.0, energyShield absent = 0.0
    assert_eq!(
        entry.armour.unwrap_or(1.0),
        hand.armour,
        "{}: armour",
        hand.id
    );
    assert_eq!(
        entry.evasion.unwrap_or(1.0),
        hand.evasion,
        "{}: evasion",
        hand.id
    );
    assert_eq!(
        entry.energy_shield.unwrap_or(0.0),
        hand.energy_shield,
        "{}: energy_shield",
        hand.id
    );
    assert_eq!(entry.fire_resist, hand.fire_resist, "{}: fire", hand.id);
    assert_eq!(entry.cold_resist, hand.cold_resist, "{}: cold", hand.id);
    assert_eq!(
        entry.lightning_resist, hand.lightning_resist,
        "{}: lightning",
        hand.id
    );
    assert_eq!(entry.chaos_resist, hand.chaos_resist, "{}: chaos", hand.id);
    assert_eq!(
        entry.base_damage_ignores_attack_speed, hand.base_damage_ignores_attack_speed,
        "{}: base_damage_ignores_attack_speed",
        hand.id
    );
    assert_eq!(
        entry.limit.as_deref().unwrap_or(""),
        hand.limit.to_pob2_str(),
        "{}: limit",
        hand.id
    );
    assert_eq!(
        entry.spectre_reservation, hand.spectre_reservation,
        "{}: spectre_reservation",
        hand.id
    );
    assert_eq!(
        entry.companion_reservation, hand.companion_reservation,
        "{}: companion_reservation",
        hand.id
    );
    assert_eq!(entry.skill_list, hand.skill_list, "{}: skill_list", hand.id);
}

/// minions.json: 32 entries (vendor Data/Minions.lua in full), ascending by id.
#[test]
fn minions_count_and_order() {
    let def = game_data()
        .minions()
        .unwrap()
        .expect("minions.json should be present");
    assert_eq!(def.minions.len(), 32, "Minions.lua entry count");
    let ids: Vec<&str> = def.minions.iter().map(|m| m.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "ascending by id");
}

/// Value-for-value check of the 4 hand-transcribed constants (a migration invariant).
#[test]
fn minions_match_handwritten_constants() {
    let def = game_data().minions().unwrap().unwrap();
    assert_matches_handwritten(find(&def, "RaisedZombie"), &minion_def_zombie());
    assert_matches_handwritten(
        find(&def, "SummonedRagingSpirit"),
        &minion_def_raging_spirit(),
    );
    assert_matches_handwritten(
        find(&def, "RaisedSkeletonWarriors"),
        &minion_def_skeletal_warrior(),
    );
    assert_matches_handwritten(
        find(&def, "RaisedSkeletonStormMage"),
        &minion_def_skeletal_storm_mage(),
    );
}

/// A field-by-field spot check of RaisedZombie (vendor Minions.lua:9-21 +
/// the weaponType1 column).
#[test]
fn zombie_fields_from_vendor() {
    let def = game_data().minions().unwrap().unwrap();
    let z = find(&def, "RaisedZombie");
    assert_eq!(z.life, 0.7); // Minions.lua:12
    assert_eq!(z.damage, 0.75); // :18
    assert_eq!(z.attack_time, 1.25); // :21
    assert!(z.base_damage_ignores_attack_speed); // :13
    assert_eq!(z.monster_tags.len(), 10, "vendor has 10 tags in full (:11)");
    assert_eq!(z.weapon_type1.as_deref(), Some("One Hand Axe"));
}

/// SummonedRagingSpirit's modList fully serialized (all of `mod()`'s
/// arguments; vendor Minions.lua:68's `mod("Speed", "MORE", 40, 1, 0)`).
#[test]
fn raging_spirit_mod_list_full_args() {
    let def = game_data().minions().unwrap().unwrap();
    let r = find(&def, "SummonedRagingSpirit");
    assert_eq!(r.mod_list.len(), 1);
    let m = &r.mod_list[0];
    assert_eq!(m.name, "Speed");
    assert_eq!(m.mod_type, "MORE");
    assert_eq!(m.value, LuaValueDef::Number(40.0));
    assert_eq!(m.flags, Some(LuaValueDef::Number(1.0)));
    assert_eq!(m.keyword_flags, Some(LuaValueDef::Number(0.0)));
    assert!(m.tags.is_empty());
}

/// spectres.json: 617 distinct entries (vendor 0.5.4).
///
/// Note: vendor Data/Spectres.lua has 619 assignment blocks, 2 of which
/// share a key — Lua table semantics means the later write wins, and
/// PoB2's runtime likewise only ever sees 617 entries; this table is
/// faithful to that runtime semantics.
#[test]
fn spectres_count() {
    let def = game_data()
        .spectres()
        .unwrap()
        .expect("spectres.json should be present");
    assert_eq!(def.minions.len(), 617);
}

/// A spot check on Lightless Abomination (vendor Spectres.lua:10-30 +
/// :49's modList): life=2.2 (lowered in 0.5.4) / armour=0.4 /
/// fireResist=75 / StunDuration OVERRIDE 3.
#[test]
fn spectre_lightless_abomination() {
    let def = game_data().spectres().unwrap().unwrap();
    let c = find(
        &def,
        "Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre",
    );
    assert_eq!(c.name, "Lightless Abomination");
    assert_eq!(c.life, 2.2);
    assert_eq!(c.armour, Some(0.4));
    assert_eq!(c.fire_resist, 75.0);
    assert_eq!(c.spectre_reservation, 90.0);
    assert_eq!(c.monster_category.as_deref(), Some("Demon"));
    let m = &c.mod_list[0];
    assert_eq!(
        (m.name.as_str(), m.mod_type.as_str()),
        ("StunDuration", "OVERRIDE")
    );
    assert_eq!(m.value, LuaValueDef::Number(3.0));
}

/// granted_effect_minions.json: a sample of the foreign-key sidecar.
#[test]
fn granted_effect_minions_samples() {
    let def = game_data()
        .granted_effect_minions()
        .unwrap()
        .expect("granted_effect_minions.json should be present");
    assert!(
        def.entries.len() >= 25,
        "foreign-key sidecar entry count (measured 31)"
    );
    let ids: Vec<&str> = def.entries.iter().map(|e| e.effect_id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "ascending by effect_id");
    let find = |id: &str| {
        def.entries
            .iter()
            .find(|e| e.effect_id == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    };
    // act_int.lua:16180-16186 RagingSpiritsPlayer.minionList
    assert_eq!(
        find("RagingSpiritsPlayer").minion_list,
        ["SummonedRagingSpirit"]
    );
    // sup_int.lua:6250-6258 TriggeredLivingLightningPlayer.minionList
    assert_eq!(
        find("TriggeredLivingLightningPlayer").minion_list,
        ["LivingLightning"]
    );
    // other.lua:10523/10590-10592 Manifest Weapon: minionList + borrowed weapon slot + item set
    let manifest = find("ManifestWeaponPlayer");
    assert_eq!(manifest.minion_list, ["ManifestWeapon"]);
    assert_eq!(manifest.minion_uses, ["Weapon 1"]);
    assert!(manifest.minion_has_item_set);
    // Summon-family active skills include at least the skeleton/zombie kinds
    assert!(
        def.entries
            .iter()
            .any(|e| e.minion_list.iter().any(|m| m == "RaisedZombie")),
        "RaisedZombie foreign key should exist"
    );
    assert!(
        def.entries.iter().any(|e| e
            .minion_list
            .iter()
            .any(|m| m.starts_with("RaisedSkeleton"))),
        "skeleton-family foreign key should exist"
    );
}

/// The A3 merge: `granted_effects()` folds the
/// `granted_effect_minions.json` sidecar into `GrantedEffectDef.minion_list`
/// and other fields at load time.
#[test]
fn granted_effects_merge_minion_list() {
    let effects = game_data().granted_effects().unwrap();
    let by_id: std::collections::HashMap<&str, &_> =
        effects.iter().map(|e| (e.id.as_str(), e)).collect();
    // RaiseZombiePlayer → [RaisedZombie]
    let zombie = by_id
        .get("RaiseZombiePlayer")
        .expect("RaiseZombiePlayer should be present");
    assert_eq!(zombie.minion_list, ["RaisedZombie"]);
    // RagingSpiritsPlayer → [SummonedRagingSpirit]
    assert_eq!(
        by_id
            .get("RagingSpiritsPlayer")
            .expect("RagingSpiritsPlayer should be present")
            .minion_list,
        ["SummonedRagingSpirit"]
    );
    // Manifest Weapon: minion_uses + item set are also merged in
    let manifest = by_id
        .get("ManifestWeaponPlayer")
        .expect("ManifestWeaponPlayer should be present");
    assert_eq!(manifest.minion_uses, ["Weapon 1"]);
    assert!(manifest.minion_has_item_set);
    // A non-summon skill's (e.g. Fireball) minion_list should be empty
    // (backward compatible)
    if let Some(fb) = by_id.get("FireballPlayer") {
        assert!(
            fb.minion_list.is_empty(),
            "non-summon skill minion_list should be empty"
        );
    }
}

/// mirage_configs.json: 5 configs (vendor CalcMirages.lua's five
/// branches), a spot check on mirage_archer's stat names (:74-76).
#[test]
fn mirage_configs_five_branches() {
    let def = game_data()
        .mirage_configs()
        .unwrap()
        .expect("mirage_configs.json should be present");
    assert_eq!(def.configs.len(), 5);
    let ids: Vec<&str> = def.configs.iter().map(|c| c.mirage_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "generals_cry",
            "mirage_archer",
            "sacred_wisps",
            "saviour_mirage_warriors",
            "tawhoas_chosen"
        ]
    );
    let archer = &def.configs[1];
    assert_eq!(
        archer.trigger.skill_data_flag.as_deref(),
        Some("triggeredByMirageArcher")
    );
    assert_eq!(archer.count_stat.as_deref(), Some("MirageArcherMaxCount"));
    assert_eq!(
        archer.less_damage_stat.as_deref(),
        Some("MirageArcherLessDamage")
    );
    assert_eq!(
        archer.less_attack_speed_stat.as_deref(),
        Some("MirageArcherLessAttackSpeed")
    );
    assert_eq!(
        archer.source_skill_filter.weapon_type.as_deref(),
        Some("Bow")
    );
    assert!(archer.calc_main_skill_offence);
    assert!(archer.handler_id.is_none());
}

/// Missing-table tolerance: in an empty directory, every new domain
/// returns Ok(None), no panic.
#[test]
fn missing_overlay_files_yield_none() {
    let dir = std::env::temp_dir().join(format!("pobr-pre-m5a-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = GameData::new(&dir);
    assert!(data.minions().unwrap().is_none());
    assert!(data.spectres().unwrap().is_none());
    assert!(data.granted_effect_minions().unwrap().is_none());
    assert!(data.mirage_configs().unwrap().is_none());
}
