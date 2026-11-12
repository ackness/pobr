//! Host-only generation and drift checking for shared API DTOs.

use std::path::PathBuf;

use ts_rs::{Config, TS};

use super::request::{
    CharacterOverride, GemInput, JewelInput, SlotIssue, SlotItemInput, SocketGroupInput,
    UtilitySlotInput, WeaponSwapInput,
};

fn destination() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/api/generated/request.ts")
}

fn declarations() -> String {
    let config = Config::default();
    let mut output = String::from(
        "// Generated from Rust request DTOs by ts-rs. Do not edit manually.\n\
         // Regenerate: cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored\n\n",
    );
    for declaration in [
        SlotIssue::decl(&config),
        CharacterOverride::decl(&config),
        GemInput::decl(&config),
        SocketGroupInput::decl(&config),
        SlotItemInput::decl(&config),
        UtilitySlotInput::decl(&config),
        WeaponSwapInput::decl(&config),
        JewelInput::decl(&config),
    ] {
        output.push_str("export ");
        output.push_str(&declaration);
        output.push_str("\n\n");
    }
    normalize_declarations(output)
}

fn normalize_declarations(output: String) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
        + "\n"
}

fn data_info_destination() -> PathBuf {
    destination().with_file_name("dataInfo.ts")
}

fn data_info_declarations() -> String {
    let config = Config::default();
    normalize_declarations(format!(
        "// Generated from Rust data-info DTOs by ts-rs. Do not edit manually.\n\
         // Regenerate: cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored\n\n\
         export {}\n\nexport {}\n",
        crate::locales::LocaleInfo::decl(&config),
        super::DataInfo::decl(&config),
    ))
}

#[test]
fn typescript_contract_is_current() {
    assert_eq!(
        std::fs::read_to_string(data_info_destination()).expect("read generated data info types"),
        data_info_declarations(),
        "data info DTOs changed; run export_typescript_contract"
    );
    assert_eq!(
        std::fs::read_to_string(destination()).expect("read generated request types"),
        declarations(),
        "request DTOs changed; run the export_typescript_contract ignored test and commit the generated file"
    );
}

#[test]
#[ignore = "explicitly regenerate the committed TypeScript contract"]
fn export_typescript_contract() {
    let path = destination();
    std::fs::create_dir_all(path.parent().unwrap()).expect("create generated directory");
    std::fs::write(path, declarations()).expect("write generated request types");
    std::fs::write(data_info_destination(), data_info_declarations())
        .expect("write generated data info types");
}
