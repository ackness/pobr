//! Writes a "single-set edit result" back into a multi-set build XML — so exporting
//! doesn't lose the other loadouts.
//!
//! `xml_write::write_build_xml` **fully regenerates** a single-set XML from the edit
//! state. If that were used as the export output directly, importing a multi-set build
//! and exporting once would wipe out every other Spec / SkillSet / ItemSet, along with
//! their `title` (the basis loadout binding relies on).
//!
//! [`merge_active_sets`] therefore works the other way around: it takes the **original
//! XML as the base** and only replaces the set that active points to with the edit
//! result, leaving every other byte untouched. Same idea as
//! [`crate::loadout::select_sets`] — the multi-set data always lives in the XML, and the
//! edit state is just one slice of it.
//!
//! # Item pool
//!
//! `<Item id>` is a **global pool** under `<Items>`, shared by reference (`itemId`)
//! across multiple ItemSets. So the edited result's items can't overwrite the pool —
//! they get **appended** to the end of the pool and renumbered, with that set's Slot
//! `itemId` re-pointed accordingly. Old entries stay even if nothing references them
//! anymore — PoB2's own pool is append-only too, and deleting entries would scramble the
//! references of other ItemSets.

use crate::loadout::active_selection;

/// Merges `edited` (a single-set XML) back into `base` (possibly multi-set), replacing
/// only the set that `base` has as active.
///
/// If `base` has no matching element, that category is skipped as-is; if both sides
/// lack it, this degrades to returning `edited` (a hand-built build has no original XML
/// to merge into).
pub fn merge_active_sets(base: &str, edited: &str) -> String {
    let sel = active_selection(base);
    let mut out = base.to_string();

    // Tree / skills: self-contained elements, replaced wholesale, keeping the original title/id attributes
    for (tag, idx) in [
        ("Spec", sel.tree.unwrap_or(1)),
        ("SkillSet", sel.skill.unwrap_or(1)),
    ] {
        let (Some(new_el), Some(old)) = (nth_element(edited, tag, 1), nth_element(&out, tag, idx))
        else {
            continue;
        };
        let merged = merge_attrs(&out[old.clone()], &edited[new_el], tag);
        out.replace_range(old, &merged);
    }

    // Items: append to the item pool (renumbered) first, then replace this ItemSet
    let base_max_id = max_item_id(&out);
    let renumbered = renumber_items(edited, base_max_id);
    if let Some(new_set) = nth_element(&renumbered, "ItemSet", 1)
        && let Some(old) = nth_element(&out, "ItemSet", sel.item.unwrap_or(1))
    {
        let merged = merge_attrs(&out[old.clone()], &renumbered[new_set], "ItemSet");
        out.replace_range(old, &merged);
    }
    // Append the edited result's `<Item>` blocks to the end of the pool (right before the first `<ItemSet`).
    let blocks: String = item_blocks(&renumbered).concat();
    if !blocks.is_empty()
        && let Some(at) = out.find("<ItemSet")
    {
        // Match the indentation of the line at the insertion point.
        let indent = out[..at].rfind('\n').map_or(0, |nl| at - nl - 1);
        let pad = " ".repeat(indent);
        out.insert_str(at, &format!("{blocks}{pad}"));
    }

    out
}

/// Locates the byte range of the `n`-th (1-based) `<tag …>…</tag>` or self-closing `<tag …/>`.
///
/// These tags (Spec / SkillSet / ItemSet) never self-nest, so there's no need for
/// balanced-depth counting; but `</SkillSet>` and `</Skill>` share a prefix, so the
/// closing tag must match the full name.
fn nth_element(xml: &str, tag: &str, n: usize) -> Option<std::ops::Range<usize>> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut seen = 0;
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&open) {
        let start = from + rel;
        let after = &xml[start + open.len()..];
        // `<Spec` should not match `<SpecFoo`.
        if !after.starts_with([' ', '\t', '\r', '\n', '>', '/']) {
            from = start + open.len();
            continue;
        }
        seen += 1;
        let tag_end = start + xml[start..].find('>')?;
        let self_closing = xml[start..=tag_end].ends_with("/>");
        let end = if self_closing {
            tag_end + 1
        } else {
            start + xml[start..].find(&close)? + close.len()
        };
        if seen == n {
            return Some(start..end);
        }
        from = end;
    }
    None
}

