//! [`ItemDraft::build_raw`]：编辑态草稿 → PoB 物品文本块（逆向序列化）。
//!
//! 行序严格对照 PoB2 `Classes/Item.lua::BuildRaw`（1284-1483）：
//! `Rarity` → 标题/基底 → Charm Slots → Spirit → 防御四项 → Unique ID → League →
//! Crafted（+Prefix/Suffix）→ Catalyst/CatalystQuality → Talisman Tier → Item Level →
//! variant 块（Variant 名列表 → Selected Variant → variant 行 → Has Alt 块）→ Quality →
//! Sockets/Rune → LevelReq → Radius → Limited to → Requires Class →
//! `Implicits: N` → rune/enchant/classReq/implicit/explicit 行 → 状态行。
//!
//! 往返契约：`parse(build_raw(parse(x))) == parse(x)`（语义不动点）。字节级不保证。

use crate::draft::{ItemDraft, LineBucket, ModLineDraft};

impl ItemDraft {
    /// 把草稿逆向序列化为 PoB 物品文本块（行以 `\n` 连接，无尾换行）。
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

        // variant 块。
        if !self.variant.names.is_empty() {
            for name in &self.variant.names {
                lines.push(format!("Variant: {name}"));
            }
            if let Some(sel) = self.variant.selected {
                lines.push(format!("Selected Variant: {sel}"));
            }
            // alt 变体块。
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

        // `Implicits: N` 头 = rune + enchant + classReq + implicit 行数（不含 explicit）。
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

        // 按桶序写出词条行（rune → enchant → classReq → implicit → explicit）。
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

        // 状态行。
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

/// 单条词条行重建：标注前缀 + 干净文本（对照 BuildRaw 的 `writeModLine`）。
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

/// 数值格式化：整数省略小数（对照 Lua `tostring`，`1292.0 → 1292`）。
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
