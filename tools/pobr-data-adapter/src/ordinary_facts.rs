//! Pinned physical facts to ordinary catalog domains, isolated from complete runtime packages.

mod base_items;
mod convert;
mod gem_effects;
mod monster_resistances;
mod quality;
mod skills;
mod stat_set_codec;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;

use crate::native_facts::{Output, reader};
use crate::raw_source::sha256_hex;

pub(crate) struct Args {
    input: PathBuf,
    expected_manifest_sha256: String,
    version: String,
    out: PathBuf,
    domain: Domain,
    quality_policy: Option<quality::Args>,
    codec: Option<CodecArgs>,
}

struct CodecArgs {
    source: PathBuf,
    source_sha256: String,
    policy: PathBuf,
    policy_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    StatsMods,
    BaseItems,
    Skills,
    GemQuality,
    StatSetCodec,
    GemEffects,
    MonsterResistances,
}

impl Domain {
    fn tables(self) -> &'static [&'static str] {
        match self {
            Self::StatsMods => &convert::TABLES,
            Self::BaseItems => &base_items::TABLES,
            Self::Skills => &skills::TABLES,
            Self::GemQuality => &quality::TABLES,
            Self::StatSetCodec => &stat_set_codec::TABLES,
            Self::GemEffects => &gem_effects::TABLES,
            Self::MonsterResistances => &monster_resistances::TABLES,
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::StatsMods => "ordinary-stats-mods",
            Self::BaseItems => "ordinary-base-items",
            Self::Skills => "ordinary-skills",
            Self::GemQuality => "ordinary-gem-quality",
            Self::StatSetCodec => "ordinary-stat-set-codec",
            Self::GemEffects => "ordinary-gem-effects",
            Self::MonsterResistances => "ordinary-monster-resistances",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::StatsMods => "Stats/Mods",
            Self::BaseItems => "base-items",
            Self::Skills => "skills",
            Self::GemQuality => "gem-quality",
            Self::StatSetCodec => "stat-set-codec",
            Self::GemEffects => "gem-effects",
            Self::MonsterResistances => "monster-resistances",
        }
    }
}

pub(crate) fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut values = BTreeMap::new();
    let mut args = args;
    while let Some(flag) = args.next() {
        if ![
            "--ordinary-facts",
            "--ordinary-domain",
            "--facts-manifest-sha256",
            "--version",
            "--out",
            "--quality-policy",
            "--quality-policy-sha256",
            "--codec-source",
            "--codec-source-sha256",
            "--codec-policy",
            "--codec-policy-sha256",
        ]
        .contains(&flag.as_str())
        {
            return Err(format!("unsupported ordinary-facts argument: {flag}"));
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        if value.starts_with("--") || values.insert(flag.clone(), value).is_some() {
            return Err(format!("missing value or duplicate argument: {flag}"));
        }
    }
    let domain = match values.remove("--ordinary-domain").as_deref() {
        None | Some("stats-mods") => Domain::StatsMods,
        Some("base-items") => Domain::BaseItems,
        Some("skills") => Domain::Skills,
        Some("gem-quality") => Domain::GemQuality,
        Some("stat-set-codec") => Domain::StatSetCodec,
        Some("gem-effects") => Domain::GemEffects,
        Some("monster-resistances") => Domain::MonsterResistances,
        Some(value) => return Err(format!("unsupported ordinary domain: {value}")),
    };
    if domain != Domain::GemQuality
        && (values.contains_key("--quality-policy")
            || values.contains_key("--quality-policy-sha256"))
    {
        return Err("quality policy arguments require --ordinary-domain gem-quality".into());
    }
    if !matches!(domain, Domain::StatSetCodec | Domain::GemEffects)
        && [
            "--codec-source",
            "--codec-source-sha256",
            "--codec-policy",
            "--codec-policy-sha256",
        ]
        .iter()
        .any(|flag| values.contains_key(*flag))
    {
        return Err(
            "codec arguments require --ordinary-domain stat-set-codec or gem-effects".into(),
        );
    }
    let mut take = |flag: &str| values.remove(flag).ok_or_else(|| format!("missing {flag}"));
    Ok(Args {
        input: take("--ordinary-facts")?.into(),
        expected_manifest_sha256: take("--facts-manifest-sha256")?,
        version: take("--version")?,
        out: take("--out")?.into(),
        domain,
        quality_policy: if domain == Domain::GemQuality {
            Some(quality::Args {
                path: take("--quality-policy")?.into(),
                expected_sha256: take("--quality-policy-sha256")?,
            })
        } else {
            None
        },
        codec: if matches!(domain, Domain::StatSetCodec | Domain::GemEffects) {
            Some(CodecArgs {
                source: take("--codec-source")?.into(),
                source_sha256: take("--codec-source-sha256")?,
                policy: take("--codec-policy")?.into(),
                policy_sha256: take("--codec-policy-sha256")?,
            })
        } else {
            None
        },
    })
}

