//! `extract-lua --what parser-rules`: extracts vendor `Modules/ModParser.lua`'s
//! six parse-rule tables (excluding special) into `data/<version>/overlay/mod_parser_rules.json`
//!
//! Responsibility split (matches the existing extraction targets):
//! - The Lua bootstrap script (`extract_parser_rules.lua`) handles headless
//!   loading, pulling tables via upvalues, mask/enum reverse-lookup, and
//!   closure-probe inference, then emits JSONL;
//! - The Rust side handles derived fields ([`derive_pattern_meta`]: literal /
//!   anchored), sorting, count self-checks (zero tolerance at the pinned
//!   vendor commit), and byte-stable serialization.
//!
//! Unlike the other `--what` targets: ModParser.lua needs the full PoB2
//! environment (all of ModTools / Data), so it's bootstrapped the **same
//! headless way as pob2-oracle** — the child process's cwd must be vendor
//! `src/` with `LUA_PATH` pointing at `runtime/lua`, so this doesn't reuse
//! [`crate::extract_lua::invoke_luajit_jsonl`] (which sets neither cwd nor env).
//!
//! Also includes the parser-rules drift diff (`sync-pob-catalog
//! parser-rules-drift`): byte-diffs a fresh re-extraction against what's
//! committed, plus a per-section diff summary (task 3).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::process::{Command, Stdio};

use pobr_data::catalog::parser_rules::{
    FlagTypeDef, FormDef, MOD_PARSER_RULES_SCHEMA, ModParserRulesDoc, NameMapDef, PhraseNamesDef,
    PhraseValueDef, PreFlagDef, RuleEffectsDef, StatMapValue, TagPhraseDef, TagTemplate,
};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory).
const BOOTSTRAP_LUA: &str = include_str!("extract_parser_rules.lua");

/// The vendor commit (`.pob2-version.txt`) the count self-check is pinned
/// to. At this commit, every section's entry count must match
/// [`PINNED_SECTION_COUNTS`] exactly; at any other commit (version-bump
/// drills) mismatches only warn instead of erroring, so the extractor can absorb vendor drift.
pub const PINNED_VENDOR_COMMIT: &str = "2df5a7433dd2f1609e2fad8a6c3c917f923fe34f";

/// Per-section entry counts at the pinned commit (measured 2026-06; earlier
/// estimates of 776/684 were superseded by these measured values).
///
/// `flag_types` = 24 (vendor's main table) + 1 (the legacy `hindered`
/// special case restored during route-B extraction, see
/// [`normalize_legacy_consistency`]) = 25.
pub const PINNED_SECTION_COUNTS: &[(&str, usize)] = &[
    ("forms", 91),
    ("name_map", 775),
    // 202 extracted minus dead entries removed ([`VENDOR_DEAD_FLAG_PHRASES`]).
    ("flag_phrases", 202 - VENDOR_DEAD_FLAG_PHRASES.len()),
    ("pre_flags", 219),
    // 682 extracted plus pobr's own additions ([`POBR_EXTRA_TAG_PHRASES`]).
    ("tag_phrases", 682 + POBR_EXTRA_TAG_PHRASES.len()),
    ("suffix_types", 40),
    ("damage_types", 5),
    ("pen_types", 6),
    ("regen_types", 32),
    ("degen_types", 32),
    ("cost_types_map", 32),
    ("base_cost_types", 32),
    ("flag_types", 25),
    ("unsupported", 1),
];

/// The full set of form ids in formList at the pinned commit.
pub const PINNED_FORM_IDS: &[&str] = &[
    "BASE",
    "BASECOST",
    "CHANCE",
    "DEGEN",
    "DEGENFLAT",
    "DEGENPERCENT",
    "DMG",
    "DMGATTACKS",
    "DMGBOTH",
    "DMGSPELLS",
    "DMGTHORNS",
    "DMGTHORNSBASE",
    "DOUBLED",
    "FLAG",
    "GAIN",
    "GRANTS",
    "GRANTS_GLOBAL",
    "INC",
    "LESS",
    "LOSE",
    "MORE",
    "OVERRIDE",
    "PEN",
    "RED",
    "REGENFLAT",
    "REGENPERCENT",
    "REMOVES",
    "TOTALCOST",
];

/// pobr's own extra unsupported entries (vendor only has `mirrored`; `split`
/// comes from the current hardcoded `pobr-core::mod_parser` and must be
/// carried over with its source noted when migrating to the table).
///
/// This line survived the B3 gate switch. All historical debt on it has
/// **fully** cleared (kept in sync with the overlay JSON surgery — this
/// table is the single source for regen): the two deadeye entries cleared
/// with the gain-as fallback fix (PR#50); the gemling grenade entry cleared
/// with the "grenade phrase preempted by skillNameList" fix (PR#53); the
/// blood-mage curse entry cleared once the curse mechanism was fully
/// verified end-to-end plus the same-group support granted-level fix
/// (honestly exposing its per-hit undercount, see the ninja_parity dot
/// baseline note). The only thing left on the frozen list is legacy `split`.
const POBR_EXTRA_UNSUPPORTED: &[&str] = &["split"];

