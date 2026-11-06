use std::path::Path;
use std::sync::OnceLock;

/// Runtime JSON files and the shared overlay, read once per test binary under
/// the relative paths the browser stages them with.
pub fn pack() -> &'static [(String, Vec<u8>)] {
    static FILES: OnceLock<Vec<(String, Vec<u8>)>> = OnceLock::new();
    FILES.get_or_init(|| {
        let version = pobr_gamedata::current_data_dir();
        let mut files = Vec::new();
        walk(&version, &version, "", &mut files);
        let common = version.parent().unwrap().join("overlay-common");
        walk(&common, &common, "overlay-common/", &mut files);
        files
    })
}

fn walk(root: &Path, dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, prefix, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((format!("{prefix}{relative}"), std::fs::read(&path).unwrap()));
        }
    }
}

/// Stages the pack as bytes, optionally replacing one file. The caller
/// vouches for the bytes, including a replacement that no longer matches the manifest.
pub fn init(override_file: Option<(&str, &[u8])>) {
    let files = pack();
    if let Some((path, _)) = override_file {
        assert!(files.iter().any(|(file, _)| file == path), "missing {path}");
    }
    for (path, bytes) in files {
        let bytes = override_file
            .filter(|(file, _)| *file == path.as_str())
            .map_or(bytes.as_slice(), |(_, bytes)| bytes);
        pobr_wasm::stage_data_bytes(path, bytes.to_vec());
    }
    pobr_wasm::init_staged_data_verified().expect("verified init");
}