pub(crate) fn run(args: Args) -> Result<String, String> {
    reader::require_absent(&args.out)?;
    let mut output = Output::prepare_paths(&args.input, &args.out)?;
    let input = reader::open(
        &args.input,
        &args.expected_manifest_sha256,
        &args.version,
        args.domain.tables(),
    )?;
    let mut overlay = BTreeMap::new();
    let mut policy_bytes = None;
    let mut codec_policy_bytes = None;
    let mut legacy_gem_effects_bytes = None;
    // Complete validation and serialization before reserving any output.
    let (base, conversion, output_schema, scope) = match args.domain {
        Domain::StatsMods => {
            let converted = convert::convert(&input)?;
            let base = BTreeMap::from([
                ("stats.json".into(), pretty_bytes(&converted.stats)?),
                ("mods.json".into(), pretty_bytes(&converted.mods)?),
            ]);
            (
                base,
                json!({
                    "stats":converted.stats.len(), "mods":converted.mods.len(), "stat_slots":8,
                    "filtered_shells":converted.filtered_shells,
                    "unassigned_stat_ranges":converted.unassigned_ranges,
                    "source_row_reference":"tables/Mods.json#/rows/{source_row}/values",
                    "mapping": {
                        "Stats":"Id -> id; IsLocal -> is_local; Semantic/Category -> optional raw u32 metadata",
                        "Mods":"Id -> id; Name -> name (null/empty -> absent); ModType -> raw mod_type and ModType.Name -> group; Domain -> domain; GenerationType -> generation_type; Level -> level",
                        "stats":"Stat1..8 -> Stats.Id by physical row; StatNMin/Max -> signed min/max in slot order; null StatN is unassigned, never row zero",
                        "families":"Families -> ModFamily.Id in source order; null differs from empty",
                        "tags":"Tags -> Tags.Id in source order",
                        "spawn_weights":"SpawnWeight_Tags -> Tags.Id paired with SpawnWeight_Values, preserving order and zero"
                    }
                }),
                "pobr_data::catalog::{StatDef,ModDef,ModStat,SpawnWeight}",
                vec![
                    "Only Stats/Mods and their ModType/ModFamily/Tags dependencies were read; unselected payloads and other foreign keys are not certified.",
                    "Physical rows identify source records inside the pinned payload, never stable business IDs. Source values remain in the immutable F package; no input was edited.",
                    "Semantic/Category remain raw metadata in the existing DTO; no enum or classification meaning is inferred.",
                    "Hardmode ranges, generation weights, implicit tags and other fields outside the ordinary DTO remain source facts, not newly applied semantics.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, vendor fallback, download, CURRENT update, calculation model or schema-4 admission change.",
                ],
            )
        }
        Domain::BaseItems => {
            let converted = base_items::convert(&input)?;
            (
                BTreeMap::from([("base_items.json".into(), pretty_bytes(&converted.bases)?)]),
                converted.receipt,
                "pobr_data::catalog::{BaseItemDef,WeaponBaseStats,ArmourBaseStats,UtilityBaseStats}",
                vec![
                    "Only base-item DTO fields and required joins from the selected eleven physical tables were converted; other foreign keys are not certified.",
                    "Physical row references are distinct from stable IDs. Unmapped fields remain in the pinned source; the receipt lists their payload/column/row scope.",
                    "Charm inherent effects are unavailable in this slice; empty charm_buff is not a claim of no effect or complete base-item semantics.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, vendor fallback, download, CURRENT update, calculation model or schema-4 admission change.",
                ],
            )
        }
        Domain::Skills => {
            let converted = skills::convert(&input)?;
            (
                converted.base,
                converted.receipt,
                "pobr_data::catalog::{SkillGemDef,GrantedEffectDef,SkillLevelDef,SkillStatSetDef,CostTypeDef}",
                vec![
                    "Only the selected fifteen skill tables were read; physical facts feed the original ordinary converters after strict validation.",
                    "All skill families are considered; no restricted skill profile or stat whitelist is used. Compatibility gaps and original conversion exclusions remain explicit in the receipt.",
                    "Quality applicability, vendor set ordinals, prepared skill/minion overrides and language assets remain pending; no other patch's assets are substituted.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, vendor fallback, download, CURRENT update, calculation model or schema-4 admission change.",
                ],
            )
        }
        Domain::GemQuality => {
            let policy = args
                .quality_policy
                .as_ref()
                .ok_or("missing quality policy arguments")?;
            let converted = quality::convert(&input, policy)?;
            output.protect(&converted.policy_parent)?;
            overlay = converted.overlay;
            policy_bytes = Some(converted.policy_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::{GemQualityStatsDef,GemQualityStatDef,QualityStat}",
                vec![
                    "Only GrantedEffects, Stats and GrantedEffectQualityStats were read; target F supplies all numeric values and stable IDs.",
                    "The explicitly pinned policy records target applicability of the existing effect-wide model, with prior source/patch/vendor evidence kept separate. It does not supply numeric values.",
                    "Main/alt order and unapplied scope arrays are preserved. Exclusions and unconsumed fields remain traceable target facts, not claims of no effect.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, vendor fallback, download, CURRENT update, calculation model or schema-4 admission change.",
                ],
            )
        }
        Domain::StatSetCodec => {
            let codec = args
                .codec
                .as_ref()
                .ok_or("missing codec source/policy arguments")?;
            let converted = stat_set_codec::convert(&input, codec)?;
            for parent in &converted.protected_parents {
                output.protect(parent)?;
            }
            overlay = converted.overlay;
            codec_policy_bytes = Some(converted.policy_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::{StatSetLabelsDef,StatSetLabelDef}",
                vec![
                    "The explicitly pinned legacy codec is copied verbatim, including its original metadata, record order, labels and export indices; it is not relabelled as target acquisition.",
                    "Only target GrantedEffects and GrantedEffectStatSets are read. Raw stable-ID ownership is verified with explicit exact-key exceptions; no index inference, ordered-reference deduplication or branch remapping occurs.",
                    "Historical pre-merge binding evidence is separate from prepared runtime selection, full BD execution and compatibility with later exporters. Native-label priority and conservative primary fallback remain unchanged.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, implicit old numeric/vendor input, Lua execution, download, CURRENT update or consumer model change.",
                ],
            )
        }
        Domain::GemEffects => {
            let codec = args
                .codec
                .as_ref()
                .ok_or("missing codec source/policy arguments")?;
            let converted = gem_effects::convert(&input, codec)?;
            for parent in &converted.protected_parents {
                output.protect(parent)?;
            }
            overlay = converted.overlay;
            codec_policy_bytes = Some(converted.policy_bytes);
            legacy_gem_effects_bytes = Some(converted.source_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::{GemEffectsDef,GemEffectDef}",
                vec![
                    "All normal gem relationships derive from the five selected target F tables; only the original placeholder predicate excludes rows.",
                    "The explicitly pinned legacy codec supplies display order and primary-owner precedence only. Every legacy relationship tuple must match target facts; no exceptions or remapping.",
                    "All shared-primary gems remain in the output. Non-preferred rows precede preferred rows to preserve existing base-first plus legacy-owner selection with the unchanged overlay last-wins consumer.",
                    "Legacy codec bytes and provenance, target source acquisition and historical source-check evidence remain separate. Full ordinary BD/Web behavior and complete runtime compatibility are not established by this domain.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, implicit old numeric/vendor input, Lua execution, download, CURRENT update or consumer model change.",
                ],
            )
        }
        Domain::MonsterResistances => {
            let converted = monster_resistances::convert(&input)?;
            (
                converted.base,
                converted.receipt,
                "pobr_data::catalog::{MonsterResistancesDef,MonsterResistanceDef}",
                vec![
                    "Only MonsterVarieties, MonsterTypes and MonsterResistances were read. All target normal-rarity base resistances derive from F through the original ordinary join.",
                    "Every ordered resistance occurrence contributes, including duplicates, negative values and zeros. An explicit empty reference list means known base zeros; missing or invalid facts fail.",
                    "Other rarities, future level arrays, flags, full minion/spectre catalogs, minionList/loadout and totem catalogs remain separate work; they are not declared effectless or certified by this table.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, compatibility policy, old numeric/vendor input, download, CURRENT update or consumer model change.",
                ],
            )
        }
    };
    let marker = reader::json_bytes(&json!({
        "schema": "pobr-domain-candidate/v1",
        "domain": args.domain.marker(),
        "version": args.version,
        "source_id": input.manifest["source_identity"]["source"],
        "source_manifest_sha256": args.expected_manifest_sha256,
        "publication": "blocked: domain-only candidate; not a complete runtime package"
    }))?;
    let mut sidecars = BTreeMap::from([
        ("_domain_candidate.json", marker),
        ("source-manifest.json", input.manifest_bytes.clone()),
        ("source-lock.json", input.lock_bytes.clone()),
    ]);
    if let Some(bytes) = policy_bytes {
        sidecars.insert("quality-policy.json", bytes);
    }
    if let Some(bytes) = codec_policy_bytes {
        sidecars.insert("codec-policy.json", bytes);
    }
    if let Some(bytes) = legacy_gem_effects_bytes {
        sidecars.insert("legacy-gem-effects.json", bytes);
    }
    let files: BTreeMap<_, _> = base
        .iter()
        .map(|(name, data)| (format!("base/{name}"), data))
        .chain(
            overlay
                .iter()
                .map(|(name, data)| (format!("overlay/{name}"), data)),
        )
        .chain(sidecars.iter().map(|(name, data)| (name.to_string(), data)))
        .map(|(name, data)| (name, json!({"bytes":data.len(), "sha256":sha256_hex(data)})))
        .collect();
    let selected: BTreeMap<_, _> = input
        .bytes
        .iter()
        .map(|(path, bytes)| {
            (
                path,
                json!({"bytes":bytes.len(), "sha256":sha256_hex(bytes)}),
            )
        })
        .collect();
    let receipt = reader::json_bytes(&json!({
        "format": "pobr-ordinary-facts-receipt/v1",
        "status": "domain-candidate",
        "complete_runtime": false,
        "adoption": "not-adopted",
        "target": {"game":"poe2", "realm":input.manifest["source_identity"]["realm"], "version":args.version},
        "adapter": {
            "name":"pobr-data-adapter/ordinary-facts", "version":env!("CARGO_PKG_VERSION"),
            "implementation_revision":format!("sha256:{}", implementation_sha256()),
            "output_schema":output_schema
        },
        "input": {
            "root":args.input.canonicalize().map_err(|e| e.to_string())?,
            "manifest_sha256":args.expected_manifest_sha256,
            "source_identity":input.manifest["source_identity"],
            "producer":input.manifest["producer"],
            "schema":input.manifest["schema"],
            "selected_payloads":selected
        },
        "conversion": conversion,
        "files":files,
        "scope": scope
    }))?;
    output.reserve()?;
    // Mark even an interrupted write as a domain candidate before writing ordinary files.
    output.write_new(
        "_domain_candidate.json",
        &sidecars["_domain_candidate.json"],
    )?;
    if !base.is_empty() {
        output.write_base(&base)?;
    }
    if !overlay.is_empty() {
        output.write_overlay(&overlay)?;
    }
    for (name, bytes) in &sidecars {
        if *name != "_domain_candidate.json" {
            output.write_new(name, bytes)?;
        }
    }
    output.write_new("producer-receipt.json", &receipt)?;
    output.verify_namespace()?;
    Ok(format!(
        "ordinary {} domain candidate: {}; receipt sha256 {}",
        args.domain.label(),
        args.out.display(),
        sha256_hex(&receipt)
    ))
}

