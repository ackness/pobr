//! Loadout —— 天赋树 / 装备 / 技能的**成组切换**（PoB2 `Build.lua:617 SyncLoadouts`）。
//!
//! PoB2 不单独存 loadout：它从各 set 的 `title` 按命名约定**推导**出来，所以本
//! 特性零格式扩展、与 PoB2 双向兼容——在 PoB2 里建的组 pobr 认得，反之亦然。
//!
//! 两种绑定方式（vendor `SyncLoadouts` 逐条对应）：
//!
//! 1. **精确同名**：各类 set 的 `title` 完全相同即成组（`title="Mapping"` × 3）。
//! 2. **花括号标识**：`title="升级期 {lvl30}"` —— 靠 `{lvl30}` 绑定，各类显示名可
//!    不同；`{lvl30,lvl50}` 逗号分隔表示一套同时属于多个组。
//!
//! **单套豁免**：某一类只有一套（或没有）时，该类不参与匹配（vendor 的
//! `oneItem` / `oneSkill`）。于是「只切树和装备、技能共用一套」无需为技能建组。
//!
//! # 与 vendor 的已知差异
//!
//! - **ConfigSet 不参与**：pobr 尚未解析多套 `<Config>`，等价于 vendor 的
//!   `oneConfig` 恒真。若 build 真有多套 ConfigSet，PoB2 会额外要求 config 也匹配，
//!   此处会多推导出组。接入多套 config 时补上即可。
//! - **树版本前缀不参与匹配**：vendor 在非最新 `treeVersion` 时把显示名前缀成
//!   `[0.4] Mapping` 再去匹配 item/skill 的纯 title（`Build.lua:695`），导致旧版本树
//!   的精确同名组匹配不上。这里只用纯 `title` 匹配——树版本全为最新时行为一致。

/// 一个 set 的引用：XML 文档序（1-based，即 `activeSpec` / `activeItemSet` /
/// `activeSkillSet` 的取值口径）+ 显示名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRef {
    /// 1-based 文档序。
    pub id: usize,
    /// `title` 属性原文；缺省为 `"Default"`（与 vendor `spec.title or "Default"` 一致）。
    pub title: String,
}

/// build XML 里各类 set 的清单（按文档序）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildSets {
    pub trees: Vec<SetRef>,
    pub items: Vec<SetRef>,
    pub skills: Vec<SetRef>,
}

/// 一个推导出的 loadout：组名 + 各类选中的 set 文档序。
///
/// `item` / `skill` 为 `None` 表示该类未参与绑定（单套豁免）——切换时保持原样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loadout {
    /// 展示名。标识符绑定时为 `"<去标识符的名字> {<标识符>}"`（同 vendor）。
    pub name: String,
    pub tree: usize,
    pub item: Option<usize>,
    pub skill: Option<usize>,
}

/// 拆出 `title` 里的 `{a,b}` 标识符与去掉标识符后的名字。
///
/// 等价 vendor 的 `string.match(title, "%{([%w,]+)%}")` + `gsub` + trim：仅接受
/// **ASCII 字母数字与逗号**，其余（如 `{中文}` / `{a-b}` / `{}`）视为普通文本，
/// 该 set 走精确同名分支。名字被清空时回退 `"Default"`。
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

/// 同一标识符下的目标 set（同 id 多套时后写覆盖，与 vendor 的 map 赋值一致）。
struct LinkTarget {
    set_id: usize,
}

/// 建立「标识符 → set」映射；无标识符的 set 归入纯名字集合。
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

