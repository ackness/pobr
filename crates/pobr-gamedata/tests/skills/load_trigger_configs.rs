//! Load test for `overlay/trigger_configs.json`
//! (schema in [`pobr_data::catalog::triggers`]; the storage side of the
//! guardrail's "61-entry extraction count assertion" + handler count monitoring).

use pobr_data::catalog::triggers::TriggerConfigsDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> TriggerConfigsDef {
    GameData::new(repo_data_root().join(version()))
        .trigger_configs()
        .unwrap()
        .expect("trigger_configs.json should be present")
}

/// The 61-entry count assertion (a drift guardrail: vendor's configTable
/// entry count = the stored entry count); keys are unique and ascending.
#[test]
fn sixty_one_entries_sorted_unique() {
    let def = load();
    assert_eq!(
        def.configs.len(),
        61,
        "vendor configTable's 61 entries fully ingested"
    );
    assert!(
        def.configs
            .windows(2)
            .all(|w| w[0].key.name < w[1].key.name),
        "key.name should be strictly ascending (unique)"
    );
}

/// Handler-entry discipline: the `trigger:` prefix + count monitoring (doc
/// 20 §5's overall-phase <100 gate; currently 15 entries — any change must
/// update the tracking ledger).
#[test]
fn handler_discipline() {
    let def = load();
    let handlers: Vec<&str> = def
        .configs
        .iter()
        .filter_map(|c| c.handler_id.as_deref())
        .collect();
    assert_eq!(
        handlers.len(),
        15,
        "a change in handler entry count must be synced to the tracking ledger"
    );
    assert!(
        handlers.len() < 100,
        "handler total exceeds doc 20 §5's monitoring gate"
    );
    for h in handlers {
        assert!(
            h.starts_with("trigger:"),
            "handler id {h} is missing the trigger: prefix"
        );
    }
}

/// Curation discipline: every stored entry is verified:false; each carries
/// a vendor line-range anchor + a valid kind; a restricted predicate is
/// capped at three fields (the schema itself is three fields — this
/// asserts a non-empty predicate is constrained).
#[test]
fn curation_discipline() {
    let def = load();
    for c in &def.configs {
        assert!(
            !c.verified,
            "{}: stored entries must be verified:false",
            c.key.name
        );
        assert!(
            c.vendor_ref.starts_with("Modules/CalcTriggers.lua:"),
            "{}: missing vendor anchor",
            c.key.name
        );
        assert!(
            matches!(
                c.key.kind.as_str(),
                "skill" | "triggered_by" | "unique_item"
            ),
            "{}: invalid kind {}",
            c.key.name,
            c.key.kind
        );
        for cond in [&c.source_skill_cond, &c.triggered_skill_cond]
            .into_iter()
            .flatten()
        {
            assert!(
                !cond.is_empty(),
                "{}: an empty predicate should omit the field",
                c.key.name
            );
        }
    }
}

/// A spot check on the CoC entry (the data prerequisite for folding in
/// crit): trigger_on_crit + the source-skill predicate + the PoE2 join key
/// `MetaCastOnCritPlayer`.
#[test]
fn coc_entry_shape() {
    let def = load();
    let coc = def
        .configs
        .iter()
        .find(|c| c.key.name == "cast on critical strike")
        .expect("missing CoC entry");
    assert!(coc.trigger_on_crit);
    assert!(
        coc.match_effect_ids
            .iter()
            .any(|id| id == "MetaCastOnCritPlayer")
    );
    let cond = coc
        .source_skill_cond
        .as_ref()
        .expect("CoC source predicate");
    assert!(cond.any_skill_types.iter().any(|t| t == "Attack"));
}

/// A restricted-predicate spot check: Law of the Wilds' any/all/not three sections.
#[test]
fn law_of_the_wilds_predicate() {
    let def = load();
    let entry = def
        .configs
        .iter()
        .find(|c| c.key.name == "law of the wilds")
        .expect("missing law of the wilds");
    let cond = entry.source_skill_cond.as_ref().expect("source predicate");
    assert_eq!(cond.any_skill_types, vec!["Melee", "Attack"]);
    assert_eq!(cond.all_mod_flags, vec!["Claw"]);
    assert_eq!(cond.not_skill_types, vec!["SummonsTotem"]);
}

/// Missing-table tolerance: a non-existent version directory returns
/// Ok(None), not an error.
#[test]
fn missing_overlay_tolerated() {
    let missing = GameData::new(repo_data_root().join("0.0.0.0-nonexistent"))
        .trigger_configs()
        .unwrap();
    assert!(missing.is_none());
}
