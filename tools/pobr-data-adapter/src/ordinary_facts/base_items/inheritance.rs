//! Explicit community interpretation of pinned item metadata, in the ordinary DTO.

mod source;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::BaseItemDef;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::ordinary_facts::BranchCodecArgs;
use crate::ordinary_facts::convert::{column, text};

pub(in crate::ordinary_facts) const POLICY: &str =
    "pypoe-66d1d744-base-tag-child-first-ordered-union/v1";
const FORMAT: &str = "repoe-item-inheritance/v1";

pub(in crate::ordinary_facts) struct Converted {
    pub sidecars: BTreeMap<String, Vec<u8>>,
    pub manifest_sha256: String,
    pub owned_ids: BTreeSet<String>,
    pub receipt: Value,
}

pub(in crate::ordinary_facts) fn convert(
    input: &VerifiedFacts,
    bases: &mut [BaseItemDef],
    args: &BranchCodecArgs,
) -> Result<Converted, String> {
    let archive = source::open(args, &input.manifest)?;
    let tables = Tables::selected(input, &super::TABLES)?;
    column(
        input,
        "BaseItemTypes",
        &["InheritsFrom"],
        &json!({
            "name":"InheritsFrom","type":"string","array":false,"interval":false,"references":null
        }),
    )?;
    let roots = archive.document["roots"]
        .as_object()
        .ok_or("missing item roots")?;
    let tag_ids = (0..tables.len("Tags"))
        .map(|row| Ok(text(&tables.row("Tags", row)?["Id"])?.to_owned()))
        .collect::<Result<BTreeSet<_>, String>>()?;
    let mut selected_roots = BTreeSet::new();
    let mut by_id = BTreeMap::new();
    for row in 0..tables.len("BaseItemTypes") {
        let raw = tables.row("BaseItemTypes", row)?;
        let stem = text(&raw["InheritsFrom"])?;
        let root = format!("{stem}.it");
        source::resource(&root)?;
        if !roots.contains_key(&root) {
            return Err(format!(
                "BaseItemTypes/{row}: missing inheritance root {root}"
            ));
        }
        selected_roots.insert(root.clone());
        by_id.insert(text(&raw["Id"])?.to_owned(), (row, root));
    }
    if selected_roots != roots.keys().cloned().collect() {
        return Err("inheritance root inventory differs from selected BaseItemTypes".into());
    }
    let mut records = Vec::new();
    let mut owned_ids = BTreeSet::new();
    for base in bases {
        let (row, root) = by_id.get(&base.id).ok_or("base has no source row")?;
        let inheritance = &roots[root];
        let interpretation = &inheritance["tag_interpretation"];
        let direct = base.tags.clone();
        if interpretation["status"] == "supported" {
            let inherited: Vec<String> = serde_json::from_value(interpretation["tags"].clone())
                .map_err(|e| format!("inherited tags: {e}"))?;
            // BaseItemDef tags already represent a set in the ordinary loader.
            // Preserve every raw occurrence in the archive/receipt, not in this set.
            base.tags.extend(inherited);
            base.tags.sort();
            base.tags.dedup();
            owned_ids.insert(base.id.clone());
        }
        let unknown_tags: Vec<_> = base
            .tags
            .iter()
            .filter(|tag| !tag_ids.contains(*tag))
            .collect();
        records.push(json!({"id":base.id,"source_row":row,"inherits_from":root,
            "direct_table_tags":direct,"chain_child_to_parent":inheritance["chain_child_to_parent"],
            "base_tag_occurrences":inheritance["base_tag_occurrences"],"tag_interpretation":interpretation,
            "output_tags":base.tags,"tags_absent_from_target_tag_table":unknown_tags,
            "ownership":if owned_ids.contains(&base.id){"target-table-plus-explicit-community-inheritance"}else{"unresolved-inheritance; direct table tags and declared compatibility retained"}}));
    }
    Ok(Converted {
        sidecars: archive.sidecars,
        manifest_sha256: args.sha256.clone(),
        owned_ids,
        receipt: json!({"manifest_sha256":args.sha256,"source_identity":archive.manifest["source_identity"],
            "interpretation_policy":archive.document["interpretation_policy"],"records":records,
            "summary":archive.manifest["summary"],"dto_rule":"exact direct Tags.Id union interpreted inherited tags, sorted and deduplicated; raw order/duplicates remain in the pinned archive",
            "scope":"Base.tag only. This is a fixed community parser interpretation, not certification of official merge laws. Unknown blocks/keys remain raw; rejected roots retain diagnostics and compatibility. No actor, rarity, socket, unique, mod-eligibility or gameplay rule is inferred."}),
    })
}
