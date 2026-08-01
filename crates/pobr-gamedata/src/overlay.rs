//! The deterministic base → overlay merge engine.
//!
//! Merge semantics are centralized **in one place** in gamedata (the
//! adapter's build-time skill_overrides merge is an equivalent; both share
//! the rules documented here). Rules (all locked in by unit tests):
//!
//! 1. **object + object**: key-level override — every key of overlay is
//!    recursively merged into base; a key unique to base is kept as-is.
//! 2. **array + array**: if every element on **both** sides is an object
//!    with a string `"id"` field, merge by id — elements sharing an id are
//!    merged recursively (base's order is preserved), and overlay's new
//!    ids are appended in the order they appear; otherwise (either side
//!    has an element without an id), overlay **wholesale replaces** base.
//! 3. **same-kind scalars** (null/bool/number/string): overlay overrides base.
//! 4. **a JSON type conflict** (e.g. object vs. array, string vs. number):
//!    returns [`MergeError::TypeConflict`], **not silenced**.
//!
//! Determinism: object keys are ordered by `serde_json::Map` (a BTreeMap);
//! array merging preserves base's order plus overlay's append order — the
//! same input always produces the same output.

use std::fmt;

use serde_json::Value;

/// A merge failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    /// base and overlay disagree on JSON type at the same path.
    TypeConflict {
        /// The conflict location (JSON Pointer style, e.g. `/mods/3/stats`;
        /// the root is an empty string).
        path: String,
        /// base's JSON type name.
        base_kind: &'static str,
        /// overlay's JSON type name.
        overlay_kind: &'static str,
    },
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeConflict {
                path,
                base_kind,
                overlay_kind,
            } => write!(
                f,
                "overlay merge 类型冲突（路径 `{path}`）：base 为 {base_kind}，overlay 为 {overlay_kind}"
            ),
        }
    }
}

impl std::error::Error for MergeError {}

/// A JSON value's type name (for error messages).
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Whether an element is an object with a string `"id"` field (the
/// prerequisite for merging arrays by id).
fn has_string_id(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|obj| obj.get("id"))
        .is_some_and(Value::is_string)
}

/// Whether an array can be merged by id as a whole (every element is an
/// object with a string id; an empty array counts as mergeable).
fn is_id_array(values: &[Value]) -> bool {
    values.iter().all(has_string_id)
}

/// Deterministically merges overlay into base, returning the merged
/// result. See the module doc for the rules.
pub fn merge(base: Value, overlay: Value) -> Result<Value, MergeError> {
    merge_at(base, overlay, String::new())
}

fn merge_at(base: Value, overlay: Value, path: String) -> Result<Value, MergeError> {
    match (base, overlay) {
        // Rule 1: object key-level override (recursive).
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let child_path = format!("{path}/{key}");
                match base_map.remove(&key) {
                    Some(base_value) => {
                        let merged = merge_at(base_value, overlay_value, child_path)?;
                        base_map.insert(key, merged);
                    }
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
            Ok(Value::Object(base_map))
        }
        // Rule 2: arrays merge by id; wholesale replace if either side has
        // an element without an id.
        (Value::Array(base_arr), Value::Array(overlay_arr)) => {
            if is_id_array(&base_arr) && is_id_array(&overlay_arr) {
                merge_id_arrays(base_arr, overlay_arr, &path)
            } else {
                Ok(Value::Array(overlay_arr))
            }
        }
        // Rule 3: same-kind scalars, overlay overrides.
        (base @ (Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)), overlay)
            if std::mem::discriminant(&base) == std::mem::discriminant(&overlay) =>
        {
            Ok(overlay)
        }
        // Rule 4: a type conflict errors out, not silenced.
        (base, overlay) => Err(MergeError::TypeConflict {
            path,
            base_kind: kind(&base),
            overlay_kind: kind(&overlay),
        }),
    }
}

