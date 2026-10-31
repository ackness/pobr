//! collect — collecting character base / passive nodes / jewel radius expansion / keystones / items·gems.

use pobr_core::Modifier;
use pobr_core::mod_parser::ParseCtx;
use pobr_core::skill_source::GemModSource;
use pobr_data::item::Item;
use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_tree::{ClassContext, JewelRadius};

use super::sources::{GEM_PROPERTY_READER, LineFate, LineHolder, SourceWriter, item_sections_mut};
use crate::build::Build;
use crate::build::RadiusJewel;
use crate::build_data::BuildData;
use crate::error::BuildError;
use pobr_core::CharacterBase;
use pobr_core::passive::{
    AllocatedNode, NativePassiveDiagnostic, NativePassiveInput, select_native_passive,
};
use pobr_core::rules::passive_stats::NativePassiveRejection;
use pobr_tree::{collect_allocated_mods_for_class, compute_radius_jewel_effect_with_radii};

use super::skill::resolve::{parse_gem_property_bonus, small_passive_effect_inc};
pub(crate) use pobr_core::passive::{GrantTargetKind, parse_grant_line};

/// The increase, in percent, a jewel's socket gives everything the jewel
/// grants. One of the tree's own jewel sockets takes The Adorned's increase
/// for the jewel (`adorned`, "Jewel Socket Passive Skills containing
/// Corrupted Rare Jewels"). A passive skill that holds a socket is not a
/// Jewel Socket Passive Skill: it takes its own "100% increased Effect of
/// bonuses gained from Socketed Jewel" (`phylactery_jewel_socket_effect_+%`,
/// the Lich's Crystalline Phylactery). A socket an item grants and an
/// equipment socket take neither.
pub(crate) fn jewel_socket_increase(
    socket: crate::build::JewelSocket,
    build: &Build,
    data: &BuildData,
    adorned: impl FnOnce() -> Option<f64>,
) -> Option<f64> {
    use crate::build::JewelSocket;
    use pobr_data::catalog::PassiveNodeKind;

    let node = match socket {
        JewelSocket::Tree => return adorned(),
        JewelSocket::Node(node) => node,
        JewelSocket::Granted | JewelSocket::Equipment => return None,
    };
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    match nodes.get(&node) {
        Some(def) if def.kind != PassiveNodeKind::JewelSocket => {
            let inc: f64 = def
                .stats
                .iter()
                .filter_map(|line| socketed_jewel_effect(line))
                .sum();
            (inc != 0.0).then_some(inc)
        }
        // A jewel socket, or a node the selected tree does not have.
        _ => adorned(),
    }
}

/// The reader of a passive skill's lines about the socket it holds.
pub(crate) const SOCKETED_JEWEL_READER: &str =
    "the jewel in the passive's socket (collect::jewel_socket_increase)";

/// Whether a passive's line is about the socket it holds: "Can Socket a
/// non-Unique Basic Jewel into the Phylactery", or the socketed jewel's effect.
pub(crate) fn is_socket_line(line: &str) -> bool {
    socketed_jewel_effect(line).is_some()
        || pobr_core::skill_env::clean_grant_text(line).starts_with("can socket ")
}

/// "100% increased Effect of bonuses gained from Socketed Jewel": 100.
pub(crate) fn socketed_jewel_effect(line: &str) -> Option<f64> {
    // Lower case, without the tree's markup.
    let line = pobr_core::skill_env::clean_grant_text(line);
    let (number, rest) = line.split_once('%')?;
    let value: f64 = number.trim().parse().ok()?;
    match rest.trim() {
        "increased effect of bonuses gained from socketed jewel" => Some(value),
        "reduced effect of bonuses gained from socketed jewel" => Some(-value),
        _ => None,
    }
}

