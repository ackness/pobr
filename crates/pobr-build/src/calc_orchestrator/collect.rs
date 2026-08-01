//! collect — collecting character base / passive nodes / jewel radius expansion / keystones / items·gems.

use super::*;

/// Derives [`CharacterBase`] from class name + level (attributes come from the class's
/// starting values; tree/item attribute boosts go through the modifier pipeline, this
/// entry point only lands the inherent derivation). Returns `None` for an unknown class
/// (skips CharacterBase injection).
pub(crate) fn character_base(build: &Build, data: &BuildData) -> Option<CharacterBase> {
    let attrs = data.class_attributes(&build.character.class_name)?;
    Some(CharacterBase {
        level: build.character.level,
        strength: f64::from(attrs.strength),
        dexterity: f64::from(attrs.dexterity),
        intelligence: f64::from(attrs.intelligence),
    })
}

/// Resolves allocated passive nodes into node-attributed [`AllocatedNode`]s (via
/// [`collect_allocated_mods_for_class`], handling JewelSocket / Mastery gating +
/// isSwitchable variant selection by class/ascendancy; unknown nodes are skipped).
pub(crate) fn resolve_passive_nodes(build: &Build, data: &BuildData) -> Vec<AllocatedNode> {
    // isSwitchable variant context (matching PoB's curClassName / curAscendClassName,
    // PassiveSpec.lua:1251-1256): sourced from the Build XML header's class/ascendancy name.
    let class = ClassContext {
        class_name: &build.character.class_name,
        ascendancy_name: &build.character.ascendancy_name,
    };
    // Multi-version tree support (mirroring PoB2's TreeData/<v>): an old-league build
    // selects historical tree mods by `<Spec treeVersion>` (e.g. node 53853 "Backup
    // Plan" has two mod lines 50/50 in 0_3 vs three lines 20/40/40 in 0_5); falls back
    // to the default tree when there's no matching historical tree data.
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    collect_allocated_mods_for_class(&build.tree, nodes, class)
        .into_iter()
        .map(|node| {
            // Ascendancy nodes are determined by their PassiveNodeDef::ascendancy_id.
            let ascendancy = nodes
                .get(&node.node_id.0)
                .map(|def| def.ascendancy_id.is_some())
                .unwrap_or(false);
            AllocatedNode {
                node_id: node.node_id,
                ascendancy,
                modifier_texts: combine_wrapped_then_filter(node.modifier_texts, engine_ctx(data)),
            }
        })
        .collect()
}

/// Notables granted by anointing: scans every equipment/jewel mod line, parses out
/// `GrantedPassive` LIST entries (matching vendor's `Allocates <name>` enchant,
/// ModParser.lua:5809), matches them by name (ASCII case-insensitive — already
/// lowercase-normalized on the parse side) against **Notable** nodes (matching vendor's
/// `spec.tree.notableMap`, CalcSetup.lua:1322-1331 only looks up notables), and appends
/// them as [`AllocatedNode`]s. Already-allocated nodes are skipped (matching vendor's
/// `allocNodes[id]` idempotency semantics). A same-named notable (e.g. a switchable
/// variant) takes the smallest skill id (deterministic).
pub(crate) fn append_granted_passives(
    build: &Build,
    data: &BuildData,
    nodes: &mut Vec<AllocatedNode>,
) {
    let mut allocated: std::collections::HashSet<u32> = nodes.iter().map(|n| n.node_id.0).collect();
    for def in granted_passive_defs(build, data) {
        if !allocated.insert(def.skill) {
            continue; // Already allocated/already granted, idempotent.
        }
        nodes.push(AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(def.skill),
            ascendancy: def.ascendancy_id.is_some(),
            // Tree stats can also wrap across lines (same source data as combine_wrapped_then_filter).
            modifier_texts: combine_wrapped_then_filter(def.stats.clone(), engine_ctx(data)),
        });
    }
}

