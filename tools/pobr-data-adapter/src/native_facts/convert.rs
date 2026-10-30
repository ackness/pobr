//! Normalize stable identities and relationships, checking projected facts against sealed tables.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::model::{
    Candidate, ChannelInput, ClassRelation, Effect, Graph, GraphInput, Group, Identity, InputRef,
    Instance, LinkInput, Passive, PassiveInput, PassiveStat, Quality, QualityChannel, QualityStat,
    SkillsInput, StatSet, StatSetLink, Summary,
};
use super::trust::{self, VerifiedInput, require};
use super::{Profile, core_skills};

pub(super) fn normalize(input: &VerifiedInput) -> Result<Candidate, String> {
    let tables = Tables::new(input)?;
    let passives: PassiveInput = decode(&input.documents["passives.json"])?;
    let skills: SkillsInput = decode(&input.documents["skills.json"])?;
    let graph: GraphInput = decode(&input.documents["graph.json"])?;
    let mut candidate = Candidate {
        format: "pobr-native-domain-candidate/v1".into(),
        input_manifest_sha256: crate::raw_source::sha256_hex(&input.manifest_bytes),
        summary: Summary {
            passive_definitions: 0,
            graph_instances: 0,
            graph_stat_entries: 0,
            class_overrides: 0,
            effects: 0,
            qualities: 0,
        },
        passive_order: Vec::new(),
        passives: BTreeMap::new(),
        graph: normalize_graph(graph, &passives, &tables, input)?,
        instances: Vec::new(),
        class_relations: Vec::new(),
        effect_order: Vec::new(),
        effects: BTreeMap::new(),
        stat_set_order: Vec::new(),
        stat_sets: BTreeMap::new(),
        qualities: Vec::new(),
        source_diagnostics: "source-diagnostics.json".into(),
        core_skills: None,
        core_summary: None,
    };
    require(
        passives.definitions.len() == tables.len("PassiveSkills"),
        "incomplete passive definitions",
    )?;
    let instance_ids: BTreeSet<_> = passives
        .default_graph_instances
        .iter()
        .map(|i| i.graph_id)
        .collect();
    for (row, definition) in passives.definitions.into_iter().enumerate() {
        require(
            definition.definition.row == row,
            "passive definition order mismatch",
        )?;
        let raw = tables.identity("PassiveSkills", &definition.definition)?;
        require(
            raw["PassiveSkillGraphId"] == definition.graph_id,
            "passive graph id mismatch",
        )?;
        require(
            definition.in_default_graph == instance_ids.contains(&definition.graph_id),
            "passive graph membership mismatch",
        )?;
        let raw_stats = array(&raw["Stats"])?;
        require(
            raw_stats.len() == definition.stats.len(),
            "passive stats truncated",
        )?;
        let mut stats = Vec::new();
        for (index, stat) in definition.stats.into_iter().enumerate() {
            tables.identity("Stats", &stat.stat)?;
            require(
                stat.slot == index + 1 && stat.value_field == format!("Stat{}Value", stat.slot),
                "passive slot mismatch",
            )?;
            require(
                raw_stats[index] == stat.stat.row && raw[&stat.value_field] == stat.value,
                "passive stat/value mismatch",
            )?;
            stats.push(PassiveStat {
                stat_id: stat.stat.id,
                raw_stat: InputRef::row("Stats", stat.stat.row),
                slot: stat.slot,
                value_field: stat.value_field,
                value: stat.value,
            });
        }
        candidate
            .passive_order
            .push(definition.definition.id.clone());
        candidate.passives.insert(
            definition.definition.id,
            Passive {
                raw: InputRef::row("PassiveSkills", row),
                graph_id_raw: definition.graph_id,
                in_default_graph: definition.in_default_graph,
                stats,
            },
        );
    }
    for instance in passives.default_graph_instances {
        tables.identity("PassiveSkills", &instance.definition)?;
        let definition = candidate
            .passives
            .get(&instance.definition.id)
            .ok_or("unknown instance definition")?;
        require(
            definition.graph_id_raw == instance.graph_id && definition.in_default_graph,
            "instance definition mismatch",
        )?;
        candidate.summary.graph_stat_entries += definition.stats.len();
        candidate.instances.push(Instance {
            graph_id: instance.graph_id,
            definition_id: instance.definition.id,
            group: instance.group,
            radius_raw: instance.radius,
            position_raw: instance.position,
            connections: instance.connections,
            splines: instance.splines,
        });
    }
    require(
        passives.class_overrides.len() == tables.len("ClassPassiveSkillOverrides"),
        "class rows truncated",
    )?;
    for (row, relation) in passives.class_overrides.into_iter().enumerate() {
        require(row == relation.row, "class relation order mismatch")?;
        let raw = tables.row("ClassPassiveSkillOverrides", row)?;
        tables.identity("Characters", &relation.character)?;
        tables.identity("PassiveSkills", &relation.base)?;
        tables.identity("PassiveSkills", &relation.replacement)?;
        require(
            raw["CharacterToOverrideFor"] == relation.character.row
                && raw["SkillToOverride"] == relation.base.row
                && raw["Override"] == relation.replacement.row,
            "class foreign-key mismatch",
        )?;
        for (id, graph_id, member) in [
            (
                &relation.base.id,
                relation.base_graph_id,
                relation.base_in_default_graph,
            ),
            (
                &relation.replacement.id,
                relation.replacement_graph_id,
                relation.replacement_in_default_graph,
            ),
        ] {
            let definition = candidate
                .passives
                .get(id)
                .ok_or("unknown class definition")?;
            require(
                definition.graph_id_raw == graph_id && definition.in_default_graph == member,
                "class graph membership mismatch",
            )?;
        }
        candidate.class_relations.push(ClassRelation {
            raw: InputRef::row("ClassPassiveSkillOverrides", row),
            character_id: relation.character.id,
            raw_character: InputRef::row("Characters", relation.character.row),
            base_definition_id: relation.base.id,
            replacement_definition_id: relation.replacement.id,
            base_graph_id: relation.base_graph_id,
            replacement_graph_id: relation.replacement_graph_id,
            base_in_default_graph: relation.base_in_default_graph,
            replacement_in_default_graph: relation.replacement_in_default_graph,
        });
    }
    for row in 0..tables.len("GrantedEffectStatSets") {
        let id = text(&tables.row("GrantedEffectStatSets", row)?["Id"])?;
        candidate.stat_set_order.push(id.into());
        candidate.stat_sets.insert(
            id.into(),
            StatSet {
                raw: InputRef::row("GrantedEffectStatSets", row),
            },
        );
    }
    require(
        skills.effects.len() == tables.len("GrantedEffects"),
        "effects truncated",
    )?;
    for (row, effect) in skills.effects.into_iter().enumerate() {
        require(effect.effect.row == row, "effect order mismatch")?;
        let raw = tables.identity("GrantedEffects", &effect.effect)?;
        let links = normalize_links(&effect.ordered_stat_sets, raw, &tables)?;
        candidate.effect_order.push(effect.effect.id.clone());
        candidate.effects.insert(
            effect.effect.id,
            Effect {
                raw: InputRef::row("GrantedEffects", row),
                ordered_stat_sets: links,
            },
        );
    }
    require(
        skills.qualities.len() == tables.len("GrantedEffectQualityStats"),
        "quality rows truncated",
    )?;
    for (row, quality) in skills.qualities.into_iter().enumerate() {
        require(quality.row == row, "quality order mismatch")?;
        let raw = tables.row("GrantedEffectQualityStats", row)?;
        let effect_raw = tables.identity("GrantedEffects", &quality.effect)?;
        require(
            raw["GrantedEffect"] == quality.effect.row,
            "quality effect mismatch",
        )?;
        let links = normalize_links(&quality.ordered_stat_sets, effect_raw, &tables)?;
        require(
            candidate
                .effects
                .get(&quality.effect.id)
                .is_some_and(|e| e.ordered_stat_sets == links),
            "quality ordered stat sets mismatch",
        )?;
        candidate.qualities.push(Quality {
            raw: InputRef::row("GrantedEffectQualityStats", row),
            effect_id: quality.effect.id,
            main: normalize_channel(
                quality.main,
                raw,
                ["ApplyToStatSets", "Stats", "StatsValuesPermille"],
                &tables,
            )?,
            alternate: normalize_channel(
                quality.alternate,
                raw,
                ["AltApplyToStatSets", "AltStats", "AltStatValuesPermille"],
                &tables,
            )?,
        });
    }
    // Preserve the entire diagnostics document byte-for-byte, including unknown diagnostic shapes.
    require(
        input.documents["diagnostics.json"]["diagnostics"].is_array()
            && input.documents["diagnostics.json"]["scope"] == input.manifest["scope"],
        "source diagnostics/scope mismatch",
    )?;
    candidate.summary.passive_definitions = candidate.passives.len();
    candidate.summary.graph_instances = candidate.instances.len();
    candidate.summary.class_overrides = candidate.class_relations.len();
    candidate.summary.effects = candidate.effects.len();
    candidate.summary.qualities = candidate.qualities.len();
    let mut source_summary = input.manifest["summary"].clone();
    if input.profile == Profile::CoreSkillsA0 {
        let (core, summary) = core_skills::normalize(input, &tables)?;
        let counts = serde_json::to_value(&summary).map_err(|e| e.to_string())?;
        for (key, value) in counts.as_object().ok_or("invalid core summary")? {
            require(
                source_summary[key] == *value,
                "core counts differ from source summary",
            )?;
            source_summary
                .as_object_mut()
                .ok_or("invalid source summary")?
                .remove(key);
        }
        candidate.format = "pobr-native-domain-candidate/v2".into();
        candidate.core_skills = Some(core);
        candidate.core_summary = Some(summary);
    }
    let expected: Summary = decode(&source_summary)?;
    require(
        candidate.summary == expected,
        "measured projection counts differ from source summary",
    )?;
    Ok(candidate)
}

