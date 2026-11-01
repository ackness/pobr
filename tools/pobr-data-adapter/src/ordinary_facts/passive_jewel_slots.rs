//! Target physical jewel slots for the existing character-share import contract.

use pobr_data::catalog::passive_jewel_slots::{PassiveJewelSlotDef, PassiveJewelSlotsDef};
use serde_json::{Value, json};

use super::convert::{column, text};
use crate::native_facts::reader::{Tables, VerifiedFacts, selected};
use crate::raw_source::sha256_hex;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

pub(super) const TABLES: [&str; 1] = ["PassiveSkills"];

pub(super) struct Converted {
    pub bytes: Vec<u8>,
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts, extra: &selected::Input) -> Result<Converted, String> {
    let base = Tables::selected(input, &TABLES)?;
    let slots = extra.tables(&["PassiveJewelSlots"])?;
    for (field, kind) in [
        ("Id", "string"),
        ("PassiveSkillGraphId", "u16"),
        ("IsJewelSocket", "bool"),
    ] {
        column(
            input,
            "PassiveSkills",
            &[field],
            &json!({"name":field,"type":kind,
            "array":false,"interval":false,"references":null}),
        )?;
    }
    for (field, kind, array, target) in [
        ("Slot", "foreignrow", false, Some("PassiveSkills")),
        ("SinisterJewelSocket", "bool", false, None),
        (
            "ClusterJewelSize",
            "foreignrow",
            false,
            Some("PassiveTreeExpansionJewelSizes"),
        ),
        ("ProxySlot", "foreignrow", false, Some("PassiveSkills")),
        ("ReplacesSlot", "row", false, Some("PassiveJewelSlots")),
        ("StartIndices", "i32", true, None),
    ] {
        extra.column(
            "PassiveJewelSlots",
            field,
            &json!({"name":field,"type":kind,
            "array":array,"interval":false,"references":target.map(|t|json!({"table":t}))}),
        )?;
    }
    let mut output = Vec::new();
    let mut records = Vec::new();
    for index in 0..slots.len("PassiveJewelSlots") {
        let raw = slots.row("PassiveJewelSlots", index)?;
        if ["ClusterJewelSize", "ProxySlot", "ReplacesSlot"]
            .iter()
            .any(|field| !raw[field].is_null())
            || raw["StartIndices"] != json!([])
        {
            return Err(format!(
                "slot {index}: cluster/proxy/replacement semantics need a consumer decision"
            ));
        }
        let row = raw["Slot"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("invalid passive slot reference")?;
        let passive = base.row("PassiveSkills", row)?;
        if passive["IsJewelSocket"] != true {
            return Err(format!(
                "slot {index}: referenced passive is not a jewel socket"
            ));
        }
        let graph = passive["PassiveSkillGraphId"]
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .ok_or("invalid jewel graph ID")?;
        let slot = PassiveJewelSlotDef {
            index: u32::try_from(index).map_err(|e| e.to_string())?,
            passive_id: text(&passive["Id"])?.into(),
            passive_skill_graph_id: u32::from(graph),
            sinister_jewel_socket: raw["SinisterJewelSocket"]
                .as_bool()
                .ok_or("invalid sinister jewel flag")?,
        };
        records.push(
            json!({"source_row":index,"raw":raw,"passive_row":row,"output":slot,
            "passive_pointer":format!("/rows/{row}/values")}),
        );
        output.push(slot);
    }
    let mut document =
        serde_json::to_value(PassiveJewelSlotsDef { slots: output }).map_err(|e| e.to_string())?;
    document["_meta"] = json!({"generator":"pobr-data-adapter ordinary passive-jewel-slots",
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
        "supplement_manifest_sha256":sha256_hex(&extra.manifest_bytes),
        "import_contract":"PassiveJewel_<n> uses physical row n, as the existing ordinary importer defines; not an independent China-client equivalence claim."});
    Ok(Converted {
        bytes: super::pretty_bytes(&document)?,
        receipt: json!({"records":records,"supplement_manifest_sha256":sha256_hex(&extra.manifest_bytes),
            "supplement":extra.manifest,"ownership":"Every physical slot and its exact PassiveSkills FK, stable ID, raw graph ID and sinister flag. No tree-membership filter or graph-ID fallback.",
            "limitations":"Cluster/proxy/replacement cases are rejected. Remaining raw fields are evidence, not new cluster behavior. Slot membership does not certify allocation legality, active tree membership or China-realm equivalence."}),
    })
}