/// Bundles the data-driven parse rules from `data` into a parse context (injected calls
/// go through the new engine; an old data pack lacking rules falls back to the legacy
/// parser, value unchanged). Used by scattered passive ingest calls that don't go
/// through `CalculationSession` (which already injects rules) to consistently use the
/// same parse path as the primary flow.
pub(crate) fn engine_ctx(data: &BuildData) -> ParseCtx<'_> {
    match data.parser_rules.as_deref() {
        Some(rules) => ParseCtx::with_engine(rules),
        None => ParseCtx::none(),
    }
}

/// Parses every equipment/jewel mod line's `GrantedPassive` (`Allocates <name>`
/// enchant), matching them by name against Notable node definitions and returning them
/// deduplicated (shared parsing logic for [`append_granted_passives`] and
/// [`gem_property_bonuses`]; see the former for the semantics).
pub(crate) fn granted_passive_defs<'d>(
    build: &Build,
    data: &'d BuildData,
) -> Vec<&'d pobr_data::catalog::PassiveNodeDef> {
    use pobr_core::ModValue;

    // Collects granted names (all three equipment segments + jewel mods; a line that
    // fails to parse is silently skipped — matching skip-and-collect semantics).
    let item_texts = build.items.values().flat_map(|item| {
        item.implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
    });
    let jewel_texts = build.jewels.iter().flat_map(|jewel| {
        jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
    });
    let ctx = engine_ctx(data);
    let mut granted: Vec<String> = Vec::new();
    for text in item_texts.chain(jewel_texts) {
        let Ok(outcome) = ctx.parse(text) else {
            continue;
        };
        for m in outcome.mods {
            if m.name.as_str() == "GrantedPassive"
                && let ModValue::Text(name) = &m.value
            {
                granted.push(name.clone());
            }
        }
    }
    if granted.is_empty() {
        return Vec::new();
    }

    // Notable name (lowercase) → node (same name takes the smallest skill id, deterministic).
    let mut by_name: std::collections::BTreeMap<String, &pobr_data::catalog::PassiveNodeDef> =
        std::collections::BTreeMap::new();
    for def in data.passive_nodes.values() {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable {
            continue;
        }
        let Some(name) = &def.name else { continue };
        by_name
            .entry(name.to_ascii_lowercase())
            .and_modify(|existing| {
                if def.skill < existing.skill {
                    *existing = def;
                }
            })
            .or_insert(def);
    }

    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in granted {
        let Some(def) = by_name.get(name.to_ascii_lowercase().as_str()) else {
            continue; // Unknown name (outside the tree/a variant), safely skipped (under-counting is the safe failure mode).
        };
        if seen.insert(def.skill) {
            out.push(*def);
        }
    }
    out
}

/// "Wrap-merge" parsing for tree node mods (matching vendor's `PassiveTree.lua:445-462`:
/// when a single line fails to parse, retries by concatenating subsequent lines one at a
/// time; on success the merged-in lines are consumed, and if every attempt fails the
/// line is dropped as-is while subsequent lines continue to be parsed independently).
///
/// Multi-word long stats in tree data get wrapped across multiple lines (vendor
/// tree.lua's sd array / the cataloged JSON's `\n`, flattened by
/// `pobr_tree::split_lines`) — e.g. the Demolitionist ascendancy's "Gain 4% of Damage as
/// Extra Fire Damage for / every different Grenade fired in the past 8 seconds" is
/// **one** mod split across two lines. Only the tree path needs this merge (equipment
/// mods are already cataloged line by line).
pub(crate) fn combine_wrapped_then_filter(texts: Vec<String>, ctx: ParseCtx<'_>) -> Vec<String> {
    let parses = |t: &str| gate_parses(ctx, t);
    let mut out = Vec::new();
    let mut i = 0;
    while i < texts.len() {
        if parses(&texts[i]) {
            out.push(texts[i].clone());
            i += 1;
            continue;
        }
        // Retries by concatenating subsequent lines one at a time (matching vendor :448-462's comb loop).
        let mut combined: Option<(String, usize)> = None;
        for end in (i + 1)..texts.len() {
            let comb = texts[i..=end].join(" ");
            if parses(&comb) {
                combined = Some((comb, end));
                break;
            }
        }
        match combined {
            Some((comb, end)) => {
                out.push(comb);
                i = end + 1;
            }
            None => {
                // Diagnostic semantics match filter_parseable (visibility into structural drops).
                if pobr_core::dbg_env!("POBR_DBG_DROPPED").is_some() {
                    eprintln!("[POBR_DROP] {}", texts[i]);
                }
                i += 1;
            }
        }
    }
    out
}

