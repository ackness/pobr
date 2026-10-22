//! Host-only generation and drift checking for the shared request DTOs.

use std::path::PathBuf;

use ts_rs::{Config, TS};

use super::request::{
    CharacterOverride, GemInput, JewelInput, SlotIssue, SlotItemInput, SocketGroupInput,
    WeaponSwapInput,
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
        WeaponSwapInput::decl(&config),
        JewelInput::decl(&config),
    ] {
        output.push_str("export ");
        output.push_str(&declaration);
        output.push_str("\n\n");
    }
    // Normalize trailing spaces and emit exactly one final newline.
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
        + "\n"
}

#[test]
fn typescript_contract_is_current() {
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
}
