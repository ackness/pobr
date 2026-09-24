//! Write-only source assembly. Consuming the writer opens the derived-read phase.
//!
//! No Deref, ModDb access, aggregation queries, or session escape hatch is exposed.
//! Injection helpers accept this type; reservation and minion readers require the
//! completed CalculationSession returned by finish_sources.

use super::*;
use pobr_core::mod_parser::ParseError;
use pobr_core::passive::AllocatedNode;
use pobr_core::skill_source::GemModSource;
use pobr_data::item::Item;

pub(super) struct SourceWriter {
    session: CalculationSession,
}

impl SourceWriter {
    pub fn new(session: CalculationSession) -> Self {
        Self { session }
    }

    pub fn finish_sources(self) -> CalculationSession {
        self.session
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
        self.session.add_item(slot, item)
    }

    pub fn add_weapon_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        self.session.add_weapon_item(slot, item)
    }

    pub fn add_flask_charm(&mut self, slot: &str, item: &Item) {
        self.session.add_flask_charm(slot, item)
    }

    pub fn add_passive_nodes(&mut self, nodes: &[AllocatedNode]) -> Result<(), ParseError> {
        self.session.add_passive_nodes(nodes)
    }

    pub fn add_skill_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.session.add_skill_gem(gem)
    }

    pub fn add_support_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.session.add_support_gem(gem)
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