/// Keystone name → that keystone's modifier list (a tree keystone node's stats parsed
/// via passive ingest). Injected by "You have \<Keystone\>"-type granting mods during
/// env_finalize's `merge_keystones` stage (equivalent to CalcPerform.lua:66-76).
///
/// **Excludes already-allocated keystones**: their mods are already injected with Tree
/// attribution by `add_passive_nodes`; a missing key in the map means merge silently
/// skips it — equivalent to PoB2's `env.keystonesAdded` deduplication across the
/// tree/mod dual sources.
pub(crate) fn keystone_mod_map(
    data: &BuildData,
    allocated: &[AllocatedNode],
) -> std::collections::BTreeMap<String, Vec<Modifier>> {
    let allocated_ids: std::collections::HashSet<u32> =
        allocated.iter().map(|n| n.node_id.0).collect();
    let mut map = std::collections::BTreeMap::new();
    for (id, def) in &data.passive_nodes {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Keystone || allocated_ids.contains(id) {
            continue;
        }
        let Some(name) = def.name.clone() else {
            continue;
        };
        let node = AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(*id),
            ascendancy: def.ascendancy_id.is_some(),
            modifier_texts: filter_parseable(def.stats.clone(), engine_ctx(data)),
        };
        // A keystone that fails to parse (hard error) / produces zero mods doesn't enter the map (merge silently skips it, safely under-counting).
        let Ok(ingest) = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(&node),
            engine_ctx(data),
        ) else {
            continue;
        };
        if !ingest.modifiers.is_empty() {
            map.insert(name, ingest.modifiers);
        }
    }
    map
}

/// Maps a `Radius:` tier text to a [`JewelRadius`]. Falls back to `Large` (PoB2's
/// default tree jewel radius) when unrecognized/missing, so the geometric approximation stays usable.
pub(crate) fn parse_jewel_radius(label: Option<&str>) -> JewelRadius {
    match label.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => JewelRadius::Small,
        Some("medium") => JewelRadius::Medium,
        Some("large") => JewelRadius::Large,
        Some("very large") => JewelRadius::VeryLarge,
        _ => JewelRadius::Large,
    }
}

/// The target node kind for a radius jewel's `also grant` line (the granted object is determined by its prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantTargetKind {
    Notable,
    /// `Small Passive Skills` = a normal (non-notable/keystone/socket/mastery) node.
    Small,
}

/// Parses a `<Kind> Passive Skills in Radius also grant <mod>` line → (target kind, granted mod text).
///
/// Only recognizes the `Notable` / `Small` prefixes; any other prefix (e.g. keystone
/// grants, no samples seen so far) returns None.
pub(crate) fn parse_grant_line(line: &str) -> Option<(GrantTargetKind, String)> {
    const MARKER: &str = "Passive Skills in Radius also grant";
    let idx = line.find(MARKER)?;
    let prefix = line[..idx].trim();
    let kind = match prefix.to_ascii_lowercase().as_str() {
        "notable" => GrantTargetKind::Notable,
        "small" => GrantTargetKind::Small,
        _ => return None,
    };
    let granted = line[idx + MARKER.len()..].trim();
    if granted.is_empty() {
        None
    } else {
        Some((kind, granted.to_string()))
    }
}

