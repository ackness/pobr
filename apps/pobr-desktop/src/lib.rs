//! pobr-desktop (the library part): the testable "build an example Build,
//! compute a summary" logic.
//!
//! The GUI framework (planned to be [egui](https://github.com/emilk/egui) /
//! `eframe`) hasn't been introduced yet: headless CI can't verify a GUI, and
//! a heavy GUI dependency would slow down builds, so this is deferred. The
//! current binary only serves as a placeholder/smoke test for the future GUI
//! — it builds a built-in example Build, runs one `pobr-build` orchestrated
//! calculation, translates the window title via `pobr-i18n`, and prints a
//! core result summary to stdout.
//!
//! "Build the example Build" and "generate a localized summary string" are
//! split out here as pure functions, so unit tests can assert the summary
//! contains expected fields (e.g. `"DPS"`) without depending on the GUI runtime.

use pobr_build::{Build, CharacterIdentity, OrchestratorOptions, calculate};
use pobr_core::calc::MinimalInput;
use pobr_data::prelude::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_i18n::{CANONICAL_LANGUAGE, LanguageId, Translator};

/// The example Build's level (matches [`example_build`], for test assertions).
pub const EXAMPLE_BUILD_LEVEL: u32 = 1;

/// The example Build's character class name.
const EXAMPLE_CLASS_NAME: &str = "Ranger";

/// The example Build's base life (from character setup, injected into
/// orchestration as [`MinimalInput`]).
const EXAMPLE_BASE_LIFE: f64 = 50.0;

/// Builds a minimal built-in example Build (level 1, a single ring with a parseable modifier).
///
/// For placeholder/demo purposes only: the mod text goes through the
/// PoB-compatible English parse path, ensuring `pobr-core` can aggregate a
/// non-trivial life / resistance output. Uses the REAL authoritative
/// `pobr_data` / `pobr_build` types.
pub fn example_build() -> Build {
    let ring = Item {
        base: ItemBaseId::from("Iron Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![
            "+25 to maximum Life".to_string(),
            "+30% to Fire Resistance".to_string(),
        ],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    };

    Build::new()
        .with_character(CharacterIdentity {
            level: EXAMPLE_BUILD_LEVEL,
            class_name: EXAMPLE_CLASS_NAME.to_string(),
            ascendancy_name: String::new(),
        })
        .set_item(EquipmentSlot::Ring1, ring)
}

/// Translates the app title (the `app.title` key) using a [`Translator`] for the given language.
///
/// Falls back to the canonical language (en-US) if parsing fails.
pub fn app_title(language: &str) -> String {
    let translator = Translator::new(LanguageId::new(language))
        .or_else(|_| Translator::new(LanguageId::new(CANONICAL_LANGUAGE)))
        .expect("en-US canonical pack always loads");
    translator.text("app.title").into_owned()
}

/// Computes the given Build and produces a readable, localized summary string.
///
/// The summary always includes these field labels: `Life` / `Mana` /
/// `Fire Res` / `Cold Res` / `Lightning Res` / `DPS`, headed by `app.title`,
/// for reuse by the stdout smoke test and the future GUI.
///
/// Orchestrated via [`pobr_build::calculate`]: assembles a [`MinimalInput`]
/// carrying the example base life, injects the Build's equipment mod lines
/// into a [`CalculationSession`], and produces a scalar `OutputTable`.
///
/// [`CalculationSession`]: pobr_core::calc::CalculationSession
pub fn build_summary(build: &Build, language: &str) -> Result<String, pobr_build::BuildError> {
    let options = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: EXAMPLE_BASE_LIFE,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![],
    };
    let out = calculate(build, &options)?;

    let title = app_title(language);
    let mut summary = String::new();
    summary.push_str(&format!("=== {title} ===\n"));
    summary.push_str(&format!("Level: {}\n", build.character.level));
    summary.push_str(&format!("Class: {}\n", build.character.class_name));
    summary.push_str(&format!("Life: {:.0}\n", out.life));
    summary.push_str(&format!("Mana: {:.0}\n", out.mana));
    summary.push_str(&format!("Fire Res: {:.0}%\n", out.fire_resistance));
    summary.push_str(&format!("Cold Res: {:.0}%\n", out.cold_resistance));
    summary.push_str(&format!(
        "Lightning Res: {:.0}%\n",
        out.lightning_resistance
    ));
    summary.push_str(&format!("DPS: {:.1}\n", out.dps));

    Ok(summary)
}