/// Replaces `old_el`'s content with `new_el`'s, but **keeps the attributes only
/// `old_el` has** (`title` / `id`, the basis for loadout binding) — the edit state
/// doesn't carry these, and overwriting them outright would sever the binding.
fn merge_attrs(old_el: &str, new_el: &str, tag: &str) -> String {
    let Some(old_end) = old_el.find('>') else {
        return new_el.to_string();
    };
    let Some(new_end) = new_el.find('>') else {
        return new_el.to_string();
    };
    let old_open = &old_el[..old_end];
    let new_open = &new_el[..new_end];

    let mut merged = new_open.to_string();
    for (name, value) in attrs(old_open) {
        if !has_attr(new_open, &name) {
            merged.push_str(&format!(" {name}=\"{value}\""));
        }
    }
    let _ = tag;
    format!("{merged}{}", &new_el[new_end..])
}

/// Splits a start tag's attribute list (`name="value"`; values contain no `"` — PoB
/// already escapes them on write).
fn attrs(open_tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = open_tag;
    while let Some(eq) = rest.find("=\"") {
        let name: String = rest[..eq]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let vs = eq + 2;
        let Some(ve) = rest[vs..].find('"') else {
            break;
        };
        if !name.is_empty() {
            out.push((name, rest[vs..vs + ve].to_string()));
        }
        rest = &rest[vs + ve + 1..];
    }
    out
}

fn has_attr(open_tag: &str, name: &str) -> bool {
    open_tag.contains(&format!("{name}=\""))
}

/// The largest existing `<Item id>` in the `<Items>` pool (0 for an empty pool).
fn max_item_id(xml: &str) -> usize {
    let mut max = 0;
    let mut from = 0;
    while let Some(rel) = xml[from..].find("<Item id=\"") {
        let vs = from + rel + "<Item id=\"".len();
        let Some(ve) = xml[vs..].find('"') else { break };
        if let Ok(n) = xml[vs..vs + ve].parse::<usize>() {
            max = max.max(n);
        }
        from = vs + ve;
    }
    max
}

/// Adds `offset` to every `<Item id>` and `<Slot itemId>` in `edited`, to avoid
/// colliding with the base document's pool numbers.
fn renumber_items(edited: &str, offset: usize) -> String {
    let mut out = edited.to_string();
    for (needle, _) in [("<Item id=\"", 0), ("itemId=\"", 0)] {
        let mut from = 0;
        let mut next = String::with_capacity(out.len());
        while let Some(rel) = out[from..].find(needle) {
            let vs = from + rel + needle.len();
            let Some(ve) = out[vs..].find('"') else { break };
            let raw = &out[vs..vs + ve];
            next.push_str(&out[from..vs]);
            match raw.parse::<usize>() {
                // itemId="0" is the empty-slot sentinel and is not renumbered.
                Ok(0) | Err(_) => next.push_str(raw),
                Ok(n) => next.push_str(&(n + offset).to_string()),
            }
            from = vs + ve;
        }
        next.push_str(&out[from..]);
        out = next;
    }
    out
}

/// Extracts every `<Item id=…>…</Item>` block (with a trailing newline, ready to concatenate directly).
fn item_blocks(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut n = 1;
    while let Some(r) = nth_element(xml, "Item", n) {
        out.push(format!("{}\n", &xml[r]));
        n += 1;
    }
    out
}

/// The three set categories that can form a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    Tree,
    Skill,
    Item,
}

impl SetKind {
    fn tag(self) -> &'static str {
        match self {
            SetKind::Tree => "Spec",
            SetKind::Skill => "SkillSet",
            SetKind::Item => "ItemSet",
        }
    }
}

