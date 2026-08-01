//! Loadout — a **grouped switch** across passive tree / items / skills (PoB2
//! `Build.lua:617 SyncLoadouts`).
//!
//! PoB2 doesn't store loadouts separately: it **derives** them from each set's `title`
//! following a naming convention, so this feature needs zero format extension and is
//! bidirectionally compatible with PoB2 — a group created in PoB2 is recognized by pobr,
//! and vice versa.
//!
//! Two binding styles (matching vendor `SyncLoadouts` one for one):
//!
//! 1. **Exact same title**: sets of every category with an identical `title` form a
//!    group (`title="Mapping"` × 3).
//! 2. **Brace identifier**: `title="Leveling {lvl30}"` — bound via `{lvl30}`, so display
//!    names can differ across categories; a comma-separated `{lvl30,lvl50}` means one
//!    set belongs to multiple groups at once.
//!
//! **Single-set exemption**: when a category has only one set (or none), that category
//! is excluded from matching (vendor's `oneItem` / `oneSkill`). So "only switch tree and
//! items, skills share one set" needs no skill group.
//!
//! # Known differences from vendor
//!
//! - **ConfigSet doesn't participate**: pobr doesn't yet parse multiple `<Config>` sets,
//!   which is equivalent to vendor's `oneConfig` always being true. If a build actually
//!   has multiple ConfigSets, PoB2 would additionally require config to match too, so
//!   this would over-derive groups. Fix once multi-config support lands.
//! - **Tree version prefix doesn't participate in matching**: vendor prefixes the
//!   display name with `[0.4] Mapping` when `treeVersion` isn't the latest, before
//!   matching against item/skill's plain title (`Build.lua:695`), which means exact-name
//!   groups on an old-version tree fail to match. Here we only match on the plain
//!   `title` — behavior is identical when the tree version is always the latest.

/// A reference to one set: its XML document order (1-based, matching the value
/// semantics of `activeSpec` / `activeItemSet` / `activeSkillSet`) plus its display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRef {
    /// 1-based document order.
    pub id: usize,
    /// Raw `title` attribute; defaults to `"Default"` (matching vendor's `spec.title or "Default"`).
    pub title: String,
}

/// The list of sets of each category in the build XML (in document order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildSets {
    pub trees: Vec<SetRef>,
    pub items: Vec<SetRef>,
    pub skills: Vec<SetRef>,
}

/// A derived loadout: group name + the selected set document order for each category.
///
/// `item` / `skill` being `None` means that category didn't participate in binding
/// (single-set exemption) — leave it untouched when switching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loadout {
    /// Display name. When bound by identifier, it's `"<name with identifier stripped> {<identifier>}"` (matches vendor).
    pub name: String,
    pub tree: usize,
    pub item: Option<usize>,
    pub skill: Option<usize>,
}

/// Splits the `{a,b}` identifier out of a `title` and the name with the identifier stripped.
///
/// Equivalent to vendor's `string.match(title, "%{([%w,]+)%}")` + `gsub` + trim: only
/// **ASCII alphanumerics and commas** are accepted; anything else (e.g. `{中文}` /
/// `{a-b}` / `{}`) is treated as plain text, and that set falls through to the exact-name
/// branch. Falls back to `"Default"` if stripping leaves an empty name.
fn split_link_ids(title: &str) -> (Vec<String>, String) {
    let plain = || {
        let t = title.trim();
        if t.is_empty() { "Default" } else { t }.to_string()
    };
    let (Some(open), Some(close)) = (title.find('{'), title.find('}')) else {
        return (Vec::new(), plain());
    };
    if close < open {
        return (Vec::new(), plain());
    }
    let inner = &title[open + 1..close];
    if inner.is_empty() || !inner.chars().all(|c| c.is_ascii_alphanumeric() || c == ',') {
        return (Vec::new(), plain());
    }
    let ids: Vec<String> = inner
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return (Vec::new(), plain());
    }
    let stripped = format!("{}{}", &title[..open], &title[close + 1..]);
    let name = stripped.trim();
    let name = if name.is_empty() { "Default" } else { name };
    (ids, name.to_string())
}

/// The target set for a given identifier (last write wins on duplicate ids, matching
/// vendor's map assignment).
struct LinkTarget {
    set_id: usize,
}

