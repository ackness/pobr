//! 测试专用规则加载 helper（M6 D-T8 A2，`feature = "test-rules"`）。
//!
//! pobr-core 本体零 I/O / 不依赖 serde_json（CLAUDE.md：I/O 收口在 pobr-gamedata
//! 一处）。本模块**仅在 `test-rules` feature 下编译**，破例引入 serde_json + fs
//! 读取仓库 `data/4.5.0.3.4/` 的解析规则文件，编译出 [`CompiledParserRules`]——
//! 供下游集成测试（独立 crate）在 A2 穿线时取规则，免去各测试自行复制加载逻辑。
//! 生产构建恒不启用此 feature，零 I/O 铁律不变。
//!
//! 加载逻辑与 `tests/parser_dual_run.rs::load_rules` 一致（同一份规则 + special
//! 拼接），是其可复用化提取。

use std::path::PathBuf;

use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef, SpecialTemplateDef};

use super::compiled::CompiledParserRules;

/// 仓库根（`crates/pobr-core` 上溯两级）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 数据版本目录（当前 `4.5.0.3.4`）。
fn data_root() -> PathBuf {
    repo_root().join("data").join(pobr_data::data_version())
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

/// 从仓库数据目录加载并编译完整 [`CompiledParserRules`]（解析规则六表 + special
/// 通道拼接），供下游集成测试穿线 A2 引擎路径。
///
/// 读 `overlay/mod_parser_rules.json` + `overlay/special_mods.json` +
/// `generated/special_derived.json` + `generated/special_vendor.json`
/// （三源同序对齐 pobr-gamedata `load_ruleset`；缺 special 文件按空拼接，id 冲突在
/// [`CompiledParserRules::compile_with_special`] fail-fast）。文件缺失/不可解析
/// 直接 panic（测试环境，仓库数据包恒在）。
pub fn test_compiled_rules() -> CompiledParserRules {
    let path = data_root().join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("读取 mod_parser_rules.json");
    let doc: ModParserRulesDoc = serde_json::from_str(&json).expect("反序列化规则表");
    let mut special = load_special("overlay/special_mods.json");
    special.extend(load_special("generated/special_derived.json"));
    special.extend(load_special("generated/special_vendor.json"));
    CompiledParserRules::compile_with_special(&doc, &special).expect("编译规则表")
}