/// Duplicates the `index`-th set and appends it to the end of its category, setting the
/// new set's `title` to `title`.
///
/// Used for "create a new stage based on the current group" — duplicating rather than
/// creating an empty set, because PoB2's `CustomLoadout` is also duplicate-based
/// (`Build.lua:790 CopyTree` / `CopyItemSet`), and an empty tree/item set built from
/// scratch isn't useful to the user. Returns `None` if the source set doesn't exist.
pub fn duplicate_set(xml: &str, kind: SetKind, index: usize, title: &str) -> Option<String> {
    let tag = kind.tag();
    let src = nth_element(xml, tag, index)?;
    let copy = set_title(&xml[src], tag, title);
    // Append after the last element of the same category, keeping document order = selection order.
    let mut last = None;
    let mut n = 1;
    while let Some(r) = nth_element(xml, tag, n) {
        last = Some(r);
        n += 1;
    }
    let at = last?.end;
    let mut out = xml.to_string();
    out.insert_str(at, &format!("\n{copy}"));
    Some(normalize_ids(&out, kind))
}

/// Changes the `index`-th set's `title` (inserts it if the attribute is missing).
/// Returns `None` if that set doesn't exist.
pub fn rename_set(xml: &str, kind: SetKind, index: usize, title: &str) -> Option<String> {
    let tag = kind.tag();
    let r = nth_element(xml, tag, index)?;
    let renamed = set_title(&xml[r.clone()], tag, title);
    let mut out = xml.to_string();
    out.replace_range(r, &renamed);
    Some(out)
}

/// Removes the `index`-th set. Deleting the last remaining set isn't allowed (PoB2 also
/// always keeps at least one), returns `None` in that case.
pub fn remove_set(xml: &str, kind: SetKind, index: usize) -> Option<String> {
    let tag = kind.tag();
    nth_element(xml, tag, 2)?; // Not allowed to remove when only one set is left
    let r = nth_element(xml, tag, index)?;
    let mut out = xml.to_string();
    out.replace_range(r, "");
    Some(normalize_ids(&out, kind))
}

/// Renumbers the `id` attribute of this category's elements to match document order (1..n).
///
/// **Required**: `<SkillSet>` / `<ItemSet>` selection is matched by the `id`
/// **attribute** (`xml_build::parse_socket_groups`), while the loadout list numbers by
/// **document order**. Duplicating without renumbering the id would produce two
/// `id="1"`s, so switching to the second group by `id="2"` would find no such set and
/// skills/items would go blank; deleting would leave a gap, misaligning the two
/// numbering schemes for every set after it. Renumbering keeps the two schemes
/// permanently equal.
///
/// `<Spec>` has no id attribute (it's already selected by document order), so it's returned unchanged.
fn normalize_ids(xml: &str, kind: SetKind) -> String {
    if kind == SetKind::Tree {
        return xml.to_string();
    }
    let tag = kind.tag();
    let mut out = xml.to_string();
    let mut n = 1;
    while let Some(r) = nth_element(&out, tag, n) {
        let el = &out[r.clone()];
        let end = el.find('>').unwrap_or(el.len());
        let open = &el[..end];
        let new_open = match open.find("id=\"") {
            Some(at) => {
                let vs = at + "id=\"".len();
                match open[vs..].find('"') {
                    Some(ve) => format!("{}{n}{}", &open[..vs], &open[vs + ve..]),
                    None => open.to_string(),
                }
            }
            None => format!("<{tag} id=\"{n}\"{}", &open[1 + tag.len()..]),
        };
        let replaced = format!("{new_open}{}", &el[end..]);
        out.replace_range(r, &replaced);
        n += 1;
    }
    out
}