/// Builds an "identifier → set" map; sets with no identifier go into the plain-name set.
fn index_links(sets: &[SetRef]) -> (std::collections::HashMap<String, LinkTarget>, Vec<String>) {
    let mut links = std::collections::HashMap::new();
    let mut plain = Vec::new();
    for set in sets {
        let (ids, _name) = split_link_ids(&set.title);
        if ids.is_empty() {
            plain.push(set.title.clone());
        } else {
            for id in ids {
                links.insert(id, LinkTarget { set_id: set.id });
            }
        }
    }
    (links, plain)
}

/// Derives all loadouts from the set lists (order: exact-name matches first, then
/// identifier bindings — matches vendor).
pub fn derive_loadouts(sets: &BuildSets) -> Vec<Loadout> {
    // Single-set exemption: a category with only one set (or none) doesn't participate in matching.
    let one_item = sets.items.len() <= 1;
    let one_skill = sets.skills.len() <= 1;

    let (item_links, item_plain) = index_links(&sets.items);
    let (skill_links, skill_plain) = index_links(&sets.skills);

    let mut out = Vec::new();

    // 1) Exact same title: for tree specs without an identifier, require a same-named
    //    set to exist in each category (exempt categories excluded).
    for spec in &sets.trees {
        let (ids, _) = split_link_ids(&spec.title);
        if !ids.is_empty() {
            continue; // Identifiers are handled in the next pass.
        }
        let item = sets.items.iter().find(|s| s.title == spec.title);
        let skill = sets.skills.iter().find(|s| s.title == spec.title);
        if (one_item || item_plain.contains(&spec.title))
            && (one_skill || skill_plain.contains(&spec.title))
        {
            out.push(Loadout {
                name: spec.title.clone(),
                tree: spec.id,
                item: if one_item { None } else { item.map(|s| s.id) },
                skill: if one_skill { None } else { skill.map(|s| s.id) },
            });
        }
    }

    // 2) Identifier binding: match each of the tree spec's identifiers individually
    for spec in &sets.trees {
        let (ids, name) = split_link_ids(&spec.title);
        for link_id in ids {
            let item = item_links.get(&link_id);
            let skill = skill_links.get(&link_id);
            if (one_item || item.is_some()) && (one_skill || skill.is_some()) {
                out.push(Loadout {
                    name: format!("{name} {{{link_id}}}"),
                    tree: spec.id,
                    item: if one_item {
                        None
                    } else {
                        item.map(|t| t.set_id)
                    },
                    skill: if one_skill {
                        None
                    } else {
                        skill.map(|t| t.set_id)
                    },
                });
            }
        }
    }

    out
}

/// The set document order to select for each category in one switch (`None` = leave the XML's existing active value untouched).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetSelection {
    pub tree: Option<usize>,
    pub item: Option<usize>,
    pub skill: Option<usize>,
}

impl From<&Loadout> for SetSelection {
    fn from(l: &Loadout) -> Self {
        Self {
            tree: Some(l.tree),
            item: l.item,
            skill: l.skill,
        }
    }
}

/// Rewrites the three active attributes in the build XML and returns the new XML —
/// **this is how switching a loadout is implemented**.
///
/// Why edit the XML instead of adding a parameter to the parse functions: `parse_build`
/// has six sub-parsers each reading an active attribute, and threading a parameter
/// through all of them would cut across the whole parse chain (which is where parity is
/// hardest fought). Instead this only replaces a single number on the three
/// **structural tags** `<Tree activeSpec>` / `<Items activeItemSet>` /
/// `<Skills activeSkillSet>`, leaving every other byte untouched — item text / Notes /
/// escaping are never touched, and `parse_build`'s behavior is byte-for-byte unchanged
/// when no selection is passed.
pub fn select_sets(xml: &str, sel: &SetSelection) -> String {
    let mut out = xml.to_string();
    for (elem, attr, value) in [
        ("<Tree", "activeSpec", sel.tree),
        ("<Items", "activeItemSet", sel.item),
        ("<Skills", "activeSkillSet", sel.skill),
    ] {
        let Some(n) = value else { continue };
        out = set_tag_attr(&out, elem, attr, n);
    }
    out
}

