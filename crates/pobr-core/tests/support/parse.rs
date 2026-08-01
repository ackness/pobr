//! 共享测试支撑：从仓库数据目录加载真实解析规则（一次编译、全测试复用），
//! 提供 engine 版 `parse_mod` / `ctx`。
//!
//! 收尾删除 legacy 手写解析器后，测试与生产同走数据驱动引擎；本模块与
//! `pobr-core` 的 `test-rules` feature helper 同源（加载逻辑一致），因 crate
//! 自身集成测试无法启用自身 feature 而独立存在（dev-deps 已有 serde_json）。

use std::path::PathBuf;
use std::sync::LazyLock;

use pobr_core::mod_parser::{
    CompiledParserRules, ParseCtx, ParseError, ParseOutcome, parse_mod_engine,
};
use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef, SpecialTemplateDef};

/// 数据版本目录（`crates/pobr-core` 上溯两级到仓库根）。
fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("data")
        .join(pobr_data::data_version())
}

/// 载入一份 special_mods 数据文件（缺文件 → 空，由拼接侧兜底）。
fn load_special(rel: &str) -> Vec<SpecialTemplateDef> {
    let path = data_root().join(rel);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let def: SpecialModsDef = serde_json::from_str(&json).expect("反序列化 special_mods");
    def.entries
}

static RULES: LazyLock<std::sync::Arc<CompiledParserRules>> = LazyLock::new(|| {
    let path = data_root().join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("读取 mod_parser_rules.json");
    let doc: ModParserRulesDoc = serde_json::from_str(&json).expect("反序列化规则表");
    // special_mods 两层（与 pobr-gamedata `load_ruleset` 同序）：版本无关策展层
    // `data/overlay-common/`（相对版本目录是 `../overlay-common/`）打底，版本层按 id
    // 覆盖 / 追加，再拼 derived / vendor。
    let mut special = load_special("../overlay-common/special_mods.json");
    for v in load_special("overlay/special_mods.json") {
        match special.iter_mut().find(|e| e.id == v.id) {
            Some(slot) => *slot = v,
            None => special.push(v),
        }
    }
    special.extend(load_special("generated/special_derived.json"));
    special.extend(load_special("generated/special_vendor.json"));
    std::sync::Arc::new(
        CompiledParserRules::compile_with_special(&doc, &special).expect("编译规则表"),
    )
});

/// 真实规则集（解析规则六表 + special 通道）。
#[allow(dead_code)]
pub fn rules() -> &'static CompiledParserRules {
    &RULES
}

/// 真实规则集的共享 `Arc`（`CalculationSession::set_parser_rules` 注入用）。
#[allow(dead_code)]
pub fn rules_arc() -> std::sync::Arc<CompiledParserRules> {
    RULES.clone()
}

/// 注入真实引擎规则的 [`pobr_core::calc::CalculationSession`]。
#[allow(dead_code)]
pub fn session(input: pobr_core::calc::MinimalInput) -> pobr_core::calc::CalculationSession {
    let mut s = pobr_core::calc::CalculationSession::new(input);
    s.set_parser_rules(rules_arc());
    s
}

/// 注入真实规则的解析上下文。
#[allow(dead_code)]
pub fn ctx() -> ParseCtx<'static> {
    ParseCtx::with_engine(&RULES)
}

/// engine 版单行解析（签名对齐历史 `pobr_core::mod_parser::parse_mod`，
/// 引擎永不返回 `Err`——无法识别的行为整行 Unsupported）。
#[allow(dead_code)]
pub fn parse_mod(text: &str) -> Result<ParseOutcome, ParseError> {
    Ok(parse_mod_engine(text, &RULES))
}
