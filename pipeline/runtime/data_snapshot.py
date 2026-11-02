#!/usr/bin/env python3
"""Install validated owner data and build diagnostic domain candidates.

PoBR never regenerates a snapshot from raw tables or vendor Lua; complete
snapshots come from the owner publications (see build_data / locked_update)."""

import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
# These are maintenance inputs/reports, never browser calculation inputs.
MAINTENANCE_FILES = {
    "generated/modifier-audit.json", "generated/parsed_mods.json",
    "generated/parse-coverage.json", "generated/test_pins.json",
    "overlay/stat_id_map.json",
    "generated/translation-audit.json", "generated/locale-generation-receipt.json",
}


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def locked_tree_source(root, source_id):
    """The first migrated source is deliberately a domain, not a product snapshot."""
    document = read_json(root / "pipeline/imports/sources.lock.json")
    if document["schema_version"] != 1:
        raise ValueError("Unsupported source lock schema")
    source = document["sources"][source_id]
    if source["kind"] != "official-passive-tree":
        raise ValueError("Only official passive-tree candidates are supported")
    if source["repository"] != "https://github.com/grindinggear/poe2-skilltree-export" or source["path"] != "data.json":
        raise ValueError("Unexpected official tree location")
    for key, size in [("revision", 40), ("sha256", 64)]:
        if not isinstance(source[key], str) or not re.fullmatch(f"[0-9a-f]{{{size}}}", source[key]):
            raise ValueError(f"Source {key} must be a full immutable digest")
    patch_version(source["upstream_version"])
    if source["game_patch"] is not None:
        patch_version(source["game_patch"])
    if source["realm"] not in {"international", "wegame"}:
        raise ValueError("Unknown source realm")
    if not source["rights"]["owner"] or not source["rights"]["terms"] or not source["rights"]["scope"]:
        raise ValueError("Source rights and usage scope must be recorded")
    return source


def fetch_tree_source(root, source_id, offline=False):
    from pipeline.tree.tree_inputs import prepare
    return prepare(root, source_id, locked_tree_source(root, source_id), offline)[0]


def tree_adapter_inputs(root):
    required = [root / name for name in [
        "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
        "tools/pobr-data-adapter/Cargo.toml", "crates/pobr-data/Cargo.toml",
        "tools/pobr-data-adapter/src/main.rs", "crates/pobr-data/src/lib.rs",
        "pipeline/__init__.py", "pipeline/runtime/__init__.py", "pipeline/tree/__init__.py",
        "pipeline/imports/__init__.py",
        "pipeline/runtime/data_snapshot.py", "pipeline/tree/tree_inputs.py", "pipeline/tree/tree-inputs.lock.json",
        "pipeline/runtime/build_data.py", "pipeline/runtime/data_bundle.py", "pipeline/imports/data_imports.py",
    ]]
    inputs = set(required)
    inputs.update((root / "tools/pobr-data-adapter/src").rglob("*.rs"))
    inputs.update((root / "crates/pobr-data/src").rglob("*.rs"))
    return {p.relative_to(root).as_posix(): digest(p) for p in sorted(inputs)}


def corrected_tree_input(root, source, raw, staging):
    """Quarantine reviewed orphan mappings for diagnostic candidates only."""
    descriptor = source.get("corrections")
    if descriptor is None:
        return raw, None
    relative = f"pipeline/policies/tree-corrections/{source['revision']}.json"
    if descriptor["path"] != relative:
        raise ValueError("Tree corrections must be pinned to the source revision")
    path = root / relative
    if digest(path) != descriptor["sha256"]:
        raise ValueError("Tree correction hash differs from sources.lock.json")
    correction = read_json(path)
    if (correction["schema"] != "pobr-tree-corrections/v1"
            or correction["source_revision"] != source["revision"]
            or correction["input_sha256"] != source["sha256"]
            or correction.get("candidate_only") is not True
            or digest(raw) != source["sha256"]):
        raise ValueError("Tree correction does not match the fixed source bytes/revision")
    document = read_json(raw)
    quarantined = correction["quarantined_override_pairs"]
    if not isinstance(quarantined, list) or not quarantined or not correction["reason"]:
        raise ValueError("Tree corrections require an explicit nonempty quarantine list and reason")
    seen = set()
    for entry in quarantined:
        name, base, replacement = entry["class"], entry["base_skill"], entry["replacement_skill"]
        if not isinstance(name, str) or not name or type(base) is not int or type(replacement) is not int:
            raise ValueError("Invalid tree correction selector or skill ID")
        if (name, base) in seen:
            raise ValueError("Duplicate tree correction")
        seen.add((name, base))
        key = str(base)
        # A future source containing the base, even under a different map key,
        # must be reviewed again. Never remove a mapping from a live graph.
        if (key in document["nodes"] or key in document.get("skillOverrides", {})
                or any(node.get("skill") == base or key in map(str, node.get("out", []))
                       or key in map(str, node.get("in", []))
                       or base in (node.get("unlockConstraint") or {}).get("nodes", [])
                       for node in document["nodes"].values())
                or any(key in {str(edge.get("from")), str(edge.get("to"))}
                       for edge in document.get("edges", []))
                or any(key in map(str, group.get("nodes", [])) for group in document.get("groups", {}).values())):
            raise ValueError(f"Tree correction base {base} is present or referenced; review required")
        classes = [c for c in document["classes"] if c["name"] == name]
        pairs = classes[0].get("overridePairs") if len(classes) == 1 else None
        if not isinstance(pairs, dict) or pairs.get(key) != replacement:
            raise ValueError(f"Tree correction no longer matches {name} {base} -> {replacement}")
        target = document.get("skillOverrides", {}).get(str(replacement))
        if not target or target.get("skill") != replacement:
            raise ValueError(f"Tree correction replacement {replacement} is missing or mismatched")
        del pairs[key]
    normalized = staging / "official-tree-normalized.json"
    write_json(normalized, document)
    return normalized, {"path": relative, "sha256": descriptor["sha256"],
                        "input_sha256": source["sha256"], "normalized_sha256": digest(normalized),
                        "candidate_only": True, "resolution": correction["resolution"],
                        "quarantined_override_pairs": quarantined, "reason": correction["reason"]}