/// Reads the XML's current active triple (`None` when an attribute is missing; the
/// caller falls back to 1). Used to look up "which loadout is currently active" after switching.
pub fn active_selection(xml: &str) -> SetSelection {
    SetSelection {
        tree: read_tag_attr(xml, "<Tree", "activeSpec"),
        item: read_tag_attr(xml, "<Items", "activeItemSet"),
        skill: read_tag_attr(xml, "<Skills", "activeSkillSet"),
    }
}

/// Reads a numeric attribute on `elem`'s start tag.
fn read_tag_attr(xml: &str, elem: &str, attr: &str) -> Option<usize> {
    let start = xml.match_indices(elem).find_map(|(i, _)| {
        let rest = &xml[i + elem.len()..];
        rest.starts_with([' ', '\t', '\r', '\n', '>', '/'])
            .then_some(i)
    })?;
    let end = start + xml[start..].find('>')?;
    let tag = &xml[start..end];
    let at = tag.find(&format!("{attr}=\""))? + attr.len() + 2;
    let ve = tag[at..].find('"')?;
    tag[at..at + ve].parse().ok()
}

/// Sets `attr` to `n` inside `elem`'s start tag (inserts it if missing). Returns the
/// input unchanged if the tag isn't found.
fn set_tag_attr(xml: &str, elem: &str, attr: &str, n: usize) -> String {
    // Locate the start tag: `<Tree` must be immediately followed by whitespace or `>`,
    // to avoid matching `<TreeView`.
    let Some(start) = xml.match_indices(elem).find_map(|(i, _)| {
        let rest = &xml[i + elem.len()..];
        rest.starts_with([' ', '\t', '\r', '\n', '>', '/'])
            .then_some(i)
    }) else {
        return xml.to_string();
    };
    let Some(end_rel) = xml[start..].find('>') else {
        return xml.to_string();
    };
    let end = start + end_rel;
    let tag = &xml[start..end];

    let needle = format!("{attr}=\"");
    let replaced = match tag.find(&needle) {
        Some(at) => {
            let vs = at + needle.len();
            let Some(ve_rel) = tag[vs..].find('"') else {
                return xml.to_string();
            };
            format!("{}{n}{}", &tag[..vs], &tag[vs + ve_rel..])
        }
        // Attribute missing: insert it right after the element name (`<Tree` → `<Tree activeSpec="2"`).
        None => format!(
            "{}{} {attr}=\"{n}\"{}",
            &tag[..elem.len()],
            "",
            &tag[elem.len()..]
        ),
    };
    format!("{}{replaced}{}", &xml[..start], &xml[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(id: usize, title: &str) -> SetRef {
        SetRef {
            id,
            title: title.to_string(),
        }
    }

    #[test]
    fn splits_brace_identifier_and_strips_it_from_the_name() {
        // Arrange / Act
        let (ids, name) = split_link_ids("升级期 {lvl30}");

        // Assert
        assert_eq!(ids, vec!["lvl30"]);
        assert_eq!(name, "升级期");
    }

    #[test]
    fn splits_comma_separated_identifiers_so_one_set_joins_several_loadouts() {
        let (ids, name) = split_link_ids("Shared {lvl30,lvl50}");
        assert_eq!(ids, vec!["lvl30", "lvl50"]);
        assert_eq!(name, "Shared");
    }

    #[test]
    fn rejects_non_alphanumeric_identifiers_as_plain_text() {
        // vendor's regex %{([%w,]+)%} only accepts alphanumerics and commas — CJK text
        // and hyphens don't count as identifiers.
        assert_eq!(split_link_ids("阶段 {中文}").0, Vec::<String>::new());
        assert_eq!(split_link_ids("phase {a-b}").0, Vec::<String>::new());
        assert_eq!(split_link_ids("empty {}").0, Vec::<String>::new());
    }

    #[test]
    fn falls_back_to_default_when_stripping_leaves_an_empty_name() {
        let (ids, name) = split_link_ids("{solo}");
        assert_eq!(ids, vec!["solo"]);
        assert_eq!(name, "Default");
    }

    #[test]
    fn rewrites_existing_active_attributes() {
        // Arrange
        let xml = r#"<PathOfBuilding2><Tree activeSpec="1"><Spec/></Tree><Items activeItemSet="1" showStatDifferences="true"/><Skills activeSkillSet="1"/></PathOfBuilding2>"#;

        // Act
        let out = select_sets(
            xml,
            &SetSelection {
                tree: Some(2),
                item: Some(3),
                skill: Some(4),
            },
        );

        // Assert
        assert!(out.contains(r#"<Tree activeSpec="2">"#));
        assert!(out.contains(r#"activeItemSet="3""#));
        assert!(out.contains(r#"activeSkillSet="4""#));
        assert!(
            out.contains(r#"showStatDifferences="true""#),
            "the tag's other attributes are preserved"
        );
    }

    #[test]
    fn inserts_the_attribute_when_missing() {
        let xml = "<PathOfBuilding2><Tree><Spec/></Tree></PathOfBuilding2>";
        let out = select_sets(
            xml,
            &SetSelection {
                tree: Some(2),
                ..Default::default()
            },
        );
        assert!(out.contains(r#"<Tree activeSpec="2">"#), "got: {out}");
    }

    #[test]
    fn leaves_everything_else_byte_identical() {
        // Item text contains newlines and special characters — switching must never touch them.
        let xml = "<PathOfBuilding2><Tree activeSpec=\"1\"/><Items activeItemSet=\"1\"><Item id=\"1\">Rarity: RARE\nFoo &amp; Bar\n\"quoted\"</Item></Items></PathOfBuilding2>";
        let out = select_sets(
            xml,
            &SetSelection {
                tree: Some(5),
                ..Default::default()
            },
        );
        assert!(out.contains("Rarity: RARE\nFoo &amp; Bar\n\"quoted\""));
        assert_eq!(out.replace(r#"activeSpec="5""#, r#"activeSpec="1""#), xml);
    }

    #[test]
    fn none_selection_is_a_no_op() {
        let xml = r#"<PathOfBuilding2><Tree activeSpec="1"/></PathOfBuilding2>"#;
        assert_eq!(select_sets(xml, &SetSelection::default()), xml);
    }

    #[test]
    fn does_not_match_a_longer_element_name() {
        // `<TreeView>` is not `<Tree>` — prefix matching must check the following character.
        let xml = r#"<PathOfBuilding2><TreeView activeSpec="9"/><Tree activeSpec="1"/></PathOfBuilding2>"#;
        let out = select_sets(
            xml,
            &SetSelection {
                tree: Some(7),
                ..Default::default()
            },
        );
        assert!(
            out.contains(r#"<TreeView activeSpec="9""#),
            "TreeView should not be changed: {out}"
        );
        assert!(out.contains(r#"<Tree activeSpec="7""#));
    }

    #[test]
    fn matches_loadout_by_identical_titles_across_all_three() {
        // Arrange
        let sets = BuildSets {
            trees: vec![set(1, "Leveling"), set(2, "Mapping")],
            items: vec![set(1, "Leveling"), set(2, "Mapping")],
            skills: vec![set(1, "Leveling"), set(2, "Mapping")],
        };

        // Act
        let loadouts = derive_loadouts(&sets);

        // Assert
        assert_eq!(loadouts.len(), 2);
        assert_eq!(loadouts[0].name, "Leveling");
        assert_eq!(
            (loadouts[0].tree, loadouts[0].item, loadouts[0].skill),
            (1, Some(1), Some(1))
        );
        assert_eq!(
            (loadouts[1].tree, loadouts[1].item, loadouts[1].skill),
            (2, Some(2), Some(2))
        );
    }

    #[test]
    fn matches_loadout_by_identifier_even_when_display_names_differ() {
        let sets = BuildSets {
            trees: vec![set(1, "升级期 {lvl30}"), set(2, "大后期 {end}")],
            items: vec![set(1, "便宜装 {lvl30}"), set(2, "毕业装 {end}")],
            skills: vec![set(1, "起手技能 {lvl30}"), set(2, "主流派 {end}")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 2);
        // The group name takes the tree's name + identifier (matches vendor).
        assert_eq!(loadouts[0].name, "升级期 {lvl30}");
        assert_eq!(
            (loadouts[0].tree, loadouts[0].item, loadouts[0].skill),
            (1, Some(1), Some(1))
        );
        assert_eq!(loadouts[1].name, "大后期 {end}");
    }

    #[test]
    fn one_set_can_belong_to_several_loadouts_via_comma_list() {
        let sets = BuildSets {
            trees: vec![set(1, "早期 {a}"), set(2, "中期 {b}")],
            items: vec![set(1, "过渡装 {a,b}"), set(2, "别的 {z}")],
            skills: vec![set(1, "技能一 {a}"), set(2, "技能二 {b}")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 2);
        // The same item set is shared by two loadouts.
        assert_eq!(loadouts[0].item, Some(1));
        assert_eq!(loadouts[1].item, Some(1));
    }

    #[test]
    fn exempts_a_category_that_has_only_one_set() {
        // Only one skill set → skill is exempt from matching, but tree+item can still form a group.
        let sets = BuildSets {
            trees: vec![set(1, "Leveling"), set(2, "Mapping")],
            items: vec![set(1, "Leveling"), set(2, "Mapping")],
            skills: vec![set(1, "Default")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 2);
        assert_eq!(
            loadouts[0].skill, None,
            "single-set exemption: no skill set specified"
        );
        assert_eq!(loadouts[0].item, Some(1));
    }

    #[test]
    fn skips_a_tree_whose_counterparts_are_missing() {
        // The Mapping tree has no matching item set → no group forms (vendor "only exact match").
        let sets = BuildSets {
            trees: vec![set(1, "Leveling"), set(2, "Mapping")],
            items: vec![set(1, "Leveling"), set(2, "别的名字")],
            skills: vec![set(1, "Leveling"), set(2, "Mapping")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 1);
        assert_eq!(loadouts[0].name, "Leveling");
    }

    #[test]
    fn single_set_build_yields_one_loadout_with_everything_exempt() {
        // The most common real build: every category has exactly one set → a single Default group, everything exempt.
        let sets = BuildSets {
            trees: vec![set(1, "Default")],
            items: vec![set(1, "Default")],
            skills: vec![set(1, "Default")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 1);
        assert_eq!(
            loadouts[0],
            Loadout {
                name: "Default".to_string(),
                tree: 1,
                item: None,
                skill: None,
            }
        );
    }
}

#[cfg(test)]
mod xml_tests {
    use super::*;
    use crate::xml_build::parse_build_sets;

    /// A real PoB2 build from the repo: exactly one set per category → a single fully-exempt Default group.
    #[test]
    fn real_build_yields_a_single_default_loadout() {
        // Arrange
        let xml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo-bd-test/builds/witch-blood-mage-coiling-bolts/decoded.xml"
        ))
        .expect("read the real build XML");

        // Act
        let sets = parse_build_sets(&xml).expect("parse the set list");
        let loadouts = derive_loadouts(&sets);

        // Assert
        assert_eq!(
            (sets.trees.len(), sets.items.len(), sets.skills.len()),
            (1, 1, 1)
        );
        assert_eq!(
            sets.items[0].title, "Default",
            "ItemSet has its own title=\"Default\""
        );
        assert_eq!(loadouts.len(), 1);
        assert_eq!(loadouts[0].tree, 1);
    }

    /// Round trip with multiple sets + identifier binding: derives two groups directly from XML.
    #[test]
    fn multi_set_xml_derives_identifier_bound_loadouts() {
        let xml = r#"<PathOfBuilding2>
  <Tree activeSpec="1">
    <Spec title="升级期 {lvl30}" nodes="1,2"/>
    <Spec title="大后期 {end}" nodes="3,4"/>
  </Tree>
  <Skills activeSkillSet="1">
    <SkillSet id="1" title="起手 {lvl30}"/>
    <SkillSet id="2" title="成型 {end}"/>
  </Skills>
  <Items activeItemSet="1">
    <ItemSet id="1" title="便宜装 {lvl30}"/>
    <ItemSet id="2" title="毕业装 {end}"/>
  </Items>
</PathOfBuilding2>"#;

        let sets = parse_build_sets(xml).expect("parse");
        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 2);
        assert_eq!(loadouts[0].name, "升级期 {lvl30}");
        assert_eq!(
            (loadouts[0].tree, loadouts[0].item, loadouts[0].skill),
            (1, Some(1), Some(1))
        );
        assert_eq!(
            (loadouts[1].tree, loadouts[1].item, loadouts[1].skill),
            (2, Some(2), Some(2))
        );
    }
}
