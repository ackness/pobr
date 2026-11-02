"""Import external display data and retained API fixtures without relabelling them."""

from pathlib import Path
import tempfile

from pipeline.runtime import data_bundle as bundle
from pipeline.imports.data_imports import GitInputs, acquire, git_address, pinned, regular, require, unpack, validate_pin

ROOT = Path(__file__).resolve().parents[2]
ASSET = "supportDescriptions.zh-CN.json"
DESTINATION = ROOT / "web/src/lib" / ASSET


def prepare(lock_path, prepared, cache, offline=False, fetcher=None):
    raw_lock = regular(lock_path)
    lock = bundle.decode(raw_lock)
    if "support_display" not in lock:
        return None
    spec = lock["support_display"]
    require(spec.get("repository") == "ackness/poe2-zh-cn-stats"
            and spec.get("publication") == "published", "Independent display publication required")
    git_address(spec, spec["path"])
    validate_pin(spec["manifest"])
    validate_pin(spec["asset"])
    verified = bundle.verify(prepared)
    version = lock["target"]["game_patch"]
    require(verified["data_version"] == version
            and verified["runtime_identity"] == lock["runtime_identity"], "Display runtime binding mismatch")
    catalog = bundle.read_file(Path(prepared), f"data/{version}/overlay/trade_catalog.json")
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".display-import-", dir=cache.parent) as temporary:
        fetcher = fetcher or GitInputs(Path(temporary) / "git")
        manifest_raw = acquire(spec, spec["path"], spec["manifest"], cache, offline, fetcher)
        manifest = bundle.decode(manifest_raw)
        expected_target = {"game": lock["target"]["game"], "version": version,
                           "realm": lock["target"]["realm"]}
        require(manifest.get("schema") == "poe2-support-display-publication/v1"
                and manifest.get("target") == expected_target and manifest.get("locale") == "zh-CN",
                "Display target/schema mismatch")
        require(manifest.get("catalog_sha256") == spec["catalog_sha256"] == bundle.sha256(catalog),
                "Display catalog binding mismatch")
        require(manifest.get("files") == {ASSET: spec["asset"]}, "Unexpected display inventory")
        name = (Path(spec["path"]).parent / ASSET).as_posix()
        raw = acquire(spec, name, spec["asset"], cache, offline, fetcher)
        asset = bundle.decode(raw)
        meta, provenance = asset["_meta"], manifest["provenance"]
        require(meta["data_version"] == provenance["generation_data_version"] == spec["source_data_version"]
                and meta["catalog_sha256"] == spec["catalog_sha256"], "Display provenance binding mismatch")
        require(all(meta[k] == provenance[k] for k in ("source", "source_commit", "source_file", "source_sha256")),
                "Display source provenance mismatch")
        require(regular(lock_path) == raw_lock, "Display lock changed during import")
        return pinned(raw, spec["asset"])


def install(raw, output=DESTINATION):
    if raw is None:
        return
    output = Path(output).absolute()
    require(".." not in output.parts and all(not p.is_symlink() for p in (output, *output.parents)),
            "Unsafe display output path")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(prefix=".display-", dir=output.parent, delete=False) as handle:
        temporary = Path(handle.name)
        handle.write(raw)
    try:
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)


def prepare_test_fixtures(lock_path, cache, offline=False, fetcher=None):
    raw_lock = regular(lock_path)
    lock = bundle.decode(raw_lock)
    if "test_fixtures" not in lock:
        return None
    spec = lock["test_fixtures"]
    require(spec.get("repository") == "ackness/repoe" and spec.get("publication") == "published",
            "Independent test fixture publication required")
    git_address(spec, spec["path"])
    validate_pin(spec)
    validate_pin(spec["manifest"])
    names = spec["files"]
    require(isinstance(names, list) and names and len(names) == len(set(names))
            and all(len(bundle.relative_path(n).parts) == 1 and n.endswith(".json") for n in names),
            "Invalid test fixture inventory")
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".fixture-import-", dir=cache.parent) as temporary:
        temp = Path(temporary)
        fetcher = fetcher or GitInputs(temp / "git")
        archive = acquire(spec, spec["path"], spec, cache, offline, fetcher)
        manifest = unpack(archive, spec, temp / "unpacked")
        require(manifest.get("schema") == "repoe-data-publication/v1"
                and manifest.get("target") == lock["target"]
                and set(manifest["files"]) == set(names), "Test fixture target/inventory mismatch")
        files = {name: regular(temp / "unpacked" / name) for name in names}
        require(regular(lock_path) == raw_lock, "Test fixture lock changed during import")
        return files


def install_test_fixtures(files, output=ROOT / "web/src/fixtures"):
    if files is None:
        return
    output = Path(output)
    require({p.name for p in output.glob("*.json")} <= set(files), "Unlisted local API fixture")
    for name, raw in files.items():
        install(raw, output / name)
