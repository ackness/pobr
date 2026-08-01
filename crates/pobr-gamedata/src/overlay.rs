//! base → overlay 确定性 merge 引擎。
//!
//! merge 语义在 gamedata **一处收口**（adapter 构建期的 skill_overrides merge 是
//! 等价物，两处规则共享本文档）。规则（全部由单测锁定）：
//!
//! 1. **对象 + 对象**：key 级覆盖——overlay 的每个 key 递归 merge 进 base；
//!    base 独有 key 原样保留。
//! 2. **数组 + 数组**：若两侧元素**全部**是带字符串 `"id"` 字段的对象，则按 id
//!    合并——同 id 元素递归 merge（保持 base 顺序），overlay 新 id 按其出现顺序
//!    追加；否则（任一侧含无 id 元素）overlay **整体替换** base。
//! 3. **同类标量**（null/bool/number/string）：overlay 覆盖 base。
//! 4. **JSON 类型冲突**（如对象 vs 数组、字符串 vs 数字）：返回
//!    [`MergeError::TypeConflict`]，**不静默**。
//!
//! 确定性：对象 key 按 `serde_json::Map`（BTreeMap）排序；数组合并保持
//! base 顺序 + overlay 追加顺序——同输入恒同输出。

use std::fmt;

use serde_json::Value;

/// merge 失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    /// base 与 overlay 在同一路径上的 JSON 类型不一致。
    TypeConflict {
        /// 冲突位置（JSON Pointer 风格，如 `/mods/3/stats`；根为空串）。
        path: String,
        /// base 侧 JSON 类型名。
        base_kind: &'static str,
        /// overlay 侧 JSON 类型名。
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

/// JSON 值的类型名（错误信息用）。
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

/// 元素是否为带字符串 `"id"` 字段的对象（数组按 id 合并的前提）。
fn has_string_id(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|obj| obj.get("id"))
        .is_some_and(Value::is_string)
}

/// 数组是否整体可按 id 合并（所有元素都是带字符串 id 的对象；空数组视为可合并）。
fn is_id_array(values: &[Value]) -> bool {
    values.iter().all(has_string_id)
}

/// 把 overlay 确定性地 merge 进 base，返回合并结果。规则见模块文档。
pub fn merge(base: Value, overlay: Value) -> Result<Value, MergeError> {
    merge_at(base, overlay, String::new())
}

fn merge_at(base: Value, overlay: Value, path: String) -> Result<Value, MergeError> {
    match (base, overlay) {
        // 规则 1：对象 key 级覆盖（递归）。
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
        // 规则 2：数组按 id 合并；任一侧含无 id 元素则整体替换。
        (Value::Array(base_arr), Value::Array(overlay_arr)) => {
            if is_id_array(&base_arr) && is_id_array(&overlay_arr) {
                merge_id_arrays(base_arr, overlay_arr, &path)
            } else {
                Ok(Value::Array(overlay_arr))
            }
        }
        // 规则 3：同类标量 overlay 覆盖。
        (base @ (Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)), overlay)
            if std::mem::discriminant(&base) == std::mem::discriminant(&overlay) =>
        {
            Ok(overlay)
        }
        // 规则 4：类型冲突报错，不静默。
        (base, overlay) => Err(MergeError::TypeConflict {
            path,
            base_kind: kind(&base),
            overlay_kind: kind(&overlay),
        }),
    }
}

/// 按 `"id"` 字段合并两个对象数组：同 id 递归 merge（保持 base 顺序），
/// overlay 新 id 按出现顺序追加。
fn merge_id_arrays(
    base_arr: Vec<Value>,
    overlay_arr: Vec<Value>,
    path: &str,
) -> Result<Value, MergeError> {
    /// 取元素 id（调用前已由 `is_id_array` 保证存在）。
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

    /// 规则 1：对象 key 级覆盖——overlay 改写既有 key、新增 key，base 独有 key 保留。
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

    /// 规则 2a：两侧均为带 id 对象数组 → 按 id 合并（同 id 递归 merge，保持 base 顺序）。
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

    /// 规则 2b：overlay 的新 id 按出现顺序追加在 base 之后。
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

    /// 规则 2c：任一侧含无 id 元素 → overlay 整体替换。
    #[test]
    fn arrays_without_id_are_replaced_wholesale() {
        let base = json!([1, 2, 3]);
        let overlay = json!([4]);
        assert_eq!(merge(base, overlay).unwrap(), json!([4]));

        // 元素为对象但缺 id 字段，同样整体替换。
        let base = json!([{"name": "a"}]);
        let overlay = json!([{"name": "b"}]);
        assert_eq!(merge(base, overlay).unwrap(), json!([{"name": "b"}]));

        // id 非字符串（数字）不参与 id 合并语义。
        let base = json!([{"id": 1, "value": 1}]);
        let overlay = json!([{"id": 2, "value": 2}]);
        assert_eq!(
            merge(base, overlay).unwrap(),
            json!([{"id": 2, "value": 2}])
        );
    }

    /// 规则 2d：overlay 为空 id 数组时保留 base（空数组视为可 id 合并、合并零项）。
    #[test]
    fn empty_overlay_id_array_keeps_base() {
        let base = json!([{"id": "alpha", "value": 1}]);
        let overlay = json!([]);
        assert_eq!(
            merge(base, overlay).unwrap(),
            json!([{"id": "alpha", "value": 1}])
        );
    }

    /// 规则 3：同类标量 overlay 覆盖（number/string/bool/null）。
    #[test]
    fn same_kind_scalars_overlay_wins() {
        assert_eq!(merge(json!(1), json!(2)).unwrap(), json!(2));
        assert_eq!(merge(json!("a"), json!("b")).unwrap(), json!("b"));
        assert_eq!(merge(json!(true), json!(false)).unwrap(), json!(false));
        assert_eq!(merge(Value::Null, Value::Null).unwrap(), Value::Null);
    }

    /// 规则 4：类型冲突报错不静默，并带冲突路径。
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

        // 标量 vs 标量的不同类同样冲突（string vs number）。
        let err = merge(json!("a"), json!(1)).unwrap_err();
        assert_eq!(
            err,
            MergeError::TypeConflict {
                path: String::new(),
                base_kind: "string",
                overlay_kind: "number",
            }
        );

        // null 不作为「删除」语义：null vs 对象同样是类型冲突。
        assert!(merge(json!({"a": 1}), Value::Null).is_err());
    }

    /// id 数组内同 id 元素递归 merge 时的类型冲突同样报错（路径含数组下标）。
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

    /// 确定性：同输入两次 merge 输出逐字节一致。
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