/// Determines an attribute-choice node (PoBR's equivalent of PoB2 tree.lua's
/// `isAttribute=true` nodes): its mod is the "+N to any [Attributes|Attribute]"
/// three-way-choice form. The catalog carries no isAttribute flag, so this is determined
/// from the node's mod text (matching the text form used by pobr-tree's attribute-choice rewrite).
pub(crate) fn is_attribute_node(def: &pobr_data::catalog::PassiveNodeDef) -> bool {
    def.stats.iter().any(|s| {
        let lower = s.to_ascii_lowercase();
        lower.contains(" to any ") && lower.contains("attribute")
    })
}

/// The geometric expansion result for one radius jewel: the list of **allocated**
/// Notable node ids within radius, and the small (normal, non-attribute) node count.
pub(crate) struct RadiusJewelExpansion<'a> {
    jewel: &'a RadiusJewel,
    /// Allocated Notable node ids within radius (includes attribute notables; the
    /// effect-scaling consumer filters further on its own).
    notable_nodes: Vec<u32>,
    small_count: usize,
}

/// Runs radius geometric expansion on every radius jewel (circle center = socket node
/// coordinates, tier = the `Radius:` line).
///
/// Candidates are filtered only from the **allocated** node set; a jewel with missing
/// socket coordinates or a failed geometry computation is skipped (nothing is invented).
/// Shared between [`radius_jewel_grant_texts`] (grant-mod expansion) and
/// [`radius_jewel_notable_effect_copies`] (notable effect scaling).
pub(crate) fn radius_jewel_expansions<'a>(
    build: &'a Build,
    data: &BuildData,
) -> Vec<RadiusJewelExpansion<'a>> {
    if build.radius_jewels.is_empty() {
        return Vec::new();
    }
    // The allocated node set (with kind). Coordinates come from data.passive_nodes
    // (x/y backfilled from tree data).
    //
    // Only uses allocated nodes from the **active weapon set**: even though PoB2 keeps
    // the non-active set's exclusive points in allocNodes, and radius jewel grants get
    // written into their modList too, **every** mod on those nodes (including jewel
    // grants) gets a `Condition: WeaponSet<N>` tag appended (CalcSetup.lua:222-223, the
    // node's own allocMode takes priority over the jewel-source branch at :224-227) — so
    // grants on non-active-set nodes have zero net effect. Confirmed by oracle Tabulate
    // (a gemling crit jewel): of 6 in-radius notables, only the 1 from the active set
    // contributes +7 (critModList has exactly one entry `7 @ Tree:32763`, CritChance
    // 8.55). PoBR's parse layer already strips non-active-set exclusive points, so using
    // `tree.allocated_nodes` directly here is equivalent.
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();

    // Position table: the socket itself + every allocated node (candidates are filtered only from the allocated set).
    let mut positions: std::collections::HashMap<u32, (f64, f64)> =
        std::collections::HashMap::new();
    for (&skill, def) in &data.passive_nodes {
        if let (Some(x), Some(y)) = (def.x, def.y)
            && allocated.contains(&skill)
        {
            positions.insert(skill, (x, y));
        }
    }

    let mut out: Vec<RadiusJewelExpansion<'a>> = Vec::new();
    for jewel in &build.radius_jewels {
        // The socket's coordinates must be available, or the circle center can't be
        // determined (skipped, nothing invented).
        let Some(socket_pos) =
            data.passive_nodes
                .get(&jewel.socket_node)
                .and_then(|d| match (d.x, d.y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                })
        else {
            continue;
        };

        let radius = parse_jewel_radius(jewel.radius_label.as_deref());

        // Merge the socket's coordinates into the position table (compute excludes the socket itself).
        let mut pos = positions.clone();
        pos.insert(jewel.socket_node, socket_pos);

        // The tier's effective radius is resolved from the injected jewel_radii data
        // (when there's no data, BuildData already falls back to Default, which is
        // value-for-value equal to the JSON, so output is unchanged).
        let effect = match compute_radius_jewel_effect_with_radii(
            jewel.socket_node,
            radius,
            &data.jewel_radii,
            &pos,
            Vec::new(),
        ) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Allocated nodes within radius, tallied by kind. Small excludes attribute-choice
        // nodes ("+5 to any Attribute" three-way-choice nodes): vendor's
        // `<Kind> Passive Skills in Radius also grant` handler requires
        // `node.type == "Normal" and not node.isAttribute` (PoB2 ModParser.lua:6855-6857;
        // the corresponding tree.lua node carries `isAttribute=true`).
        let mut notable_nodes: Vec<u32> = Vec::new();
        let mut small_count = 0usize;
        for &skill in &effect.affected_nodes {
            let Some(def) = data.passive_nodes.get(&skill) else {
                continue;
            };
            match def.kind {
                pobr_data::catalog::PassiveNodeKind::Notable => notable_nodes.push(skill),
                pobr_data::catalog::PassiveNodeKind::Normal if !is_attribute_node(def) => {
                    small_count += 1;
                }
                _ => {}
            }
        }
        notable_nodes.sort_unstable();
        out.push(RadiusJewelExpansion {
            jewel,
            notable_nodes,
            small_count,
        });
    }
    out
}