fn normalize_channel(
    input: ChannelInput,
    raw: &Value,
    fields: [&str; 3],
    tables: &Tables<'_>,
) -> Result<QualityChannel, String> {
    require(
        serde_json::to_value(&input.scope_raw).map_err(|e| e.to_string())? == raw[fields[0]],
        "quality raw scope mismatch",
    )?;
    let raw_stats = array(&raw[fields[1]])?;
    let raw_values = array(&raw[fields[2]])?;
    require(
        raw_stats.len() == input.stats.len() && raw_values.len() == input.stats.len(),
        "quality stats truncated or unequal",
    )?;
    let mut stats = Vec::new();
    for (slot, stat) in input.stats.into_iter().enumerate() {
        tables.identity("Stats", &stat.stat)?;
        require(
            stat.slot == slot
                && raw_stats[slot] == stat.stat.row
                && raw_values[slot] == stat.value_permille,
            "quality stat/value mismatch",
        )?;
        stats.push(QualityStat {
            stat_id: stat.stat.id,
            raw_stat: InputRef::row("Stats", stat.stat.row),
            slot,
            value_permille: stat.value_permille,
        });
    }
    Ok(QualityChannel {
        scope_raw: input.scope_raw,
        stats,
    })
}

fn normalize_links(
    links: &[LinkInput],
    raw: &Value,
    tables: &Tables<'_>,
) -> Result<Vec<StatSetLink>, String> {
    let mut expected = Vec::new();
    if !raw["StatSet"].is_null() {
        expected.push(("StatSet", None, &raw["StatSet"]));
    }
    for (slot, value) in array(&raw["AdditionalStatSets"])?.iter().enumerate() {
        if !value.is_null() {
            expected.push(("AdditionalStatSets", Some(slot), value));
        }
    }
    require(
        expected.len() == links.len(),
        "effect stat-set identities truncated",
    )?;
    links
        .iter()
        .zip(expected)
        .map(|(link, (field, slot, row))| {
            tables.identity("GrantedEffectStatSets", &link.stat_set)?;
            // The producer's index is explicitly a hypothesis; preserve it without interpreting scopes.
            require(
                link.source_field == field && link.source_slot == slot && *row == link.stat_set.row,
                "effect stat-set identity/order mismatch",
            )?;
            Ok(StatSetLink {
                stat_set_id: link.stat_set.id.clone(),
                source_field: link.source_field.clone(),
                source_slot: link.source_slot,
                local_index_hypothesis: link.local_index_hypothesis,
            })
        })
        .collect()
}

