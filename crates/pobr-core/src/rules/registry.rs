//! Handler registry: the dispatch channel from a data entry's `handler_id` to
//! real Rust logic.
//!
//! Entries in tables like `overlay/special_mods.json` /
//! `overlay/config_options.json` that can't be expressed in the restricted
//! template DSL (about 10% — the DSL's hard boundary) carry only a stable
//! `handler_id` string. At runtime this registry looks up the matching Rust
//! closure and runs it, producing a [`HandlerOutcome`]. The registry itself
//! is zero-I/O and its set of registrations is fixed at startup.
//!
//! Monitoring constraint: handler entry count should stay under 100;
//! approaching 10% of the total special entries signals a data-split failure.

use std::collections::BTreeMap;
use std::fmt;

use crate::config::CalcConfig;
use crate::mod_db::ModDb;
use crate::modifier::Modifier;

/// Input context handed to a handler.
///
/// Each field is available "as needed" — the same handler sees a different
/// context depending on where it's invoked from:
/// - **The config consumption point** (`config_interpreter::interpret`, when
///   the build layer interprets a raw `<Input>`): only `inputs` (the
///   entry's parsed numeric placeholder parameters) is set; db / cfg /
///   main-skill context doesn't exist yet -> `None`.
/// - **The buff consumption point** (`buff_expander::expand_misc_buffs`,
///   env_finalize stage 6): `player_db` / `enemy_db` / `cfg` come from a
///   read-only `Env` snapshot; `main_skill` awaits orchestration-layer
///   wiring (`None` until then).
///
/// A handler **must produce conservative zero output** for a missing field
/// (better to omit than to guess wrong) — this is the line between
/// "implemented but context-gated" and a stub: the former has complete logic
/// that just activates once its fields are wired up.
#[derive(Debug, Clone, Copy, Default)]
pub struct HandlerCtx<'a> {
    /// Numeric placeholder parameters captured by the data entry (`$1..$n`
    /// already evaluated; a config single-input entry stores it as `input`).
    pub inputs: &'a [f64],
    /// Read-only reference to the player modDB (supplied at the buff
    /// consumption point; `None` at the config consumption point).
    pub player_db: Option<&'a ModDb>,
    /// Read-only reference to the enemy modDB (same, as needed).
    pub enemy_db: Option<&'a ModDb>,
    /// Read-only reference to the calc context (for flag/sum queries; as
    /// needed).
    pub cfg: Option<&'a CalcConfig>,
    /// Main-skill context (used for vendor `SkillName` tag /
    /// `mainSkill.…selfCast` gating; as needed — `None` when the
    /// consumption point hasn't wired it up yet, and handlers depending on
    /// it produce conservative zero output).
    pub main_skill: Option<&'a MainSkillCtx>,
    /// Raw capture-group text from the special channel (the unevaluated
    /// `$1..$n` strings) — for handlers that need a text payload (e.g.
    /// `allocates (.+)` -> `GrantedPassive LIST Text(name)`, where numeric
    /// `inputs` can't carry a name). `&[]` at the config consumption point
    /// (no text captures there).
    pub raw_captures: &'a [String],
}

impl<'a> HandlerCtx<'a> {
    /// A context carrying only numeric placeholder parameters (the config
    /// consumption point's shape).
    pub fn with_inputs(inputs: &'a [f64]) -> Self {
        Self {
            inputs,
            ..Self::default()
        }
    }

    /// Numeric placeholder parameters plus raw special-channel capture text
    /// (the special consumption point's shape).
    pub fn with_inputs_and_captures(inputs: &'a [f64], raw_captures: &'a [String]) -> Self {
        Self {
            inputs,
            raw_captures,
            ..Self::default()
        }
    }

    /// The first numeric placeholder parameter (convenience accessor for
    /// config single-input entries; defaults to 0.0).
    pub fn input(&self) -> f64 {
        self.inputs.first().copied().unwrap_or(0.0)
    }
}

/// Main-skill context ([`HandlerCtx::main_skill`]).
///
/// Corresponds to two vendor gating mechanisms: the `{ type = "SkillName",
/// skillName = … }` tag (the ConfigOptions.lua BypassCD family), matched via
/// `skill_name`; and `mainSkill.activeEffect.srcInstance.selfCast`
/// (CalcPerform.lua:574, Fanaticism), determined via `self_cast`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MainSkillCtx {
    /// The main skill's display name (matches vendor's `SkillName` tag
    /// `skillName` field).
    pub skill_name: String,
    /// Whether the main skill is self-cast (cast by the player's own hand,
    /// as opposed to triggered/totem/trap/mine).
    pub self_cast: bool,
}

/// What a handler produces.
///
/// The four output channels land wherever each consumption point routes
/// them:
/// - `player_mods` / `enemy_mods` -> the matching actor's modDB (the config
///   consumption point injects via `ConfigOutcome`'s bucket, the buff
///   consumption point via `BuffExpansion`); the consumption point attaches
///   the SourceId attribution uniformly (a handler carries no origin of its
///   own);
/// - `conditions` -> the cfg condition table (`ConfigOutcome::conditions` at
///   the config consumption point, `BuffExpansion::conditions_set` at the
///   buff consumption point; only `true` entries);
/// - `scalars` -> the cfg multiplier table (**merged additively**, matching
///   vendor's `modDB.multipliers[var] = (… or 0) + v` form).
#[derive(Debug, Clone, Default)]
pub struct HandlerOutcome {
    /// Modifiers to write into the player modDB.
    pub player_mods: Vec<Modifier>,
    /// Modifiers to write into the enemy modDB.
    pub enemy_mods: Vec<Modifier>,
    /// Condition flags to set (`(var, enabled)`).
    pub conditions: Vec<(String, bool)>,
    /// Multiplier scalars (`(var, value)`, merged additively).
    pub scalars: Vec<(String, f64)>,
}