/// 从 set 清单推导全部 loadout（顺序：先精确同名，后标识符绑定——同 vendor）。
pub fn derive_loadouts(sets: &BuildSets) -> Vec<Loadout> {
    // 单套豁免：该类只有一套（或没有）时不参与匹配。
    let one_item = sets.items.len() <= 1;
    let one_skill = sets.skills.len() <= 1;

    let (item_links, item_plain) = index_links(&sets.items);
    let (skill_links, skill_plain) = index_links(&sets.skills);

    let mut out = Vec::new();

    // ── 1) 精确同名：无标识符的 tree spec，要求各类存在同名 set（豁免类除外）──
    for spec in &sets.trees {
        let (ids, _) = split_link_ids(&spec.title);
        if !ids.is_empty() {
            continue; // 带标识符的走下一段
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

    // ── 2) 标识符绑定：按 tree spec 的标识符逐个匹配 ──
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

/// 一次切换要选中的各类 set 文档序（`None` = 保持 XML 原有 active 不动）。
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

/// 改写 build XML 里的三个 active 属性，返回新 XML——**切换 loadout 的实现方式**。
///
/// 之所以改 XML 而不是给解析函数加参数：`parse_build` 下挂六个各自读 active 属性
/// 的解析函数，逐个穿参会横切整条解析链（parity 的主战场）。这里只替换
/// `<Tree activeSpec>` / `<Items activeItemSet>` / `<Skills activeSkillSet>` 三个
/// **结构标签**上的一个数字，其余字节原样保留——物品文本 / Notes / 转义一概不碰，
/// 不传 selection 时 `parse_build` 的行为逐字不变。
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

/// 读出 XML 当前的 active 三元组（属性缺失时为 `None`，解析侧按 1 兜底）。
/// 供切换后反查「现在是哪个 loadout」。
pub fn active_selection(xml: &str) -> SetSelection {
    SetSelection {
        tree: read_tag_attr(xml, "<Tree", "activeSpec"),
        item: read_tag_attr(xml, "<Items", "activeItemSet"),
        skill: read_tag_attr(xml, "<Skills", "activeSkillSet"),
    }
}

/// 读 `elem` 开始标签上的数字属性。
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

/// 在 `elem` 开始标签内把 `attr` 设为 `n`（属性缺失则插入）。找不到该标签时原样返回。
fn set_tag_attr(xml: &str, elem: &str, attr: &str, n: usize) -> String {
    // 定位开始标签：`<Tree` 后必须紧跟空白或 `>`，避免命中 `<TreeView`。
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
        // 属性缺失：插在元素名之后（`<Tree` → `<Tree activeSpec="2"`）。
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
        // vendor 正则 %{([%w,]+)%} 只收字母数字与逗号——中文/连字符不算标识符。
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
            "同标签其余属性保留"
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
        // 物品文本含换行与特殊字符——切换绝不能碰它们。
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
        // `<TreeView>` 不是 `<Tree>`——前缀匹配必须看后一个字符。
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
            "TreeView 不该被改：{out}"
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
        // 组名取树的名字 + 标识符（vendor 口径）。
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
        // 同一件装备集被两个 loadout 共用。
        assert_eq!(loadouts[0].item, Some(1));
        assert_eq!(loadouts[1].item, Some(1));
    }

    #[test]
    fn exempts_a_category_that_has_only_one_set() {
        // 只有一套技能 → 技能不参与匹配，仍能按树+装备成组。
        let sets = BuildSets {
            trees: vec![set(1, "Leveling"), set(2, "Mapping")],
            items: vec![set(1, "Leveling"), set(2, "Mapping")],
            skills: vec![set(1, "Default")],
        };

        let loadouts = derive_loadouts(&sets);

        assert_eq!(loadouts.len(), 2);
        assert_eq!(loadouts[0].skill, None, "单套豁免：不指定技能集");
        assert_eq!(loadouts[0].item, Some(1));
    }

    #[test]
    fn skips_a_tree_whose_counterparts_are_missing() {
        // Mapping 树没有对应的装备集 → 不成组（vendor "only exact match"）。
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
        // 最常见的真实 build：各类都只有一套 → 一个 Default 组，全豁免。
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

    /// 仓库里的真实 PoB2 build：各类恰好一套 → 一个全豁免的 Default 组。
    #[test]
    fn real_build_yields_a_single_default_loadout() {
        // Arrange
        let xml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo-bd-test/builds/witch-blood-mage-coiling-bolts/decoded.xml"
        ))
        .expect("读取真实 build XML");

        // Act
        let sets = parse_build_sets(&xml).expect("解析 set 清单");
        let loadouts = derive_loadouts(&sets);

        // Assert
        assert_eq!(
            (sets.trees.len(), sets.items.len(), sets.skills.len()),
            (1, 1, 1)
        );
        assert_eq!(
            sets.items[0].title, "Default",
            "ItemSet 自带 title=\"Default\""
        );
        assert_eq!(loadouts.len(), 1);
        assert_eq!(loadouts[0].tree, 1);
    }

    /// 多套 + 标识符绑定的往返：从 XML 直接推导出两个组。
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

        let sets = parse_build_sets(xml).expect("解析");
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