fn normalize_graph(
    graph: GraphInput,
    passives: &PassiveInput,
    tables: &Tables<'_>,
    input: &VerifiedInput,
) -> Result<Graph, String> {
    let tree = tables.identity("PassiveSkillTrees", &graph.tree)?;
    require(
        format!("{}.psg", text(&tree["PassiveSkillGraph"])?) == graph.raw_resource,
        "physical Default tree resource mismatch",
    )?;
    require(
        graph.tree.id == "Default"
            && graph.raw_resource == "Metadata/PassiveSkillGraph.psg"
            && graph.header == [3, 0]
            && graph.warnings.is_empty()
            && !graph.radius_semantics.is_empty()
            && graph.parsed_bytes
                == input.manifest["source_identity"]["raw_resources"][&graph.raw_resource]["bytes"],
        "invalid default graph identity/framing",
    )?;
    let instances: BTreeMap<_, _> = passives
        .default_graph_instances
        .iter()
        .map(|i| (i.graph_id, i))
        .collect();
    require(
        instances.len() == passives.default_graph_instances.len(),
        "duplicate graph instance",
    )?;
    // Only instantiated IDs require a unique physical join. Off-graph placeholder IDs
    // may repeat and still represent distinct definitions.
    let mut physical_definitions = BTreeMap::<u64, Vec<usize>>::new();
    for row in 0..tables.len("PassiveSkills") {
        if let Some(id) = tables.row("PassiveSkills", row)?["PassiveSkillGraphId"].as_u64()
            && instances.contains_key(&id)
        {
            physical_definitions.entry(id).or_default().push(row);
        }
    }
    for instance in instances.values() {
        let definitions = physical_definitions.get(&instance.graph_id);
        require(
            definitions.is_some_and(|rows| rows.len() == 1),
            "instantiated graph id must identify exactly one physical definition",
        )?;
        require(
            definitions.is_some_and(|rows| rows[0] == instance.definition.row),
            "instance selected physical definition mismatch",
        )?;
        tables.identity("PassiveSkills", &instance.definition)?;
        require(
            instance.connections.len() == instance.splines.len(),
            "graph connection/spline lengths differ",
        )?;
        for edge in &instance.connections {
            require(instances.contains_key(edge), "unresolved graph edge")?;
        }
    }
    for root in &graph.roots {
        require(
            instances.contains_key(&root.graph_id),
            "unresolved graph root",
        )?;
    }
    let mut seen = BTreeSet::new();
    let mut groups = Vec::new();
    for (index, group) in graph.groups.into_iter().enumerate() {
        require(group.group == index, "graph group order mismatch")?;
        let mut node_order = Vec::new();
        for node in group.nodes {
            require(seen.insert(node.graph_id), "duplicate graph node")?;
            let instance = instances
                .get(&node.graph_id)
                .ok_or("unresolved graph node")?;
            require(
                instance.group == index
                    && instance.radius == node.radius
                    && instance.position == node.position
                    && instance.connections == node.connections
                    && instance.splines == node.splines,
                "graph instance/topology mismatch",
            )?;
            node_order.push(node.graph_id);
        }
        groups.push(Group {
            x_raw: group.x,
            y_raw: group.y,
            flag_raw: group.flag,
            unknown1: group.unknown1,
            unknown2: group.unknown2,
            node_order,
        });
    }
    require(
        seen.len() == instances.len(),
        "graph instances missing from groups",
    )?;
    Ok(Graph {
        tree_id: graph.tree.id,
        raw_tree: InputRef::row("PassiveSkillTrees", graph.tree.row),
        raw: InputRef {
            payload: "graph.json".into(),
            pointer: String::new(),
        },
        header: graph.header,
        skills_per_orbit_raw: graph.skills_per_orbit,
        roots: graph.roots,
        groups,
    })
}

