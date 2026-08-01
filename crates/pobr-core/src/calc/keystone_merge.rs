//! Merging keystones granted by mods.
//!
//! Mirrors PoB2 `CalcPerform.lua:66-76 mergeKeystones`: tabulates the player
//! db's `Keystone` LIST mod (item/jewel affixes like "You have \<Keystone\>" or
//! a bare keystone name line, produced by mod_parser as
//! `Modifier{name:"Keystone", List, Text(name)}`) and, for each not yet
//! injected, looks it up in [`Env::keystone_mods`] (built by pobr-build from
//! the passive tree's keystone nodes and injected via
//! `session.set_keystone_mods`) to write that keystone's mods into the player
//! modDB, attributed to SourceId = (`GrantedKeystone`, `"keystone.<name>"`).
//!
//! Deduplication semantics (equivalent to PoB2's `env.keystonesAdded`):
//! - **Idempotent across passes** (env_finalize stages 1 and 5 both run this):
//!   injected mods carry a `GrantedKeystone` SourceId, so this function skips
//!   any keystone already present with that SourceId in the player db — no
//!   extra state field is needed on `Env`.
//! - **No duplicate for an already-allocated tree node**: on PoB2's tree, an
//!   allocated keystone node also only emits a `Keystone` LIST mod and is
//!   injected once through this same merge; pobr's tree path (passive ingest)
//!   injects the node's mods directly instead, so pobr-build **excludes
//!   already-allocated keystones** when building the `keystone_mods` map (a
//!   missing map key is silently skipped here, equivalent to PoB2's
//!   `keystoneMap[name]` nil-check branch).
//! - Multiple `Keystone` LIST mods with the same name (several items granting
//!   the same keystone) are deduplicated with a `HashSet` within one call.
//!
//! Relationship to `rules/keystone_registry.rs`: this module only handles
//! "mod → keystone mod injection"; mechanism gates like CI/EB are still
//! decided by keystone_registry reading flags (having the corresponding flag
//! among the injected mods wires it up automatically).
//!
//! No-op safe (this is the invariant anchor for the D1 migration): with no
//! `Keystone` LIST mod or an empty `keystone_mods` map, the player db gets no
//! writes and every output value is unchanged.

use std::collections::HashSet;

use pobr_data::prelude::*;

use super::Env;

/// ModName for the `Keystone` LIST channel (PoB2 `Tabulate("LIST", nil, "Keystone")`).
const KEYSTONE_LIST_NAME: &str = "Keystone";

/// Attribution SourceId for a keystone injection (`GrantedKeystone, "keystone.<name>"`).
fn keystone_source_id(name: &str) -> SourceId {
    SourceId::new(SourceKind::GrantedKeystone, format!("keystone.{name}"))
}