/// The numeric scaling semantics of vendor's `ModStore:ScaleAddMod` (ModStore.lua:45-80):
/// `m_modf(round(value * scale, 2))` — rounds to two decimal places first, then
/// **truncates** (toward zero, e.g. `30.5 → 30`, `14.76 → 14`).
pub(crate) fn vendor_scale_mod_value(value: f64, scale: f64) -> f64 {
    let rounded = (value * scale * 100.0).round() / 100.0;
    rounded.trunc()
}

/// Scales the **first** numeric token in a mod's text via [`vendor_scale_mod_value`] and
/// writes it back (e.g. `10% increased X` ×1.22 → `12% increased X`). Returns None when
/// there's no numeric token (flag-type mods aren't scaled, matching vendor's semantics —
/// a non-numeric mod is AddMod'd as-is).
pub(crate) fn scale_leading_number(text: &str, scale: f64) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let end = text[start..]
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let value: f64 = text[start..end].parse().ok()?;
    let scaled = vendor_scale_mod_value(value, scale);
    Some(format!("{}{}{}", &text[..start], scaled, &text[end..]))
}

/// Expands every radius jewel's `also grant` mod by radius geometry into global modifier text.
///
/// For each jewel: with the socket node's coordinates as the circle center, filters
/// **allocated** nodes by the `Radius:` tier, tallied by kind (notable /
/// small=normal); each `also grant` line is injected `count` times as grant mod text.
/// This replicates PoB2's accumulation effect of "each allocated node of the matching
/// kind within radius gets its own copy of the grant".
///
/// Notable effect scaling (a Time-Lost jewel's "N% increased Effect of Notable Passive
/// Skills in Radius"): vendor writes the grant mod into the node's modList and then
/// applies a whole-list `ScaleAddList` to Notable nodes (CalcSetup.lua:246-275),
/// equivalent to scaling the granted value by ×(1+inc/100) (truncated,
/// [`vendor_scale_mod_value`]). On overlapping radii, vendor's last write on the same
/// node overwrites to a single effect; PoBR approximates it as the granting jewel's own
/// effect (no overlap in the current corpus).
pub(crate) fn radius_jewel_grant_texts(build: &Build, data: &BuildData) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for exp in radius_jewel_expansions(build, data) {
        let notable_scale = 1.0 + f64::from(exp.jewel.notable_effect_inc) / 100.0;
        for line in &exp.jewel.grant_lines {
            let Some((kind, granted)) = parse_grant_line(line) else {
                continue;
            };
            let (count, text) = match kind {
                GrantTargetKind::Notable => {
                    let scaled = if exp.jewel.notable_effect_inc > 0 {
                        scale_leading_number(&granted, notable_scale).unwrap_or(granted)
                    } else {
                        granted
                    };
                    (exp.notable_nodes.len(), scaled)
                }
                GrantTargetKind::Small => (exp.small_count, granted),
            };
            for _ in 0..count {
                out.push(text.clone());
            }
        }
    }
    out
}

