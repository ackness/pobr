//! Bonded modifiers of what an item's sockets hold.
//!
//! A Rune or an Idol gives the item it is socketed in its ordinary modifiers
//! and, on lines that start with `Bonded:`, modifiers that are in effect only
//! for a character that gains their benefit. The game states the two ways:
//!
//! - "Gain the benefits of Bonded modifiers on Runes and Idols", a passive:
//!   every Bonded modifier of the character's items.
//! - "Idols socketed in this item gain the benefits of their Bonded
//!   modifiers", a modifier of the item (the Fox Idol gives it to a Body
//!   Armour): those of the Idols in that item, and not those of its Runes.
//!
//! [`resolve_bonded_modifiers`] settles this once for a build. A Bonded line
//! in effect becomes an ordinary line of its item and the others are removed,
//! so everything that reads an item's lines (gem level and quality bonuses,
//! the item's local defences, the modifier parser) sees what applies.
//!
//! The item text does not say which socket a line comes from. The table of
//! socketables ([`BuildData::augments`]) does: it lists the lines each one
//! gives each class of item.

use std::collections::{HashMap, HashSet};

use pobr_data::catalog::RuneDef;
use pobr_data::item::Item;

use crate::build::Build;
use crate::build_data::BuildData;

use super::super::collect::granted_passive_defs;
use super::super::skill::resolve::clean_grant_text;

/// The marker of a Bonded modifier line.
const BONDED: &str = "bonded:";
/// The passive that gives every Bonded modifier.
const ALL_BONDED: &str = "gain the benefits of bonded modifiers on runes and idols";
/// The item modifier that gives those of the Idols in the item.
const IDOLS_BONDED: &str =
    "idols socketed in this item gain the benefits of their bonded modifiers";
/// The kind of a socketable that is an Idol.
const IDOL: &str = "Idol";

/// `build` with the Bonded modifiers of its items settled: the ones in effect
/// as ordinary lines, the others gone. `None` when no item has one.
pub(crate) fn resolve_bonded_modifiers(build: &Build, data: &BuildData) -> Option<Build> {
    if !build.items.values().any(has_bonded_line) {
        return None;
    }
    let all = gains_all_bonded(build, data);
    let mut resolved = build.clone();
    for item in resolved.items.values_mut() {
        if let Some(settled) = resolve_item(item, all, &data.augments) {
            *item = settled;
        }
    }
    Some(resolved)
}

/// Whether the character has the passive that gives every Bonded modifier:
/// allocated on the tree, or allocated by an item.
fn gains_all_bonded(build: &Build, data: &BuildData) -> bool {
    let states = |stat: &String| clean_grant_text(stat).trim() == ALL_BONDED;
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    build
        .tree
        .allocated_nodes
        .iter()
        .filter_map(|id| nodes.get(&id.0))
        .chain(granted_passive_defs(build, data))
        .any(|node| node.stats.iter().any(states))
}

/// The body of a Bonded modifier line; `None` for any other line.
fn bonded_body(text: &str) -> Option<&str> {
    let (marker, body) = text.trim_start().split_at_checked(BONDED.len())?;
    marker.eq_ignore_ascii_case(BONDED).then(|| body.trim())
}

fn item_lines(item: &Item) -> impl Iterator<Item = &String> {
    item.implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
}

fn has_bonded_line(item: &Item) -> bool {
    item_lines(item).any(|text| bonded_body(text).is_some())
}

