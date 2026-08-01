//! 把「单套编辑结果」写回多套 build XML —— 导出时不丢其余 loadout。
//!
//! `xml_write::write_build_xml` 从编辑态**全量生成**一份单套 XML。若直接拿它当
//! 导出产物，一个多套 build 导入后只要点一次导出，其余 Spec / SkillSet / ItemSet
//! 连同它们的 `title`（= loadout 绑定的依据）就全没了。
//!
//! [`merge_active_sets`] 因此反过来做：以**原始 XML 为底**，只把 active 指向的那
//! 一套替换成编辑结果，其余字节不动。与 [`crate::loadout::select_sets`] 同一思路
//! ——多套数据始终活在 XML 里，编辑态只是它的一个切片。
//!
//! # 物品池
//!
//! `<Item id>` 是 `<Items>` 下的**全局池**，多个 ItemSet 靠 `itemId` 共享引用。所以
//! 编辑结果的物品不能覆盖池子，而是整体**追加**到池尾并重新编号，再把该套 Slot 的
//! `itemId` 重指过去。旧条目即便没人引用也留着——PoB2 自己的池同样只增不减，删除
//! 会打乱其余 ItemSet 的引用。

use crate::loadout::active_selection;

/// 把 `edited`（单套 XML）合并回 `base`（可能多套），只替换 `base` 里 active 的那套。
///
/// `base` 无对应元素时按原样跳过该类；两边都缺时退化为返回 `edited`（手搓 build
/// 没有原始 XML 可合并）。
pub fn merge_active_sets(base: &str, edited: &str) -> String {
    let sel = active_selection(base);
    let mut out = base.to_string();

    // 天赋树 / 技能：自包含元素，整体替换，保留原 title/id 属性
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

    // 物品：先追加物品池（重编号），再替换该套 ItemSet
    let base_max_id = max_item_id(&out);
    let renumbered = renumber_items(edited, base_max_id);
    if let Some(new_set) = nth_element(&renumbered, "ItemSet", 1)
        && let Some(old) = nth_element(&out, "ItemSet", sel.item.unwrap_or(1))
    {
        let merged = merge_attrs(&out[old.clone()], &renumbered[new_set], "ItemSet");
        out.replace_range(old, &merged);
    }
    // 编辑结果的 `<Item>` 块整体追加到池尾（第一个 `<ItemSet` 之前）。
    let blocks: String = item_blocks(&renumbered).concat();
    if !blocks.is_empty()
        && let Some(at) = out.find("<ItemSet")
    {
        // 对齐插入点所在行的缩进。
        let indent = out[..at].rfind('\n').map_or(0, |nl| at - nl - 1);
        let pad = " ".repeat(indent);
        out.insert_str(at, &format!("{blocks}{pad}"));
    }

    out
}

