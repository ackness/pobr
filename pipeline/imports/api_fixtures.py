"""Import pinned, patch-independent API response samples for Web tests."""

from pathlib import Path
import re
import tempfile

from pipeline.runtime import data_bundle as bundle
from pipeline.imports.data_imports import (OwnerInputs, acquire, regular, replace_atomically, require,
                                           source_address, unpack, validate_pin)

ROOT = Path(__file__).resolve().parents[2]


def prepare(lock_path, cache, offline=False, fetcher=None):
    raw_lock = regular(lock_path)
    lock = bundle.decode(raw_lock)
    if "test_fixtures" not in lock:
        return None
    spec = lock["test_fixtures"]
    require(isinstance(spec, dict), "Invalid test_fixtures data lock entry")
    require(spec.get("repository") == "ackness/repoe", "Independent test fixture publication required")
    source_address(spec, spec.get("path"))
    validate_pin(spec)
    # Captured API responses do not change with the game patch: the entry
    # records the target they were published for once the lock moves on.
    published = spec.get("target", lock["target"])
    require(isinstance(published, dict) and set(published) == {"game", "game_patch", "realm"}
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", str(published["game_patch"])), "Invalid test fixture target")
    require("manifest" in spec, "Missing test_fixtures manifest pin")
    validate_pin(spec["manifest"])
    require(type(spec.get("expanded_bytes")) is int and spec["expanded_bytes"] > 0,
            "Invalid test fixture expanded size")
    names = spec.get("files")
    require(isinstance(names, list) and names and all(isinstance(n, str) for n in names)
            and len(names) == len(set(names))
            and all(len(bundle.relative_path(n).parts) == 1 and n.endswith(".json") for n in names),
            "Invalid test fixture inventory")
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".fixture-import-", dir=cache.parent) as temporary:
        temp = Path(temporary)
        fetcher = fetcher or OwnerInputs(temp / "git")
        archive = acquire(spec, spec.get("path") or spec.get("asset"), spec, cache, offline, fetcher)
        manifest = unpack(archive, spec, temp / "unpacked")
        require(manifest.get("schema") == "repoe-data-publication/v1"
                and manifest.get("target") == published
                and set(manifest["files"]) == set(names), "Test fixture target/inventory mismatch")
        files = {name: regular(temp / "unpacked" / name) for name in names}
        require(regular(lock_path) == raw_lock, "Test fixture lock changed during import")
        return files


def install(files, output=ROOT / "web/src/fixtures"):
    if files is None:
        return
    output = Path(output)
    require({p.name for p in output.glob("*.json")} <= set(files), "Unlisted local API fixture")
    for name, raw in files.items():
        replace_atomically(output / name, raw)