/// Resolve item-granted sockets using the selected tree's stable IDs and names.
/// Numeric node IDs and the number of available sockets belong to the data pack.
pub(crate) fn resolve_granted_socket_jewels(build: &Build, data: &BuildData) -> Option<Build> {
    use pobr_data::catalog::PassiveNodeKind;

    if build.granted_socket_jewels.is_empty() {
        return None;
    }
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let mut sinister: Vec<_> = nodes
        .values()
        .filter(|node| node.kind == PassiveNodeKind::JewelSocket)
        .filter_map(|node| {
            // GGG IDs can have disambiguating trailing underscores; vendor uses
            // the corresponding voices_jewel_slot<N> aliases in the same order.
            let ordinal = node
                .id
                .strip_prefix("voices_jewel_slot")?
                .trim_end_matches('_')
                .parse::<usize>()
                .ok()?;
            Some((ordinal, node.skill))
        })
        .collect();
    sinister.sort_unstable();
    let count = build
        .jewel_items()
        .flat_map(|item| item.implicit_texts.iter().chain(&item.modifier_texts))
        .filter_map(|text| {
            let (number, suffix) = text.trim().strip_prefix("Allocates ")?.split_once(' ')?;
            matches!(
                suffix.trim().to_ascii_lowercase().as_str(),
                "sinister jewel socket" | "sinister jewel sockets"
            )
            .then(|| number.parse::<usize>().ok())
            .flatten()
        })
        .fold(0usize, usize::saturating_add);
    let mut granted: std::collections::HashSet<u32> = sinister
        .into_iter()
        .take(count)
        .map(|(_, skill)| skill)
        .collect();
    // A jewel allocates a socket the same way an item does: a Megalomaniac's
    // "Allocates Zarokh's Gift" opens that Sinister socket.
    for text in build
        .items
        .values()
        .chain(build.limbs.values())
        .chain(build.jewel_items())
        .flat_map(|item| {
            item.implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
        })
    {
        if let Some(name) = text.trim().strip_prefix("Allocates ")
            && let Some(node) = nodes
                .values()
                .filter(|node| node.kind == PassiveNodeKind::JewelSocket)
                .filter(|node| {
                    node.name
                        .as_ref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name.trim()))
                })
                .min_by_key(|node| node.skill)
        {
            granted.insert(node.skill);
        }
    }
    let mut resolved = build.clone();
    for (node, item, radius) in &build.granted_socket_jewels {
        if granted.contains(node) {
            resolved
                .jewels
                .push((crate::build::JewelSocket::Granted, item.clone()));
            resolved
                .radius_jewels
                .extend(radius.iter().cloned().map(|mut radius| {
                    radius.socket = crate::build::JewelSocket::Granted;
                    radius
                }));
        }
    }
    // Unallocated records can belong to the other weapon set. Keep them
    // available for that set's preparation; only active grants were consumed.
    resolved
        .granted_socket_jewels
        .retain(|(node, _, _)| !granted.contains(node));
    Some(resolved)
}

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
/// A line of an allocated node that does not parse goes to `unsupported` with
/// its node: the node is allocated, so the line was meant to count.
pub(crate) fn resolve_passive_nodes(
    build: &Build,
    data: &BuildData,
    unsupported: &mut Vec<(u32, String)>,
    native_diagnostics: &mut Vec<NativePassiveDiagnostic>,
) -> Vec<AllocatedNode> {
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
    let transformations = crate::jewel_tree::passive_jewel_state(build, data);
    let mut collected: std::collections::HashMap<_, _> =
        collect_allocated_mods_for_class(&build.tree, nodes, class)
            .into_iter()
            .map(|node| (node.node_id, node.modifier_texts))
            .collect();
    build
        .tree
        .allocated_nodes
        .iter()
        .filter_map(|node_id| {
            let mut modifier_texts = collected.remove(node_id).unwrap_or_default();
            if let Some(change) = transformations.nodes.get(&node_id.0) {
                if change.replace {
                    modifier_texts = change.stats.clone();
                } else {
                    modifier_texts.extend(change.stats.iter().cloned());
                }
            }
            // Gated sockets/masteries still contribute nothing. An ordinary
            // native node whose text was removed must retain its rejection.
            if modifier_texts.is_empty()
                && !nodes.get(&node_id.0).is_some_and(|def| {
                    def.native_stats.is_some()
                        && !matches!(
                            def.kind,
                            pobr_data::catalog::PassiveNodeKind::JewelSocket
                                | pobr_data::catalog::PassiveNodeKind::Mastery
                        )
                })
            {
                return None;
            }
            // Ascendancy nodes are determined by their PassiveNodeDef::ascendancy_id.
            let ascendancy = nodes
                .get(&node_id.0)
                .map(|def| def.ascendancy_id.is_some())
                .unwrap_or(false);
            let native = nodes.get(&node_id.0).and_then(|def| {
                let packet = def.native_stats.as_ref()?;
                let ineligible = if data.native_data_version.as_deref()
                    != Some(packet.data_version.as_str())
                    || packet.tree_version != build.tree_version
                    || !std::ptr::eq(nodes, &data.passive_nodes)
                {
                    Some(NativePassiveRejection::VersionMismatch)
                } else if transformations.nodes.contains_key(&node_id.0)
                    || transformations.conquered.contains(&node_id.0)
                    || !def.variants.is_empty()
                    || build.tree.attribute_overrides.contains_key(node_id)
                    || build.tree.mastery_effects.contains_key(node_id)
                {
                    Some(NativePassiveRejection::TransformedNode)
                } else if def.kind != pobr_data::catalog::PassiveNodeKind::Normal
                    || def.ascendancy_id.is_some()
                    || def.unlock_constraint.is_some()
                    || transformations
                        .class_starts
                        .values()
                        .any(|id| *id == node_id.0)
                {
                    Some(NativePassiveRejection::UnsupportedContext)
                } else if def.id != packet.node_slug || def.stats != packet.modifier_texts {
                    Some(NativePassiveRejection::BindingMismatch)
                } else {
                    None
                };
                Some(NativePassiveInput {
                    packet: packet.clone(),
                    ineligible,
                })
            });
            let mut node = AllocatedNode {
                node_id: *node_id,
                ascendancy,
                modifier_texts,
                native,
            };
            // Select before the parser-only gate: a complete native node needs no
            // parser catalog. A rejected packet hands ALL original text to the
            // existing wrap/filter path and cannot be revived by its parsed subset.
            match select_native_passive(&node) {
                Some(Ok(_)) => return Some(node),
                Some(Err(issue)) => native_diagnostics.push(issue),
                None => {}
            }
            node.native = None;
            let (texts, dropped) = combine_wrapped(node.modifier_texts, engine_ctx(data));
            node.modifier_texts = texts;
            unsupported.extend(dropped.into_iter().map(|line| (node_id.0, line)));
            Some(node)
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
    unsupported: &mut Vec<(u32, String)>,
) {
    let mut allocated: std::collections::HashSet<u32> = nodes.iter().map(|n| n.node_id.0).collect();
    for def in granted_passive_defs(build, data) {
        if !allocated.insert(def.skill) {
            continue; // Already allocated/already granted, idempotent.
        }
        // Tree stats can also wrap across lines (same source data as combine_wrapped_then_filter).
        let (modifier_texts, dropped) = combine_wrapped(def.stats.clone(), engine_ctx(data));
        unsupported.extend(dropped.into_iter().map(|line| (def.skill, line)));
        nodes.push(AllocatedNode {
            native: None,
            node_id: pobr_data::passive_tree::NodeId(def.skill),
            ascendancy: def.ascendancy_id.is_some(),
            modifier_texts,
        });
    }
}

/// Bundles the data-driven parse rules from `data` into a parse context (injected calls
/// go through the engine; a data pack lacking rules leaves text unsupported).
/// Used by scattered passive ingest calls that don't go
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
/// deduplicated (shared parsing logic for [`append_granted_passives`],
/// [`gem_property_bonuses`] and [`radius_jewel_expansions`]; see the first for
/// the semantics).
pub(crate) fn granted_passive_defs<'d>(
    build: &Build,
    data: &'d BuildData,
) -> Vec<&'d pobr_data::catalog::PassiveNodeDef> {
    use pobr_core::ModValue;

    // Collects granted names (all three segments of equipment and limbs + jewel
    // mods; a line that fails to parse is silently skipped — matching
    // skip-and-collect semantics).
    let item_texts = build
        .items
        .values()
        .chain(build.limbs.values())
        .flat_map(|item| {
            item.implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
        });
    let jewel_texts = build.jewel_items().flat_map(|jewel| {
        jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
    });
    let ctx = engine_ctx(data);
    let mut granted: Vec<String> = Vec::new();
    for text in item_texts.chain(jewel_texts) {
        // Only the `allocates (.+)` rules write `GrantedPassive`, so a line without
        // the word cannot grant one: skip parsing (and cloning the memoized outcome).
        if !pobr_core::skill_env::contains_ascii_ci(text, "allocates") {
            continue;
        }
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

    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in granted {
        let def = data.notable_named(build.tree_version.as_deref(), &name);
        let Some(def) = def else {
            continue; // Unknown name (outside the tree/a variant), safely skipped (under-counting is the safe failure mode).
        };
        if seen.insert(def.skill) {
            out.push(def);
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
/// **one** mod split across two lines. Equipment reuses the same merge
/// ([`filter_item_parseable`]: Berek's rings' "Wind Skills ... count / as being
/// boosted by ... Ground" wraps the same way in the item text).
pub(crate) fn combine_wrapped_then_filter(texts: Vec<String>, ctx: ParseCtx<'_>) -> Vec<String> {
    combine_wrapped(texts, ctx).0
}

/// [`combine_wrapped_then_filter`] with the lines it dropped: `(parsed, dropped)`.
pub(crate) fn combine_wrapped(texts: Vec<String>, ctx: ParseCtx<'_>) -> (Vec<String>, Vec<String>) {
    let parses = |t: &str| gate_parses(ctx, t);
    let mut out = Vec::new();
    let mut dropped = Vec::new();
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
                // POBR_DBG_DROPPED=1 prints each line as it is dropped.
                if pobr_core::dbg_env!("POBR_DBG_DROPPED").is_some() {
                    eprintln!("[POBR_DROP] {}", texts[i]);
                }
                dropped.push(texts[i].clone());
                i += 1;
            }
        }
    }
    (out, dropped)
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
    build: &Build,
    data: &BuildData,
    allocated: &[AllocatedNode],
) -> std::collections::BTreeMap<String, Vec<Modifier>> {
    let allocated_ids: std::collections::HashSet<u32> =
        allocated.iter().map(|n| n.node_id.0).collect();
    let mut map = std::collections::BTreeMap::new();
    for (id, def) in data.passive_nodes_for(build.tree_version.as_deref()) {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Keystone || allocated_ids.contains(id) {
            continue;
        }
        let Some(name) = def.name.clone() else {
            continue;
        };
        let node = AllocatedNode {
            native: None,
            node_id: pobr_data::passive_tree::NodeId(*id),
            ascendancy: def.ascendancy_id.is_some(),
            // The same line handling as an allocated keystone, so a granted
            // keystone means what the allocated one does.
            modifier_texts: combine_wrapped_then_filter(def.stats.clone(), engine_ctx(data)),
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

/// Determines an attribute-choice node (PoBR's equivalent of PoB2 tree.lua's
/// `isAttribute=true` nodes): its mod is the "+N to any [Attributes|Attribute]"
/// three-way-choice form. The catalog carries no isAttribute flag, so this is determined
/// from the node's mod text (matching the text form used by pobr-tree's attribute-choice rewrite).
pub(crate) fn is_attribute_node(def: &pobr_data::catalog::PassiveNodeDef) -> bool {
    pobr_core::skill_env::is_attribute_node(def)
}

/// The geometric expansion result for one radius jewel: the list of **allocated**
/// Notable node ids within radius, and the small (normal, non-attribute) node count.
pub(crate) struct RadiusJewelExpansion<'a> {
    jewel: &'a RadiusJewel,
    /// Allocated Notable node ids within radius (includes attribute notables; the
    /// effect-scaling consumer filters further on its own).
    notable_nodes: Vec<u32>,
    small_nodes: Vec<u32>,
}

/// Where a radius jewel reaches: its socket's position and its radius. `None` when the
/// socket has no coordinates in the selected tree.
fn radius_circle(
    jewel: &RadiusJewel,
    build: &Build,
    data: &BuildData,
) -> Option<((f64, f64), JewelRadius)> {
    let socket = data
        .passive_nodes_for(build.tree_version.as_deref())
        .get(&jewel.socket_node)?;
    let position = (socket.x?, socket.y?);
    let radius = JewelRadius::Custom(
        parse_jewel_radius(jewel.radius_label.as_deref())
            .units_for_tree(&data.jewel_radii, build.tree_version.as_deref()),
    );
    Some((position, radius))
}

/// For each of `nodes` that a radius jewel of the build reaches, the places (in
/// `build.radius_jewels`) of the jewels whose circle holds it. A radius jewel treats
/// a node by whether the circle holds it and by nothing else about where it is, so two
/// nodes with the same definition and the same list are treated alike. Computed with
/// the geometry [`radius_jewel_expansions`] uses.
pub fn radius_jewel_membership(
    build: &Build,
    data: &BuildData,
    nodes: &[u32],
) -> std::collections::BTreeMap<u32, Vec<usize>> {
    let tree_nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let mut positions: std::collections::HashMap<u32, (f64, f64)> = nodes
        .iter()
        .filter_map(|skill| {
            let def = tree_nodes.get(skill)?;
            Some((*skill, (def.x?, def.y?)))
        })
        .collect();
    let mut membership: std::collections::BTreeMap<u32, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (place, jewel) in build.radius_jewels.iter().enumerate() {
        let Some((socket_pos, radius)) = radius_circle(jewel, build, data) else {
            continue;
        };
        // The socket is the centre, and is never one of the nodes it reaches.
        let displaced = positions.insert(jewel.socket_node, socket_pos);
        if let Ok(effect) = compute_radius_jewel_effect_with_radii(
            jewel.socket_node,
            radius,
            &data.jewel_radii,
            &positions,
            Vec::new(),
        ) {
            for skill in effect.affected_nodes {
                membership.entry(skill).or_default().push(place);
            }
        }
        match displaced {
            Some(position) => positions.insert(jewel.socket_node, position),
            None => positions.remove(&jewel.socket_node),
        };
    }
    membership
}

/// Runs radius geometric expansion on every radius jewel (circle center = socket node
/// coordinates, tier = the `Radius:` line).
///
/// Candidates are filtered only from the **allocated** node set (the tree's, and the
/// Notables items allocate); a jewel with missing socket coordinates or a failed
/// geometry computation is skipped (nothing is invented).
/// Shared between [`radius_jewel_grant_modifiers`] (grant-mod expansion) and
/// [`passive_effect_copies`] (notable effect scaling).
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
    let tree_nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let mut allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();
    // A Notable an item allocates ("Allocates <Notable>": an anointment, a
    // Megalomaniac) shows as allocated on the passive tree, so a jewel's
    // radius sees it like one allocated with a passive point.
    for def in granted_passive_defs(build, data) {
        allocated.insert(def.skill);
    }

    // Position table: the socket itself + every allocated node (candidates are filtered only from the allocated set).
    let mut positions: std::collections::HashMap<u32, (f64, f64)> =
        std::collections::HashMap::new();
    // A hundred or so allocated points, looked up, rather than every point of the tree.
    for &skill in &allocated {
        if let Some(def) = tree_nodes.get(&skill)
            && let (Some(x), Some(y)) = (def.x, def.y)
        {
            positions.insert(skill, (x, y));
        }
    }

    let mut out: Vec<RadiusJewelExpansion<'a>> = Vec::new();
    for jewel in &build.radius_jewels {
        // The socket's coordinates must be available, or the circle center can't be
        // determined (skipped, nothing invented).
        let Some((socket_pos, radius)) = radius_circle(jewel, build, data) else {
            continue;
        };

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
        let mut small_nodes = Vec::new();
        for &skill in &effect.affected_nodes {
            let Some(def) = tree_nodes.get(&skill) else {
                continue;
            };
            if def.ascendancy_id.is_some() {
                continue;
            }
            match def.kind {
                pobr_data::catalog::PassiveNodeKind::Notable => notable_nodes.push(skill),
                pobr_data::catalog::PassiveNodeKind::Normal if !is_attribute_node(def) => {
                    small_nodes.push(skill);
                }
                _ => {}
            }
        }
        notable_nodes.sort_unstable();
        small_nodes.sort_unstable();
        out.push(RadiusJewelExpansion {
            jewel,
            notable_nodes,
            small_nodes,
        });
    }
    out
}

/// ScaleAddMod's default integer branch: round to two decimal places, then
/// truncate toward zero. Scaling that needs the precision exception table uses
/// the core ScaleAddMod primitive instead.
pub(crate) fn vendor_scale_mod_value(value: f64, scale: f64) -> f64 {
    pobr_core::skill_env::vendor_scale_mod_value(value, scale)
}

/// Parse each radius grant once and scale its modifiers for each allocated node.
/// Structured scaling preserves multiple values and the data pack's precision
/// rules; rewriting only the first number in the source text loses both.
pub(crate) fn radius_jewel_grant_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    super::selection_memo::radius_jewel_grants(build, data, || {
        compute_radius_jewel_grant_modifiers(build, data)
    })
}

fn compute_radius_jewel_grant_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let expansions = radius_jewel_expansions(build, data);
    if expansions.is_empty() {
        return mods;
    }
    let effects = radius_node_effects(&expansions);
    let global_small = small_passive_effect_inc(build, data);
    let ctx = engine_ctx(data);
    let jewel_effect = pobr_core::skill_env::CorruptedJewelEffect::from_jewels(build.jewel_items());
    for exp in &expansions {
        // An increase to a jewel socket's effect makes everything the jewel
        // grants that much stronger, before the node it lands on scales it.
        let jewel_scale = 1.0
            + jewel_socket_increase(exp.jewel.socket, build, data, || {
                jewel_effect.increase(exp.jewel.rarity, exp.jewel.corrupted)
            })
            .unwrap_or(0.0)
                / 100.0;
        for line in &exp.jewel.grant_lines {
            let Some((kind, granted)) = parse_grant_line(line) else {
                continue;
            };
            if !gate_parses(ctx, &granted) {
                continue;
            }
            let Ok(parsed) = ctx.parse(&granted) else {
                continue;
            };
            let nodes = match kind {
                GrantTargetKind::Notable => &exp.notable_nodes,
                GrantTargetKind::Small => &exp.small_nodes,
            };
            for node in nodes {
                let inc = f64::from(*effects.get(node).unwrap_or(&0))
                    + if kind == GrantTargetKind::Small {
                        global_small
                    } else {
                        0.0
                    };
                // What a jewel grants a node is that node's: it counts where
                // the node counts and is attributed to it.
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::PassiveNode,
                    format!("node.{node}"),
                ))
                .with_raw_text(granted.to_string());
                for modifier in &parsed.mods {
                    let modifier = modifier.clone().with_origin(origin.clone());
                    let mut by_jewel = pobr_core::ModDb::new();
                    by_jewel.scale_add_mod(modifier, jewel_scale, &data.high_precision);
                    for modifier in by_jewel.iter_mods() {
                        let mut scaled = pobr_core::ModDb::new();
                        scaled.scale_add_mod(
                            modifier.clone(),
                            1.0 + inc / 100.0,
                            &data.high_precision,
                        );
                        mods.extend(scaled.iter_mods().cloned());
                    }
                }
            }
        }
    }
    mods
}

