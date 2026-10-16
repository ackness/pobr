//! Write-only source assembly. Consuming the writer opens the derived-read phase.
//!
//! No Deref, ModDb access, aggregation queries, or session escape hatch is exposed.
//! Injection helpers accept this type; reservation and minion readers require the
//! completed CalculationSession returned by finish_sources.

use pobr_core::calc::{BuffSpec, CalculationSession};
use pobr_core::mod_parser::ParseError;
use pobr_core::passive::AllocatedNode;
use pobr_core::skill_source::GemModSource;
use pobr_core::{ItemModSection, Modifier};
use pobr_data::item::{EquipmentSlot, Item, LimbSlot};
use pobr_data::monster::EnemyTier;
use pobr_data::source::SourceId;

/// What became of a line of the build when a calculation received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineFate {
    /// Handed to the parser; its modifiers are stored under its source.
    Parsed,
    /// No rule parses it whole: it is among the calculation's unsupported lines.
    Unparsed,
    /// Taken before any modifier is stored, and read by what is named: the
    /// item's own defences or damage, gem levels, a jewel's radius.
    ReadBy(&'static str),
    /// Taken before any modifier is stored and left out of this calculation,
    /// for the reason named.
    LeftOut(&'static str),
}

/// A line of the build as a calculation received it: the source its
/// modifiers are stored under, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedLine {
    pub source: SourceId,
    pub text: String,
    pub fate: LineFate,
}

/// The reader of gem level and quality bonuses, which no rule parses.
pub(crate) const GEM_PROPERTY_READER: &str = "gem levels and quality, settled before any modifier is stored (skill::resolve::gem_property_bonuses)";

/// Whose lines arrive: an equipped item or limb, by its slot id, each of
/// whose sections is a source of its own; or a jewel, all of whose lines are
/// attributed to it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum LineHolder<'a> {
    Slot(&'a str),
    Jewel(&'a SourceId),
}

impl LineHolder<'_> {
    /// The source the lines of `section` are stored under, as item ingest names it.
    pub fn source(self, section: ItemModSection) -> SourceId {
        match self {
            Self::Slot(slot) => SourceId::new(
                section.source_kind(),
                format!("item.{slot}.{}", section.id_suffix()),
            ),
            Self::Jewel(source) => source.clone(),
        }
    }
}

/// The sections of `item`'s lines.
pub(crate) fn item_sections_mut(item: &mut Item) -> [(ItemModSection, &mut Vec<String>); 3] {
    [
        (ItemModSection::Implicit, &mut item.implicit_texts),
        (ItemModSection::Explicit, &mut item.modifier_texts),
        (ItemModSection::Enchant, &mut item.enchant_texts),
    ]
}

pub(super) struct SourceWriter {
    session: CalculationSession,
    /// The lines received, once [`Self::keep_received_lines`] asked for them.
    received: Option<Vec<ReceivedLine>>,
}

impl SourceWriter {
    pub fn new(session: CalculationSession) -> Self {
        Self {
            session,
            received: None,
        }
    }

    pub fn finish_sources(self) -> CalculationSession {
        self.session
    }

    /// Keeps every line received from here on with what became of it, for a
    /// diagnosis ([`Self::received_lines`]). A plain calculation keeps none.
    pub fn keep_received_lines(&mut self) {
        self.received = Some(Vec::new());
    }

    /// The lines received since [`Self::keep_received_lines`], in order.
    pub fn received_lines(&mut self) -> Vec<ReceivedLine> {
        self.received.take().unwrap_or_default()
    }

    /// Notes that `text` of `source` arrived and what became of it, when
    /// lines are kept.
    pub fn note_line(&mut self, source: &SourceId, text: &str, fate: LineFate) {
        if let Some(received) = &mut self.received {
            received.push(ReceivedLine {
                source: source.clone(),
                text: text.to_string(),
                fate,
            });
        }
    }

    /// Notes each line of `item` in `slot` as handed to the parser.
    fn note_item_lines(&mut self, slot: &str, item: &Item) {
        if self.received.is_none() {
            return;
        }
        let holder = LineHolder::Slot(slot);
        for (section, texts) in [
            (ItemModSection::Implicit, &item.implicit_texts),
            (ItemModSection::Explicit, &item.modifier_texts),
            (ItemModSection::Enchant, &item.enchant_texts),
        ] {
            let source = holder.source(section);
            for text in texts {
                self.note_line(&source, text, LineFate::Parsed);
            }
        }
    }

    pub fn add_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.session.add_modifiers(modifiers)
    }

    pub fn add_modifier_texts(
        &mut self,
        texts: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), ParseError> {
        self.session.add_modifier_texts(texts)
    }

    pub fn add_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        self.note_item_lines(slot.id(), item);
        self.session.add_item(slot, item)
    }

    pub fn add_weapon_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        self.note_item_lines(slot.id(), item);
        self.session.add_weapon_item(slot, item)
    }

    pub fn add_limb(&mut self, slot: LimbSlot, item: &Item) -> Result<(), ParseError> {
        self.note_item_lines(slot.id(), item);
        self.session.add_limb(slot, item)
    }

    pub fn add_flask_charm(&mut self, slot: &str, item: &Item) {
        if self.received.is_some() {
            let source = pobr_core::item::flask_charm_source(slot);
            for text in item
                .implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
            {
                self.note_line(&source, text, LineFate::Parsed);
            }
        }
        self.session.add_flask_charm(slot, item)
    }

    pub fn add_passive_nodes(&mut self, nodes: &[AllocatedNode]) -> Result<(), ParseError> {
        if self.received.is_some() {
            for node in nodes {
                let source = pobr_core::passive::node_source(node.node_id.0, node.ascendancy);
                for text in &node.modifier_texts {
                    self.note_line(&source, text, LineFate::Parsed);
                }
            }
        }
        self.session.add_passive_nodes(nodes)
    }

    pub fn add_skill_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.session.add_skill_gem(gem)
    }

    pub fn add_support_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.session.add_support_gem(gem)
    }

    pub fn add_party_buff_skill(&mut self, spec: pobr_core::calc::session::PartyBuffSpec) {
        self.session.add_party_buff_skill(spec);
    }

    pub fn add_buff_skill(&mut self, spec: BuffSpec) {
        self.session.add_buff_skill(spec)
    }

    pub fn add_warcry_skill(&mut self, spec: pobr_core::calc::WarcrySpec) {
        self.session.add_warcry_skill(spec)
    }

    pub fn set_multiplier(&mut self, name: impl Into<String>, value: f64) {
        self.session.set_multiplier(name, value)
    }

    pub fn set_condition(&mut self, name: impl Into<String>, value: bool) {
        self.session.set_condition(name, value)
    }

    pub fn set_keystone_mods(&mut self, map: std::collections::BTreeMap<String, Vec<Modifier>>) {
        self.session.set_keystone_mods(map)
    }

    pub fn set_hand_sources(
        &mut self,
        sources: Vec<pobr_core::calc::HandSource>,
        double_hits: bool,
    ) {
        self.session.set_hand_sources(sources, double_hits)
    }

    pub fn setup_enemy(&mut self, level: u32, tier: EnemyTier) {
        self.session.setup_enemy(level, tier)
    }

    pub fn add_enemy_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.session.add_enemy_modifiers(modifiers)
    }

    pub fn apply_enemy_exposure(&mut self, elements: [bool; 3], magnitude: f64) {
        self.session.apply_enemy_exposure(elements, magnitude)
    }

    pub fn record_unsupported_modifier_text(&mut self, text: String) {
        self.session.record_unsupported_modifier_text(text)
    }
}
