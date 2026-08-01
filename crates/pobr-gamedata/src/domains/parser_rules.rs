//! `overlay/mod_parser_rules.json` loader — the ModParser rule six-table
//! set (special excluded) + the §1.7 small lookup tables, schema in
//! [`pobr_data::catalog::parser_rules`]
//!
//! Data source: vendor PoB2 `Modules/ModParser.lua`, extracted by
//! `sync-pob-catalog extract-lua --what parser-rules` bootstrapping
//! headless (schema id `mod_parser_rules/v1`). Consumer = the data-driven
//! scan engine (`CompiledParserRules::compile` lives in pobr-core; this
//! loader has zero semantics, zero compilation, keeping the I/O boundary —
//! gamedata only loads). Actually populating `RuleSet.parser_rules` is a
//! wiring-up concern; this loader lands with the data first.

use pobr_data::catalog::parser_rules::ModParserRulesDoc;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the ModParser rule tables (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (an old data pack
    /// without this overlay domain) — the consumer behaves as "the data
    /// engine is unavailable" (the dual-run framework can only use
    /// Legacy, backward compatible); other I/O / parse errors still
    /// propagate, not silenced.
    pub fn mod_parser_rules(&self) -> Result<Option<ModParserRulesDoc>, LoadError> {
        match self.load_json_at::<ModParserRulesDoc>(self.overlay_path("mod_parser_rules.json")) {
            Ok(doc) => Ok(Some(doc)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::stat_map::StatMapValue;

    use crate::GameData;

    fn golden_version_dir() -> std::path::PathBuf {
        crate::repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION)
    }

    fn real_data() -> GameData {
        // This module's tests pin section counts / vendor-commit real
        // samples (version-specific, growing with extraction), so it loads
        // the golden verification version rather than the active version
        // (decoupling data from calc, see
        // pobr_data::GOLDEN_PARITY_DATA_VERSION).
        GameData::new(golden_version_dir())
    }

    /// Repo real data: every section's entry count matches the blessed
    /// snapshot (a consumer-side mirror of the count self-check). Counts
    /// grow with vendor extraction — the actual numbers live in
    /// `generated/test_pins.json` (refreshed with `POBR_BLESS_PINS=1`
    /// after a regen, see [`crate::test_pins`]); this function only keeps
    /// the structural guard (core form ids must be present).
    #[test]
    fn real_data_section_counts() {
        let doc = real_data()
            .mod_parser_rules()
            .expect("加载 mod_parser_rules.json 不应失败")
            .expect("仓库数据包应含 mod_parser_rules 域");
        let forms: std::collections::BTreeSet<&str> =
            doc.forms.iter().map(|f| f.form.as_str()).collect();
        // flag_types includes a pobr-added entry `hindered`→`Condition:Hindered`
        // (see m6-dualrun-report §2.5), so the count is always vendor + 1.
        crate::test_pins::assert_pin(
            &golden_version_dir(),
            "parser_rules.section_counts",
            serde_json::json!({
                "forms": doc.forms.len(),
                "name_map": doc.name_map.len(),
                "flag_phrases": doc.flag_phrases.len(),
                "pre_flags": doc.pre_flags.len(),
                "tag_phrases": doc.tag_phrases.len(),
                "suffix_types": doc.suffix_types.len(),
                "damage_types": doc.damage_types.len(),
                "pen_types": doc.pen_types.len(),
                "regen_types": doc.regen_types.len(),
                "degen_types": doc.degen_types.len(),
                "cost_types_map": doc.cost_types_map.len(),
                "base_cost_types": doc.base_cost_types.len(),
                "flag_types": doc.flag_types.len(),
                "distinct_forms": forms.len(),
                "unsupported": doc.unsupported,
                "unsupported_pobr_extra": doc.unsupported_pobr_extra,
            }),
        );
        // Structural guard: a missing core form id means the extraction
        // channel is broken — it shouldn't drift with the data.
        for id in [
            "INC", "RED", "MORE", "LESS", "BASE", "PEN", "DMG", "DOUBLED",
        ] {
            assert!(forms.contains(id), "form id 集应含 {id}");
        }
    }

    /// Spot-check pins: field-by-field comparison against vendor
    /// ModParser.lua's original text (a migration-invariant sample check).
    #[test]
    fn real_data_sample_pins() {
        let doc = real_data().mod_parser_rules().unwrap().unwrap();

        // formList: `["^(%d+)%% increased"] = "INC"` (ModParser.lua:63)
        let inc = doc
            .forms
            .iter()
            .find(|f| f.pattern == "^(%d+)%% increased")
            .expect("INC form 应存在");
        assert_eq!(inc.form, "INC");
        assert_eq!(inc.literal.as_deref(), Some("% increased"));
        assert!(inc.anchored);

        // modNameList: `["attributes"]` is vendor's `{ "Str", "Dex", "Int", "All" }`
        // (:161), already expanded at extraction time via route B into
        // PoBR sub-names (an aggregate-phrase expansion, dropping vendor's `All`).
        let attributes = doc
            .name_map
            .iter()
            .find(|e| e.phrase == "attributes")
            .expect("attributes 应存在");
        assert_eq!(
            attributes.names,
            vec!["Strength", "Dexterity", "Intelligence"]
        );

        // modNameList with a tag: `["mana cost of attacks"]` (SkillType.Attack
        // reverse-looked-up name)
        let mana_cost = doc
            .name_map
            .iter()
            .find(|e| e.phrase == "mana cost of attacks")
            .expect("mana cost of attacks 应存在");
        assert_eq!(mana_cost.names, vec!["ManaCost"]);
        assert_eq!(mana_cost.effects.tags.len(), 1);
        assert_eq!(mana_cost.effects.tags[0].tag_type, "SkillType");
        assert_eq!(
            mana_cost.effects.tags[0].fields.get("skill_type"),
            Some(&StatMapValue::Text("Attack".into()))
        );

        // modFlagList: `["with maces"] = { flags = bor(ModFlag.Mace, ModFlag.Hit) }`
        // (the mask decomposed into a bit-ascending name array)
        let with_maces = doc
            .flag_phrases
            .iter()
            .find(|e| e.phrase == "with maces")
            .expect("with maces 应存在");
        assert_eq!(with_maces.effects.flags, vec!["Hit", "Mace"]);

        // preFlagList: `["^minions [cthd][ae][ukva][sel]e? "] = { addToMinion = true }`
        let minions = doc
            .pre_flags
            .iter()
            .find(|e| e.pattern == "^minions [cthd][ae][ukva][sel]e? ")
            .expect("minions pre-flag 应存在");
        assert!(minions.effects.add_to_minion);
        assert_eq!(minions.literal.as_deref(), Some("minions "));
        assert!(minions.anchored);

        // modTagList plain-table entry: `["per power charge"]`
        let per_power = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per power charge")
            .expect("per power charge 应存在");
        assert_eq!(per_power.effects.tags[0].tag_type, "Multiplier");
        assert_eq!(
            per_power.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("PowerCharge".into()))
        );
        assert!(!per_power.inferred);

        // modTagList closure entry (a probe-inferred template):
        // `["per (%d+) rage"] = function(num)
        // return { tag = { type = "Multiplier", var = "Rage", div = num } } end`
        let per_rage = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+) rage")
            .expect("per (%d+) rage 应存在");
        assert!(per_rage.inferred);
        assert_eq!(
            per_rage.effects.tags[0].fields.get("div"),
            Some(&StatMapValue::Text("$1".into()))
        );
        assert_eq!(
            per_rage.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("Rage".into()))
        );

        // A string-concatenation template: `per (%d+)%% (%a+) effect on enemy`'s
        // var = firstToUpper(effectName) .. "Effect" → "$2:cap+Effect"
        let effect = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+)%% (%a+) effect on enemy")
            .expect("effect on enemy 应存在");
        assert_eq!(
            effect.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("$2:cap+Effect".into()))
        );

        // A failed-inference fallback: `per (%d+) rampage kills`
        // (limit = 1000/num is beyond the DSL)
        let rampage = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+) rampage kills")
            .expect("rampage kills 应存在");
        assert!(
            rampage
                .handler_id
                .as_deref()
                .is_some_and(|id| id.starts_with("tag_phrase:"))
        );
        assert!(rampage.effects.tags.is_empty());

        // flagTypes' hexproof special case (the embedded-mod shape)
        let hexproof = doc
            .flag_types
            .iter()
            .find(|e| e.phrase == "hexproof")
            .expect("hexproof 应存在");
        let mod_def = hexproof.mod_def.as_ref().expect("hexproof 应为 mod 形态");
        assert_eq!(mod_def.name, "CurseEffectOnSelf");
        assert_eq!(mod_def.mod_type, "MORE");
        assert_eq!(mod_def.value, -100.0);

        // The regen derived table (expanded at vendor load time via
        // appendMod): a multi-resource-name set
        let life_mana = doc
            .regen_types
            .iter()
            .find(|e| e.phrase == "life and mana")
            .expect("life and mana 应存在");
        assert_eq!(life_mana.names, vec!["LifeRegen", "ManaRegen"]);
        // The "maximum" variant is added in by vendor at load time
        assert!(
            doc.regen_types.iter().any(|e| e.phrase == "maximum life"),
            "regen_types 应含 maximum 变体"
        );
    }

    /// A missing file (an old data pack) → `Ok(None)`, backward compatible.
    #[test]
    fn missing_file_returns_none() {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-parser-rules-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let loaded = GameData::new(&dir).mod_parser_rules().unwrap();
        assert!(loaded.is_none());
    }

    /// The number of handler fallbacks from probe-inferred entries stays
    /// within budget (≤15, a structural guard); the exact count drifts
    /// with vendor and lives in the blessed snapshot.
    #[test]
    fn handler_fallback_within_budget() {
        let doc = real_data().mod_parser_rules().unwrap().unwrap();
        let handlers = doc
            .pre_flags
            .iter()
            .filter(|e| e.handler_id.is_some())
            .count()
            + doc
                .tag_phrases
                .iter()
                .filter(|e| e.handler_id.is_some())
                .count();
        assert!(handlers <= 15, "handler 兜底 {handlers} 超出蓝图预算 ≤15");
        crate::test_pins::assert_pin(
            &golden_version_dir(),
            "parser_rules.handler_fallbacks",
            handlers,
        );
    }
}