/// Sets `title` on an element's start tag (replaces it if already present).
fn set_title(element: &str, tag: &str, title: &str) -> String {
    let end = element.find('>').unwrap_or(element.len());
    let open = &element[..end];
    let escaped = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    let new_open = match open.find("title=\"") {
        Some(at) => {
            let vs = at + "title=\"".len();
            match open[vs..].find('"') {
                Some(ve) => format!("{}{escaped}{}", &open[..vs], &open[vs + ve..]),
                None => open.to_string(),
            }
        }
        None => format!("<{tag} title=\"{escaped}\"{}", &open[1 + tag.len()..]),
    };
    format!("{new_open}{}", &element[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exporting after editing one of two sets: the other set, title included, must be kept unchanged.
    #[test]
    fn keeps_the_other_sets_and_their_titles() {
        // Arrange
        let base = r#"<PathOfBuilding2>
<Tree activeSpec="1"><Spec title="早期 {a}" nodes="1,2"/><Spec title="后期 {b}" nodes="3,4"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1" title="早期 {a}"><Skill enabled="true"/></SkillSet><SkillSet id="2" title="后期 {b}"><Skill enabled="false"/></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">OLD</Item><ItemSet id="1" title="早期 {a}"><Slot name="Belt" itemId="1"/></ItemSet><ItemSet id="2" title="后期 {b}"><Slot name="Belt" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#;
        let edited = r#"<PathOfBuilding2>
<Tree activeSpec="1"><Spec nodes="9,9,9"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" slot="X"/></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">NEW</Item><ItemSet id="1"><Slot name="Belt" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#;

        // Act
        let out = merge_active_sets(base, edited);

        // Assert: the other set and all titles are kept
        assert!(
            out.contains(r#"title="后期 {b}""#),
            "另一套的 title 丢了：{out}"
        );
        assert_eq!(out.matches("<Spec").count(), 2, "Spec 应仍为两套");
        assert_eq!(out.matches("<SkillSet").count(), 2);
        assert_eq!(out.matches("<ItemSet").count(), 2);
        // The edited set has been updated
        assert!(out.contains(r#"nodes="9,9,9""#), "第一套树未更新");
        assert!(
            out.contains(r#"title="早期 {a}""#),
            "编辑套的 title 也要保留"
        );
        // The new item is appended, the old item is kept
        assert!(out.contains("NEW") && out.contains("OLD"));
    }

    #[test]
    fn renumbers_appended_items_so_ids_do_not_collide() {
        let base = r#"<Items activeItemSet="1"><Item id="1">A</Item><Item id="2">B</Item><ItemSet id="1"><Slot name="Belt" itemId="2"/></ItemSet></Items>"#;
        let edited = r#"<Items activeItemSet="1"><Item id="1">C</Item><ItemSet id="1"><Slot name="Belt" itemId="1"/></ItemSet></Items>"#;

        let out = merge_active_sets(base, edited);

        // Base document's max id=2 → the edited item becomes 3, and the slot reference follows.
        assert!(out.contains(r#"<Item id="3">C</Item>"#), "{out}");
        assert!(out.contains(r#"itemId="3""#), "{out}");
        assert!(out.contains(r#"<Item id="1">A</Item>"#), "旧池保留");
    }

    #[test]
    fn switching_active_then_editing_writes_back_to_that_set() {
        // When active points to the second set, the edited result must land on the second set, not the first.
        let base =
            r#"<Tree activeSpec="2"><Spec title="A" nodes="1"/><Spec title="B" nodes="2"/></Tree>"#;
        let edited = r#"<Tree activeSpec="1"><Spec nodes="7,7"/></Tree>"#;

        let out = merge_active_sets(base, edited);

        assert!(
            out.contains(r#"<Spec title="A" nodes="1"/>"#),
            "第一套不该被动：{out}"
        );
        assert!(
            out.contains(r#"nodes="7,7""#) && out.contains(r#"title="B""#),
            "{out}"
        );
    }

    #[test]
    fn nth_element_does_not_confuse_skillset_with_skill() {
        // `</SkillSet>` and `</Skill>` share the same prefix — the closing tag must match the full name.
        let xml = r#"<SkillSet id="1"><Skill a="1"></Skill><Skill b="2"></Skill></SkillSet>"#;
        let r = nth_element(xml, "SkillSet", 1).expect("found");
        assert_eq!(&xml[r], xml, "应覆盖整个 SkillSet");
    }

    #[test]
    fn self_closing_elements_are_bounded_correctly() {
        let xml = r#"<Tree><Spec title="A" nodes="1"/><Spec title="B" nodes="2"/></Tree>"#;
        let r = nth_element(xml, "Spec", 2).expect("found");
        assert_eq!(&xml[r], r#"<Spec title="B" nodes="2"/>"#);
    }

    const TWO_SPECS: &str =
        r#"<Tree activeSpec="1"><Spec title="A" nodes="1"/><Spec title="B" nodes="2"/></Tree>"#;

    #[test]
    fn duplicating_a_set_appends_a_copy_with_the_new_title() {
        // Act
        let out = duplicate_set(TWO_SPECS, SetKind::Tree, 1, "C {c}").expect("dup");

        // Assert: three sets now, the copy shares the source content, has the new title, and sits at the end (document order = selection order).
        assert_eq!(out.matches("<Spec").count(), 3, "{out}");
        assert!(out.contains(r#"title="C {c}" nodes="1""#), "{out}");
        assert!(
            out.rfind("C {c}") > out.rfind(r#"title="B""#),
            "副本应在末尾"
        );
    }

    /// Regression pin: SkillSet/ItemSet selection matches by the `id` **attribute**;
    /// duplicating without renumbering the id would make looking up the second group by
    /// `id="2"` find no such set → skills/items go entirely blank (observed in practice
    /// as DPS dropping to zero).
    #[test]
    fn duplicating_renumbers_ids_so_the_copy_is_selectable() {
        let xml =
            r#"<Skills activeSkillSet="1"><SkillSet id="1" title="A"><Skill/></SkillSet></Skills>"#;

        let out = duplicate_set(xml, SetKind::Skill, 1, "B").expect("dup");

        assert!(out.contains(r#"<SkillSet id="1" title="A""#), "{out}");
        assert!(
            out.contains(r#"<SkillSet id="2" title="B""#),
            "副本必须拿到新 id：{out}"
        );
    }

    #[test]
    fn removing_closes_the_id_gap() {
        // After removing a set in the middle, the id of every following set must shift down, or document order and id would stay permanently misaligned.
        let xml = r#"<Items><ItemSet id="1" title="A"/><ItemSet id="2" title="B"/><ItemSet id="3" title="C"/></Items>"#;

        let out = remove_set(xml, SetKind::Item, 2).expect("remove");

        assert!(out.contains(r#"<ItemSet id="1" title="A"/>"#), "{out}");
        assert!(
            out.contains(r#"<ItemSet id="2" title="C"/>"#),
            "C 应补位到 id=2：{out}"
        );
        assert!(!out.contains(r#"id="3""#));
    }

    #[test]
    fn tree_specs_keep_no_id_attribute() {
        // `<Spec>` is already selected by document order and shouldn't get an id stuffed in.
        let out = duplicate_set(TWO_SPECS, SetKind::Tree, 1, "C").expect("dup");
        assert!(!out.contains("<Spec id="), "{out}");
    }

    #[test]
    fn renaming_sets_the_title_and_escapes_it() {
        let out = rename_set(TWO_SPECS, SetKind::Tree, 2, r#"Q&A "x""#).expect("rename");
        assert!(out.contains(r#"title="Q&amp;A &quot;x&quot;""#), "{out}");
        assert!(out.contains(r#"title="A""#), "另一套不动");
    }

    #[test]
    fn renaming_inserts_title_when_absent() {
        let xml = r#"<Tree><Spec nodes="1"/></Tree>"#;
        let out = rename_set(xml, SetKind::Tree, 1, "New").expect("rename");
        assert!(out.contains(r#"<Spec title="New" nodes="1"/>"#), "{out}");
    }

    #[test]
    fn removing_a_set_keeps_the_others() {
        let out = remove_set(TWO_SPECS, SetKind::Tree, 1).expect("remove");
        assert_eq!(out.matches("<Spec").count(), 1);
        assert!(out.contains(r#"title="B""#));
    }

    #[test]
    fn refuses_to_remove_the_last_set() {
        // PoB2 also always keeps at least one set — removing them all would leave the build with no carrier for tree/skills/items.
        let one = r#"<Tree><Spec title="A" nodes="1"/></Tree>"#;
        assert!(remove_set(one, SetKind::Tree, 1).is_none());
    }

    #[test]
    fn empty_slot_sentinel_is_not_renumbered() {
        // itemId="0" = an empty slot; adding an offset would turn it into a reference to a nonexistent item.
        let out = renumber_items(r#"<Slot name="Belt" itemId="0"/>"#, 5);
        assert_eq!(out, r#"<Slot name="Belt" itemId="0"/>"#);
    }
}
