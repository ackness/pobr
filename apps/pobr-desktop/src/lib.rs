//! pobr-desktop（库部分）：可测的「示例 Build 构造 + 计算摘要」逻辑。
//!
//! GUI 框架（计划采用 [egui](https://github.com/emilk/egui) / `eframe`）尚未引入：
//! headless CI 无法验证 GUI，且重 GUI 依赖会拖慢编译，故延后。当前 binary 仅作为
//! 未来 GUI 的占位/烟雾测试 —— 构造一个内置示例 Build、跑一次 `pobr-build` 编排计算、
//! 用 `pobr-i18n` 翻译窗口标题，并把核心结果摘要打印到 stdout。
//!
//! 这里把「构造示例 Build」「生成本地化摘要字符串」拆为纯函数，便于单元测试断言
//! 摘要包含预期字段（如 `"DPS"`），而无需依赖 GUI 运行时。

use pobr_build::{Build, CharacterIdentity, OrchestratorOptions, calculate};
use pobr_core::calc::MinimalInput;
use pobr_data::prelude::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_i18n::{CANONICAL_LANGUAGE, LanguageId, Translator};

/// 示例 Build 的等级（与 [`example_build`] 一致，供测试断言）。
pub const EXAMPLE_BUILD_LEVEL: u32 = 1;

/// 示例 Build 的角色职业名。
const EXAMPLE_CLASS_NAME: &str = "Ranger";

/// 示例 Build 的基础生命（来自角色装配，作为 [`MinimalInput`] 注入编排）。
const EXAMPLE_BASE_LIFE: f64 = 50.0;

/// 构造一个内置的最小示例 Build（等级 1，单件戒指附带可解析 modifier）。
///
/// 仅用于占位/演示：词条文本走英文 PoB 兼容解析路径，确保 `pobr-core` 能聚合出
/// 非平凡的生命 / 抗性输出。采用 REAL `pobr_data` / `pobr_build` 权威类型。
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

/// 用给定语言的 [`Translator`] 翻译应用标题（`app.title` key）。
///
/// 解析失败时回退到规范语言（en-US）。
pub fn app_title(language: &str) -> String {
    let translator = Translator::new(LanguageId::new(language))
        .or_else(|_| Translator::new(LanguageId::new(CANONICAL_LANGUAGE)))
        .expect("en-US canonical pack always loads");
    translator.text("app.title").into_owned()
}

/// 计算给定 Build 并生成可读的本地化摘要字符串。
///
/// 摘要固定包含以下字段标签：`Life` / `Mana` / `Fire Res` / `Cold Res` /
/// `Lightning Res` / `DPS`，以 `app.title` 作为表头，供 stdout 烟雾测试与未来 GUI 复用。
///
/// 通过 [`pobr_build::calculate`] 编排：装配一个携带示例基础生命的 [`MinimalInput`]，
/// 把 Build 装备词条注入 [`CalculationSession`]，产出标量 `OutputTable`。
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
