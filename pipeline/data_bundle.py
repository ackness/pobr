"""Seal a portable runtime data combination, including its shared rule layer."""

import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile


SCHEMA = "pobr-data-bundle/v1"
LAYERS = {"base", "overlay", "generated", "i18n"}


def encoded(document):
    return json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"Duplicate JSON key: {key}")
            result[key] = value
        return result

    def invalid(value):
        raise ValueError(f"Non-JSON number: {value}")

    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid)


def relative_path(name):
    if (not isinstance(name, str) or not name or "\\" in name or ":" in name
            or any(part in {"", ".", ".."} for part in name.split("/"))):
        raise ValueError(f"Noncanonical bundle path: {name!r}")
    return Path(name)


def read_file(root, name):
    relative = relative_path(name)
    path = root
    for part in relative.parts:
        path = path / part
        if path.is_symlink():
            raise ValueError(f"Symlinks are not runtime bundle inputs: {name}")
    return path.read_bytes()


def walk_error(error):
    raise error


def json_paths(root):
    try:
        mode = root.lstat().st_mode
    except FileNotFoundError:
        return []
    if not stat.S_ISDIR(mode):
        raise ValueError(f"Shared runtime layer must be a directory without symlinks: {root}")
    found = []
    for directory, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
        for name in dirs + files:
            path = Path(directory) / name
            if path.is_symlink():
                raise ValueError(f"Symlinks are not runtime bundle inputs: {path}")
        found.extend((Path(directory) / name).relative_to(root).as_posix()
                     for name in files if name.endswith(".json"))
    return sorted(found)


def validate_declarations(manifest, inventory):
    # Match DataManifest/DomainSections deserialization and schema-3 closure in
    # crates/pobr-gamedata/src/manifest.rs, including the supported flat shape.
    def strings(value):
        return isinstance(value, list) and all(isinstance(item, str) for item in value)

    languages = manifest.get("languages")
    domains = manifest.get("domains")
    if isinstance(domains, list):
        domains = {"base": domains}
    if not strings(languages) or not isinstance(domains, dict) or not strings(domains.get("base")):
        raise ValueError("Invalid runtime manifest domain/language envelope")
    for section in ["base", "overlay", "generated"]:
        values = domains.get(section, [])
        if not strings(values):
            raise ValueError(f"Invalid runtime manifest domain list: {section}")
        for domain in values:
            name = f"{section}/{domain}.json"
            if name not in inventory:
                raise ValueError(f"Declared domain lacks an inventory entry: {name}")
    for language in languages:
        if not any(name.startswith(f"i18n/{language}/") for name in inventory):
            raise ValueError(f"Declared language lacks files: {language}")


def snapshot_inputs(root, version):
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", version):
        raise ValueError(f"Invalid data version: {version!r}")
    data = root / "data"
    source = data / version
    if (source / "_domain_candidate.json").exists() or (source / "_drift.json").exists():
        raise ValueError("Unaccepted domain/drift candidates cannot become runtime bundles")
    raw = read_file(data, f"{version}/manifest.json")
    manifest = decode(raw)
    if (not isinstance(manifest, dict) or type(manifest.get("schema_version")) is not int
            or manifest["schema_version"] != 3 or manifest.get("poe_version") != version):
        raise ValueError("Runtime bundles require a matching schema-3 snapshot")
    inventory = manifest.get("files")
    if not isinstance(inventory, dict) or not inventory:
        raise ValueError("Snapshot runtime inventory is missing or empty")
    validate_declarations(manifest, inventory)
    files = {f"data/{version}/manifest.json": raw, "data/CURRENT": (version + "\n").encode()}
    runtime_hashes = {}
    for name, expected in sorted(inventory.items()):
        path = relative_path(name)
        if path.parts[0] not in LAYERS or len(path.parts) < 2 or path.suffix != ".json":
            raise ValueError(f"Invalid runtime snapshot path: {name}")
        value = read_file(data, f"{version}/{name}")
        if not isinstance(expected, str) or not re.fullmatch(r"[a-f0-9]{64}", expected) or sha256(value) != expected:
            raise ValueError(f"Snapshot hash mismatch: {name}")
        decode(value)
        files[f"data/{version}/{name}"] = value
        runtime_hashes[name] = expected
    common = data / "overlay-common"
    for name in json_paths(common):
        value = read_file(data, f"overlay-common/{name}")
        decode(value)
        files[f"data/overlay-common/{name}"] = value
        runtime_hashes[f"overlay-common/{name}"] = sha256(value)
    # Match GameData::identity for the default pack, excluding local user patches.
    identity = sha256(encoded(["pobr-data-pack-v1", 3, version, runtime_hashes]))
    return files, identity


def document(version, files, runtime_identity):
    payload = {
        "schema": SCHEMA,
        "data_version": version,
        "required_manifest_schema": 3,
        "runtime_identity": runtime_identity,
        "files": {name: sha256(value) for name, value in sorted(files.items())},
        "verification": "runtime byte integrity; not client-source or game-semantic certification",
        "user_patch": "excluded; apply and validate separately",
    }
    return {**payload, "bundle_id": sha256(encoded(payload))}


def verify(output, expected_id=None):
    output = Path(output)
    receipt = decode(read_file(output, "bundle.json"))
    if not isinstance(receipt, dict):
        raise ValueError("Runtime bundle receipt must be an object")
    if expected_id is not None and (
            not re.fullmatch(r"[a-f0-9]{64}", expected_id) or receipt.get("bundle_id") != expected_id):
        raise ValueError("Runtime bundle differs from its expected fixed identity")
    version = receipt.get("data_version")
    files, identity = snapshot_inputs(output, version)
    if receipt != document(version, files, identity):
        raise ValueError("Runtime bundle receipt or identity differs from its content")
    expected = set(files) | {"bundle.json"}
    actual = set()
    for directory, dirs, names in os.walk(output, followlinks=False, onerror=walk_error):
        for name in dirs + names:
            path = Path(directory) / name
            if path.is_symlink():
                raise ValueError(f"Symlinks are not allowed in a runtime bundle: {path}")
        actual.update((Path(directory) / name).relative_to(output).as_posix() for name in names)
    if actual != expected:
        raise ValueError("Runtime bundle contains missing or unlisted files")
    if read_file(output, "data/CURRENT") != (version + "\n").encode():
        raise ValueError("Runtime bundle CURRENT differs from its version")
    return receipt


def create(root, output, version=None):
    root = Path(root).resolve()
    output = Path(output).resolve()
    data = (root / "data").resolve()
    if output == data or output.is_relative_to(data) or data.is_relative_to(output):
        raise ValueError("Runtime bundle output must be separate from published data/")
    if output.exists():
        raise FileExistsError(f"Refusing to replace an existing runtime bundle: {output}")
    version = version or (data / "CURRENT").read_text(encoding="utf-8").strip()
    files, identity = snapshot_inputs(root, version)
    receipt = document(version, files, identity)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".data-bundle-", dir=output.parent) as temporary:
        stage = Path(temporary)
        for name, raw in files.items():
            path = stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        (stage / "bundle.json").write_bytes(encoded(receipt) + b"\n")
        verify(stage)
        if snapshot_inputs(root, version) != (files, identity):
            raise ValueError("Runtime inputs changed during bundle generation")
        # Reserve exclusively; the receipt is the completion marker, written last.
        # Failed completion leaves an invalid partial directory, never a valid pack.
        output.mkdir()
        (stage / "data").rename(output / "data")
        os.link(stage / "bundle.json", output / "bundle.json")
    return receipt