/// B3 table migration: the engine-data equivalent of the named herald
/// condition phrases hardcoded in legacy (`legacy.rs`'s herald buff
/// condition family). Vendor's ModParser.lua:6437 registers `while affected
/// by <skillname>` -> `Condition AffectedBy<name without spaces>`
/// **dynamically at runtime** for aura/herald gem names — static extraction
/// can't reach this (headless extraction doesn't bootstrap gem data). For
/// now this is a static enumeration of the legacy set; a systematic fix
/// would generate this from the full gem catalog (C5 territory).
const POBR_EXTRA_TAG_PHRASES: &[(&str, &str)] = &[
    ("while affected by herald of ash", "AffectedByHeraldofAsh"),
    (
        "while affected by herald of blood",
        "AffectedByHeraldofBlood",
    ),
    ("while affected by herald of ice", "AffectedByHeraldofIce"),
    (
        "while affected by herald of plague",
        "AffectedByHeraldofPlague",
    ),
    (
        "while affected by herald of thunder",
        "AffectedByHeraldofThunder",
    ),
];

/// Vendor dead entries: flag phrases that exist in modFlagList but that
/// vendor's `parseMod` **never actually matches** at runtime —
/// skillNameList's SkillName stripping (order=1, runs before the flag scan)
/// preemptively eats the skill name out of the phrase, and a non-empty
/// leftover means the whole line doesn't apply. Verification method: feed
/// the real mod text through `tools/pob2-oracle/run-parsemod.sh` and check
/// the leftover; re-run the same method whenever a version upgrade produces
/// a similar over/under-apply in parity.
///
/// **0.5.4b (vendor 0.22.0) emptied the grenade entry**: vendor's gem-name
/// registration loop gained a `not grantedEffect.fromItem` exclusion
/// (ModParser.lua:6423, the 0.21->0.22 delta), so
/// `MeleeGrenadeLauncherPlayer` (name "Grenade", fromItem) no longer
/// registers a skillNameList entry -> `grenade` / `for grenade skills`
/// reverts to being matched by modFlagList's **live** `SkillType.Grenade`
/// tag (confirmed via run-parsemod: `15% increased Cooldown Recovery Rate
/// for Grenade Skills` -> CooldownRecovery INC 15 + SkillType 159, empty
/// leftover). The 0.21-era dead entry / preempted rewrite (PR#53) is
/// reverted accordingly; this is the root cause of deadeye's 3x15 CDR
/// tree-mod under-apply (Speed 0.164 vs oracle 0.254).
const VENDOR_DEAD_FLAG_PHRASES: &[&str] = &[];

/// Entries preempted by vendor's skillNameList (mechanism explained in
/// [`VENDOR_DEAD_FLAG_PHRASES`] — the "keep the line, rewrite to an inert
/// SkillName tag" variant). Emptied as of 0.5.4b (same fromItem exclusion above).
const VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES: &[(&str, &str)] = &[];

/// The full overlay document (generation side; the consumption-side schema
/// is [`pobr_data::catalog::parser_rules::ModParserRulesDoc`], with a matching serde shape).
#[derive(Debug, Serialize, Deserialize)]
pub struct ParserRulesDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file).
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The rule sections.
    #[serde(flatten)]
    pub rules: ModParserRulesDoc,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_parser_rules(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_headless_jsonl(args)?;
    let mut rules = assemble_rules(rows)?;
    finalize_rules(&mut rules);
    let meta = build_meta(args)?;
    for warning in self_check(&rules, &meta.vendor_commit)? {
        eprintln!("extract-parser-rules: {warning}");
    }
    let doc = ParserRulesDoc { meta, rules };
    let mut json = serde_json::to_string_pretty(&doc).expect("parser rules 文档序列化不应失败");
    json.push('\n');
    Ok(json)
}