/// Local effects overwrite in jewel order, matching PoB2's node-local values.
fn radius_node_effects(
    expansions: &[RadiusJewelExpansion<'_>],
) -> std::collections::BTreeMap<u32, u32> {
    let mut effects = std::collections::BTreeMap::new();
    for exp in expansions {
        for (nodes, inc) in [
            (&exp.notable_nodes, exp.jewel.notable_effect_inc),
            (&exp.small_nodes, exp.jewel.small_effect_inc),
        ] {
            if inc > 0 {
                for node in nodes {
                    effects.insert(*node, inc);
                }
            }
        }
    }
    effects
}

/// Scale each node once: global and local small effects add before truncation.
pub(crate) fn passive_effect_copies(
    build: &Build,
    data: &BuildData,
    passive_nodes: &[AllocatedNode],
) -> Result<Vec<Modifier>, BuildError> {
    fn delta(m: Modifier, scale: f64, data: &BuildData) -> Option<Modifier> {
        let value = match m.value {
            pobr_core::ModValue::Number(v)
                if matches!(m.mod_type, ModType::Base | ModType::Inc) =>
            {
                let extra = pobr_core::calc::buff_pass::scale_value(
                    &data.high_precision,
                    m.name.as_str(),
                    m.mod_type,
                    v,
                    scale,
                ) - v;
                if extra == 0.0 {
                    return None;
                }
                pobr_core::ModValue::Number(extra)
            }
            pobr_core::ModValue::NestedMods(nested) => {
                let nested: Vec<_> = nested
                    .into_iter()
                    .filter_map(|m| delta(m, scale, data))
                    .collect();
                if nested.is_empty() {
                    return None;
                }
                pobr_core::ModValue::NestedMods(nested)
            }
            _ => return None,
        };
        Some(Modifier { value, ..m })
    }
    let node_effect = radius_node_effects(&radius_jewel_expansions(build, data));
    let global_small = small_passive_effect_inc(build, data);
    let tree_nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let mut out: Vec<Modifier> = Vec::new();
    for node in passive_nodes {
        let Some(def) = tree_nodes.get(&node.node_id.0) else {
            continue;
        };
        if def.ascendancy_id.is_some() || is_attribute_node(def) {
            continue;
        }
        let inc = f64::from(*node_effect.get(&node.node_id.0).unwrap_or(&0))
            + if def.kind == pobr_data::catalog::PassiveNodeKind::Normal {
                global_small
            } else {
                0.0
            };
        if inc <= 0.0 {
            continue;
        }
        let scale = 1.0 + inc / 100.0;
        let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(node),
            engine_ctx(data),
        )
        .map_err(|e| BuildError::Parse(e.to_string()))?;
        out.extend(
            ingest
                .modifiers
                .into_iter()
                .filter_map(|m| delta(m, scale, data)),
        );
    }
    Ok(out)
}

