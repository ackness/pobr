//! `extract-lua --what minions|spectres|minion-list` (data production):
//!
//! - `minions` / `spectres`: runs vendor `Data/Minions.lua` /
//!   `Data/Spectres.lua` (bootstrap script `extract_minions.lua`, whose stub
//!   fully serializes `mod(...)`/`flag(...)` constructor arguments — this is
//!   how the stub layer avoids the argument-dropping warning), producing
//!   `overlay/minions.json` / `overlay/spectres.json`;
//! - `minion-list`: runs vendor `Data/Skills/*.lua` (bootstrap script
//!   `extract_minion_list.lua`), extracting the `minionList`/`minionUses`/
//!   `minionHasItemSet` / `ignoreMinionTypes` sidecar, producing
//!   `overlay/granted_effect_minions.json`.
//!
//! The shared layer (luajit JSONL invocation / vendor version parsing /
//! byte-stable serialization conventions) reuses [`crate::extract_lua`].

use std::io;

use serde::Serialize;

use pobr_data::catalog::actors::{GrantedEffectMinionDef, MinionEntryDef};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap scripts (embedded at compile time).
const MINIONS_BOOTSTRAP_LUA: &str = include_str!("extract_minions.lua");
const MINION_LIST_BOOTSTRAP_LUA: &str = include_str!("extract_minion_list.lua");

/// The parameters that differ between the `minions` and `spectres` extraction targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinionsKind {
    /// `Data/Minions.lua` -> `overlay/minions.json` (schema `minions/v1`).
    Minions,
    /// `Data/Spectres.lua` -> `overlay/spectres.json` (schema `spectres/v1`).
    Spectres,
}

impl MinionsKind {
    fn schema(self) -> &'static str {
        match self {
            Self::Minions => "minions/v1",
            Self::Spectres => "spectres/v1",
        }
    }

    fn vendor_file(self) -> &'static str {
        match self {
            Self::Minions => "Minions",
            Self::Spectres => "Spectres",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Minions => "minions",
            Self::Spectres => "spectres",
        }
    }
}

/// The full `overlay/minions.json` / `overlay/spectres.json` document (generation side).
#[derive(Debug, Serialize)]
struct MinionsDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    minions: Vec<MinionEntryDef>,
}

/// Run the minions / spectres extraction, returning byte-stable JSON text.
pub fn run_extract_minions(args: &ExtractLuaArgs, kind: MinionsKind) -> io::Result<String> {
    // The data file is a fixed single file; error when an explicit --files doesn't match the target (guards against misuse).
    let expected = [kind.vendor_file().to_string()];
    if args.files != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--what {} always extracts Data/{}.lua and does not accept a custom --files (got {:?})",
                kind.what(),
                kind.vendor_file(),
                args.files
            ),
        ));
    }
    let mut entries: Vec<MinionEntryDef> = invoke_luajit_jsonl(args, MINIONS_BOOTSTRAP_LUA)?;
    let mut extracted_files = vec![format!("Data/{}.lua", kind.vendor_file())];
    if matches!(kind, MinionsKind::Minions) {
        let template = "Export/Minions/Minions.txt";
        let text = std::fs::read_to_string(args.vendor_root.join(template))?;
        let mappings = monster_variety_ids(&text)?;
        for entry in &mut entries {
            entry.monster_variety_id = Some(mappings.get(&entry.id).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing monster variety for {}", entry.id),
                )
            })?);
        }
        extracted_files.push(template.into());
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let meta = build_meta(args, kind.schema(), kind.what(), extracted_files)?;
    let doc = MinionsDoc {
        meta,
        minions: entries,
    };
    Ok(to_pretty_json(&doc))
}

/// The template links PoB minion names to the game's monster variety ids;
/// resistance values themselves are loaded from the official game tables.
fn monster_variety_ids(text: &str) -> io::Result<std::collections::BTreeMap<String, String>> {
    let mut ids = std::collections::BTreeMap::new();
    for line in text
        .lines()
        .filter_map(|line| line.strip_prefix("#monster "))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2 || ids.insert(fields[1].into(), fields[0].into()).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or duplicate #monster mapping",
            ));
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    #[test]
    fn monster_mapping_keeps_game_ids_and_rejects_duplicates() {
        let ids = super::monster_variety_ids("#monster Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonArsonistPlayerSummoned RaisedSkeletonArsonist\n#emit\n").unwrap();
        assert_eq!(
            ids["RaisedSkeletonArsonist"],
            "Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonArsonistPlayerSummoned"
        );
        assert!(super::monster_variety_ids("#monster A B\n#monster C B\n").is_err());
    }
}

/// The full `overlay/granted_effect_minions.json` document (generation side).
#[derive(Debug, Serialize)]
struct GrantedEffectMinionsDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    entries: Vec<GrantedEffectMinionDef>,
}

/// Current overlay document schema identifier.
pub const GRANTED_EFFECT_MINIONS_SCHEMA: &str = "granted_effect_minions/v1";

/// Run the minion-list foreign-key sidecar extraction, returning byte-stable JSON text.
pub fn run_extract_minion_list(args: &ExtractLuaArgs) -> io::Result<String> {
    let mut entries: Vec<GrantedEffectMinionDef> =
        invoke_luajit_jsonl(args, MINION_LIST_BOOTSTRAP_LUA)?;
    entries.sort_by(|a, b| a.effect_id.cmp(&b.effect_id));
    let extracted = args
        .files
        .iter()
        .map(|name| format!("Data/Skills/{name}.lua"))
        .collect();
    let meta = build_meta(
        args,
        GRANTED_EFFECT_MINIONS_SCHEMA,
        "minion-list",
        extracted,
    )?;
    let doc = GrantedEffectMinionsDoc { meta, entries };
    Ok(to_pretty_json(&doc))
}

/// Assemble `_meta` (vendor commit / regen command; the canonical
/// relative-path convention matches `extract_lua::build_meta`).
fn build_meta(
    args: &ExtractLuaArgs,
    schema: &str,
    what: &str,
    extracted_files: Vec<String>,
) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what {what} --vendor-root vendor/PathOfBuilding-PoE2/src"
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: schema.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

/// Uniform serde_json pretty serialization (identical input always yields identical output, with a trailing newline).
pub(crate) fn to_pretty_json<T: Serialize>(doc: &T) -> String {
    let mut json =
        serde_json::to_string_pretty(doc).expect("overlay document serialization should not fail");
    json.push('\n');
    json
}
