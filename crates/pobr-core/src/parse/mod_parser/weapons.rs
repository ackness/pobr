//! Missing weapon phrases come from the loaded weapon rows. The compatibility
//! parser's existing phrases keep priority and their original payloads.

use pobr_data::catalog::WeaponTypeTable;
use pobr_data::catalog::parser_rules::{
    FlagPhraseDef, ModParserRulesDoc, NameMapDef, RuleEffectsDef, StatMapValue, TagPhraseDef,
    TagTemplate,
};
use pobr_data::modifier::ModFlags;

pub(super) fn with_weapon_words(
    doc: &ModParserRulesDoc,
    weapons: &WeaponTypeTable,
) -> ModParserRulesDoc {
    let mut doc = doc.clone();
    for row in &weapons.0 {
        let condition = RuleEffectsDef {
            tags: vec![TagTemplate {
                tag_type: "Condition".into(),
                fields: [(
                    "var".into(),
                    StatMapValue::Text(format!("Using{}", row.flag)),
                )]
                .into(),
            }],
            ..Default::default()
        };
        let known_bit = ModFlags::weapon_type_bit(&row.flag).is_some();
        let hit_effects = RuleEffectsDef {
            flags: vec!["Hit".into(), row.flag.clone()],
            ..Default::default()
        };
        let mut words = vec![row.flag.as_str(), row.id.as_str()];
        words.extend(row.label.as_deref());
        words.sort_unstable();
        words.dedup();
        for word in words {
            if word == "None" || word.trim().is_empty() {
                continue;
            }
            let word = word.to_ascii_lowercase();
            // Aliases must not claim another weapon's name or an existing
            // non-weapon scope (for example, a label of "Spell").
            if weapons.0.iter().any(|other| {
                other.flag != row.flag
                    && [
                        Some(other.flag.as_str()),
                        Some(other.id.as_str()),
                        other.label.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|alias| alias.eq_ignore_ascii_case(&word))
            }) || doc
                .flag_phrases
                .iter()
                .any(|entry| entry.phrase == word && !entry.effects.flags.contains(&row.flag))
            {
                continue;
            }
            let plural = if let Some(stem) = word.strip_suffix("staff") {
                format!("{stem}staves")
            } else {
                format!("{word}s")
            };
            for article in ["a", "an"] {
                let phrase = format!("while wielding {article} {word}");
                // Escape Lua pattern characters in data-provided names.
                let pattern: String = phrase
                    .chars()
                    .flat_map(|c| {
                        let escape = "^$()%.[]*+-?".contains(c);
                        escape.then_some('%').into_iter().chain(std::iter::once(c))
                    })
                    .collect();
                if !doc.tag_phrases.iter().any(|entry| entry.pattern == pattern) {
                    doc.tag_phrases.push(TagPhraseDef {
                        pattern,
                        literal: Some(phrase),
                        effects: condition.clone(),
                        ..Default::default()
                    });
                }
            }
            for phrase in [
                format!("with {plural}"),
                format!("with {word} attacks"),
                format!("to {word} attacks"),
            ] {
                // Unknown type bits cannot route a hit to the correct hand.
                // Leave those phrases unparsed so ingestion reports the line.
                if known_bit && !doc.flag_phrases.iter().any(|entry| entry.phrase == phrase) {
                    doc.flag_phrases.push(FlagPhraseDef {
                        phrase,
                        effects: hit_effects.clone(),
                    });
                }
            }
            let phrase = format!("{word} damage");
            if known_bit && !doc.name_map.iter().any(|entry| entry.phrase == phrase) {
                doc.name_map.push(NameMapDef {
                    phrase,
                    names: vec!["Damage".into()],
                    effects: hit_effects.clone(),
                });
            }
        }
    }
    // Plain scans choose the longest phrase at the earliest position. Pattern
    // scans use table order as their final tie-break, so keep that order stable.
    doc.tag_phrases.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc
}
