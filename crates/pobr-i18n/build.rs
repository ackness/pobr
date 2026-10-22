use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

fn flatten(prefix: Option<&str>, value: toml::Value, bundle: &mut BTreeMap<String, String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, child) in table {
                let key = match prefix {
                    Some(prefix) => format!("{prefix}.{key}"),
                    None => key,
                };
                flatten(Some(&key), child, bundle);
            }
        }
        toml::Value::String(value) => {
            if let Some(key) = prefix {
                bundle.insert(key.to_owned(), value);
            }
        }
        // Preserve the runtime loader's string-only display-text contract.
        _ => {}
    }
}

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut generated = String::new();
    for (language, name) in [("en-US", "EN"), ("zh-TW", "ZH")] {
        let mut bundle = BTreeMap::new();
        // Later files replace earlier values, matching the original loader.
        for section in ["ui", "stats", "errors"] {
            let relative = format!("locales/{language}/{section}.toml");
            println!("cargo:rerun-if-changed={relative}");
            let source = std::fs::read_to_string(root.join(&relative)).expect("read locale");
            let table: toml::Table = source
                .parse()
                .unwrap_or_else(|error| panic!("invalid locale {relative}: {error}"));
            flatten(None, toml::Value::Table(table), &mut bundle);
        }
        writeln!(generated, "const {name}: &[(&str, &str)] = &[").unwrap();
        for (key, value) in bundle {
            // Debug formatting emits valid escaped Rust string literals.
            writeln!(generated, "    ({key:?}, {value:?}),").unwrap();
        }
        writeln!(generated, "];\n").unwrap();
    }
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("bundles.rs"), generated).expect("write locale tables");
}