/// Merges two object arrays by their `"id"` field: elements sharing an id
/// are merged recursively (base's order is preserved), and overlay's new
/// ids are appended in the order they appear.
fn merge_id_arrays(
    base_arr: Vec<Value>,
    overlay_arr: Vec<Value>,
    path: &str,
) -> Result<Value, MergeError> {
    /// Gets an element's id (guaranteed present by `is_id_array` before this is called).
    fn id_of(value: &Value) -> &str {
        value
            .as_object()
            .and_then(|obj| obj.get("id"))
            .and_then(Value::as_str)
            .expect("is_id_array 已保证元素含字符串 id")
    }

    let mut merged = base_arr;
    for overlay_item in overlay_arr {
        let overlay_id = id_of(&overlay_item).to_owned();
        match merged
            .iter()
            .position(|item| id_of(item) == overlay_id.as_str())
        {
            Some(index) => {
                let child_path = format!("{path}/{index}");
                let base_item = std::mem::take(&mut merged[index]);
                merged[index] = merge_at(base_item, overlay_item, child_path)?;
            }
            None => merged.push(overlay_item),
        }
    }
    Ok(Value::Array(merged))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{MergeError, merge};

    /// Rule 1: object key-level override — overlay rewrites existing keys
    /// and adds new ones, base's unique keys are kept.
    #[test]
    fn object_keys_merge_with_overlay_priority() {
        let base = json!({"a": 1, "b": 2, "nested": {"x": 1, "y": 2}});
        let overlay = json!({"b": 20, "c": 3, "nested": {"y": 20, "z": 30}});
        let merged = merge(base, overlay).unwrap();
        assert_eq!(
            merged,
            json!({"a": 1, "b": 20, "c": 3, "nested": {"x": 1, "y": 20, "z": 30}})
        );
    }

    /// Rule 2a: both sides are id-bearing object arrays → merge by id
    /// (elements sharing an id merge recursively, base's order preserved).
    #[test]
    fn id_arrays_merge_by_id_preserving_base_order() {
        let base = json!([
            {"id": "alpha", "value": 1, "keep": true},
            {"id": "beta", "value": 2}
        ]);
        let overlay = json!([{"id": "alpha", "value": 10}]);
        let merged = merge(base, overlay).unwrap();
        assert_eq!(
            merged,
            json!([
                {"id": "alpha", "value": 10, "keep": true},
                {"id": "beta", "value": 2}
            ])
        );
    }

    /// Rule 2b: overlay's new ids are appended after base, in the order they appear.
    #[test]
    fn id_arrays_append_new_ids_in_overlay_order() {
        let base = json!([{"id": "alpha", "value": 1}]);
        let overlay = json!([{"id": "gamma", "value": 3}, {"id": "beta", "value": 2}]);
        let merged = merge(base, overlay).unwrap();
        assert_eq!(
            merged,
            json!([
                {"id": "alpha", "value": 1},
                {"id": "gamma", "value": 3},
                {"id": "beta", "value": 2}
            ])
        );
    }

    /// Rule 2c: either side has an element without an id → overlay
    /// wholesale replaces base.
    #[test]
    fn arrays_without_id_are_replaced_wholesale() {
        let base = json!([1, 2, 3]);
        let overlay = json!([4]);
        assert_eq!(merge(base, overlay).unwrap(), json!([4]));

        // Elements are objects but missing the id field, still wholesale replaced.
        let base = json!([{"name": "a"}]);
        let overlay = json!([{"name": "b"}]);
        assert_eq!(merge(base, overlay).unwrap(), json!([{"name": "b"}]));

        // A non-string id (a number) doesn't participate in id-merge semantics.
        let base = json!([{"id": 1, "value": 1}]);
        let overlay = json!([{"id": 2, "value": 2}]);
        assert_eq!(
            merge(base, overlay).unwrap(),
            json!([{"id": 2, "value": 2}])
        );
    }

    /// Rule 2d: an empty overlay id array keeps base (an empty array
    /// counts as id-mergeable, merging in zero items).
    #[test]
    fn empty_overlay_id_array_keeps_base() {
        let base = json!([{"id": "alpha", "value": 1}]);
        let overlay = json!([]);
        assert_eq!(
            merge(base, overlay).unwrap(),
            json!([{"id": "alpha", "value": 1}])
        );
    }

    /// Rule 3: same-kind scalars, overlay wins (number/string/bool/null).
    #[test]
    fn same_kind_scalars_overlay_wins() {
        assert_eq!(merge(json!(1), json!(2)).unwrap(), json!(2));
        assert_eq!(merge(json!("a"), json!("b")).unwrap(), json!("b"));
        assert_eq!(merge(json!(true), json!(false)).unwrap(), json!(false));
        assert_eq!(merge(Value::Null, Value::Null).unwrap(), Value::Null);
    }

    /// Rule 4: a type conflict errors out, not silenced, and carries the conflict path.
    #[test]
    fn type_conflict_errors_with_path() {
        let base = json!({"outer": {"inner": [1, 2]}});
        let overlay = json!({"outer": {"inner": {"oops": true}}});
        let err = merge(base, overlay).unwrap_err();
        assert_eq!(
            err,
            MergeError::TypeConflict {
                path: "/outer/inner".into(),
                base_kind: "array",
                overlay_kind: "object",
            }
        );

        // A scalar-vs-scalar type mismatch is also a conflict (string vs number).
        let err = merge(json!("a"), json!(1)).unwrap_err();
        assert_eq!(
            err,
            MergeError::TypeConflict {
                path: String::new(),
                base_kind: "string",
                overlay_kind: "number",
            }
        );

        // null isn't treated as "delete" semantics: null vs. an object is
        // also a type conflict.
        assert!(merge(json!({"a": 1}), Value::Null).is_err());
    }

    /// A type conflict while recursively merging same-id elements inside
    /// an id array also errors out (the path includes the array index).
    #[test]
    fn type_conflict_inside_id_array_reports_indexed_path() {
        let base = json!([{"id": "alpha", "value": 1}]);
        let overlay = json!([{"id": "alpha", "value": "ten"}]);
        let err = merge(base, overlay).unwrap_err();
        assert_eq!(
            err,
            MergeError::TypeConflict {
                path: "/0/value".into(),
                base_kind: "number",
                overlay_kind: "string",
            }
        );
    }

    /// Determinism: merging the same input twice produces byte-identical output.
    #[test]
    fn merge_is_deterministic() {
        let base = json!({"list": [{"id": "b", "v": 1}, {"id": "a", "v": 2}], "k": 1});
        let overlay = json!({"list": [{"id": "a", "v": 20}, {"id": "c", "v": 3}], "k2": 2});
        let once = merge(base.clone(), overlay.clone()).unwrap();
        let twice = merge(base, overlay).unwrap();
        assert_eq!(
            serde_json::to_string(&once).unwrap(),
            serde_json::to_string(&twice).unwrap()
        );
    }
}
