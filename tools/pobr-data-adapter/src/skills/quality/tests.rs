use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pobr-quality-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let fixture = Self(dir);
        fixture.write(
            "GrantedEffects.json",
            &json!([
                {"_index": 9, "Id": "TestSkill"}, {"_index": 42, "Id": "UnreviewedSkill"}
            ]),
        );
        fixture.write(
            "Stats.json",
            &json!([
                {"_index": 11, "Id": "damage"}, {"_index": 3, "Id": "cost"}
            ]),
        );
        fixture.write("GrantedEffectQualityStats.json", &json!([
            {"GrantedEffect": 9, "Stats": [11, 3, 11], "StatsValuesPermille": [550, -50, 250],
             "ApplyToStatSets": [0, 2], "AltStats": [3], "AltStatValuesPermille": [100], "AltApplyToStatSets": [1]},
            {"GrantedEffect": 42, "Stats": [11], "StatsValuesPermille": [1000],
             "ApplyToStatSets": [], "AltStats": [], "AltStatValuesPermille": [], "AltApplyToStatSets": []}
        ]));
        fixture.write("source.json", &json!({
            "schema":"gem-quality-source/v1", "poe_version":"test", "provenance":{"kind":"synthetic"},
            "inputs":{}, "compatibility": {
                "reference_vendor_commit":"synthetic", "reference_sha256":"synthetic",
                "scope_policy":"preserve_effect_wide_quality", "effects":["TestSkill"],
                "excluded_effects":{"UnreviewedSkill":"Not supported by the fixture"}
            }
        }));
        fixture.review_inputs();
        fixture
    }

    fn args(&self) -> QualityArgs {
        QualityArgs {
            raw: self.0.clone(),
            source: self.0.join("source.json"),
            out: self.0.join("out"),
            patch: "test".into(),
        }
    }

    fn write(&self, name: &str, value: &Value) {
        fs::write(self.0.join(name), serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&fs::read(self.0.join(name)).unwrap()).unwrap()
    }

    fn review_inputs(&self) {
        let mut receipt = self.read("source.json");
        for name in [
            "GrantedEffects.json",
            "Stats.json",
            "GrantedEffectQualityStats.json",
        ] {
            receipt["inputs"][name] = sha256(&fs::read(self.0.join(name)).unwrap()).into();
        }
        self.write("source.json", &receipt);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn resolves_sparse_ids_preserves_order_duplicates_alt_and_scope_diagnostics() {
    let f = Fixture::new();
    let json = build(&f.args()).unwrap();
    assert_eq!(json, build(&f.args()).unwrap());
    let doc: Value = serde_json::from_str(&json).unwrap();
    let loaded: pobr_data::catalog::GemQualityStatsDef = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.effects.len(), 1);
    assert_eq!(
        doc["effects"],
        json!([{"effect_id":"TestSkill","stats":[
            {"stat":"damage","per_quality_rate":0.55},
            {"stat":"cost","per_quality_rate":-0.05},
            {"stat":"damage","per_quality_rate":0.25},
            {"stat":"cost","per_quality_rate":0.1,"alt":true}
        ]}])
    );
    assert_eq!(
        doc["_meta"]["unapplied_stat_set_scopes"]["TestSkill"],
        json!({"main":[0,2],"alt":[1]})
    );
    assert!(!json.contains(f.0.to_str().unwrap()));
}

#[test]
fn stale_or_wrong_version_receipt_preserves_previous_output() {
    let f = Fixture::new();
    run(f.args()).unwrap();
    let dest = f.0.join("out/test/overlay/gem_quality_stats.json");
    let previous = fs::read(&dest).unwrap();
    let mut rows = f.read("GrantedEffectQualityStats.json");
    rows[0]["StatsValuesPermille"][0] = json!(900);
    f.write("GrantedEffectQualityStats.json", &rows);
    assert!(run(f.args()).unwrap_err().contains("fingerprint differs"));
    assert_eq!(previous, fs::read(&dest).unwrap());
    let mut args = f.args();
    args.patch = "other".into();
    assert!(run(args).unwrap_err().contains("game version"));
    assert!(!f.0.join("out/other").exists());
    assert_eq!(fs::read_dir(dest.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn reviewed_inputs_still_require_valid_relationships_and_complete_scope() {
    for (field, value, error) in [
        ("Stats", json!([11]), "length mismatch"),
        ("Stats", json!([11, 999, 11]), "dangling stat"),
        ("GrantedEffect", json!(999), "dangling GrantedEffect"),
        ("AltStatValuesPermille", json!([]), "length mismatch"),
    ] {
        let f = Fixture::new();
        let mut rows = f.read("GrantedEffectQualityStats.json");
        rows[0][field] = value;
        f.write("GrantedEffectQualityStats.json", &rows);
        f.review_inputs();
        assert!(build(&f.args()).unwrap_err().contains(error), "{field}");
    }
    let f = Fixture::new();
    let mut source = f.read("source.json");
    source["compatibility"]["excluded_effects"] = json!({});
    f.write("source.json", &source);
    assert!(
        build(&f.args())
            .unwrap_err()
            .contains("no reviewed compatibility decision")
    );
    source["compatibility"]["excluded_effects"] =
        json!({"UnreviewedSkill":"pending","RemovedSkill":"pending"});
    f.write("source.json", &source);
    assert!(build(&f.args()).unwrap_err().contains("disappeared"));
}

#[test]
fn rejects_missing_columns_duplicate_ids_effects_and_invalid_decisions() {
    let f = Fixture::new();
    let mut rows = f.read("GrantedEffectQualityStats.json");
    rows[0]
        .as_object_mut()
        .unwrap()
        .remove("AltApplyToStatSets");
    f.write("GrantedEffectQualityStats.json", &rows);
    f.review_inputs();
    assert!(build(&f.args()).unwrap_err().contains("missing field"));
    let f = Fixture::new();
    let mut rows = f.read("GrantedEffectQualityStats.json");
    let first = rows[0].clone();
    rows.as_array_mut().unwrap().push(first);
    f.write("GrantedEffectQualityStats.json", &rows);
    f.review_inputs();
    assert!(build(&f.args()).unwrap_err().contains("duplicate effect"));
    let f = Fixture::new();
    f.write(
        "Stats.json",
        &json!([{"_index":11,"Id":"damage"},{"_index":11,"Id":"cost"}]),
    );
    f.review_inputs();
    assert!(
        build(&f.args())
            .unwrap_err()
            .contains("duplicate ID / row index")
    );
    for effects in [
        json!(["TestSkill", "TestSkill"]),
        json!(["UnreviewedSkill"]),
    ] {
        let f = Fixture::new();
        let mut source = f.read("source.json");
        source["compatibility"]["effects"] = effects;
        f.write("source.json", &source);
        assert!(
            build(&f.args())
                .unwrap_err()
                .contains("overlapping quality compatibility scope")
        );
    }
}