/// A line with its numbers replaced, so that two rolls of one modifier, or the
/// sum of two, compare equal.
fn template(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_number = false;
    for c in text.trim().chars() {
        if c.is_ascii_digit() || (in_number && c == '.') {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// `item` with its Bonded lines settled; `None` when it has none.
fn resolve_item(item: &Item, all: bool, augments: &HashMap<String, RuneDef>) -> Option<Item> {
    if !has_bonded_line(item) {
        return None;
    }
    let idols_gain = item_lines(item).any(|text| clean_grant_text(text).trim() == IDOLS_BONDED);
    let of_idols = (!all && idols_gain).then(|| BondedLines::of(item, augments));
    let settle = |texts: &[String]| -> Vec<String> {
        texts
            .iter()
            .flat_map(|text| match bonded_body(text) {
                None => vec![text.clone()],
                Some(body) if all => vec![body.to_string()],
                Some(body) => of_idols
                    .as_ref()
                    .map_or_else(Vec::new, |lines| lines.of_idols(body)),
            })
            .collect()
    };
    Some(Item {
        implicit_texts: settle(&item.implicit_texts),
        modifier_texts: settle(&item.modifier_texts),
        enchant_texts: settle(&item.enchant_texts),
        ..item.clone()
    })
}

/// What the table of socketables says about the Bonded lines of one item.
struct BondedLines {
    /// The Bonded lines its Idols give, by template: each Idol's own line.
    idols: HashMap<String, Vec<String>>,
    /// The templates of the Bonded lines its other socketables give.
    others: HashSet<String>,
    /// Whether everything socketed in the item is an Idol the table has.
    only_idols: bool,
}

impl BondedLines {
    fn of(item: &Item, augments: &HashMap<String, RuneDef>) -> Self {
        let on_item: HashSet<String> = item_lines(item).map(|line| template(line)).collect();
        let mut lines = Self {
            idols: HashMap::new(),
            others: HashSet::new(),
            only_idols: !item.socketed.is_empty(),
        };
        for name in &item.socketed {
            let Some(augment) = augments.get(name) else {
                lines.only_idols = false;
                continue;
            };
            let is_idol = augment.slots.values().any(|slot| slot.kind == IDOL);
            lines.only_idols &= is_idol;
            for body in given_lines(augment, &on_item)
                .into_iter()
                .filter_map(bonded_body)
            {
                if is_idol {
                    lines
                        .idols
                        .entry(template(body))
                        .or_default()
                        .push(body.to_string());
                } else {
                    lines.others.insert(template(body));
                }
            }
        }
        lines
    }

    /// What the item's Bonded line `body` is worth from its Idols alone.
    fn of_idols(&self, body: &str) -> Vec<String> {
        let template = template(body);
        match (self.idols.get(&template), self.others.contains(&template)) {
            // Only Idols give it: the line is theirs as the item states it.
            (Some(_), false) => vec![body.to_string()],
            // An Idol and something else give it, and the item states their
            // sum: the Idols' share is each one's own line.
            (Some(lines), true) => lines.clone(),
            (None, true) => Vec::new(),
            // The table has no such line. It can only be an Idol's when
            // nothing else is socketed.
            (None, false) if self.only_idols => vec![body.to_string()],
            (None, false) => Vec::new(),
        }
    }
}

/// The lines `augment` gives the item whose line templates are `on_item`:
/// those of the class of item whose lines are all on it. When the table words
/// a line differently from the item, no class matches and every class counts,
/// each line once.
fn given_lines<'a>(augment: &'a RuneDef, on_item: &HashSet<String>) -> Vec<&'a str> {
    let all_on_item = |lines: &[String]| lines.iter().all(|line| on_item.contains(&template(line)));
    if let Some(class) = augment
        .slots
        .values()
        .filter(|class| all_on_item(&class.lines))
        .max_by_key(|class| class.lines.len())
    {
        return class.lines.iter().map(String::as_str).collect();
    }
    let mut seen = HashSet::new();
    augment
        .slots
        .values()
        .flat_map(|class| &class.lines)
        .filter(|line| seen.insert(template(line)))
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::RuneSlotDef;
    use pobr_data::item::{ItemBaseId, ItemRarity, RolledDefence};

    use super::*;

    const UNLOCK: &str = "Idols socketed in this item gain the benefits of their Bonded modifiers";

    fn class(kind: &str, lines: &[&str]) -> RuneSlotDef {
        RuneSlotDef {
            kind: kind.to_string(),
            required_level: None,
            limit: None,
            limit_id: None,
            socket_bound: false,
            can_socket_in_unique_items: true,
            can_socket_in_jewellery: true,
            can_socket_in_corrupted_sanctified: true,
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            rank: Vec::new(),
            stat_order: Vec::new(),
        }
    }

    /// A table with the Fox Idol, another Idol and two Runes, each with the
    /// lines it gives a Body Armour and a second class of item.
    fn augments() -> HashMap<String, RuneDef> {
        let entries: [(&str, &str, &[&str], &[&str]); 4] = [
            (
                "Fox Idol",
                "Idol",
                &[UNLOCK, "Bonded: +5% to Quality of all Skills"],
                &[
                    "50% increased Presence Area of Effect",
                    "Bonded: Minions have 30% increased Area of Effect",
                ],
            ),
            (
                "Stag Idol",
                "Idol",
                &["+30 to maximum Life", "Bonded: 3% increased maximum Life"],
                &["Bonded: 9% increased Spell Damage"],
            ),
            (
                "Medved's Tending",
                "Rune",
                &[
                    "Can roll Soul modifiers",
                    "Bonded: 3% increased maximum Life",
                    "Bonded: 3% increased maximum Mana",
                ],
                &[],
            ),
            (
                "Body Rune",
                "Rune",
                &["+40 to maximum Life", "Bonded: +20 to maximum Mana"],
                &[],
            ),
        ];
        entries
            .into_iter()
            .map(|(name, kind, body_armour, sceptre)| {
                let mut slots = BTreeMap::new();
                slots.insert("body armour".to_string(), class(kind, body_armour));
                if !sceptre.is_empty() {
                    slots.insert("sceptre".to_string(), class(kind, sceptre));
                }
                (
                    name.to_string(),
                    RuneDef {
                        name: name.to_string(),
                        slots,
                    },
                )
            })
            .collect()
    }

    fn item(socketed: &[&str], lines: &[&str]) -> Item {
        Item {
            base: ItemBaseId::from("Sacrificial Regalia"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: Vec::new(),
            modifier_texts: vec!["+50 to maximum Life".to_string()],
            enchant_texts: lines.iter().map(|line| (*line).to_string()).collect(),
            rolled_defence: RolledDefence::default(),
            socketed: socketed.iter().map(|name| (*name).to_string()).collect(),
            parsed_stats: Vec::new(),
        }
    }

    fn settled(item: &Item, all: bool) -> Vec<String> {
        resolve_item(item, all, &augments())
            .expect("the item has Bonded lines")
            .enchant_texts
    }

    #[test]
    fn an_item_without_bonded_lines_is_left_alone() {
        let plain = item(&["Body Rune"], &["+40 to maximum Life"]);
        assert!(resolve_item(&plain, true, &augments()).is_none());
    }

    #[test]
    fn bonded_lines_do_nothing_without_their_benefit() {
        let armour = item(
            &["Body Rune", "Stag Idol"],
            &[
                "+70 to maximum Life",
                "Bonded: +20 to maximum Mana",
                "Bonded: 3% increased maximum Life",
            ],
        );
        assert_eq!(settled(&armour, false), ["+70 to maximum Life"]);
    }

    #[test]
    fn the_passive_gives_every_bonded_line() {
        let armour = item(
            &["Body Rune", "Stag Idol"],
            &[
                "+70 to maximum Life",
                "Bonded: +20 to maximum Mana",
                "Bonded: 3% increased maximum Life",
            ],
        );
        assert_eq!(
            settled(&armour, true),
            [
                "+70 to maximum Life",
                "+20 to maximum Mana",
                "3% increased maximum Life"
            ]
        );
    }

    /// The Fox Idol with a Rune: the Idol's quality applies, the Rune's Life
    /// and Mana do not.
    #[test]
    fn the_fox_idol_gives_the_bonded_lines_of_idols_only() {
        let armour = item(
            &["Fox Idol", "Medved's Tending"],
            &[
                "Bonded: 3% increased maximum Life",
                "Bonded: 3% increased maximum Mana",
                "Bonded: +5% to Quality of all Skills",
                UNLOCK,
                "Can roll Soul modifiers",
            ],
        );
        assert_eq!(
            settled(&armour, false),
            [
                "+5% to Quality of all Skills",
                UNLOCK,
                "Can roll Soul modifiers"
            ]
        );
        // The other lists of the item are untouched.
        let resolved = resolve_item(&armour, false, &augments()).expect("resolved");
        assert_eq!(resolved.modifier_texts, ["+50 to maximum Life"]);
        assert_eq!(resolved.socketed, armour.socketed);
    }

    /// The item states the value it has, which need not be the table's.
    #[test]
    fn a_line_of_idols_alone_keeps_the_value_the_item_states() {
        let armour = item(
            &["Fox Idol", "Stag Idol"],
            &[
                "+30 to maximum Life",
                "Bonded: 4% increased maximum Life",
                "Bonded: +7% to Quality of all Skills",
                UNLOCK,
            ],
        );
        assert_eq!(
            settled(&armour, false),
            [
                "+30 to maximum Life",
                "4% increased maximum Life",
                "+7% to Quality of all Skills",
                UNLOCK
            ]
        );
    }

    /// The Stag Idol and Medved's Tending both give "3% increased maximum
    /// Life"; the item states 6%, of which the Idol's 3% applies.
    #[test]
    fn a_line_shared_with_a_rune_counts_the_idols_own_share() {
        let armour = item(
            &["Fox Idol", "Stag Idol", "Medved's Tending"],
            &[
                "+30 to maximum Life",
                "Bonded: 6% increased maximum Life",
                "Bonded: 3% increased maximum Mana",
                "Bonded: +5% to Quality of all Skills",
                UNLOCK,
                "Can roll Soul modifiers",
            ],
        );
        assert_eq!(
            settled(&armour, false),
            [
                "+30 to maximum Life",
                "3% increased maximum Life",
                "+5% to Quality of all Skills",
                UNLOCK,
                "Can roll Soul modifiers"
            ]
        );
    }

    /// A line of another class of item is not on this one, so it decides
    /// nothing here: the Stag Idol's Spell Damage belongs to a Sceptre.
    #[test]
    fn only_the_lines_for_this_class_of_item_count() {
        let on_item: HashSet<String> = ["+30 to maximum Life", "Bonded: 3% increased maximum Life"]
            .map(template)
            .into();
        assert_eq!(
            given_lines(&augments()["Stag Idol"], &on_item),
            ["+30 to maximum Life", "Bonded: 3% increased maximum Life"]
        );
    }

    #[test]
    fn a_line_the_table_does_not_have_is_an_idols_only_when_all_are_idols() {
        let lines = [
            "Bonded: 12% increased Rarity of Items found",
            "Bonded: +5% to Quality of all Skills",
            UNLOCK,
        ];
        let idols = item(&["Fox Idol", "Stag Idol"], &lines);
        assert_eq!(
            settled(&idols, false),
            [
                "12% increased Rarity of Items found",
                "+5% to Quality of all Skills",
                UNLOCK
            ]
        );
        for other in ["Body Rune", "Something Newer"] {
            let mixed = item(&["Fox Idol", other], &lines);
            assert_eq!(
                settled(&mixed, false),
                ["+5% to Quality of all Skills", UNLOCK],
                "{other}"
            );
        }
    }

    #[test]
    fn the_marker_is_matched_whatever_its_case() {
        assert_eq!(bonded_body("Bonded: +1 to X"), Some("+1 to X"));
        assert_eq!(bonded_body("  bonded:+1 to X"), Some("+1 to X"));
        assert_eq!(bonded_body("+1 to X while Bonded: no"), None);
        assert_eq!(bonded_body("Bond"), None);
    }

    #[test]
    fn a_template_ignores_the_numbers() {
        assert_eq!(
            template("Bonded: +5.5% to Quality"),
            template("bonded: +12% to quality")
        );
        assert_ne!(template("+5 to Life"), template("+5 to Mana"));
    }
}
