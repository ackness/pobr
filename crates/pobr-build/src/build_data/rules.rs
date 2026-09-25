use super::BuildData;

impl pobr_core::skill_env::ParserRulesLookup for BuildData {
    fn parser_rules(&self) -> Option<&pobr_core::mod_parser::CompiledParserRules> {
        self.parser_rules.as_deref()
    }
}

impl pobr_core::skill_env::StatMapLookup for BuildData {
    fn stat_map_catalog(&self) -> Option<&pobr_core::rules::stat_map_engine::StatMapCatalog> {
        self.stat_map_catalog.as_deref()
    }
}
