//! Reject ambiguous object identities before typed native catalog decoding.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

#[derive(Clone, Copy)]
struct Check;

impl<'de> DeserializeSeed<'de> for Check {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Check {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON with unique object keys")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate native JSON object key"));
            }
            map.next_value_seed(Check)?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while sequence.next_element_seed(Check)?.is_some() {}
        Ok(())
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
}

pub(super) fn check(bytes: &[u8]) -> Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Check.deserialize(&mut deserializer)?;
    deserializer.end()
}

#[cfg(test)]
mod tests {
    #[test]
    fn duplicate_keys_are_rejected_at_every_depth_after_unescaping() {
        for raw in [
            r#"{"a":1,"a":2}"#,
            r#"[{"a":{"b":0,"b":1}}]"#,
            r#"{"SparkPlayer":[],"Spark\u0050layer":[]}"#,
        ] {
            assert!(
                super::check(raw.as_bytes())
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate")
            );
        }
        for raw in [
            r#"{"a":1,"b":1}"#,
            r#"[{"a":true},{"a":false},null,"x",-1,1.25]"#,
        ] {
            super::check(raw.as_bytes()).unwrap();
        }
        assert!(super::check(b"{} {}").is_err());
    }
}