def tree_candidate(root, source_id, output, offline=False):
    """Generate only an isolated tree domain; never publish or advance CURRENT."""
    output = output.resolve()
    published = (root / "data").resolve()
    if output == published or output.is_relative_to(published):
        raise ValueError("Tree candidate output must be outside published data/")
    if output.exists():
        raise FileExistsError(f"Refusing to replace an existing candidate: {output}")
    lock = root / "pipeline/imports/sources.lock.json"
    lock_hash = digest(lock)
    source = locked_tree_source(root, source_id)
    from pipeline.tree import tree_inputs
    raw, imported = tree_inputs.prepare(root, source_id, source, offline)
    # Include the adapter's schema and build inputs, not a timestamp or local path.
    fingerprints = tree_adapter_inputs(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".tree-candidate-", dir=output.parent) as temporary:
        staging = Path(temporary)
        env = dict(os.environ)
        # Candidate generation must never bless existing test or parity pins.
        env.pop("POBR_BLESS_PINS", None)
        adapted_input, corrections = corrected_tree_input(root, source, raw, staging)
        cargo = ["cargo", "run", "--locked", "--quiet", "-p", "pobr-data-adapter"]
        if offline:
            cargo.append("--offline")
        run([*cargo, "--", "--tree", str(adapted_input),
             "--out", str(staging), "--patch", source["upstream_version"]], root, env)
        data = staging / source["upstream_version"]
        expected = ["base/passive_tree.json", "base/passive_tree_meta.json"]
        files = {name: digest(data / name) for name in expected}
        for name in expected:
            read_json(data / name)
        if digest(lock) != lock_hash or digest(raw) != source["sha256"]:
            raise ValueError("Source changed during candidate generation")
        current_inputs = tree_adapter_inputs(root)
        if current_inputs != fingerprints:
            raise ValueError("Adapter inputs changed during candidate generation")
        tree_inputs.verify(raw.parent, imported["package"], imported["source"], imported)
        if corrections is not None and digest(root / corrections["path"]) != corrections["sha256"]:
            raise ValueError("Tree corrections changed during candidate generation")
        if corrections is not None and digest(adapted_input) != corrections["normalized_sha256"]:
            raise ValueError("Normalized tree input changed during candidate generation")
        write_json(data / "_domain_candidate.json", {
            "schema": "pobr-domain-candidate/v1", "domain": "passive-tree",
            "source_id": source_id, "source": source, "source_lock_sha256": lock_hash,
            "source_import": imported,
            "source_corrections": corrections,
            "adapter_inputs": fingerprints, "files": files,
            "publication": "blocked: domain-only candidate; patch applicability and full product validation required",
        })
        # No manifest: this partial domain must not masquerade as a product pack.
        data.replace(output)
    print(f"Tree domain candidate: {output} (not published)")
    return output


def patch_version(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value):
        raise ValueError(f"Invalid data patch: {value!r}")
    return value