impl HandlerOutcome {
    /// Convenience constructor for a handler that only produces player
    /// modifiers.
    pub fn player_mods(mods: Vec<Modifier>) -> Self {
        Self {
            player_mods: mods,
            ..Self::default()
        }
    }
}

/// A handler closure: takes a [`HandlerCtx`] (numeric placeholder parameters
/// plus read-only context, as available), returns a [`HandlerOutcome`]
/// (player/enemy mods + conditions + scalars).
///
/// This signature was deliberately settled on (the earlier
/// `Fn(&[f64]) -> Vec<Modifier>` was a skeleton) to support entries like
/// enemyIsBoss that need to write to the enemy side or read db state.
pub type Handler = Box<dyn Fn(&HandlerCtx<'_>) -> HandlerOutcome + Send + Sync>;

/// Error for registering the same `handler_id` twice (the registered set
/// must be unique and fixed at startup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHandlerError {
    /// The conflicting handler's stable ID.
    pub id: &'static str,
}

impl fmt::Display for DuplicateHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate handler_id registration: `{}`", self.id)
    }
}

impl std::error::Error for DuplicateHandlerError {}

/// Registry mapping `&'static str` handler_id -> handler closure.
///
/// Uses a `BTreeMap` to guarantee deterministic iteration order (useful for
/// coverage reports and reproducible tests).
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: BTreeMap<&'static str, Handler>,
}

impl fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field("ids", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl HandlerRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a handler; registering the same id twice returns an error
    /// (never silently overwrites).
    pub fn register(
        &mut self,
        id: &'static str,
        handler: Handler,
    ) -> Result<(), DuplicateHandlerError> {
        if self.handlers.contains_key(id) {
            return Err(DuplicateHandlerError { id });
        }
        self.handlers.insert(id, handler);
        Ok(())
    }

    /// Looks up a handler by id; returns `None` if unregistered (the caller
    /// uses this to record the entry in an uncovered-entries list).
    pub fn get(&self, id: &str) -> Option<&Handler> {
        self.handlers.get(id)
    }

    /// Number of registered handlers (used for coverage monitoring;
    /// constrained to stay under 100).
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// The registered handler_ids in deterministic ascending order — used by
    /// coverage reports.
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.handlers.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::modifier::ModType;

    use super::*;

    fn noop_handler() -> Handler {
        Box::new(|_| HandlerOutcome::default())
    }

    /// After registering, the handler can be looked up by id and invoked,
    /// producing a HandlerOutcome (mods + conditions + scalars).
    #[test]
    fn register_then_get_and_invoke() {
        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "test:scaled_life",
                Box::new(|ctx| HandlerOutcome {
                    player_mods: vec![Modifier::number("Life", ModType::Base, ctx.input())],
                    conditions: vec![("FullLife".to_string(), true)],
                    scalars: vec![("LifeScale".to_string(), 2.0)],
                    ..HandlerOutcome::default()
                }),
            )
            .unwrap();

        let handler = registry
            .get("test:scaled_life")
            .expect("should be registered");
        let out = handler(&HandlerCtx::with_inputs(&[50.0]));
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].value.as_number(), Some(50.0));
        assert_eq!(out.conditions, vec![("FullLife".to_string(), true)]);
        assert_eq!(out.scalars, vec![("LifeScale".to_string(), 2.0)]);
        assert!(out.enemy_mods.is_empty());
    }

    /// HandlerCtx's default shape: no db/cfg/main-skill context, input()
    /// defaults to 0.0.
    #[test]
    fn ctx_defaults_are_conservative() {
        let ctx = HandlerCtx::default();
        assert_eq!(ctx.input(), 0.0);
        assert!(ctx.player_db.is_none());
        assert!(ctx.enemy_db.is_none());
        assert!(ctx.cfg.is_none());
        assert!(ctx.main_skill.is_none());
    }

    /// The context carries db/cfg as available: a handler can perform
    /// aggregate queries through the read-only references.
    #[test]
    fn ctx_carries_db_and_cfg_readonly() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::number("Life", ModType::Base, 40.0));
        let cfg = CalcConfig::new();
        let ctx = HandlerCtx {
            inputs: &[],
            player_db: Some(&db),
            enemy_db: None,
            cfg: Some(&cfg),
            main_skill: None,
            raw_captures: &[],
        };
        let handler: Handler = Box::new(|ctx| {
            let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
                return HandlerOutcome::default();
            };
            let life = db.sum(
                ModType::Base,
                cfg,
                &[pobr_data::modifier::ModName::from("Life")],
            );
            HandlerOutcome::player_mods(vec![Modifier::number("X", ModType::Base, life)])
        });
        let out = handler(&ctx);
        assert_eq!(out.player_mods[0].value.as_number(), Some(40.0));
    }

    /// Registering the same id twice errors instead of silently overwriting
    /// the existing handler.
    #[test]
    fn duplicate_registration_errors() {
        let mut registry = HandlerRegistry::new();
        registry.register("dup", noop_handler()).unwrap();
        let err = registry.register("dup", noop_handler()).unwrap_err();
        assert_eq!(err, DuplicateHandlerError { id: "dup" });
        assert_eq!(registry.len(), 1);
    }

    /// An unregistered id returns None; len/is_empty/ids reflect the
    /// registered set (deterministic ascending order).
    #[test]
    fn lookup_miss_and_deterministic_ids() {
        let mut registry = HandlerRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("missing").is_none());

        registry.register("b", noop_handler()).unwrap();
        registry.register("a", noop_handler()).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
        assert_eq!(registry.ids().collect::<Vec<_>>(), vec!["a", "b"]);
    }
}
