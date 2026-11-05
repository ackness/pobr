"""Import external display data and retained API fixtures without relabelling them.

Both lock entries take either transport. The support display is published by
the language owner: since Releases, as two extra assets of its
`locale/<patch>-<n>` Release (`MANIFEST` and `ASSET`, both listed in its
`SHA256SUMS`), which `data:bump` adopts with the language package; the
retained entry is a manifest path at a `handoff/*` commit. The test fixtures
are patch-independent API response samples: their entry records the target
they were published for, so a new game patch keeps them.
"""

from pathlib import Path
import re
import tempfile

from pipeline.runtime import data_bundle as bundle
from pipeline.imports.data_imports import (OwnerInputs, acquire, git_address, pinned, regular, require,
                                           source_address, unpack, validate_pin)

ROOT = Path(__file__).resolve().parents[2]
LOCALE = "zh-CN"
ASSET = f"supportDescriptions.{LOCALE}.json"
# The support display manifest's Release asset name (the Git entry names its own `path`).
MANIFEST = f"support-display.{LOCALE}.json"
MANIFEST_SCHEMA = "poe2-support-display-publication/v1"
DISPLAY_OWNER = "ackness/poe2-zh-cn-stats"
DESTINATION = ROOT / "web/src/lib" / ASSET


def display_source(spec, name):
    """The source `acquire` reads `name` (MANIFEST or ASSET) from."""
    if "revision" in spec:
        # The retained Git entry pins the manifest path; the asset sits beside it.
        git_address(spec, spec["path"])
        if name == MANIFEST:
            return spec, spec["path"]
        return spec, (Path(spec["path"]).parent / name).as_posix()
    require("path" not in spec, "A Release support display has no Git path")
    return {"repository": spec["repository"], "tag": spec.get("tag"), "asset": name}, name


def check_manifest(manifest, target, catalog_sha256):
    """The support display manifest describes this target, locale and trade catalog."""
    expected = {"game": target["game"], "version": target["game_patch"], "realm": target["realm"]}
    require(isinstance(manifest, dict) and manifest.get("schema") == MANIFEST_SCHEMA
            and manifest.get("target") == expected and manifest.get("locale") == LOCALE,
            f"Support display target/schema mismatch: expected {MANIFEST_SCHEMA} for "
            f"{target['game_patch']} {target['realm']} {LOCALE}")
    require(manifest.get("catalog_sha256") == catalog_sha256,
            "Support display catalog binding mismatch: it was generated for trade catalog "
            f"{str(manifest.get('catalog_sha256'))[:12]}…, the game package carries {catalog_sha256[:12]}…; "
            "the language owner must publish a support display for this catalog")
    files = manifest.get("files")
    require(isinstance(files, dict) and set(files) == {ASSET}, "Unexpected display inventory")
    validate_pin(files[ASSET])
    provenance = manifest.get("provenance")
    require(isinstance(provenance, dict) and isinstance(provenance.get("generation_data_version"), str),
            "Support display manifest lacks provenance.generation_data_version")
    return files[ASSET]


def check_asset(raw, manifest):
    asset = bundle.decode(raw)
    meta, provenance = asset.get("_meta"), manifest["provenance"]
    require(isinstance(meta, dict) and meta.get("data_version") == provenance["generation_data_version"]
            and meta.get("catalog_sha256") == manifest["catalog_sha256"], "Display provenance binding mismatch")
    require(all(meta.get(k) == provenance.get(k) for k in ("source", "source_commit", "source_file", "source_sha256")),
            "Display source provenance mismatch")


def prepare(lock_path, prepared, cache, offline=False, fetcher=None):
    raw_lock = regular(lock_path)
    lock = bundle.decode(raw_lock)
    if "support_display" not in lock:
        return None
    spec = lock["support_display"]
    require(spec.get("repository") == DISPLAY_OWNER, "Independent display publication required")
    display_source(spec, MANIFEST)
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
        fetcher = fetcher or OwnerInputs(Path(temporary) / "git")
        source, name = display_source(spec, MANIFEST)
        manifest_raw = acquire(source, name, spec["manifest"], cache, offline, fetcher)
        manifest = bundle.decode(manifest_raw)
        require(spec["catalog_sha256"] == bundle.sha256(catalog), "Display catalog binding mismatch")
        expected = check_manifest(manifest, lock["target"], spec["catalog_sha256"])
        require(expected == spec["asset"]
                and manifest["provenance"]["generation_data_version"] == spec["source_data_version"],
                "Support display lock entry differs from its manifest")
        source, name = display_source(spec, ASSET)
        raw = acquire(source, name, spec["asset"], cache, offline, fetcher)
        check_asset(raw, manifest)
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
    require(spec.get("repository") == "ackness/repoe", "Independent test fixture publication required")
    source_address(spec, spec.get("path"))
    validate_pin(spec)
    # Captured API responses do not change with the game patch: the entry
    # records the target they were published for once the lock moves on.
    published = spec.get("target", lock["target"])
    require(isinstance(published, dict) and set(published) == {"game", "game_patch", "realm"}
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", str(published["game_patch"])), "Invalid test fixture target")
    validate_pin(spec["manifest"])
    names = spec["files"]
    require(isinstance(names, list) and names and len(names) == len(set(names))
            and all(len(bundle.relative_path(n).parts) == 1 and n.endswith(".json") for n in names),
            "Invalid test fixture inventory")
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".fixture-import-", dir=cache.parent) as temporary:
        temp = Path(temporary)
        fetcher = fetcher or OwnerInputs(temp / "git")
        archive = acquire(spec, spec.get("path"), spec, cache, offline, fetcher)
        manifest = unpack(archive, spec, temp / "unpacked")
        require(manifest.get("schema") == "repoe-data-publication/v1"
                and manifest.get("target") == published
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