/// The scaled copy of a radius jewel's Notable effect against a **node's own mods**.
///
/// Vendor CalcSetup.lua:246-275: applies a whole-list `ScaleAddList ×(1+inc/100)`
/// (value using [`vendor_scale_mod_value`]'s truncation semantics) to every
/// "Notable and non-attribute and non-ascendancy" node's modList within radius. PoBR's
/// equivalent: the base copy is already injected at 1.0 (by add_passive_nodes), and here
/// a **numeric delta copy** is appended, `trunc(round(v×scale,2)) − v` (BASE/INC; MORE's
/// multiplicative scaling has no additive equivalent, and tree notables currently have
/// no MORE-type numeric mods, so skipped). On overlapping radii from multiple jewels,
/// the same node's last write overwrites to a single effect (matching vendor's
/// `localNotableIncEffect = mod.value` semantics).
pub(crate) fn radius_jewel_notable_effect_copies(
    build: &Build,
    data: &BuildData,
    passive_nodes: &[AllocatedNode],
) -> Result<Vec<Modifier>, BuildError> {
    let mut node_effect: std::collections::BTreeMap<u32, u32> = Default::default();
    for exp in radius_jewel_expansions(build, data) {
        if exp.jewel.notable_effect_inc == 0 {
            continue;
        }
        for &n in &exp.notable_nodes {
            node_effect.insert(n, exp.jewel.notable_effect_inc);
        }
    }
    if node_effect.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<Modifier> = Vec::new();
    for node in passive_nodes {
        let Some(&inc) = node_effect.get(&node.node_id.0) else {
            continue;
        };
        let Some(def) = data.passive_nodes.get(&node.node_id.0) else {
            continue;
        };
        // vendor's scaling condition (CalcSetup.lua:269): Notable and non-attribute and non-ascendancy.
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable
            || def.ascendancy_id.is_some()
            || is_attribute_node(def)
        {
            continue;
        }
        let scale = 1.0 + f64::from(inc) / 100.0;
        let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(node),
            engine_ctx(data),
        )
        .map_err(|e| BuildError::Parse(e.to_string()))?;
        out.extend(
            ingest
                .modifiers
                .into_iter()
                .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                }),
        );
    }
    Ok(out)
}

/// The single parseability-gate function (from B3, which killed the dual parser):
/// determined by matching vendor's `list and not extra` (PassiveTree.lua:447's
/// `if not list or extra` triggers wrap-merge retry) — i.e.
/// `status == Parsed && unparsed == None`. The ingest side goes through the same engine;
/// the gate and ingest share one parser, so any new mod gap must be fixed in the data
/// tables (mod_parser_rules.json / overlay). When the engine isn't injected (old data
/// pack), there's no parser: everything is Unsupported → everything dropped.
pub(crate) fn gate_parses(ctx: ParseCtx<'_>, t: &str) -> bool {
    // Diagnostic: POBR_GATE_DENY=substring1,substring2 forces matching lines to be dropped (for parity bisection).
    if let Some(deny) = pobr_core::dbg_env!("POBR_GATE_DENY")
        && deny.split(',').any(|p| !p.is_empty() && t.contains(p))
    {
        return false;
    }
    let pass = ctx.parse(t).is_ok_and(|o| {
        matches!(o.status, pobr_core::mod_parser::ParseStatus::Parsed) && o.unparsed.is_none()
    });
    // Diagnostic: POBR_DBG_GATE=1 dumps lines dropped by the gate.
    if !pass && pobr_core::dbg_env!("POBR_DBG_GATE").is_some() {
        eprintln!("[GATE_DROP] {t}");
    }
    pass
}