/// 定位第 `n` 个（1-based）`<tag …>…</tag>` 或自闭合 `<tag …/>` 的字节范围。
///
/// 这几个标签（Spec / SkillSet / ItemSet）都不自嵌套，故无需平衡计数；但
/// `</SkillSet>` 与 `</Skill>` 是前缀关系，结束标签必须整名匹配。
fn nth_element(xml: &str, tag: &str, n: usize) -> Option<std::ops::Range<usize>> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut seen = 0;
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&open) {
        let start = from + rel;
        let after = &xml[start + open.len()..];
        // `<Spec` 不该命中 `<SpecFoo`。
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

/// 用 `new_el` 的内容替换 `old_el`，但**保留 `old_el` 独有的属性**（`title` / `id`
/// 等 loadout 绑定依据）——编辑态不携带这些，直接覆盖会切断绑定。
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

/// 拆开始标签的属性列表（`name="value"`，值内不含 `"`——PoB 写出时已转义）。
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

/// `<Items>` 池里现有的最大 `<Item id>`（空池为 0）。
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

/// 把 `edited` 里的 `<Item id>` 与 `<Slot itemId>` 整体加上 `offset`，避免与底稿
/// 池号冲突。
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
                // itemId="0" 是空槽哨兵，不参与重编号。
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

/// 取出全部 `<Item id=…>…</Item>` 块（含结尾换行，便于直接拼接）。
fn item_blocks(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut n = 1;
    while let Some(r) = nth_element(xml, "Item", n) {
        out.push(format!("{}\n", &xml[r]));
        n += 1;
    }
    out
}

/// 可成组的三类 set。
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

/// 复制第 `index` 套并追加到同类末尾，新套 `title` 设为 `title`。
///
/// 用于「基于当前组新建一个阶段」——复制而非新建空套，因为 PoB2 的 `CustomLoadout`
/// 也是复制语义（`Build.lua:790 CopyTree` / `CopyItemSet`），且从零建的空树/空装备
/// 集对用户没用。返回 `None` 表示源套不存在。
pub fn duplicate_set(xml: &str, kind: SetKind, index: usize, title: &str) -> Option<String> {
    let tag = kind.tag();
    let src = nth_element(xml, tag, index)?;
    let copy = set_title(&xml[src], tag, title);
    // 追加到最后一个同类元素之后，保持文档序 = 选中序。
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

/// 改第 `index` 套的 `title`（缺该属性则插入）。返回 `None` 表示该套不存在。
pub fn rename_set(xml: &str, kind: SetKind, index: usize, title: &str) -> Option<String> {
    let tag = kind.tag();
    let r = nth_element(xml, tag, index)?;
    let renamed = set_title(&xml[r.clone()], tag, title);
    let mut out = xml.to_string();
    out.replace_range(r, &renamed);
    Some(out)
}

/// 删掉第 `index` 套。最后一套不允许删（PoB2 同样保底留一套），返回 `None`。
pub fn remove_set(xml: &str, kind: SetKind, index: usize) -> Option<String> {
    let tag = kind.tag();
    nth_element(xml, tag, 2)?; // 只剩一套时不允许删
    let r = nth_element(xml, tag, index)?;
    let mut out = xml.to_string();
    out.replace_range(r, "");
    Some(normalize_ids(&out, kind))
}

/// 把该类元素的 `id` 属性重排为文档序（1..n）。
///
/// **必须做**：`<SkillSet>` / `<ItemSet>` 的选中是按 `id` **属性**匹配的
/// （`xml_build::parse_socket_groups`），而 loadout 清单按**文档序**编号。复制不改 id
/// 会产生两个 `id="1"`，切到第二组时按 `id="2"` 查无此集，技能/装备直接空掉；删除则
/// 会留下空洞，让其后各套的两种编号错位。重排后两种口径恒等。
///
/// `<Spec>` 没有 id 属性（本就按文档序选），原样返回。
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

/// 在元素的开始标签上设置 `title`（已有则替换）。
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

    /// 两套 build 编辑一套后导出：另一套连同 title 必须原样保留。
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

        // Assert：其余套与全部 title 保留
        assert!(
            out.contains(r#"title="后期 {b}""#),
            "另一套的 title 丢了：{out}"
        );
        assert_eq!(out.matches("<Spec").count(), 2, "Spec 应仍为两套");
        assert_eq!(out.matches("<SkillSet").count(), 2);
        assert_eq!(out.matches("<ItemSet").count(), 2);
        // 编辑套已更新
        assert!(out.contains(r#"nodes="9,9,9""#), "第一套树未更新");
        assert!(
            out.contains(r#"title="早期 {a}""#),
            "编辑套的 title 也要保留"
        );
        // 新物品追加、旧物品保留
        assert!(out.contains("NEW") && out.contains("OLD"));
    }

    #[test]
    fn renumbers_appended_items_so_ids_do_not_collide() {
        let base = r#"<Items activeItemSet="1"><Item id="1">A</Item><Item id="2">B</Item><ItemSet id="1"><Slot name="Belt" itemId="2"/></ItemSet></Items>"#;
        let edited = r#"<Items activeItemSet="1"><Item id="1">C</Item><ItemSet id="1"><Slot name="Belt" itemId="1"/></ItemSet></Items>"#;

        let out = merge_active_sets(base, edited);

        // 底稿最大 id=2 → 编辑物品变成 3，槽位引用同步。
        assert!(out.contains(r#"<Item id="3">C</Item>"#), "{out}");
        assert!(out.contains(r#"itemId="3""#), "{out}");
        assert!(out.contains(r#"<Item id="1">A</Item>"#), "旧池保留");
    }

    #[test]
    fn switching_active_then_editing_writes_back_to_that_set() {
        // active 指向第二套时，编辑结果要落到第二套而非第一套。
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
        // `</SkillSet>` 与 `</Skill>` 前缀相同——结束标签必须整名匹配。
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

        // Assert：三套，副本内容同源、title 为新名，且排在末尾（文档序 = 选中序）。
        assert_eq!(out.matches("<Spec").count(), 3, "{out}");
        assert!(out.contains(r#"title="C {c}" nodes="1""#), "{out}");
        assert!(
            out.rfind("C {c}") > out.rfind(r#"title="B""#),
            "副本应在末尾"
        );
    }

    /// 回归钉子：SkillSet/ItemSet 的选中按 `id` **属性**匹配，复制不重排 id 会让
    /// 第二组按 `id="2"` 查无此集 → 技能/装备整个空掉（实测表现为 DPS 归零）。
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
        // 删掉中间一套后，其后各套的 id 必须补位，否则文档序与 id 永久错位。
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
        // `<Spec>` 本就按文档序选中，不该被塞 id。
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
        // PoB2 同样保底留一套——删光会让 build 失去树/技能/装备的载体。
        let one = r#"<Tree><Spec title="A" nodes="1"/></Tree>"#;
        assert!(remove_set(one, SetKind::Tree, 1).is_none());
    }

    #[test]
    fn empty_slot_sentinel_is_not_renumbered() {
        // itemId="0" = 空槽，加偏移会变成一个不存在的引用。
        let out = renumber_items(r#"<Slot name="Belt" itemId="0"/>"#, 5);
        assert_eq!(out, r#"<Slot name="Belt" itemId="0"/>"#);
    }
}
