"""Import a fixed RePoE historical tree package; never acquire upstream game data."""

from pathlib import Path
import re
import tempfile

from pipeline.runtime.build_data import safe_directory
from pipeline.runtime.data_bundle import decode, relative_path
from pipeline.imports.data_imports import (GitInputs, acquire, encoded, git_address, pinned, regular,
                          require, sha, unpack, validate_pin, write)


LOCK_SCHEMA = "pobr-historical-tree-input-lock/v1"
PACKAGE_SCHEMA = "repoe-official-tree-input/v1"


def validate(lock, source_id):
    require(isinstance(lock, dict) and lock.get("schema") == LOCK_SCHEMA and lock.get("source_id") == source_id,
            "Historical tree input lock identity differs")
    package = lock["package"]
    require(isinstance(package, dict) and package.get("repository") == "ackness/repoe",
            "Historical game input must be owned by RePoE")
    git_address(package, package["path"])
    validate_pin(package)
    validate_pin(package["manifest"])
    require(type(package.get("expanded_bytes")) is int and package["expanded_bytes"] > 0,
            "Invalid historical tree expanded size")
    return package


def verify(directory, package, source, receipt):
    directory = safe_directory(directory)
    require({path.name for path in directory.iterdir()} == {"data.json", "manifest.json", "receipt.json"},
            "Incomplete or unlisted historical tree import")
    manifest = decode(pinned(regular(directory / "manifest.json"), package["manifest"]))
    require(isinstance(manifest, dict) and manifest.get("schema") == PACKAGE_SCHEMA
            and manifest.get("status") == "historical-diagnostic-candidate"
            and manifest.get("production_ready") is False,
            "Historical tree package is not a diagnostic input")
    require(manifest.get("source") == source, "Historical tree source identity differs")
    files = manifest.get("files")
    require(isinstance(files, dict) and set(files) == {"data.json"}, "Unexpected historical tree files")
    validate_pin(files["data.json"])
    require(files["data.json"].get("sha256") == source["sha256"], "Historical tree raw identity differs")
    raw = pinned(regular(directory / "data.json"), files["data.json"])
    require(isinstance(decode(raw), dict), "Historical tree must be a JSON object")
    producer = manifest.get("producer", {})
    require(isinstance(producer, dict) and producer.get("repository") == "ackness/repoe"
            and isinstance(producer.get("revision"), str)
            and re.fullmatch(r"[a-f0-9]{40}", producer.get("revision", ""))
            and producer.get("module") == "RePoE.official_tree_input"
            and isinstance(producer.get("files"), dict)
            and "RePoE/official_tree_input.py" in producer["files"],
            "Missing historical tree producer identity")
    # Producer pins are owner declarations bound by the package. No source code
    # is executed or added to a runtime payload by this import.
    for name, pin in producer["files"].items():
        require(isinstance(name, str) and name.startswith("RePoE/") and name.endswith(".py"),
                "Unexpected tree producer path")
        relative_path(name)
        validate_pin(pin)
    validate_pin(manifest.get("plan"))
    require(regular(directory / "receipt.json") == encoded(receipt), "Historical tree receipt differs")
    return directory / "data.json"


def prepare(root, source_id, source, offline=False, fetcher=None):
    """Verify/import exact external bytes without an upstream or bare-cache fallback."""
    root = safe_directory(root)
    lock_path = root / "pipeline/tree/tree-inputs.lock.json"
    source_path = root / "pipeline/imports/sources.lock.json"
    lock_raw, source_raw = regular(lock_path), regular(source_path)
    source = decode(encoded(source))
    require(decode(source_raw)["sources"].get(source_id) == source,
            "Historical tree declaration differs from sources.lock.json")
    package = validate(decode(lock_raw), source_id)
    # Quarantine is a PoBR interpretation policy, never a source-owner rewrite.
    source = {key: value for key, value in source.items() if key != "corrections"}
    require(source.get("kind") == "official-passive-tree", "Expected a historical official tree")
    receipt = {"schema": "pobr-historical-tree-import/v1", "source_id": source_id,
               "import_lock_sha256": sha(lock_raw), "source": source, "package": package,
               "adoption": "not-adopted",
               "scope": "Pinned historical raw input only; no new acquisition observation, patch applicability, "
                        "semantic, rights or runtime adoption decision."}
    pool = safe_directory(root / ".cache/tree-inputs")
    cache = safe_directory(root / ".cache/data-import-downloads")
    output = pool / sha(lock_raw)

    def unchanged():
        require(regular(lock_path) == lock_raw and regular(source_path) == source_raw,
                "Historical tree input declarations changed during import")

    if output.exists():
        path = verify(output, package, source, receipt)
        unchanged()
        return path, receipt
    safe_directory(output)
    pool.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".tree-import-", dir=pool) as temporary:
        temporary = Path(temporary)
        archive = acquire(package, package["path"], package, cache, offline,
                          fetcher or GitInputs(temporary / "git"))
        stage = temporary / "input"
        unpack(archive, package, stage)
        write(stage / "receipt.json", encoded(receipt))
        verify(stage, package, source, receipt)
        unchanged()
        require(not output.exists(), "Historical tree import already exists")
        stage.rename(output)
    path = verify(output, package, source, receipt)
    unchanged()
    return path, receipt