/// Keeps mod text that passes the [`gate_parses`] gate, dropping mods that fail to parse
/// / have leftover residue.
///
/// Some real mod text forms can't contribute a modifier (a legacy hard `ParseError` /
/// leftover unparsed residue from the engine); this filters at the entry point following
/// PoB's skip-and-collect semantics, making end-to-end calculation robust against real
/// data (dropped text doesn't error, nor is any value invented).
pub(crate) fn filter_parseable(texts: Vec<String>, ctx: ParseCtx<'_>) -> Vec<String> {
    texts
        .into_iter()
        .filter(|text| {
            let ok = gate_parses(ctx, text);
            // Diagnostic: POBR_DBG_DROPPED=1 dumps structurally-dropped mods (for parity investigation).
            if !ok && pobr_core::dbg_env!("POBR_DBG_DROPPED").is_some() {
                eprintln!("[POBR_DROP] {text}");
            }
            ok
        })
        .collect()
}

/// Filters an item's three mod segments (implicit / explicit / enchant) each into their
/// parseable subset, preserving which segment each mod belongs to (used by
/// [`CalculationSession::add_item`] to assign source-category attribution per segment).
pub(crate) fn filter_item_parseable(item: &Item, ctx: ParseCtx<'_>) -> Item {
    let mut filtered = item.clone();
    filtered.implicit_texts = filter_parseable(filtered.implicit_texts, ctx);
    filtered.modifier_texts = filter_parseable(filtered.modifier_texts, ctx);
    filtered.enchant_texts = filter_parseable(filtered.enchant_texts, ctx);
    filtered
}

/// Resolves enabled skill gem groups into classified (active/support) [`GemModSource`]s.
///
/// The current data pipeline hasn't yet exported a gem → mod stat set (see the module
/// doc), so `modifier_texts` is empty: gems only complete source-level attribution
/// registration (active is attributed to `SkillGem` / support to `SupportGem`, and a
/// support is linked as the parent source of the group's first active gem), contributing
/// no modifier of their own yet. An unknown gem id (not in [`BuildData`]'s gem table) is
/// treated as active (conservative, doesn't invent support semantics).
pub(crate) fn resolve_gems(build: &Build, data: &BuildData) -> Vec<GemModSource> {
    let mut gems = Vec::new();
    for group in build.enabled_socket_groups() {
        // The group's first active gem is the target a support supports (PoB Gem list order: active comes first).
        let active_gem_id = group
            .gem_ids
            .iter()
            .find(|id| data.is_support_gem(id) != Some(true))
            .cloned();

        for gem_id in &group.gem_ids {
            let is_support = data.is_support_gem(gem_id).unwrap_or(false);
            if is_support {
                let mut src = GemModSource::support(gem_id.clone(), Vec::<String>::new());
                if let Some(active) = &active_gem_id
                    && active != gem_id
                {
                    src = src.supporting(active.clone());
                }
                gems.push(src);
            } else {
                gems.push(GemModSource::active(gem_id.clone(), Vec::<String>::new()));
            }
        }
    }
    gems
}

/// Collects the mod text of every equipped item (in deterministic slot order). Used by the text-only path.
pub(crate) fn collect_item_texts(build: &Build) -> Vec<String> {
    let mut texts = Vec::new();
    for (_slot, item) in build.equipped_items() {
        texts.extend(item.enchant_texts.iter().cloned());
        texts.extend(item.implicit_texts.iter().cloned());
        texts.extend(item.modifier_texts.iter().cloned());
    }
    texts
}