pub(super) struct Tables<'src> {
    rows: BTreeMap<&'static str, Vec<&'src Value>>,
}

impl<'src> Tables<'src> {
    fn new(input: &'src VerifiedInput) -> Result<Self, String> {
        let mut rows = BTreeMap::new();
        for table in trust::tables(input.profile) {
            let doc = &input.documents[&format!("tables/{table}.json")];
            trust::exact_keys(
                doc,
                &[
                    "columns",
                    "physical_fields",
                    "raw_resource",
                    "row_key",
                    "rows",
                    "table",
                    "validation",
                ]
                .map(String::from),
            )?;
            require(
                doc["table"] == table
                    && doc["raw_resource"] == format!("Data/Balance/{table}.datc64")
                    && doc["row_key"].is_string(),
                "physical table identity mismatch",
            )?;
            let fields: Vec<String> = decode(&doc["physical_fields"])?;
            if input.profile == Profile::CoreSkillsA0 {
                require(
                    core_skills::required_fields(table)
                        .iter()
                        .all(|required| fields.iter().any(|field| field == required)),
                    "missing core-skill physical field",
                )?;
            }
            let unique: BTreeSet<_> = fields.iter().collect();
            require(
                unique.len() == fields.len() && !fields.is_empty(),
                "invalid physical field inventory",
            )?;
            let mut mapped = Vec::new();
            for (index, column) in array(&doc["columns"])?.iter().enumerate() {
                require(
                    column["source_column"] == index && column["schema"].is_object(),
                    "invalid physical column metadata",
                )?;
                let column_fields: Vec<String> = decode(&column["physical_fields"])?;
                mapped.extend(column_fields);
            }
            require(mapped == fields, "physical column mapping mismatch")?;
            let source_rows = array(&doc["rows"])?;
            let validation = &doc["validation"];
            require(
                validation["rows"] == source_rows.len()
                    && validation["warnings"] == serde_json::json!([])
                    && validation["schema_row_bytes"]
                        .as_u64()
                        .is_some_and(|v| v > 0)
                    && validation["schema_row_bytes"] == validation["actual_row_bytes"],
                "physical table validation failed",
            )?;
            let mut values = Vec::new();
            let mut ids = BTreeSet::new();
            for (index, row) in source_rows.iter().enumerate() {
                trust::exact_keys(row, &["_row".into(), "values".into()])?;
                require(row["_row"] == index, "physical row order mismatch")?;
                trust::exact_keys(&row["values"], &fields)?;
                if fields.iter().any(|f| f == "Id") {
                    let id = text(&row["values"]["Id"])?;
                    require(
                        !id.is_empty() && ids.insert(id),
                        "empty or duplicate stable game id",
                    )?;
                }
                values.push(&row["values"]);
            }
            rows.insert(table, values);
        }
        Ok(Self { rows })
    }

    pub(super) fn len(&self, table: &str) -> usize {
        self.rows[table].len()
    }

    pub(super) fn row(&self, table: &str, row: usize) -> Result<&'src Value, String> {
        self.rows[table]
            .get(row)
            .copied()
            .ok_or_else(|| format!("unresolved physical row {table}/{row}"))
    }

    pub(super) fn identity(&self, table: &str, identity: &Identity) -> Result<&'src Value, String> {
        let row = self.row(table, identity.row)?;
        require(
            row["Id"] == identity.id,
            &format!("{table} stable id/row mismatch"),
        )?;
        Ok(row)
    }
}

pub(super) fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    T::deserialize(value).map_err(|e| e.to_string())
}

pub(super) fn array(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "expected array".into())
}

fn text(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| "expected stable string id".into())
}
