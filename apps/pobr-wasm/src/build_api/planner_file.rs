//! PoE2 Build Planner v1: https://www.pathofexile.com/developer/docs/game#buildplanner
//! Inventory entries are hints, never complete item records.
use super::*;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Entry {
    Id(String),
    Object {
        id: String,
        #[serde(default)]
        weapon_set: Option<u8>,
        #[serde(default)]
        additional_text: Option<String>,
        #[serde(default)]
        support_skills: Vec<Entry>,
        #[serde(default)]
        level_interval: Option<Interval>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Interval {
    Level(u32),
    Range([u32; 2]),
}

impl Entry {
    fn id(&self) -> &str {
        match self {
            Self::Id(id) | Self::Object { id, .. } => id,
        }
    }
    fn object(&self) -> Value {
        json!({"id": self.id()})
    }
    fn validate(&self) -> bool {
        match self {
            Self::Id(_) => true,
            Self::Object {
                weapon_set,
                level_interval,
                support_skills,
                ..
            } => {
                weapon_set.is_none_or(|s| s <= 2)
                    && level_interval.as_ref().is_none_or(Interval::valid)
                    && support_skills.iter().all(Self::validate)
            }
        }
    }
}
impl Interval {
    fn valid(&self) -> bool {
        let (start, end) = match self {
            Self::Level(level) => (*level, *level),
            Self::Range([start, end]) => (*start, *end),
        };
        start <= end
    }
}

#[derive(Debug, Deserialize)]
struct InventoryHint {
    inventory_id: String,
    #[serde(default)]
    slot_x: u32,
    #[serde(default)]
    slot_y: u32,
    #[serde(default)]
    unique_name: Option<String>,
    #[serde(default)]
    additional_text: Option<String>,
    #[serde(default)]
    level_interval: Option<Interval>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannerFile {
    name: String,
    author: Option<String>,
    link: Option<String>,
    description: Option<String>,
    ascendancy: Option<String>,
    #[serde(default)]
    passives: Vec<Entry>,
    #[serde(default)]
    skills: Vec<Entry>,
    #[serde(default)]
    inventory_slots: Vec<InventoryHint>,
}

fn parse(value: &Value) -> Result<PlannerFile, super::super::ApiError> {
    let file: PlannerFile = serde_json::from_value(value.clone()).map_err(|e| {
        super::super::ApiError::decode_error(format!("invalid planner .build: {e}"))
    })?;
    if !file
        .passives
        .iter()
        .chain(&file.skills)
        .all(Entry::validate)
        || file
            .inventory_slots
            .iter()
            .any(|i| i.level_interval.as_ref().is_some_and(|v| !v.valid()))
    {
        return Err(super::super::ApiError::decode_error(
            "invalid planner weapon set or level interval",
        ));
    }
    Ok(file)
}

pub(super) fn decode(value: Value) -> Result<String, super::super::ApiError> {
    let file = parse(&value)?;
    let skills: Vec<_> = file
        .skills
        .iter()
        .map(|e| {
            let supports = match e {
                Entry::Object { support_skills, .. } => {
                    support_skills.iter().map(Entry::object).collect()
                }
                _ => Vec::new(),
            };
            json!({"id": e.id(), "support_skills": supports})
        })
        .collect();
    // Reuse the legacy ID resolution, but deliberately pass no equipment text.
    let legacy = json!({"name": file.name, "ascendancy": file.ascendancy,
        "passives": file.passives.iter().map(Entry::object).collect::<Vec<_>>(), "skills": skills, "items": []});
    let mut out: Value = serde_json::from_str(&decode_build_file_impl(&legacy.to_string())?)
        .map_err(|e| format!("decode planner: {e}"))?;
    let data = state::build_data().map_err(super::super::ApiError::not_initialized)?;
    let mut exclusive = [Vec::<u32>::new(), Vec::new()];
    for entry in &file.passives {
        if let Entry::Object {
            weapon_set: Some(set @ 1..=2),
            ..
        } = entry
            && let Some(node) = data.passive_nodes.values().find(|n| n.id == entry.id())
        {
            exclusive[usize::from(*set - 1)].push(node.skill);
        }
    }
    for entry in &file.passives {
        if let Entry::Object {
            additional_text: Some(text),
            ..
        } = entry
            && let Some(choice) = text
                .strip_prefix("Attribute: ")
                .filter(|c| matches!(*c, "str" | "dex" | "int"))
            && let Some(node) = data.passive_nodes.values().find(|n| n.id == entry.id())
        {
            out["tree"]["attribute_choices"][node.skill.to_string()] = json!(choice);
        }
    }
    for nodes in &mut exclusive {
        nodes.sort_unstable();
        nodes.dedup();
    }
    if exclusive[0].iter().any(|id| exclusive[1].contains(id)) {
        return Err(super::super::ApiError::decode_error(
            "passive appears in both exclusive weapon sets",
        ));
    }
    out["tree"]["allocated_nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|id| !exclusive[1].contains(&(id.as_u64().unwrap() as u32)));
    out["weapon_swap"] = json!({"active":1,"alternate_items":[],"exclusive_nodes":exclusive});
    out["character"]["level"] = json!(1);
    // The schema has no gem levels or quality. Recognize only the exact textual
    // hint we export; all other prose remains a hint, with explicit defaults.
    for (group, entry) in out["socket_groups"].as_array_mut().unwrap().iter_mut().zip(
        file.skills
            .iter()
            .filter(|e| cn_gem_effect_id(&data, e.id()).is_some()),
    ) {
        let mut entries = vec![entry];
        if let Entry::Object { support_skills, .. } = entry {
            entries.extend(
                support_skills
                    .iter()
                    .filter(|e| cn_gem_effect_id(&data, e.id()).is_some()),
            );
        }
        for (gem, entry) in group["gems"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(entries)
        {
            let hint = match entry {
                Entry::Object {
                    additional_text, ..
                } => additional_text.as_deref(),
                _ => None,
            };
            let (level, quality) = hint.and_then(gem_hint).unwrap_or((1, 0));
            gem["level"] = json!(level);
            gem["quality"] = json!(quality);
        }
    }
    let mut notes = vec![out["notes"].as_str().unwrap_or_default().to_owned()];
    for text in [file.author, file.link, file.description]
        .into_iter()
        .flatten()
    {
        notes.push(text);
    }
    notes.push("[import] Planner inventory entries are hints only; equipment, flasks and jewels remain empty. Character level defaults to 1. Gem level/quality default to 1/0 unless an exact Level hint is present. Level intervals and custom text are preserved for export, not evaluated. Choose a class if no ascendancy is specified.".into());
    for hint in &file.inventory_slots {
        notes.push(format!(
            "[{} ({},{})] {} {}",
            hint.inventory_id,
            hint.slot_x,
            hint.slot_y,
            hint.unique_name.as_deref().unwrap_or_default(),
            hint.additional_text.as_deref().unwrap_or_default()
        ));
    }
    out["notes"] = json!(notes.join("\n"));
    out["planner_source"] = json!(value.to_string());
    Ok(out.to_string())
}

fn gem_hint(text: &str) -> Option<(u32, u32)> {
    let text = text.strip_prefix("Level ")?;
    let (level, quality) = match text.split_once(", ") {
        Some((level, quality)) => (
            level.parse().ok()?,
            quality.strip_suffix("% Quality")?.parse().ok()?,
        ),
        None => (text.parse().ok()?, 0),
    };
    (level > 0 && level <= 40 && quality <= 100).then_some((level, quality))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRequest {
    code: String,
    name: String,
    #[serde(default)]
    all_loadouts: bool,
    source: Option<String>,
}

pub fn encode_build_file_json(request: &str) -> Result<String, String> {
    encode(request).map_err(super::super::ApiError::into_json)
}

fn encode(request: &str) -> Result<String, super::super::ApiError> {
    let req: ExportRequest = serde_json::from_str(request)
        .map_err(|e| super::super::ApiError::bad_request(e.to_string()))?;
    let source = req
        .source
        .as_ref()
        .map(|s| serde_json::from_str::<Value>(s))
        .transpose()
        .map_err(|e| super::super::ApiError::bad_request(e.to_string()))?;
    if let Some(source) = &source {
        parse(source)?;
    }
    let (build, _) = decode_selected(&req.code, &SetSelection::default())?;
    let selections: Vec<_> = if req.all_loadouts {
        build
            .loadouts
            .iter()
            .map(|l| {
                (
                    format!("{} - {}", req.name, l.name),
                    SetSelection {
                        tree: Some(l.tree),
                        item: l.item,
                        skill: l.skill,
                        config: None,
                    },
                )
            })
            .collect()
    } else {
        vec![(req.name, SetSelection::default())]
    };
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for (index, (name, selection)) in selections.into_iter().enumerate() {
        let (build, _) = decode_selected(&req.code, &selection)?;
        let file = export_build(&build, &name, source.as_ref(), &mut warnings)?;
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_control() || "\\/:*?\"<>|".contains(c) {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        files.push(json!({"filename":format!("{}{}.build", if safe.trim().is_empty() { "Unnamed" } else {safe.trim()}, if req.all_loadouts {format!(" - {}", index + 1)} else {String::new()}), "content": serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?}));
    }
    warnings.sort();
    warnings.dedup();
    Ok(json!({"files":files,"warnings":warnings}).to_string())
}

fn existing(source: Option<&Value>, collection: &str, id: &str) -> Value {
    source
        .and_then(|s| s[collection].as_array())
        .and_then(|a| {
            a.iter().find(|v| {
                v.as_str()
                    .or_else(|| v["id"].as_str())
                    .is_some_and(|other| {
                        other.replace("/Gems/", "/Gem/") == id.replace("/Gems/", "/Gem/")
                    })
            })
        })
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({"id":id}))
}

fn export_build(
    build: &BuildJson,
    name: &str,
    source: Option<&Value>,
    warnings: &mut Vec<String>,
) -> Result<Value, super::super::ApiError> {
    let data = state::build_data().map_err(super::super::ApiError::not_initialized)?;
    let game = state::game_data().map_err(super::super::ApiError::not_initialized)?;
    let meta = game.passive_tree_meta().map_err(|e| e.to_string())?;
    let manifest = game.manifest().map_err(|e| e.to_string())?;
    let current = format!(
        "0_{}",
        manifest.poe_version.split('.').nth(1).unwrap_or_default()
    );
    if build
        .tree
        .tree_version
        .as_deref()
        .is_some_and(|v| v != "Default" && v != current)
    {
        return Err(super::super::ApiError::bad_request(
            "The game planner uses the current passive tree; historical trees cannot be exported without conversion.",
        ));
    }
    let mut out = json!({"name": name, "passives":[],"skills":[],"inventory_slots":[]});
    for key in ["author", "link", "description"] {
        if let Some(value) = source.and_then(|s| s.get(key)) {
            out[key] = value.clone();
        }
    }
    if let Some(asc) = meta
        .classes
        .iter()
        .find(|c| c.name == build.character.class_name)
        .and_then(|c| {
            c.ascendancies
                .iter()
                .find(|a| a.name == build.character.ascendancy_name)
        })
    {
        out["ascendancy"] = json!(asc.id);
    }
    let mut nodes = build.tree.allocated_nodes.clone();
    if let Some(swap) = &build.weapon_swap {
        nodes.extend(swap.exclusive_nodes.iter().flatten());
    }
    nodes.sort_unstable();
    nodes.dedup();
    for id in nodes {
        let Some(node) = data.passive_nodes.get(&id) else {
            warnings.push(format!(
                "Passive {id} has no current game ID and was omitted."
            ));
            continue;
        };
        let mut entry = existing(source, "passives", &node.id);
        entry["weapon_set"] = json!(
            build
                .weapon_swap
                .as_ref()
                .and_then(|s| s.exclusive_nodes.iter().position(|n| n.contains(&id)))
                .map_or(0, |i| i + 1)
        );
        if let Some(choice) = build.tree.attribute_choices.get(&id) {
            entry["additional_text"] = json!(format!("Attribute: {choice}"));
        }
        out["passives"].as_array_mut().unwrap().push(entry);
    }
    for group in build.socket_groups.iter().filter(|g| g.enabled) {
        let mut gems = Vec::new();
        for gem in group.gems.iter().filter(|g| g.enabled) {
            let base = data
                .skill_gems
                .values()
                .filter(|g| g.granted_effect_id.as_deref() == Some(&gem.skill_id))
                .min_by(|a, b| a.id.cmp(&b.id));
            if let Some(base) = base {
                if base.gem_tags.iter().any(|tag| tag == "meta") {
                    warnings.push(format!(
                        "Meta gem {} is not supported by the game planner and was omitted.",
                        gem.skill_id
                    ));
                    continue;
                }
                gems.push((gem, base));
            } else {
                warnings.push(format!(
                    "Skill {} has no gem base ID and was omitted.",
                    gem.skill_id
                ));
            }
        }
        for (gem, base) in gems.iter().filter(|(_, b)| !b.is_support) {
            let mut entry = existing(source, "skills", &base.id);
            if entry
                .get("additional_text")
                .and_then(Value::as_str)
                .is_none_or(|text| gem_hint(text).is_some())
            {
                entry["additional_text"] =
                    json!(format!("Level {}, {}% Quality", gem.level, gem.quality));
            }
            let supports: Vec<_> = gems
                .iter()
                .filter(|(_, b)| b.is_support)
                .map(|(gem, base)| {
                    let mut s = existing(Some(&entry), "support_skills", &base.id);
                    if s.get("additional_text")
                        .and_then(Value::as_str)
                        .is_none_or(|text| gem_hint(text).is_some())
                    {
                        s["additional_text"] =
                            json!(format!("Level {}, {}% Quality", gem.level, gem.quality));
                    }
                    s
                })
                .collect();
            entry["support_skills"] = json!(supports);
            out["skills"].as_array_mut().unwrap().push(entry);
        }
    }
    if let Some(hints) = source.and_then(|s| s.get("inventory_slots")) {
        out["inventory_slots"] = hints.clone();
    }
    let active = build.weapon_swap.as_ref().map_or(1, |s| s.active);
    let mut items: Vec<_> = build
        .items
        .equipped
        .iter()
        .map(|i| (i.slot.as_str(), i.text.as_str(), active))
        .collect();
    if let Some(swap) = &build.weapon_swap {
        items.extend(
            swap.alternate_items
                .iter()
                .map(|i| (i.slot.as_str(), i.text.as_str(), 3 - active)),
        );
    }
    for (slot, text, set) in items {
        let inventory = match slot {
            "weapon1" => {
                if set == 1 {
                    "Weapon1"
                } else {
                    "Weapon2"
                }
            }
            "weapon2" => {
                if set == 1 {
                    "Offhand1"
                } else {
                    "Offhand2"
                }
            }
            "helmet" => "Helm1",
            "bodyarmour" => "BodyArmour1",
            "gloves" => "Gloves1",
            "boots" => "Boots1",
            "amulet" => "Amulet1",
            "ring1" => "Ring1",
            "ring2" => "Ring2",
            "ring3" => "Ring3",
            "belt" => "Belt1",
            _ => {
                warnings.push(format!(
                    "Equipment slot {slot} has no planner mapping and was omitted."
                ));
                continue;
            }
        };
        let hints = out["inventory_slots"].as_array_mut().unwrap();
        hints.retain(|h| {
            h["inventory_id"] != inventory
                || h["slot_x"].as_u64().unwrap_or(0) != 0
                || h["slot_y"].as_u64().unwrap_or(0) != 0
        });
        hints.push(
            json!({"inventory_id": inventory, "additional_text":text.replace(['{','}'], "")}),
        );
    }
    for item in &build.items.flasks {
        let x = match item.slot.as_str() {
            "Flask 1" => 0,
            "Flask 2" => 1,
            "Charm 1" => 2,
            "Charm 2" => 3,
            "Charm 3" => 4,
            _ => continue,
        };
        let hints = out["inventory_slots"].as_array_mut().unwrap();
        hints.retain(|h| h["inventory_id"] != "Flask1" || h["slot_x"].as_u64().unwrap_or(0) != x);
        hints.push(json!({"inventory_id":"Flask1", "slot_x":x, "additional_text":item.text.replace(['{','}'], "")}));
    }
    if !build.items.socket_jewels.is_empty() || !build.items.limbs.is_empty() {
        warnings
            .push("Jewels and limbs have no planner inventory mapping and were omitted.".into());
    }
    Ok(out)
}
