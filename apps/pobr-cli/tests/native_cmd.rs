//! Native document rejection must precede data loading and compatibility imports.

use pobr_build::native_document::MAX_NATIVE_BUILD_BYTES;
use pobr_cli::{CliError, native::calculate_native_document};
use pobr_gamedata::GameData;

#[test]
fn native_rejects_non_native_input_before_loading_data() {
    // An empty in-memory dataset cannot fall back to repository files or CURRENT.
    let data = GameData::from_memory(Default::default());
    for bytes in [
        b"".as_slice(),
        b"<PathOfBuilding><Build level=\"1\"/></PathOfBuilding>",
        b"eJwPoBBuildCode",
        br#"{}"#,
        br#"{"schema":"pobr-native-build/v2"}"#,
        br#"{"schema":"pobr-native-build/v1","unknown":"<PathOfBuilding/>"}"#,
        br#"{"schema":"pobr-native-build/v1","schema":"pobr-native-build/v1"}"#,
        br#"{"schema":"pobr-native-build/v1","\u0073chema":"pobr-native-build/v1"}"#,
        br#"{"schema":"pobr-native-build/v1"} trailing"#,
    ] {
        let error = calculate_native_document(&data, bytes)
            .expect_err("invalid native documents must not reach data loading or XML import");
        assert!(
            matches!(error, CliError::NativeDocument(_)),
            "input {:?}: {error}",
            String::from_utf8_lossy(bytes),
        );
    }
}

#[test]
fn native_rejects_oversized_input_before_json_or_data_loading() {
    let data = GameData::from_memory(Default::default());
    let bytes = vec![b' '; MAX_NATIVE_BUILD_BYTES + 1];
    let error = calculate_native_document(&data, &bytes)
        .expect_err("oversized native documents must fail before decoding or data loading");
    assert!(matches!(error, CliError::NativeDocument(_)));
    assert!(error.to_string().contains("exceeds the byte limit"));
}