def write_json(path, document):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".write-", delete=False) as file:
        temporary = Path(file.name)
        file.write((json.dumps(document, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def runtime_files(data):
    return sorted(p for p in data.rglob("*.json")
                  if p.relative_to(data).parts[0] in {"base", "overlay", "generated", "i18n"}
                  and p.relative_to(data).as_posix() not in MAINTENANCE_FILES)


def manifest(data):
    """Seal actual runtime bytes as schema 3 (fixtures and staged owner data)."""
    version = patch_version(data.name)
    if (data / "_domain_candidate.json").exists():
        raise ValueError("Domain candidates cannot be sealed as product snapshots")
    files = {p.relative_to(data).as_posix(): digest(p) for p in runtime_files(data)}
    if not files:
        raise ValueError("Cannot seal an empty data snapshot")
    for name in files:
        read_json(data / name)
    document = {"schema_version": 3, "poe_version": version,
                "languages": sorted(p.name for p in (data / "i18n").iterdir() if p.is_dir())
                if (data / "i18n").is_dir() else [],
                "domains": {layer: sorted(Path(name).stem for name in files
                                          if Path(name).parts[0] == layer and len(Path(name).parts) == 2)
                            for layer in ["base", "overlay", "generated"]},
                "files": files}
    write_json(data / "manifest.json", document)
    return document


def promote(data_root, destination_root, version, activate=False):
    """Only validated candidates reach here; restore the previous snapshot on I/O failure."""
    if (data_root / version / "_domain_candidate.json").exists():
        raise ValueError("Domain candidates cannot be published as product snapshots")
    target = destination_root / version
    backup_root = Path(tempfile.mkdtemp(prefix="data-backup-", dir=data_root.parent.parent))
    backup = backup_root / version
    current = destination_root / "CURRENT"
    marker = data_root.parent / "CURRENT"
    marker.write_text(version + "\n", encoding="utf-8")
    had_target = target.exists()
    installed = False
    try:
        if had_target:
            target.replace(backup)
        (data_root / version).replace(target)
        installed = True
        if activate:
            marker.replace(current)
    except BaseException:
        try:
            if installed:
                target.replace(data_root / version)
            if had_target and backup.exists():
                backup.replace(target)
        except OSError as error:
            # Outside the candidate cleanup tree: never delete the recoverable original.
            raise OSError(f"Publication rollback failed; original snapshot retained at {backup}") from error
        shutil.rmtree(backup_root)
        raise
    else:
        shutil.rmtree(backup_root)


def run(command, root, env):
    subprocess.run(command, cwd=root, env=env, check=True)


@contextmanager
def workspace_lock(root):
    cache = root / ".cache"
    cache.mkdir(exist_ok=True)
    with (cache / "data-update.lock").open("a", encoding="utf-8") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError("Another data update owns this workspace") from error
        yield


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    fetch = sub.add_parser("fetch-tree-source")
    fetch.add_argument("--source", default="official-passive-tree")
    fetch.add_argument("--offline", action="store_true")
    tree = sub.add_parser("tree-candidate")
    tree.add_argument("--source", default="official-passive-tree")
    tree.add_argument("--out", type=Path, required=True)
    tree.add_argument("--offline", action="store_true")
    native = sub.add_parser("native-candidate", help="Replay a byte-pinned native recipe offline; never publish")
    native.add_argument("--recipe", type=Path, required=True)
    native.add_argument("--recipe-sha256", required=True)
    for binding in ["facts", "source-lock", "descriptions", "official-tree", "compiler", "out"]:
        native.add_argument(f"--{binding}", type=Path, required=True)
    native.add_argument("--player-context", type=Path, help="Required only for the explicit v2 actor recipe")
    bundle = sub.add_parser("bundle")
    bundle.add_argument("--out", type=Path, required=True)
    bundle.add_argument("--version")
    check_bundle = sub.add_parser("verify-bundle")
    check_bundle.add_argument("--bundle", type=Path, required=True)
    check_bundle.add_argument("--expected-id", help="Trusted SHA-256 bundle identity from the selected source lock")
    args = parser.parse_args()
    if args.command == "fetch-tree-source":
        print(fetch_tree_source(ROOT, args.source, args.offline))
    elif args.command == "tree-candidate":
        tree_candidate(ROOT, args.source, args.out, args.offline)
    elif args.command == "native-candidate":
        from pipeline.native.native_candidate import replay

        receipt = replay(ROOT, args.recipe, args.recipe_sha256, args.facts, args.source_lock,
                         args.descriptions, args.official_tree, args.compiler, args.out, args.player_context)
        print(f"Native candidate complete: {receipt['assembly_key']} (not published; calculation not admitted)")
    elif args.command in {"bundle", "verify-bundle"}:
        from pipeline.runtime.data_bundle import create, verify

        if args.command == "bundle":
            with workspace_lock(ROOT):
                receipt = create(ROOT, args.out, args.version)
        else:
            receipt = verify(args.bundle, args.expected_id)
        print(f"Runtime bundle: {receipt['bundle_id']} (byte integrity verified; not activated)")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"data snapshot: {error}") from error