/// Implementation body for env_finalize stages 1/5 (mirrors CalcPerform.lua:66-76).
pub fn merge_keystones(env: &mut Env) {
    if env.keystone_mods.is_empty() {
        return;
    }

    let granted = env
        .player
        .mod_db
        .list(&env.cfg, ModName::from(KEYSTONE_LIST_NAME));
    if granted.is_empty() {
        return;
    }

    // Set of already-injected names (cross-stage idempotency): keystones already
    // present in the player db with GrantedKeystone attribution.
    let mut added: HashSet<String> = env
        .player
        .mod_db
        .iter_mods()
        .filter_map(|m| m.origin.as_ref())
        .filter(|origin| origin.source_id.kind == SourceKind::GrantedKeystone)
        .filter_map(|origin| origin.source_id.id.strip_prefix("keystone."))
        .map(str::to_string)
        .collect();

    let mut to_inject: Vec<crate::Modifier> = Vec::new();
    for name in granted {
        // Equivalent to `if not env.keystonesAdded[modObj.value]`; also
        // deduplicates multiple mods with the same name within this call.
        if !added.insert(name.clone()) {
            continue;
        }
        // Equivalent to the `env.spec.tree.keystoneMap[modObj.value]` nil
        // check: a missing map key (including "already allocated on the
        // tree" keystones excluded by pobr-build) is silently skipped.
        let Some(mods) = env.keystone_mods.get(&name) else {
            continue;
        };
        let source_id = keystone_source_id(&name);
        for modifier in mods {
            // Attribution is always overwritten to GrantedKeystone (raw_text
            // keeps the original mod line already carried by the map).
            let raw_text = modifier
                .origin
                .as_ref()
                .and_then(|origin| origin.raw_text.clone())
                .or_else(|| modifier.source.clone());
            let mut origin = ModifierSource::new(source_id.clone());
            origin.raw_text = raw_text;
            to_inject.push(modifier.clone().with_origin(origin));
        }
    }

    for modifier in to_inject {
        env.player.mod_db.add_mod(modifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::env_finalize;
    use crate::calc::{Actor, ActorBaseStats};
    use crate::{ModValue, Modifier};
    use std::collections::BTreeMap;

    fn env_with_grant(keystone: &str) -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(Modifier::new(
            KEYSTONE_LIST_NAME,
            ModType::List,
            ModValue::Text(keystone.into()),
        ));
        env
    }

    fn iron_reflexes_map() -> BTreeMap<String, Vec<Modifier>> {
        BTreeMap::from([(
            "Iron Reflexes".to_string(),
            vec![
                Modifier::flag("Keystone:IronReflexes"),
                Modifier::number("Armour", ModType::Inc, 20.0),
            ],
        )])
    }

    /// An item mod grants a keystone → its mods are injected once, attributed to GrantedKeystone.
    #[test]
    fn grants_keystone_mods_once_with_granted_keystone_source() {
        let mut env = env_with_grant("Iron Reflexes");
        env.keystone_mods = iron_reflexes_map();

        merge_keystones(&mut env);

        let injected: Vec<_> = env
            .player
            .mod_db
            .iter_mods()
            .filter(|m| {
                m.origin
                    .as_ref()
                    .is_some_and(|o| o.source_id.kind == SourceKind::GrantedKeystone)
            })
            .collect();
        assert_eq!(injected.len(), 2, "keystone 的 2 条 mod 各注入一次");
        for m in &injected {
            assert_eq!(
                m.origin.as_ref().unwrap().source_id.id,
                "keystone.Iron Reflexes"
            );
        }
        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("Armour")],),
            20.0
        );
    }

    /// Running stage 1 and stage 5 each once (PoB2 keystonesAdded idempotency)
    /// does not double-inject; multiple LIST mods with the same name are also deduplicated.
    #[test]
    fn second_pass_and_duplicate_grants_are_idempotent() {
        let mut env = env_with_grant("Iron Reflexes");
        // A second item grants the same keystone.
        env.player.mod_db.add_mod(Modifier::new(
            KEYSTONE_LIST_NAME,
            ModType::List,
            ModValue::Text("Iron Reflexes".into()),
        ));
        env.keystone_mods = iron_reflexes_map();

        merge_keystones(&mut env); // stage 1
        merge_keystones(&mut env); // stage 5

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("Armour")],),
            20.0,
            "幂等：重复授予/二次合并不得叠加"
        );
    }

    /// A missing map key (pobr-build has already excluded "already allocated
    /// on the tree" keystones) is silently skipped with no writes
    /// (equivalent to PoB2's keystoneMap[name] nil branch).
    #[test]
    fn missing_map_entry_is_silently_skipped() {
        let mut env = env_with_grant("Iron Reflexes");
        env.keystone_mods = BTreeMap::from([(
            "Acrobatics".to_string(),
            vec![Modifier::flag("Keystone:Acrobatics")],
        )]);
        let before = env.player.mod_db.iter_mods().count();

        merge_keystones(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), before);
    }

    /// An empty map leaves every value unchanged (D1 no-op-safe anchor, verified through the full env_finalize dispatch).
    #[test]
    fn empty_map_keeps_db_unchanged_through_env_finalize() {
        let mut env = env_with_grant("Iron Reflexes");
        let before = env.player.mod_db.iter_mods().count();

        env_finalize::env_finalize(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), before);
    }
}
