//! [`ItemDraft::build_raw`]: edit-view draft -> PoB item text block (reverse serialization).
//!
//! Line order strictly mirrors PoB2 `Classes/Item.lua::BuildRaw` (1284-1483):
//! `Rarity` -> title/base -> Charm Slots -> Spirit -> the four defence
//! fields -> Unique ID -> League -> Crafted (+Prefix/Suffix) ->
//! Catalyst/CatalystQuality -> Talisman Tier -> Item Level -> the variant
//! block (variant name list -> Selected Variant -> variant lines -> Has Alt
//! blocks) -> Quality -> Sockets/Rune -> LevelReq -> Radius -> Limited to ->
//! Requires Class -> `Implicits: N` -> rune/enchant/classReq/implicit/explicit
//! lines -> state lines.
//!
//! Round-trip contract: `parse(build_raw(parse(x))) == parse(x)` (a semantic
//! fixed point). Byte-for-byte equality is not guaranteed.

use crate::draft::{ItemDraft, LineBucket, ModLineDraft};

impl ItemDraft {
    /// Reverse-serializes the draft into a PoB item text block (lines joined
    /// with `\n`, no trailing newline).
    pub fn build_raw(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let h = &self.header;

        lines.push(format!("Rarity: {}", h.rarity));
        if let Some(title) = &h.title {
            lines.push(title.clone());
            lines.push(h.base_name.clone());
        } else {
            lines.push(h.base_name.clone());
        }

        if let Some(n) = h.charm_limit {
            lines.push(format!("Charm Slots: {n}"));
        }
        if let Some(v) = h.spirit {
            lines.push(format!("Spirit: {}", fmt_num(v)));
        }
        push_defence(&mut lines, "Armour", h.armour);
        push_defence(&mut lines, "Evasion", h.evasion);
        push_defence(&mut lines, "Energy Shield", h.energy_shield);
        push_defence(&mut lines, "Ward", h.ward);

        if let Some(id) = &h.unique_id {
            lines.push(format!("Unique ID: {id}"));
        }
        if let Some(league) = &h.league {
            lines.push(format!("League: {league}"));
        }
        if self.crafted {
            lines.push("Crafted: true".to_string());
        }
        if let Some(name) = &self.catalyst.name {
            lines.push(format!("Catalyst: {name}"));
        }
        if let Some(q) = self.catalyst.quality {
            lines.push(format!("CatalystQuality: {q}"));
        }
        if let Some(t) = h.talisman_tier {
            lines.push(format!("Talisman Tier: {t}"));
        }
        if let Some(il) = h.item_level {
            lines.push(format!("Item Level: {il}"));
        }

        // The variant block.
        if !self.variant.names.is_empty() {
            for name in &self.variant.names {
                lines.push(format!("Variant: {name}"));
            }
            if let Some(sel) = self.variant.selected {
                lines.push(format!("Selected Variant: {sel}"));
            }
            // The alt-variant block.
            const ALT_LABELS: [(&str, &str); 5] = [
                ("Has Alt Variant: true", "Selected Alt Variant"),
                ("Has Alt Variant Two: true", "Selected Alt Variant Two"),
                ("Has Alt Variant Three: true", "Selected Alt Variant Three"),
                ("Has Alt Variant Four: true", "Selected Alt Variant Four"),
                ("Has Alt Variant Five: true", "Selected Alt Variant Five"),
            ];
            for (i, alt) in self.variant.alts.iter().enumerate() {
                if let Some(sel) = alt {
                    lines.push(ALT_LABELS[i].0.to_string());
                    lines.push(format!("{}: {}", ALT_LABELS[i].1, sel));
                }
            }
        }

        if let Some(q) = h.quality {
            lines.push(format!("Quality: {q}"));
        }
        if h.socket_count > 0 {
            let sockets = vec!["S"; h.socket_count as usize].join(" ");
            lines.push(format!("Sockets: {sockets}"));
            for i in 0..h.socket_count as usize {
                let rune = h.runes.get(i).map(String::as_str).unwrap_or("None");
                lines.push(format!("Rune: {rune}"));
            }
        }
        if h.jewel_socket_count > 0 {
            let sockets = vec!["J"; h.jewel_socket_count as usize].join(" ");
            lines.push(format!("Sockets: {sockets}"));
        }
        if let Some(lr) = h.level_req {
            lines.push(format!("LevelReq: {lr}"));
        }
        if let Some(radius) = &h.radius_label {
            lines.push(format!("Radius: {radius}"));
        }
        if let Some(limit) = h.limited_to {
            lines.push(format!("Limited to: {limit}"));
        }
        if let Some(class) = &h.class_restriction {
            lines.push(format!("Requires Class {class}"));
        }

        // `Implicits: N` header = rune + enchant + classReq + implicit line count (explicit excluded).
        let implicit_total = self
            .lines
            .iter()
            .filter(|l| {
                matches!(
                    l.bucket,
                    LineBucket::Rune
                        | LineBucket::Enchant
                        | LineBucket::ClassRequirement
                        | LineBucket::Implicit
                )
            })
            .count();
        lines.push(format!("Implicits: {implicit_total}"));

        // Write out mod lines in bucket order (rune -> enchant -> classReq -> implicit -> explicit).
        for bucket in [
            LineBucket::Rune,
            LineBucket::Enchant,
            LineBucket::ClassRequirement,
            LineBucket::Implicit,
            LineBucket::Explicit,
        ] {
            for line in self.lines.iter().filter(|l| l.bucket == bucket) {
                lines.push(write_mod_line(line));
            }
        }

        // State lines.
        if self.states.mirrored {
            lines.push("Mirrored".to_string());
        }
        if self.states.sanctified {
            lines.push("Sanctified".to_string());
        }
        if self.states.double_corrupted {
            lines.push("Twice Corrupted".to_string());
        } else if self.states.corrupted {
            lines.push("Corrupted".to_string());
        }

        lines.join("\n")
    }
}

/// Rebuilds a single mod line: annotation prefix plus clean text (mirrors BuildRaw's `writeModLine`).
fn write_mod_line(line: &ModLineDraft) -> String {
    format!("{}{}", line.annotations.render_prefix(), line.text)
}

fn push_defence(lines: &mut Vec<String>, label: &str, value: Option<f64>) {
    if let Some(v) = value
        && v > 0.0
    {
        lines.push(format!("{label}: {}", fmt_num(v)));
    }
}

/// Numeric formatting: integral values drop the decimal point (mirrors Lua `tostring`, `1292.0 -> 1292`).
fn fmt_num(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON {
        format!("{}", v as i64)
    } else {
        let mut s = format!("{v}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}
