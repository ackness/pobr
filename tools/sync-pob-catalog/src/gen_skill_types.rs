//! `gen-skill-types`: extracts the **full** enum (name -> 1-based id) from
//! vendor's `src/Data/Global.lua::SkillType` table, writing it out as a
//! `pobr-data` text sidecar (`crates/pobr-data/src/skill_type_names.txt`,
//! embedded via `include_str!` — the same pattern as pobr-i18n's locale
//! toml; the data table doesn't live in a `.rs` file, so the
//! `no_embedded_data` guard doesn't trip).
//!
//! Unlike the extract-lua family: this doesn't run luajit, just plain
//! line-by-line regex parsing (the table body has a stable `Name = number,`
//! shape). This is where data-driven decision A1 lands — a single source of
//! truth for SkillType name -> bit mapping, eliminating the hand-maintained
//! whitelist copies in template.rs / special_mod.rs / conditions.rs; after a
//! vendor version bump, regenerating this artifact keeps the enum in sync
//! (no more patching code name by name).

use std::fs;
use std::io;
use std::path::Path;

use regex::Regex;

/// Parse the SkillType table body of Global.lua, returning (name, id) pairs, sorted lexicographically by name.
pub fn parse_skill_types(lua_source: &str) -> io::Result<Vec<(String, u32)>> {
    let entry_re = Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\d+)\s*,").unwrap();
    let mut in_table = false;
    let mut entries: Vec<(String, u32)> = Vec::new();
    for line in lua_source.lines() {
        if !in_table {
            if line.trim_start().starts_with("SkillType = {") {
                in_table = true;
            }
            continue;
        }
        if line.trim_start().starts_with('}') {
            break;
        }
        if let Some(caps) = entry_re.captures(line) {
            let name = caps[1].to_string();
            let id: u32 = caps[2]
                .parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e}")))?;
            entries.push((name, id));
        }
    }
    if entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Global.lua 未找到 SkillType 表条目（表头或行格式漂移？）",
        ));
    }
    entries.sort();
    // Names must be unique (a precondition for binary search over the
    // generated table); duplicate ids can't happen under vendor semantics either, so guard against both.
    for w in entries.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SkillType 名重复：{}", w[0].0),
            ));
        }
    }
    Ok(entries)
}

/// Extract and write the generated file, returning a description of the written path.
pub fn run_gen_skill_types(vendor_root: &Path, out: &Path) -> io::Result<String> {
    let lua_path = vendor_root.join("src/Data/Global.lua");
    let source = fs::read_to_string(&lua_path)?;
    let entries = parse_skill_types(&source)?;
    let max_id = entries.iter().map(|(_, id)| *id).max().unwrap_or(0);

    let mut body = String::new();
    body.push_str("# PoB2 SkillType 全量枚举 name → id（1-based）。生成文件，勿手改。\n");
    body.push_str(
        "# 来源：vendor src/Data/Global.lua::SkillType（vendor 版本见 vendor/.pob2-version.txt）。\n",
    );
    body.push_str("# regen：cargo run -p sync-pob-catalog -- gen-skill-types --vendor-root vendor/PathOfBuilding-PoE2 --out crates/pobr-data/src/skill_type_names.txt\n");
    body.push_str(&format!(
        "# 按 name 字典序；共 {} 条，最大 id={max_id}。消费端 pobr-data::skill_type_names（include_str! + 懒解析）。\n",
        entries.len()
    ));
    for (name, id) in &entries {
        body.push_str(&format!("{name} {id}\n"));
    }

    fs::write(out, &body)?;
    Ok(format!(
        "gen-skill-types: wrote {} ({} entries, max id {max_id})",
        out.display(),
        entries.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_entries_sorted_and_stops_at_close() {
        let lua = "\
Foo = {}\n\
SkillType = {\n\
\tAttack = 1,\n\
\tSpell = 2, -- comment\n\
\tZzz = 30,\n\
}\n\
NotSkillType = 99,\n";
        let entries = parse_skill_types(lua).unwrap();
        assert_eq!(
            entries,
            vec![
                ("Attack".to_string(), 1),
                ("Spell".to_string(), 2),
                ("Zzz".to_string(), 30)
            ]
        );
    }

    #[test]
    fn empty_table_is_error() {
        assert!(parse_skill_types("SkillType = {\n}\n").is_err());
    }
}