/// Headless bootstrap invocation: cwd = vendor `src/`, `LUA_PATH` points at
/// `runtime/lua`, `CI=true` (the same convention as `tools/pob2-oracle/run.sh`).
fn invoke_headless_jsonl(args: &ExtractLuaArgs) -> io::Result<Vec<serde_json::Value>> {
    let runtime = args.vendor_root.join("../runtime/lua");
    let lua_path = format!("{r}/?.lua;{r}/?/init.lua;./?.lua;;", r = runtime.display());
    let mut child = Command::new(&args.luajit)
        .arg("-") // read the script from stdin
        .arg(&args.vendor_root)
        .current_dir(&args.vendor_root)
        .env("LUA_PATH", lua_path)
        .env("CI", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "无法启动 luajit（{}）：{error}；请安装 luajit 或用 --luajit / POBR_LUAJIT 指定路径",
                    args.luajit.display()
                ),
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin 已配置为 piped")
        .write_all(BOOTSTRAP_LUA.as_bytes())?;

    let output = child.wait_with_output()?;
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "parser-rules 引导脚本执行失败（exit: {:?}）：{}",
            output.status.code(),
            stderr_text.trim()
        )));
    }
    for line in stderr_text.lines() {
        eprintln!("extract-parser-rules(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut rows = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

/// Dispatch JSONL rows by `section` and deserialize into each section's typed defs.
fn assemble_rules(rows: Vec<serde_json::Value>) -> io::Result<ModParserRulesDoc> {
    let mut doc = ModParserRulesDoc::default();
    for mut row in rows {
        let Some(object) = row.as_object_mut() else {
            return Err(io::Error::other("JSONL 行不是对象"));
        };
        let Some(section) = object.remove("section").and_then(|s| match s {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        }) else {
            return Err(io::Error::other("JSONL 行缺少 section 字段"));
        };
        let context = |error: serde_json::Error| {
            io::Error::other(format!("section `{section}` 行反序列化失败：{error}"))
        };
        match section.as_str() {
            "forms" => doc.forms.push(from_row::<FormDef>(row).map_err(context)?),
            "name_map" => doc
                .name_map
                .push(from_row::<NameMapDef>(row).map_err(context)?),
            "flag_phrases" => doc.flag_phrases.push(from_row(row).map_err(context)?),
            "pre_flags" => doc
                .pre_flags
                .push(from_row::<PreFlagDef>(row).map_err(context)?),
            "tag_phrases" => doc
                .tag_phrases
                .push(from_row::<TagPhraseDef>(row).map_err(context)?),
            "suffix_types" => doc
                .suffix_types
                .push(from_row::<PhraseValueDef>(row).map_err(context)?),
            "damage_types" => doc.damage_types.push(from_row(row).map_err(context)?),
            "pen_types" => doc.pen_types.push(from_row(row).map_err(context)?),
            "regen_types" => doc
                .regen_types
                .push(from_row::<PhraseNamesDef>(row).map_err(context)?),
            "degen_types" => doc.degen_types.push(from_row(row).map_err(context)?),
            "cost_types_map" => doc.cost_types_map.push(from_row(row).map_err(context)?),
            "base_cost_types" => doc.base_cost_types.push(from_row(row).map_err(context)?),
            "flag_types" => doc
                .flag_types
                .push(from_row::<FlagTypeDef>(row).map_err(context)?),
            "unsupported" => {
                #[derive(Deserialize)]
                struct Row {
                    phrase: String,
                }
                doc.unsupported
                    .push(from_row::<Row>(row).map_err(context)?.phrase);
            }
            other => {
                return Err(io::Error::other(format!("未知 JSONL section：{other}")));
            }
        }
    }
    Ok(doc)
}

fn from_row<T: serde::de::DeserializeOwned>(row: serde_json::Value) -> serde_json::Result<T> {
    serde_json::from_value(row)
}

/// Derived fields (literal / anchored) + per-section sorting + pobr's own extra unsupported entries.
fn finalize_rules(doc: &mut ModParserRulesDoc) {
    for form in &mut doc.forms {
        let (literal, anchored) = derive_pattern_meta(&form.pattern);
        form.literal = literal;
        form.anchored = anchored;
    }
    for entry in &mut doc.pre_flags {
        let (literal, anchored) = derive_pattern_meta(&entry.pattern);
        entry.literal = literal;
        entry.anchored = anchored;
    }
    // pobr's own extra tag phrases (B3 table migration, see
    // [`POBR_EXTRA_TAG_PHRASES`]) — inserted before the derive loop so
    // literal/anchored go through the same derivation as extracted entries.
    for (phrase, var) in POBR_EXTRA_TAG_PHRASES {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("var".to_string(), StatMapValue::Text((*var).to_string()));
        doc.tag_phrases.push(TagPhraseDef {
            pattern: (*phrase).to_string(),
            literal: None,
            anchored: false,
            effects: RuleEffectsDef {
                tags: vec![TagTemplate {
                    tag_type: "Condition".to_string(),
                    fields,
                }],
                ..Default::default()
            },
            inferred: false,
            handler_id: None,
        });
    }
    for entry in &mut doc.tag_phrases {
        let (literal, anchored) = derive_pattern_meta(&entry.pattern);
        entry.literal = literal;
        entry.anchored = anchored;
    }

    // Remove vendor dead entries (B3, see [`VENDOR_DEAD_FLAG_PHRASES`]): the
    // engine no longer produces a flag/tag for these phrases -> the mod text
    // is left unparsed, matching vendor's "extra non-empty leftover means the whole line doesn't apply".
    doc.flag_phrases
        .retain(|e| !VENDOR_DEAD_FLAG_PHRASES.contains(&e.phrase.as_str()));

    // skillNameList preemption rewrite (see
    // [`VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES`]): the payload changes from
    // SkillType to an inert SkillName (vendor's actual runtime output).
    for entry in &mut doc.flag_phrases {
        if let Some((_, skill_name)) = VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES
            .iter()
            .find(|(p, _)| *p == entry.phrase)
        {
            entry.effects.tags = vec![TagTemplate {
                tag_type: "SkillName".into(),
                fields: [(
                    "skillName".to_string(),
                    StatMapValue::Text((*skill_name).to_string()),
                )]
                .into_iter()
                .collect(),
            }];
        }
    }

    // Sort discipline: Lua's pairs() has no defined order -> every section is sorted lexicographically by pattern/phrase.
    doc.forms.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.name_map.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.flag_phrases.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.pre_flags.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.tag_phrases.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.suffix_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.damage_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.pen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.regen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.degen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.cost_types_map.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.base_cost_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.flag_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.unsupported.sort();
    doc.unsupported_pobr_extra = POBR_EXTRA_UNSUPPORTED
        .iter()
        .map(|s| s.to_string())
        .collect();

    // .3 route B: normalize the vendor->PoBR name table during extraction
    // (alias rename + aggregate expansion + DamageType tag). The engine
    // produces PoBR StatIds directly, so downstream needs zero changes and there's no runtime translation layer.
    normalize_name_map_to_pobr(doc);
}

/// .3 route B extraction-time normalization: normalizes `name_map`'s vendor
/// ModNames into PoBR canonical StatIds (alias table
/// [`VENDOR_NAME_ALIASES`]) and expands aggregate names by phrase
/// ([`AGGREGATE_EXPANSION`]). **The source of truth is
/// `data/overlay-common/vendor_name_aliases.json`** (this table's
/// real-rename subset matches it).
///
/// Design: only mutates `names`, never adds or removes entries (keeps
/// [`PINNED_SECTION_COUNTS`] counts intact). The DamageType tag (C5,
/// attached by final name to avoid mis-attaching to suffix-transformed
/// names), DMG-family names (`PhysicalMin` -> `PhysicalDamageMin`), damage
/// flag -> special name (C3), and PerStat -> Multiplier (C2) are all
/// normalized by the engine instead (a compose-time artifact that a static name_map can't express).
fn normalize_name_map_to_pobr(doc: &mut ModParserRulesDoc) {
    let alias: std::collections::HashMap<&str, &str> =
        VENDOR_NAME_ALIASES.iter().copied().collect();
    let aggregate: std::collections::HashMap<&str, &[&str]> =
        AGGREGATE_EXPANSION.iter().copied().collect();

    for entry in &mut doc.name_map {
        // 1. Aggregate phrase expansion takes priority (replaces the whole names group).
        if let Some(children) = aggregate.get(entry.phrase.as_str()) {
            entry.names = children.iter().map(|s| s.to_string()).collect();
        } else {
            // 2. Per-name alias normalization (real-renames apply, identity entries are no-ops).
            for n in &mut entry.names {
                if let Some(pobr) = alias.get(n.as_str()) {
                    *n = (*pobr).to_string();
                }
            }
        }
        // 3. Phrases whose embedded scope word got absorbed into a special
        //    name: clear the leftover vendor flag (legacy's parse_name maps
        //    `critical spell damage bonus` as a whole to
        //    `CriticalStrikeMultiplier` with no Spell flag, whereas vendor
        //    splits out `spell` as a flag). Only this one confirmed phrase.
        if FLAGLESS_NAME_PHRASES.contains(&entry.phrase.as_str()) {
            entry.effects.flags.clear();
        }
        // 4. Special-name normalization (C3): legacy's parse_name maps
        //    `attack damage` as a whole to the special name `AttackDamage`
        //    (vendor: `Damage` + Attack flag). Rename and clear the flag.
        if let Some(special) = SPECIAL_NAME_PHRASES
            .iter()
            .find(|(p, _)| *p == entry.phrase.as_str())
        {
            entry.names = vec![special.1.to_string()];
            entry.effects.flags.clear();
        }
    }

    // Weapon-scope keyword -> flag (C3): vendor's flag_phrases record `with
    // bow skills` as keywordFlags Bow (PoBR has no weapon keyword bit, so it
    // would be dropped), while legacy folds it into ModFlag(Hit,Bow) which
    // then gets absorbed into a special name. Normalize to
    // `flags:[Hit,<Weapon>]` (matching the shape of `with bows`), and the
    // engine's C3 derives special names like BowDamage from that.
    for entry in &mut doc.flag_phrases {
        let weapon = entry
            .effects
            .keyword_flags
            .iter()
            .find(|k| WEAPON_KEYWORDS.contains(&k.as_str()))
            .cloned();
        if let Some(w) = weapon {
            entry.effects.keyword_flags.retain(|k| k != &w);
            entry.effects.flags = vec!["Hit".to_string(), w];
        }
    }

    normalize_legacy_consistency(doc);
}

/// .3 route B (D-T8 second wave 2a): normalizes three vendor<->legacy shape
/// discrepancies during extraction, so the engine produces legacy-consistent
/// values (dual-run C1 DIFF=0/OLD_ONLY=0) while preserving the invariant
/// that `data/` is tool-regenerated (no more hand-editing
/// `mod_parser_rules.json`). All three are convergence items for "4 real bugs":
///
/// 1. `from equipped focus` (flag_phrases): vendor additionally attaches
///    `Condition(UsingFocus)`, while legacy only applies the
///    `SlotName(Weapon 2)` scope — drop the redundant UsingFocus condition.
/// 2. The `stat` field of helmet PerStat/StatThreshold (tag_phrases): vendor
///    writes `*OnHelmet` (capital H), but the stat name legacy registers is
///    `*Onhelmet` (lowercase h, from the slotName-lowercasing path) —
///    lowercase the h to match the registered stat name.
/// 3. `hindered` (flag_types): legacy's `parseEnemyInner` special-cases it as
///    a `Condition:Hindered` flag_type, but vendor's main table has no such
///    entry — restore it (same convergence scope as 2a; keeping the section
///    count in sync with the [`PINNED_SECTION_COUNTS`] pinned values).
fn normalize_legacy_consistency(doc: &mut ModParserRulesDoc) {
    // 1. focus: drop Condition(UsingFocus), keep only the SlotName scope.
    for entry in &mut doc.flag_phrases {
        if entry.phrase == FOCUS_PHRASE {
            entry.effects.tags.retain(|t| {
                !(t.tag_type == "Condition"
                    && matches!(
                        t.fields.get("var"),
                        Some(StatMapValue::Text(v)) if v == "UsingFocus"
                    ))
            });
        }
    }

    // 2. helmet stat: `*OnHelmet` -> `*Onhelmet` (only the trailing
    //    `OnHelmet` segment, to avoid mangling other slot names like `OnBody Armour`).
    for entry in &mut doc.tag_phrases {
        for tag in &mut entry.effects.tags {
            if let Some(StatMapValue::Text(stat)) = tag.fields.get_mut("stat")
                && let Some(prefix) = stat.strip_suffix("OnHelmet")
            {
                *stat = format!("{prefix}Onhelmet");
            }
        }
    }

    // 3. hindered flag_type: restore the legacy special case (missing from vendor's main table; same convergence scope as 2a).
    if !doc
        .flag_types
        .iter()
        .any(|e| e.phrase == HINDERED_FLAG_TYPE_PHRASE)
    {
        doc.flag_types.push(FlagTypeDef {
            phrase: HINDERED_FLAG_TYPE_PHRASE.to_string(),
            condition: Some("Condition:Hindered".to_string()),
            mod_def: None,
        });
        doc.flag_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    }
}

/// The focus-scope phrase (drops the redundant UsingFocus condition).
const FOCUS_PHRASE: &str = "from equipped focus";

/// The flag_type phrase restored from legacy's `parseEnemyInner` special case.
const HINDERED_FLAG_TYPE_PHRASE: &str = "hindered";

/// Weapon type names (normalized to a ModFlag when they appear in flag_phrases' keyword_flags).
const WEAPON_KEYWORDS: &[&str] = &[
    "Bow",
    "Crossbow",
    "Spear",
    "Mace",
    "Quarterstaff",
    "Warstaff",
    "Sword",
    "Claw",
    "Wand",
    "Staff",
];

/// Phrases where vendor's name_map carries a scope flag, but legacy embeds
/// the scope into the special name instead (no flag). Cleared during
/// extraction to match legacy (a C3 embedded-scope subcase).
const FLAGLESS_NAME_PHRASES: &[&str] = &["critical spell damage bonus"];

/// Phrases where vendor uses `Damage` + a scope flag, but legacy uses a standalone special name (C3): renamed and flag-cleared during extraction.
const SPECIAL_NAME_PHRASES: &[(&str, &str)] = &[("attack damage", "AttackDamage")];

/// The vendor -> PoBR alias table (20 real-renames + 56 identity entries;
/// the source of truth is `vendor_name_aliases.json`). Applied to every
/// ModName in `name_map` during extraction. Identity entries can be omitted
/// (applying the table is a no-op for them), so only the 20 real-renames are listed here.
const VENDOR_NAME_ALIASES: &[(&str, &str)] = &[
    ("ChaosResist", "ChaosResistance"),
    ("ChaosResistMax", "MaximumChaosResistance"),
    ("ColdResist", "ColdResistance"),
    ("ColdResistMax", "MaximumColdResistance"),
    ("CritChance", "CriticalStrikeChance"),
    ("CritMultiplier", "CriticalStrikeMultiplier"),
    ("Dex", "Dexterity"),
    ("ElementalResistMax", "MaximumAllElementalResistances"),
    ("EnemyBleedDuration", "BleedDuration"),
    ("EnemyFreezeBuildup", "FreezeBuildup"),
    ("EnemyIgniteDuration", "IgniteDuration"),
    ("EnemyPoisonDuration", "PoisonDuration"),
    ("FireResist", "FireResistance"),
    ("FireResistMax", "MaximumFireResistance"),
    ("Int", "Intelligence"),
    ("Life", "MaximumLife"),
    ("LightningResist", "LightningResistance"),
    ("LightningResistMax", "MaximumLightningResistance"),
    ("Mana", "MaximumMana"),
    ("Str", "Strength"),
];

/// Aggregate phrase -> PoBR child-name set (C1, matches legacy's
/// `resolve_names` table). Vendor's name_map resolves these phrases to a
/// single aggregate name, or one containing a vendor combined name
/// (`All`/`StrInt`); PoBR's downstream has no ModStore expansion layer, so
/// they're expanded into PoBR child names during extraction instead.
const AGGREGATE_EXPANSION: &[(&str, &[&str])] = &[
    (
        "all elemental resistances",
        &["FireResistance", "ColdResistance", "LightningResistance"],
    ),
    // Vendor: `["all resistances"] = { "ElementalResist", "ChaosResist" }`
    // (ModParser.lua:288). PoBR's player-side resistance calc only reads the
    // discrete `FireResistance`/`ColdResistance`/`LightningResistance` (plus
    // ChaosResistance) and doesn't recognize the aggregate name
    // `ElementalResist` (that's only consumed on the enemy side) -> without
    // expansion the elemental-resistance part would silently drop. Includes
    // chaos: vendor's `ChaosResist` -> PoBR's `ChaosResistance`.
    (
        "all resistances",
        &[
            "FireResistance",
            "ColdResistance",
            "LightningResistance",
            "ChaosResistance",
        ],
    ),
    ("all attributes", &["Strength", "Dexterity", "Intelligence"]),
    ("attributes", &["Strength", "Dexterity", "Intelligence"]),
    ("strength and intelligence", &["Strength", "Intelligence"]),
    ("strength and dexterity", &["Strength", "Dexterity"]),
    ("dexterity and intelligence", &["Dexterity", "Intelligence"]),
    // Vendor: `["skill speed"] = { "Speed", "WarcrySpeed", "TotemPlacementSpeed" }`
    // (ModParser.lua:770). Bare `Speed` -> PoBR's speed-bucket name
    // `SkillSpeed`; WarcrySpeed / TotemPlacementSpeed fan out under their own
    // names (since backlog item #9, WarcrySpeed has a real consumer —
    // `pobr-core::calc::warcry`'s warcry cast time, CalcOffence.lua:350-359;
    // TotemPlacementSpeed is still an inert scope name). Previously this only
    // produced a single `SkillSpeed` name, so "N% increased Skill Speed" text silently didn't affect warcry cast speed.
    (
        "skill speed",
        &["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"],
    ),
];

/// Extraction self-check: at the pinned commit, count / form-id-set mismatches are zero-tolerance (Err);
/// at any other commit they only produce warnings (absorbing vendor drift during drills). Per-section key uniqueness is always checked.
fn self_check(doc: &ModParserRulesDoc, vendor_commit: &str) -> io::Result<Vec<String>> {
    let mut warnings = Vec::new();
    let counts: &[(&str, usize)] = &[
        ("forms", doc.forms.len()),
        ("name_map", doc.name_map.len()),
        ("flag_phrases", doc.flag_phrases.len()),
        ("pre_flags", doc.pre_flags.len()),
        ("tag_phrases", doc.tag_phrases.len()),
        ("suffix_types", doc.suffix_types.len()),
        ("damage_types", doc.damage_types.len()),
        ("pen_types", doc.pen_types.len()),
        ("regen_types", doc.regen_types.len()),
        ("degen_types", doc.degen_types.len()),
        ("cost_types_map", doc.cost_types_map.len()),
        ("base_cost_types", doc.base_cost_types.len()),
        ("flag_types", doc.flag_types.len()),
        ("unsupported", doc.unsupported.len()),
    ];
    let pinned: BTreeMap<&str, usize> = PINNED_SECTION_COUNTS.iter().copied().collect();
    let is_pinned_commit = vendor_commit == PINNED_VENDOR_COMMIT;
    for (section, actual) in counts {
        let expected = pinned[section];
        if *actual != expected {
            let message = format!(
                "section `{section}` 条目数 {actual} ≠ 钉定值 {expected}（vendor {vendor_commit}）"
            );
            if is_pinned_commit {
                return Err(io::Error::other(format!("抽取自检失败：{message}")));
            }
            warnings.push(message);
        }
    }

    let form_ids: BTreeSet<&str> = doc.forms.iter().map(|f| f.form.as_str()).collect();
    let pinned_forms: BTreeSet<&str> = PINNED_FORM_IDS.iter().copied().collect();
    if form_ids != pinned_forms {
        let message = format!(
            "form id 集与钉定集不一致：多出 {:?}，缺少 {:?}",
            form_ids.difference(&pinned_forms).collect::<Vec<_>>(),
            pinned_forms.difference(&form_ids).collect::<Vec<_>>()
        );
        if is_pinned_commit {
            return Err(io::Error::other(format!("抽取自检失败：{message}")));
        }
        warnings.push(message);
    }

    // Key uniqueness (mandatory at any commit)
    for (section, keys) in [
        (
            "forms",
            doc.forms
                .iter()
                .map(|f| f.pattern.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "name_map",
            doc.name_map.iter().map(|f| f.phrase.as_str()).collect(),
        ),
        (
            "flag_phrases",
            doc.flag_phrases.iter().map(|f| f.phrase.as_str()).collect(),
        ),
        (
            "pre_flags",
            doc.pre_flags.iter().map(|f| f.pattern.as_str()).collect(),
        ),
        (
            "tag_phrases",
            doc.tag_phrases.iter().map(|f| f.pattern.as_str()).collect(),
        ),
    ] {
        let unique: BTreeSet<&&str> = keys.iter().collect();
        if unique.len() != keys.len() {
            return Err(io::Error::other(format!(
                "抽取自检失败：section `{section}` 存在重复键"
            )));
        }
    }

    // Closure-inference stats (reporting input only; handler budget monitoring goes through the global ledger, this is just an FYI)
    let inferred = doc.pre_flags.iter().filter(|e| e.inferred).count()
        + doc.tag_phrases.iter().filter(|e| e.inferred).count();
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
    warnings.push(format!(
        "闭包条目统计：探针推断成功 {inferred} / handler 兜底 {handlers}（预算 ≤15，全局 <100 台账见 00-index §3.3）"
    ));
    if handlers > 15 {
        warnings.push(format!(
            "警告：handler 兜底条目 {handlers} 超出蓝图预估 ≤15"
        ));
    }
    Ok(warnings)
}

/// Derives (the longest literal run, whether it's `^`-anchored) from a Lua pattern.
///
/// Literal-run semantics: after stripping the anchor, capture parens,
/// character classes (`[...]`, single-char classes like `%d`), and variable
/// characters governed by a quantifier (`+ - * ?`), take the longest
/// remaining run of consecutive literal characters (used for aho-corasick
/// pre-filtering; the engine falls back to an always-check bucket for
/// `None`/too-short literals). Patterns are all lowercase ASCII (vendor's
/// scan matches against `lower()`-ed input).
pub fn derive_pattern_meta(pattern: &str) -> (Option<String>, bool) {
    let bytes = pattern.as_bytes();
    let anchored = pattern.starts_with('^');
    let mut runs: Vec<String> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, runs: &mut Vec<String>| {
        if !cur.is_empty() {
            runs.push(std::mem::take(cur));
        }
    };
    let is_quantifier = |b: u8| matches!(b, b'+' | b'-' | b'*' | b'?');
    let mut i = usize::from(anchored);
    let n = bytes.len();
    while i < n {
        match bytes[i] {
            b'%' => {
                if i + 1 >= n {
                    i += 1;
                    continue;
                }
                let escaped = bytes[i + 1];
                if escaped.is_ascii_alphanumeric() {
                    // A class element (%d / %a / %D...): single-char wildcard, breaks the literal run
                    flush(&mut cur, &mut runs);
                    i += 2;
                    if i < n && is_quantifier(bytes[i]) {
                        i += 1;
                    }
                } else if i + 2 < n && is_quantifier(bytes[i + 2]) {
                    // An escaped punctuation char with a quantifier: its
                    // occurrence count varies, so it doesn't join a run
                    flush(&mut cur, &mut runs);
                    i += 3;
                } else {
                    // Escaped punctuation (%% / %- / %.) = a literal character
                    cur.push(escaped as char);
                    i += 2;
                }
            }
            b'[' => {
                // A character class: skip to the matching `]` (accounting
                // for `%]` escapes), breaking the run and consuming any trailing quantifier
                flush(&mut cur, &mut runs);
                i += 1;
                while i < n {
                    if bytes[i] == b'%' {
                        i += 2;
                    } else if bytes[i] == b']' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                if i < n && is_quantifier(bytes[i]) {
                    i += 1;
                }
            }
            b'(' | b')' => {
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b'.' => {
                flush(&mut cur, &mut runs);
                i += 1;
                if i < n && is_quantifier(bytes[i]) {
                    i += 1;
                }
            }
            b'$' if i == n - 1 => {
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b if is_quantifier(b) && !cur.is_empty() => {
                // A quantifier governs the previous literal character: pop it and break the run
                cur.pop();
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b => {
                cur.push(b as char);
                i += 1;
            }
        }
    }
    flush(&mut cur, &mut runs);
    // The longest run; ties are broken by taking whichever appeared first (deterministic tie-break)
    let literal = runs
        .into_iter()
        .fold(None::<String>, |best, run| match best {
            Some(b) if run.len() > b.len() => Some(run),
            None => Some(run),
            other => other,
        });
    (literal.filter(|l| !l.is_empty()), anchored)
}

/// Read the vendor version file and build `_meta`.
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --what parser-rules --vendor-root vendor/PathOfBuilding-PoE2/src",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: MOD_PARSER_RULES_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/ModParser.lua".to_string()],
        regen_command: regen,
    })
}

// parser-rules drift diff (fresh re-extraction vs what's committed)

/// Drift diff results: byte equivalence plus a human-readable diff summary.
#[derive(Debug)]
pub struct ParserRulesDrift {
    /// Whether the fresh re-extraction is byte-equivalent to the committed file.
    pub identical: bool,
    /// Diff summary lines (empty when byte-equivalent).
    pub lines: Vec<String>,
}

/// Drift diff between the committed text and the freshly re-extracted text:
/// compares bytes first, and when they differ, gives per-section
/// added/removed/changed counts plus sample keys (up to 5 per category).
pub fn diff_parser_rules(committed: &str, regenerated: &str) -> io::Result<ParserRulesDrift> {
    if committed == regenerated {
        return Ok(ParserRulesDrift {
            identical: true,
            lines: Vec::new(),
        });
    }
    let parse = |text: &str, label: &str| -> io::Result<serde_json::Value> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::other(format!("{label} 不是合法 JSON：{error}")))
    };
    let committed_doc = parse(committed, "已提交文件")?;
    let regenerated_doc = parse(regenerated, "重抽产物")?;

    let mut lines = Vec::new();
    if committed_doc.get("_meta") != regenerated_doc.get("_meta") {
        lines.push("[_meta] 头部不一致（vendor commit / regen_command 漂移？）".to_string());
    }

    let empty = serde_json::Map::new();
    let committed_map = committed_doc.as_object().unwrap_or(&empty);
    let regenerated_map = regenerated_doc.as_object().unwrap_or(&empty);
    let sections: BTreeSet<&String> = committed_map
        .keys()
        .chain(regenerated_map.keys())
        .filter(|k| *k != "_meta")
        .collect();
    for section in sections {
        let by_key = |doc: &serde_json::Map<String, serde_json::Value>| -> BTreeMap<String, serde_json::Value> {
            let mut map = BTreeMap::new();
            if let Some(serde_json::Value::Array(items)) = doc.get(section.as_str()) {
                for (index, item) in items.iter().enumerate() {
                    let key = item
                        .get("pattern")
                        .or_else(|| item.get("phrase"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| match item {
                            serde_json::Value::String(s) => s.clone(),
                            _ => format!("#{index}"),
                        });
                    map.insert(key, item.clone());
                }
            }
            map
        };
        let old = by_key(committed_map);
        let new = by_key(regenerated_map);
        let added: Vec<&String> = new.keys().filter(|k| !old.contains_key(*k)).collect();
        let removed: Vec<&String> = old.keys().filter(|k| !new.contains_key(*k)).collect();
        let changed: Vec<&String> = old
            .iter()
            .filter(|(k, v)| new.get(*k).is_some_and(|nv| nv != *v))
            .map(|(k, _)| k)
            .collect();
        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            continue;
        }
        let sample = |keys: &[&String]| -> String {
            let mut text = keys
                .iter()
                .take(5)
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            if keys.len() > 5 {
                text.push_str(", …");
            }
            text
        };
        lines.push(format!(
            "[{section}] +{} -{} ~{}{}{}{}",
            added.len(),
            removed.len(),
            changed.len(),
            if added.is_empty() {
                String::new()
            } else {
                format!("；新增 {}", sample(&added))
            },
            if removed.is_empty() {
                String::new()
            } else {
                format!("；删除 {}", sample(&removed))
            },
            if changed.is_empty() {
                String::new()
            } else {
                format!("；变更 {}", sample(&changed))
            },
        ));
    }
    if lines.is_empty() {
        lines.push("byte 不等但结构 diff 为空（空白/键序差异？请直接 diff 文件）".to_string());
    }
    Ok(ParserRulesDrift {
        identical: false,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::derive_pattern_meta;

    /// Example: literal derivation with a `^` anchor plus a `%%` escape.
    #[test]
    fn literal_for_increased_form() {
        let (literal, anchored) = derive_pattern_meta("^(%d+)%% increased");
        assert_eq!(literal.as_deref(), Some("% increased"));
        assert!(anchored);
    }

    /// A literal character governed by a quantifier (`s?`) doesn't join the run; an unanchored pattern.
    #[test]
    fn literal_drops_quantified_char() {
        let (literal, anchored) = derive_pattern_meta("costs? ([%+%-]?%d+)");
        assert_eq!(literal.as_deref(), Some("cost"));
        assert!(!anchored);
    }

    /// A pure literal pattern: the whole thing is the literal.
    #[test]
    fn literal_for_plain_pattern() {
        let (literal, anchored) = derive_pattern_meta("is doubled");
        assert_eq!(literal.as_deref(), Some("is doubled"));
        assert!(!anchored);
    }

    /// A character class (`[...]`) breaks the run; example.
    #[test]
    fn literal_breaks_on_char_class() {
        let (literal, anchored) = derive_pattern_meta("^minions [cthd][ae][ukva][sel]e? ");
        assert_eq!(literal.as_deref(), Some("minions "));
        assert!(anchored);
    }

    /// A pattern made entirely of class elements (no literal segment at all) -> None.
    #[test]
    fn literal_none_for_all_class_pattern() {
        let (literal, anchored) = derive_pattern_meta("^(%d+)");
        assert_eq!(literal, None);
        assert!(anchored);
    }

    /// `.`/`.-` wildcards break the run; a trailing `$` anchor doesn't join the literal.
    #[test]
    fn literal_handles_wildcard_and_tail_anchor() {
        let (literal, _) = derive_pattern_meta("^regenerate ([%d%.]+) (.-) per second$");
        assert_eq!(literal.as_deref(), Some("regenerate "));
    }

    /// Drift diff: byte-equivalent -> identical; changed entries -> a per-section summary.
    #[test]
    fn drift_diff_reports_sections() {
        let committed = r#"{"_meta":{"schema":"mod_parser_rules/v1"},"forms":[{"pattern":"a","form":"INC"},{"pattern":"b","form":"RED"}]}"#;
        let same = super::diff_parser_rules(committed, committed).unwrap();
        assert!(same.identical);

        let regenerated = r#"{"_meta":{"schema":"mod_parser_rules/v1"},"forms":[{"pattern":"a","form":"MORE"},{"pattern":"c","form":"RED"}]}"#;
        let drift = super::diff_parser_rules(committed, regenerated).unwrap();
        assert!(!drift.identical);
        let joined = drift.lines.join("\n");
        assert!(joined.contains("[forms]"), "应有 forms 段摘要：{joined}");
        assert!(joined.contains("+1 -1 ~1"), "增删改计数应各为 1：{joined}");
        assert!(joined.contains("`c`") && joined.contains("`b`") && joined.contains("`a`"));
    }
}