fn pretty_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn implementation_sha256() -> String {
    let mut bytes = Vec::new();
    // Content revision records the compiled implementation, not whichever checkout invokes it.
    for (name, source) in [
        (
            "ordinary_facts.rs",
            include_bytes!("ordinary_facts.rs").as_slice(),
        ),
        (
            "ordinary_facts/base_items.rs",
            include_bytes!("ordinary_facts/base_items.rs").as_slice(),
        ),
        (
            "ordinary_facts/skills.rs",
            include_bytes!("ordinary_facts/skills.rs").as_slice(),
        ),
        (
            "ordinary_facts/skills/validation.rs",
            include_bytes!("ordinary_facts/skills/validation.rs").as_slice(),
        ),
        (
            "ordinary_facts/skills/receipt.rs",
            include_bytes!("ordinary_facts/skills/receipt.rs").as_slice(),
        ),
        (
            "ordinary_facts/quality.rs",
            include_bytes!("ordinary_facts/quality.rs").as_slice(),
        ),
        (
            "ordinary_facts/quality/policy.rs",
            include_bytes!("ordinary_facts/quality/policy.rs").as_slice(),
        ),
        (
            "skills/quality.rs",
            include_bytes!("skills/quality.rs").as_slice(),
        ),
        (
            "ordinary_facts/stat_set_codec.rs",
            include_bytes!("ordinary_facts/stat_set_codec.rs").as_slice(),
        ),
        (
            "ordinary_facts/gem_effects.rs",
            include_bytes!("ordinary_facts/gem_effects.rs").as_slice(),
        ),
        (
            "ordinary_facts/gem_effects/policy.rs",
            include_bytes!("ordinary_facts/gem_effects/policy.rs").as_slice(),
        ),
        (
            "ordinary_facts/monster_resistances.rs",
            include_bytes!("ordinary_facts/monster_resistances.rs").as_slice(),
        ),
        ("monsters.rs", include_bytes!("monsters.rs").as_slice()),
        (
            "ordinary_facts/stat_set_codec/policy.rs",
            include_bytes!("ordinary_facts/stat_set_codec/policy.rs").as_slice(),
        ),
        ("skills/mod.rs", include_bytes!("skills/mod.rs").as_slice()),
        (
            "skills/gems.rs",
            include_bytes!("skills/gems.rs").as_slice(),
        ),
        (
            "skills/effects.rs",
            include_bytes!("skills/effects.rs").as_slice(),
        ),
        (
            "skills/levels.rs",
            include_bytes!("skills/levels.rs").as_slice(),
        ),
        (
            "skills/stat_sets.rs",
            include_bytes!("skills/stat_sets.rs").as_slice(),
        ),
        (
            "ordinary_facts/convert.rs",
            include_bytes!("ordinary_facts/convert.rs").as_slice(),
        ),
        (
            "native_facts/reader.rs",
            include_bytes!("native_facts/reader.rs").as_slice(),
        ),
        (
            "native_facts/trust.rs",
            include_bytes!("native_facts/trust.rs").as_slice(),
        ),
        (
            "native_facts/convert.rs",
            include_bytes!("native_facts/convert.rs").as_slice(),
        ),
        (
            "native_facts/output.rs",
            include_bytes!("native_facts/output.rs").as_slice(),
        ),
        (
            "native_facts/mod.rs",
            include_bytes!("native_facts/mod.rs").as_slice(),
        ),
        (
            "native_facts/core_skills.rs",
            include_bytes!("native_facts/core_skills.rs").as_slice(),
        ),
        (
            "native_facts/gems.rs",
            include_bytes!("native_facts/gems.rs").as_slice(),
        ),
        (
            "native_facts/items_actors.rs",
            include_bytes!("native_facts/items_actors.rs").as_slice(),
        ),
        ("main.rs", include_bytes!("main.rs").as_slice()),
        ("raw_source.rs", include_bytes!("raw_source.rs").as_slice()),
        (
            "catalog/mods.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/mods.rs").as_slice(),
        ),
        (
            "catalog/items.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/items.rs").as_slice(),
        ),
        (
            "catalog/skills.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/skills.rs").as_slice(),
        ),
        (
            "catalog/monster_resistances.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/monster_resistances.rs")
                .as_slice(),
        ),
        ("Cargo.toml", include_bytes!("../Cargo.toml").as_slice()),
        (
            "Cargo.lock",
            include_bytes!("../../../Cargo.lock").as_slice(),
        ),
        (
            "pypoe-sources.json",
            include_bytes!("../tests/fixtures/native-facts/pypoe-sources.json").as_slice(),
        ),
    ] {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(source);
        bytes.push(0);
    }
    sha256_hex(&bytes)
}
