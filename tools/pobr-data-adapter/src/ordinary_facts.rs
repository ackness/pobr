//! Pinned physical facts to ordinary catalog domains, isolated from complete runtime packages.

mod actors;
mod base_items;
mod convert;
mod gem_effects;
mod monster_resistances;
mod monster_scaling;
mod quality;
mod skill_overrides;
mod skill_stat_map;
mod skills;
mod stat_set_codec;
mod totem_varieties;

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
    branch_codec: Option<BranchCodecArgs>,
}

struct BranchCodecArgs {
    path: PathBuf,
    sha256: String,
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
    TotemVarieties,
    MonsterScaling,
    SkillStatMap,
    SkillOverrides,
    Minions,
    Spectres,
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
            Self::TotemVarieties => &totem_varieties::TABLES,
            Self::MonsterScaling => &monster_scaling::TABLES,
            Self::SkillStatMap => &skill_stat_map::TABLES,
            Self::SkillOverrides => &skill_overrides::TABLES,
            Self::Minions | Self::Spectres => &actors::TABLES,
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
            Self::TotemVarieties => "ordinary-totem-varieties",
            Self::MonsterScaling => "ordinary-monster-scaling",
            Self::SkillStatMap => "ordinary-skill-stat-map",
            Self::SkillOverrides => "ordinary-skill-overrides",
            Self::Minions => "ordinary-minions",
            Self::Spectres => "ordinary-spectres",
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
            Self::TotemVarieties => "totem-varieties",
            Self::MonsterScaling => "monster-scaling",
            Self::SkillStatMap => "skill-stat-map",
            Self::SkillOverrides => "skill-overrides",
            Self::Minions => "minions",
            Self::Spectres => "spectres",
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
            "--branch-codec",
            "--branch-codec-sha256",
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
        Some("totem-varieties") => Domain::TotemVarieties,
        Some("monster-scaling") => Domain::MonsterScaling,
        Some("skill-stat-map") => Domain::SkillStatMap,
        Some("skill-overrides") => Domain::SkillOverrides,
        Some("minions") => Domain::Minions,
        Some("spectres") => Domain::Spectres,
        Some(value) => return Err(format!("unsupported ordinary domain: {value}")),
    };
    if domain != Domain::GemQuality
        && (values.contains_key("--quality-policy")
            || values.contains_key("--quality-policy-sha256"))
    {
        return Err("quality policy arguments require --ordinary-domain gem-quality".into());
    }
    if !matches!(
        domain,
        Domain::StatSetCodec
            | Domain::GemEffects
            | Domain::SkillStatMap
            | Domain::SkillOverrides
            | Domain::Minions
            | Domain::Spectres
    ) && [
        "--codec-source",
        "--codec-source-sha256",
        "--codec-policy",
        "--codec-policy-sha256",
    ]
    .iter()
    .any(|flag| values.contains_key(*flag))
    {
        return Err(
            "codec arguments require --ordinary-domain stat-set-codec, gem-effects, skill-stat-map, skill-overrides, minions or spectres".into(),
        );
    }
    if !matches!(domain, Domain::SkillStatMap | Domain::SkillOverrides)
        && ["--branch-codec", "--branch-codec-sha256"]
            .iter()
            .any(|flag| values.contains_key(*flag))
    {
        return Err(
            "branch codec arguments require --ordinary-domain skill-stat-map or skill-overrides"
                .into(),
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
        codec: if matches!(
            domain,
            Domain::StatSetCodec
                | Domain::GemEffects
                | Domain::SkillStatMap
                | Domain::SkillOverrides
                | Domain::Minions
                | Domain::Spectres
        ) {
            Some(CodecArgs {
                source: take("--codec-source")?.into(),
                source_sha256: take("--codec-source-sha256")?,
                policy: take("--codec-policy")?.into(),
                policy_sha256: take("--codec-policy-sha256")?,
            })
        } else {
            None
        },
        branch_codec: if matches!(domain, Domain::SkillStatMap | Domain::SkillOverrides) {
            Some(BranchCodecArgs {
                path: take("--branch-codec")?.into(),
                sha256: take("--branch-codec-sha256")?,
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
    let mut branch_codec_bytes = None;
    let mut legacy_skill_overrides_bytes = None;
    let mut legacy_actors_bytes = None;
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
        Domain::TotemVarieties => {
            let converted = totem_varieties::convert(&input)?;
            overlay = converted.overlay;
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::totem_varieties::{TotemVarietiesDef,TotemVarietyDef}",
                vec![
                    "Only the selected six target F tables were read. All matched granted effects use the original totem DTO and unchanged raw percentage units.",
                    "ActiveSkill is a physical FK. SkillTotemId and SkillTotemsKey match by opaque value in their declared SkillTotems namespace; no external namespace content or reverse string link is inferred.",
                    "Only unique skin-zero matches are applied. Tagged SummonsTotem tuples without a match remain explicit unresolved records, without inferred varieties/numbers or effectless claims; tags do not restrict matched output.",
                    "The existing version-overlay-first loader and BuildData path are unchanged. Other skins, minion/totem catalogs and level semantics, complete target package and ordinary BD/Web acceptance remain separate work.",
                    "Acquisition status/evidence is copied unchanged. Fingerprints and conversion do not upgrade draft acquisition, prove full runtime completeness or adopt data.",
                    "No runtime manifest, auxiliary policy, shared/old numeric input, vendor/Lua, download, CURRENT update or consumer model change.",
                ],
            )
        }
        Domain::MonsterScaling => {
            let converted = monster_scaling::convert(&input)?;
            (
                converted.base,
                converted.receipt,
                "pobr_data::catalog::monster_scaling::MonsterScalingTable",
                vec![
                    "Only target F DefaultMonsterStats was read. All nine arrays are constructed from its complete unique DisplayLevel set; physical rows are provenance, not level indices.",
                    "Seven exact i32 source fields are checked into u32. Damage/MinionDamage retain signed finite f32-range input through the existing nearest-0.01, ties-away-from-zero convention; raw and normalized values are recorded.",
                    "No default array, old numeric file, vendor/Lua, auxiliary policy or hiddenDamageFixup calculation supplies values. Existing DTO, lookups, clamps and consumers are unchanged.",
                    "Unconsumed fields, minionGemLevel-to-monsterLevel mapping, full minion catalogs, remaining assets and ordinary target BD/Web/accuracy acceptance remain separate; unapplied does not mean effectless.",
                    "Acquisition remains unchanged draft evidence; no complete runtime manifest, source approval, CURRENT update or adoption.",
                ],
            )
        }
        Domain::SkillStatMap => {
            let codec = args
                .codec
                .as_ref()
                .ok_or("missing map source/policy arguments")?;
            let branch = args
                .branch_codec
                .as_ref()
                .ok_or("missing branch codec arguments")?;
            let converted = skill_stat_map::convert(&input, codec, branch)?;
            for parent in &converted.protected_parents {
                output.protect(parent)?;
            }
            overlay = converted.overlay;
            codec_policy_bytes = Some(converted.policy_bytes);
            branch_codec_bytes = Some(converted.branch_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::{stat_map::SkillStatMapDef,StatSetLabelsDef}",
                vec![
                    "The exact fixed community map and branch codec bytes remain legacy interpretation inputs with original metadata; no target numerical or gameplay-law acquisition is claimed.",
                    "Only target Stats/GrantedEffects/GrantedEffectStatSets are read. Every map occurrence records present/absent stat identity; each scope binds through the actual pinned export-index codec to raw stable-set ownership or an exact unbound policy exception.",
                    "All constructors, transforms, scalars, tags, unsupported and unknown contents are preserved. Ordered references are not deduplicated; export indices are never inferred from raw order or labels.",
                    "Policy exceptions must be consumed exactly. Nothing is filtered, retargeted, disabled, defaulted or declared effectless; the engine and diagnostics are unchanged.",
                    "Acquisition remains draft; full target rule applicability, remaining assets, complete runtime and ordinary BD/Web/accuracy acceptance are separate. No Lua/vendor execution, implicit candidate input, CURRENT update or adoption.",
                ],
            )
        }
        Domain::SkillOverrides => {
            let codec = args
                .codec
                .as_ref()
                .ok_or("missing overrides source/policy arguments")?;
            let branch = args
                .branch_codec
                .as_ref()
                .ok_or("missing branch codec arguments")?;
            let converted = skill_overrides::convert(&input, codec, branch)?;
            for parent in &converted.protected_parents {
                output.protect(parent)?;
            }
            overlay = converted.overlay;
            codec_policy_bytes = Some(converted.policy_bytes);
            branch_codec_bytes = Some(converted.branch_bytes);
            legacy_skill_overrides_bytes = Some(converted.source_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::skill_overrides::{SkillOverridesDef,SkillOverrideEntry}",
                vec![
                    "Only the selected five target F tables were read. Raw-owned base multipliers use all actual positive primary stat-set levels and the existing 1 + raw/10000 expression, never old numeric values or defaults.",
                    "Legacy constants retain constant semantics only while target source is constant; target changes and new levels are emitted. GPL records pairing evidence without filtering or creating source/runtime levels.",
                    "Fixed virtual and prepared community entries retain authored JSON and unknown fields. Actual branch codec membership and exact source-row/key exceptions are explicit; no new primary fallback, retargeting, law interpretation or diagnostic clearing.",
                    "The original legacy source, branch codec and policy bytes are archived with their own identities; target numeric provenance remains separate. Existing DTO, loaders, merges and core behavior are unchanged.",
                    "Acquisition remains draft; full target rule applicability, remaining assets, complete runtime and ordinary BD/Web/accuracy acceptance are separate. No Lua/vendor execution, implicit input, CURRENT update or adoption.",
                ],
            )
        }
        Domain::Minions | Domain::Spectres => {
            let codec = args
                .codec
                .as_ref()
                .ok_or("missing actor source/policy arguments")?;
            let catalog = if args.domain == Domain::Minions {
                actors::Catalog::Minions
            } else {
                actors::Catalog::Spectres
            };
            let converted = actors::convert(&input, codec, catalog)?;
            for parent in &converted.protected_parents {
                output.protect(parent)?;
            }
            overlay = converted.overlay;
            codec_policy_bytes = Some(converted.policy_bytes);
            legacy_actors_bytes = Some(converted.source_bytes);
            (
                BTreeMap::new(),
                converted.receipt,
                "pobr_data::catalog::actors::{MinionsDef,MinionEntryDef}",
                vec![
                    "Only target MonsterVarieties/MonsterTypes/MonsterResistances/Tags/GrantedEffects are read. The declared 22 actor fields use target facts through a fixed archived recipe, without legacy numeric/default fallback.",
                    "Curated catalog membership/aliases and every non-owned field remain fixed community interpretation with original metadata. Ordered tags, skill occurrences and resistance refs are preserved; no automatic creature admission, AI or new law is inferred.",
                    "This catalog uses the archived first-resistance-reference/first-array-element projection. Further occurrences remain explicit unapplied evidence. The independent normal-resistance sum/injection domain and consumers are unchanged.",
                    "Crit uses floor-based rounding with negative ties towards positive infinity; reservation preserves the original pow/sqrt/floor order and floating tails. Equality to old numeric text is never required.",
                    "The original source and policy bytes are archived; metadata separates target fields, projection reference and legacy compatibility. No implicit vendor/Lua/evidence/candidate input, CURRENT update, source promotion or adoption.",
                    "Source remains draft and the output incomplete. Remaining links/minion-level/tree/config assets, full target package, ordinary BD/Web/accuracy and minion-model/law certification remain separate.",
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
    if let Some(bytes) = branch_codec_bytes {
        sidecars.insert("branch-codec.json", bytes);
    }
    if let Some(bytes) = legacy_skill_overrides_bytes {
        sidecars.insert("legacy-skill-overrides.json", bytes);
    }
    if let Some(bytes) = legacy_actors_bytes {
        sidecars.insert("legacy-actors.json", bytes);
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
            "ordinary_facts/actors.rs",
            include_bytes!("ordinary_facts/actors.rs").as_slice(),
        ),
        (
            "ordinary_facts/actors/policy.rs",
            include_bytes!("ordinary_facts/actors/policy.rs").as_slice(),
        ),
        (
            "ordinary_facts/actors/projection.rs",
            include_bytes!("ordinary_facts/actors/projection.rs").as_slice(),
        ),
        (
            "catalog/actors.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/actors.rs").as_slice(),
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
            "ordinary_facts/totem_varieties.rs",
            include_bytes!("ordinary_facts/totem_varieties.rs").as_slice(),
        ),
        (
            "ordinary_facts/monster_scaling.rs",
            include_bytes!("ordinary_facts/monster_scaling.rs").as_slice(),
        ),
        (
            "catalog/monster_scaling.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/monster_scaling.rs").as_slice(),
        ),
        (
            "ordinary_facts/skill_overrides.rs",
            include_bytes!("ordinary_facts/skill_overrides.rs").as_slice(),
        ),
        (
            "ordinary_facts/skill_overrides/policy.rs",
            include_bytes!("ordinary_facts/skill_overrides/policy.rs").as_slice(),
        ),
        (
            "ordinary_facts/skill_overrides/source.rs",
            include_bytes!("ordinary_facts/skill_overrides/source.rs").as_slice(),
        ),
        (
            "catalog/skill_overrides.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/skill_overrides.rs").as_slice(),
        ),
        (
            "ordinary_facts/skill_stat_map.rs",
            include_bytes!("ordinary_facts/skill_stat_map.rs").as_slice(),
        ),
        (
            "ordinary_facts/skill_stat_map/policy.rs",
            include_bytes!("ordinary_facts/skill_stat_map/policy.rs").as_slice(),
        ),
        (
            "catalog/stat_map.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/stat_map.rs").as_slice(),
        ),
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
            "catalog/totem_varieties.rs",
            include_bytes!("../../../crates/pobr-data/src/catalog/totem_varieties.rs").as_slice(),
        ),
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
