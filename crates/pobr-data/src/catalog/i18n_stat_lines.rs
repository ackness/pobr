//! Mod-line input translation templates (`i18n/<lang>/stat_lines.json`, TODO Phase 7.1).
//!
//! Source = localized template pairs from GGG's stat descriptions
//! (produced by the translation repository, ackness/poe2-zh-cn-stats).
//! The consumer (pobr-wasm's contract layer) reverse-looks-up a localized
//! mod line against its template, substitutes the numbers back in, and gets
//! an English canonical line to feed into the existing parser — the engine
//! itself stays English-only.

use serde::{Deserialize, Deserializer, Serialize};

/// A pair of mod-line templates: `src` = the localized template (with
/// `{0}` / `{0:+d}` placeholders), `en` = the corresponding English
/// canonical template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatLineTemplate {
    /// The localized template (e.g. `{0:+d} 生命上限`).
    pub src: String,
    /// The English canonical template (e.g. `{0:+d} to maximum Life`).
    pub en: String,
    /// Who wrote `src` when it is not official text: the translation
    /// repository's own supplement rows carry it. Absent (or `"official"`)
    /// means official text, as for every row a pack had before origins.
    #[serde(
        default,
        deserialize_with = "text_origin",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin: Option<TextOrigin>,
}

/// Who wrote a shipped text that is not official: the translation
/// repository's vocabulary (`origin` of its supplement records). Official
/// text has no origin (`None`), so `Option<TextOrigin>` reads "official or
/// whose".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextOrigin {
    /// Drafted by a model and reviewed before shipping.
    Agent,
    /// Written or rewritten by a person.
    Human,
}

impl TextOrigin {
    /// The value as the packs spell it (`agent`, `human`).
    pub fn as_str(self) -> &'static str {
        match self {
            TextOrigin::Agent => "agent",
            TextOrigin::Human => "human",
        }
    }

    /// The origin of a text assembled from two parts: official only when
    /// both are; a part a model wrote makes the whole a model's.
    pub fn combine(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(TextOrigin::Agent), _) | (_, Some(TextOrigin::Agent)) => Some(TextOrigin::Agent),
            (Some(TextOrigin::Human), _) | (_, Some(TextOrigin::Human)) => Some(TextOrigin::Human),
            (None, None) => None,
        }
    }
}

/// `origin`: `"agent"` / `"human"`, or `"official"` / `null` / absent for
/// official text. Any other value is an error, as for any misshapen row.
fn text_origin<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<TextOrigin>, D::Error> {
    match Option::<String>::deserialize(deserializer)?.as_deref() {
        None | Some("official") => Ok(None),
        Some("agent") => Ok(Some(TextOrigin::Agent)),
        Some("human") => Ok(Some(TextOrigin::Human)),
        Some(other) => Err(serde::de::Error::unknown_variant(
            other,
            &["official", "agent", "human"],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_optional_and_official_when_absent() {
        let rows: Vec<StatLineTemplate> = serde_json::from_str(
            r#"[
                {"src": "a {0}", "en": "A {0}"},
                {"src": "b {0}", "en": "B {0}", "origin": "agent"},
                {"src": "c {0}", "en": "C {0}", "origin": "human"},
                {"src": "d {0}", "en": "D {0}", "origin": "official"},
                {"src": "e {0}", "en": "E {0}", "origin": null}
            ]"#,
        )
        .unwrap();
        let origins: Vec<_> = rows.iter().map(|row| row.origin).collect();
        assert_eq!(
            origins,
            [
                None,
                Some(TextOrigin::Agent),
                Some(TextOrigin::Human),
                None,
                None
            ]
        );
        // An official row serializes exactly as before.
        assert_eq!(
            serde_json::to_string(&rows[0]).unwrap(),
            r#"{"src":"a {0}","en":"A {0}"}"#
        );
        assert_eq!(
            serde_json::to_string(&rows[1]).unwrap(),
            r#"{"src":"b {0}","en":"B {0}","origin":"agent"}"#
        );
        assert!(
            serde_json::from_str::<StatLineTemplate>(r#"{"src":"x","en":"x","origin":"ours"}"#)
                .is_err()
        );
    }

    #[test]
    fn a_model_part_makes_the_whole_a_models() {
        use TextOrigin::{Agent, Human};
        assert_eq!(TextOrigin::combine(None, None), None);
        assert_eq!(TextOrigin::combine(None, Some(Human)), Some(Human));
        assert_eq!(TextOrigin::combine(Some(Human), Some(Agent)), Some(Agent));
        assert_eq!(TextOrigin::combine(Some(Agent), None), Some(Agent));
    }
}