/// The single parseability-gate function (from B3, which killed the dual parser):
/// a line passes when the parser read all of it (`ParseOutcome::is_complete`: parsed
/// with nothing left over), the same rule `ParseOutcome::into_applicable` applies at
/// every ingest. A failing line triggers the wrap-merge retry. The ingest side goes through the same engine;
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
    let pass = ctx.parse(t).is_ok_and(|o| o.is_complete());
    // Diagnostic: POBR_DBG_GATE=1 dumps lines dropped by the gate.
    if !pass && pobr_core::dbg_env!("POBR_DBG_GATE").is_some() {
        eprintln!("[GATE_DROP] {t}");
    }
    pass
}

/// Filters an item's three mod segments (implicit / explicit / enchant) each into their
/// parseable subset, preserving which segment each mod belongs to (used by
/// [`CalculationSession::add_item`] to assign source-category attribution per segment).
/// Rejected text remains visible in diagnostics; it must not silently disappear from
/// replacement comparisons or be applied as a partially understood modifier. Each
/// rejected line is noted with its source (`holder`'s) and what reads it:
/// `dedicated_reader` for `dedicated_texts`.
///
/// Game text can wrap across two lines of the item text (Berek's rings' "Wind
/// Skills ... count / as being boosted by ... Ground"): two adjacent lines that
/// parse as one when joined are merged first, reusing the tree's
/// [`combine_wrapped`]. The pair is recorded once as its joined line, not as
/// two unparsed lines.
pub(crate) fn filter_item_parseable(
    item: &Item,
    ctx: ParseCtx<'_>,
    session: &mut SourceWriter,
    holder: LineHolder<'_>,
    dedicated_texts: &[String],
    dedicated_reader: &'static str,
) -> Item {
    // These sources have dedicated orchestration consumers even when they do
    // not yield ordinary ModDb modifiers. Some of their game texts wrap across
    // two lines of the item text (The Adorned's, From Nothing's allocation
    // line): exempt only a pair whose join is such a line, not all text on
    // that jewel (unknown additional effects must remain visible).
    let wrapped_pairs: Vec<&[String]> = item
        .modifier_texts
        .windows(2)
        .filter(|pair| {
            let joined = pair.join(" ");
            dedicated_texts.contains(&joined)
                || (item.rarity == pobr_data::item::ItemRarity::Unique
                    && pobr_core::skill_env::CorruptedJewelEffect::parse_line(&joined).is_some())
        })
        .collect();
    // The dedicated reader of one original line (a dedicated consumer's text,
    // a gem property bonus, or half of a dedicated wrapped pair).
    let reader_of = |text: &String| -> Option<&'static str> {
        if dedicated_texts.contains(text) {
            Some(dedicated_reader)
        } else if parse_gem_property_bonus(text).is_some() {
            Some(GEM_PROPERTY_READER)
        } else if wrapped_pairs.iter().any(|pair| pair.contains(text)) {
            Some(WRAPPED_LINE_READER)
        } else {
            None
        }
    };
    // Whether a wrap-merged join belongs to a dedicated consumer (its halves
    // were already exempted above): it is read whole by that consumer, never
    // parsed as an ordinary modifier.
    let dedicated_join = |text: &String| -> Option<&'static str> {
        if dedicated_texts.contains(text) {
            Some(dedicated_reader)
        } else if item.rarity == pobr_data::item::ItemRarity::Unique
            && pobr_core::skill_env::CorruptedJewelEffect::parse_line(text).is_some()
        {
            Some(WRAPPED_LINE_READER)
        } else {
            None
        }
    };
    let mut filtered = item.clone();
    for (section, texts) in item_sections_mut(&mut filtered) {
        let source = holder.source(section);
        let originals: std::collections::HashSet<String> = texts.iter().cloned().collect();
        // Loreweave's first line already parses alone, so combine_wrapped
        // would accept it and drop its exclusion. Keep that exact pair whole
        // before the ordinary gates, with the same raw origin as core ingest.
        let paired = pobr_core::item::join_loreweave_rarity(texts)
            .map(std::borrow::Cow::into_owned)
            .collect();
        let (merged, dropped) = combine_wrapped(paired, ctx);
        *texts = merged;
        // Lines the merge dropped never reach the parser: ledger them here so
        // nothing silently vanishes.
        for text in &dropped {
            let fate = match reader_of(text) {
                Some(reader) => LineFate::ReadBy(reader),
                None => {
                    session.record_unsupported_modifier_text(text.clone());
                    LineFate::Unparsed
                }
            };
            session.note_line(&source, text, fate);
        }
        texts.retain(|text| {
            // A join the merge formed for a dedicated consumer stays with
            // that consumer: recorded once as the joined line, not as two
            // halves and not as an ordinary modifier. (Original single lines
            // keep the reader_of path below, as before.)
            if !originals.contains(text.as_str())
                && let Some(reader) = dedicated_join(text)
            {
                session.note_line(&source, text, LineFate::ReadBy(reader));
                return false;
            }
            let reader = reader_of(text);
            if gate_parses(ctx, text) {
                // A rule that recognises a line with a reader of its own
                // stores nothing for it, or what nothing reads: the line's
                // use is its reader's.
                if let Some(reader) = reader {
                    session.note_line(&source, text, LineFate::ReadBy(reader));
                }
                return true;
            }
            let fate = match reader {
                Some(reader) => LineFate::ReadBy(reader),
                None => {
                    session.record_unsupported_modifier_text(text.clone());
                    LineFate::Unparsed
                }
            };
            session.note_line(&source, text, fate);
            false
        });
    }
    filtered
}

/// The reader of half of a line the item text wraps across two.
const WRAPPED_LINE_READER: &str = "read whole with the line it wraps onto, by the jewel's own reader (jewel_tree, CorruptedJewelEffect)";

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

/// Collects the mod text of every equipped item and limb (in deterministic slot
/// order). Used by the text-only path.
pub(crate) fn collect_item_texts(build: &Build) -> Vec<String> {
    let mut texts = Vec::new();
    let equipped = build.equipped_items().into_iter().map(|(_, item)| item);
    for item in equipped.chain(build.limbs.values()) {
        texts.extend(item.enchant_texts.iter().cloned());
        texts.extend(item.implicit_texts.iter().cloned());
        texts.extend(item.modifier_texts.iter().cloned());
    }
    texts
}
