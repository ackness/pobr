#!/usr/bin/env python3
"""Generate and validate an isolated data snapshot before replacing a published one."""

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
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[1]
# These are maintenance inputs/reports, never browser calculation inputs.
MAINTENANCE_FILES = {
    "generated/modifier-audit.json", "generated/parsed_mods.json",
    "generated/parse-coverage.json", "generated/test_pins.json",
    "overlay/stat_id_map.json",
    "generated/translation-audit.json",
}


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def locked_tree_source(root, source_id):
    """The first migrated source is deliberately a domain, not a product snapshot."""
    document = read_json(root / "pipeline/sources.lock.json")
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
    source = locked_tree_source(root, source_id)
    cached = root / ".cache/sources" / source["sha256"] / "data.json"
    if cached.exists():
        if digest(cached) != source["sha256"]:
            raise ValueError("Cached source hash differs from sources.lock.json")
        return cached
    if offline:
        raise FileNotFoundError(f"Source not cached for offline replay: {source_id}")
    url = f"https://raw.githubusercontent.com/grindinggear/poe2-skilltree-export/{source['revision']}/data.json"
    # Bound the download, then verify before exposing it to any producer.
    with urlopen(url, timeout=45) as response:
        raw = response.read(25_000_001)
    if len(raw) > 25_000_000 or hashlib.sha256(raw).hexdigest() != source["sha256"]:
        raise ValueError("Downloaded source size/hash differs from sources.lock.json")
    cached.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=cached.parent, prefix=".download-", delete=False) as file:
        temporary = Path(file.name)
        file.write(raw)
    try:
        temporary.replace(cached)
    finally:
        temporary.unlink(missing_ok=True)
    return cached


def tree_adapter_inputs(root):
    required = [root / name for name in [
        "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
        "tools/pobr-data-adapter/Cargo.toml", "crates/pobr-data/Cargo.toml",
        "tools/pobr-data-adapter/src/main.rs", "crates/pobr-data/src/lib.rs",
    ]]
    inputs = set(required)
    inputs.update((root / "tools/pobr-data-adapter/src").rglob("*.rs"))
    inputs.update((root / "crates/pobr-data/src").rglob("*.rs"))
    return {p.relative_to(root).as_posix(): digest(p) for p in sorted(inputs)}


def tree_candidate(root, source_id, output, offline=False):
    """Generate only an isolated tree domain; never publish or advance CURRENT."""
    output = output.resolve()
    published = (root / "data").resolve()
    if output == published or output.is_relative_to(published):
        raise ValueError("Tree candidate output must be outside published data/")
    if output.exists():
        raise FileExistsError(f"Refusing to replace an existing candidate: {output}")
    lock = root / "pipeline/sources.lock.json"
    lock_hash = digest(lock)
    source = locked_tree_source(root, source_id)
    raw = fetch_tree_source(root, source_id, offline)
    # Include the adapter's schema and build inputs, not a timestamp or local path.
    fingerprints = tree_adapter_inputs(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".tree-candidate-", dir=output.parent) as temporary:
        staging = Path(temporary)
        env = dict(os.environ)
        # Candidate generation must never bless existing test or parity pins.
        env.pop("POBR_BLESS_PINS", None)
        cargo = ["cargo", "run", "--locked", "--quiet", "-p", "pobr-data-adapter"]
        if offline:
            cargo.append("--offline")
        run([*cargo, "--", "--tree", str(raw),
             "--out", str(staging), "--patch", source["upstream_version"]], root, env)
        data = staging / source["upstream_version"]
        expected = ["base/passive_tree.json", "base/passive_tree_meta.json"]
        files = {name: digest(data / name) for name in expected}
        for name in expected:
            read_json(data / name)
        if digest(lock) != lock_hash or digest(raw) != source["sha256"]:
            raise ValueError("Source changed during candidate generation")
        if tree_adapter_inputs(root) != fingerprints:
            raise ValueError("Adapter inputs changed during candidate generation")
        write_json(data / "_domain_candidate.json", {
            "schema": "pobr-domain-candidate/v1", "domain": "passive-tree",
            "source_id": source_id, "source": source, "source_lock_sha256": lock_hash,
            "adapter_inputs": fingerprints, "files": files,
            "publication": "blocked: domain-only candidate; patch applicability and full product validation required",
        })
        # No manifest: this partial domain must not masquerade as a product pack.
        data.replace(output)
    print(f"Tree domain candidate: {output} (not published)")
    return output


def tree_fingerprint(path):
    return {p.relative_to(path).as_posix(): digest(p) for p in path.rglob("*") if p.is_file()}


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
    """Seal actual runtime bytes; patches and shared curation remain separate layers."""
    version = patch_version(data.name)
    if (data / "_domain_candidate.json").exists():
        raise ValueError("Domain candidates cannot be sealed as product snapshots")
    if (data / "_drift.json").exists():
        raise ValueError("Refusing to publish a snapshot with adapter column drift")
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


def record_export(config, raw):
    """Called only after the exporter succeeds; bind every configured table to its patch."""
    configuration = read_json(config)
    files = {f"{language}/{table['name']}.json": None
             for language in configuration["translations"] for table in configuration["tables"]}
    for name in files:
        path = raw / name
        if not isinstance(read_json(path), list):
            raise ValueError(f"Exported table must be an array: {name}")
        files[name] = digest(path)
    document = {"schema": "official-raw-export/v1", "patch": patch_version(configuration["patch"]),
                "config_sha256": digest(config), "files": files}
    write_json(raw / "export-source.json", document)


@contextmanager
def candidate(root, version):
    """Copy every writable input. Historical snapshots are read-only validation inputs."""
    source = root / "data"
    cache = root / ".cache"
    cache.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="data-update-", dir=cache) as temporary:
        data_root = Path(temporary) / "data"
        data_root.mkdir()
        for path in source.iterdir():
            destination = data_root / path.name
            if path.name in {version, "overlay-common"}:
                shutil.copytree(path, destination)
            elif path.is_dir() and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", path.name):
                destination.symlink_to(path.resolve(), target_is_directory=True)
        (data_root / version).mkdir(exist_ok=True)
        (data_root / "CURRENT").write_text(version + "\n", encoding="utf-8")
        # Corpus discovery derives examples/ from the data directory's parent.
        if (root / "examples").is_dir():
            (data_root.parent / "examples").symlink_to((root / "examples").resolve(), target_is_directory=True)
        yield data_root


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


def export_tables(root):
    with workspace_lock(root):
        config = root / "pipeline/config.json"
        config_hash = digest(config)
        run(["pnpm", "install", "--frozen-lockfile"], root / "pipeline", os.environ)
        run(["pnpm", "exec", "pathofexile-dat"], root / "pipeline", os.environ)
        if digest(config) != config_hash:
            raise ValueError("Export configuration changed during download; refusing to record provenance")
        record_export(config, root / "pipeline/tables")
        run(["python3", "pipeline/gem-quality/advance-receipt.py", "record-export",
             "--config", "pipeline/config.json", "--raw", "pipeline/tables/English",
             "--out", "pipeline/tables/quality-export-source.json"], root, os.environ)


def regenerate(root, refresh_dictionary=False, activate=False, base_only=False, operation=None, arguments=(),
               reference_audit=None):
    with workspace_lock(root):
        _regenerate(root, refresh_dictionary, activate, base_only, operation, arguments, reference_audit)


def _regenerate(root, refresh_dictionary=False, activate=False, base_only=False, operation=None, arguments=(),
                reference_audit=None):
    # Full legacy generation still extracts vendor data. Adapting official tables
    # validates our effective rules without implicitly executing the reference.
    if reference_audit is None:
        reference_audit = not base_only
    version = patch_version(read_json(root / "pipeline/config.json")["patch"])
    config_hash = digest(root / "pipeline/config.json")
    if not operation and os.environ.get("POBR_EXPECTED_PATCH", version) != version:
        raise ValueError("Configured patch changed before generation")
    arguments = list(arguments)
    if operation == "dictionary":
        version = patch_version(arguments[arguments.index("--version") + 1]) if "--version" in arguments else patch_version(
            (root / "data/CURRENT").read_text(encoding="utf-8").strip())
    elif operation == "modifiers":
        source = Path(arguments[arguments.index("--data") + 1]) if "--data" in arguments else root / "data" / (
            root / "data/CURRENT").read_text(encoding="utf-8").strip()
        source = source.resolve()
        version = patch_version(source.name)
        if source != (root / "data" / version).resolve():
            raise ValueError("Rule extraction requires a repository snapshot")
        if "--data" in arguments:
            index = arguments.index("--data")
            del arguments[index:index + 2]
    old = patch_version(os.environ.get("OLD_PATCH") or (root / "data/CURRENT").read_text(encoding="utf-8").strip())
    if (base_only or operation) and not (root / "data" / version).is_dir():
        raise ValueError("Base adaptation requires an existing snapshot; use data:regen for a new version")
    watched = {root / "data" / name for name in {version, old, "overlay-common"}}
    original = {path: tree_fingerprint(path) for path in watched}
    with candidate(root, version) as data_root:
        env = {**os.environ, "POBR_DATA_ROOT": str(data_root), "POBR_DATA_VERSION": version,
               "POBR_CANDIDATE": "1", "POBR_DEFER_MODIFIER_AUDIT": "1", "OLD_PATCH": old}
        # Golden snapshots linked above must never be blessed by generation.
        env.pop("POBR_BLESS_PINS", None)
        if operation == "dictionary":
            run(["node", "pipeline/gen-zh-cn.mjs", *arguments], root, env)
        elif operation == "modifiers":
            run(["bash", "pipeline/refresh-modifiers.sh", "--data", str(data_root / version), *arguments], root, env)
        elif base_only:
            run(["cargo", "run", "--quiet", "-p", "pobr-data-adapter", "--", "--raw", "pipeline/tables",
                 "--out", str(data_root), "--patch", version, "--strict-columns"], root, env)
        else:
            run(["bash", "pipeline/regen-all.sh"], root, env)
        if not operation:
            dictionary = ["node", "pipeline/gen-zh-cn.mjs", "--version", version]
            if refresh_dictionary:
                dictionary.append("--refresh")
            run(dictionary, root, env)
        # Auditors use the normal hash-validating loader. Seal changed candidate
        # runtime bytes before they read them; audit outputs are maintenance-only.
        manifest(data_root / version)
        audit = ["bash", "pipeline/refresh-modifiers.sh", "--data", str(data_root / version), "--audit-only"]
        if not reference_audit:
            audit.append("--offline")
        baseline = root / "data" / old / "generated/modifier-audit.json"
        if baseline.is_file():
            audit += ["--baseline", str(baseline)]
        if operation != "modifiers":
            run(audit, root, env)
        run(["bash", "devs/scripts/driver.sh", "versions"], root, env)
        run(["cargo", "test", "--quiet", "-p", "pobr-gamedata"], root, env)
        run(["cargo", "test", "--quiet", "-p", "pobr-build", "--test", "parity", "parity_no_regression"], root, env)
        for path, fingerprint in original.items():
            if tree_fingerprint(path) != fingerprint:
                raise ValueError(f"Snapshot changed during generation; refusing to overwrite {path.name}")
        if digest(root / "pipeline/config.json") != config_hash:
            raise ValueError("Configured patch changed during generation; refusing publication")
        promote(data_root, root / "data", version, activate)
    print(f"Published validated data/{version}" + (" and advanced CURRENT" if activate else ""))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    seal = sub.add_parser("manifest")
    seal.add_argument("--data", type=Path, required=True)
    record = sub.add_parser("record-export")
    record.add_argument("--config", type=Path, required=True)
    record.add_argument("--raw", type=Path, required=True)
    regen = sub.add_parser("regenerate")
    regen.add_argument("--refresh-dictionary", action="store_true")
    regen.add_argument("--activate", action="store_true")
    adapt = sub.add_parser("adapt")
    adapt.add_argument("--reference-audit", action="store_true",
                       help="Also compare parser behavior with the installed PoB2 reference")
    sub.add_parser("export")
    fetch = sub.add_parser("fetch-tree-source")
    fetch.add_argument("--source", default="official-passive-tree")
    fetch.add_argument("--offline", action="store_true")
    tree = sub.add_parser("tree-candidate")
    tree.add_argument("--source", default="official-passive-tree")
    tree.add_argument("--out", type=Path, required=True)
    tree.add_argument("--offline", action="store_true")
    for name in ["dictionary", "modifiers"]:
        sub.add_parser(name).add_argument("arguments", nargs=argparse.REMAINDER)
    sub.add_parser("patch")
    args = parser.parse_args()
    if args.command == "manifest":
        manifest(args.data)
    elif args.command == "record-export":
        record_export(args.config, args.raw)
    elif args.command == "patch":
        print(patch_version(read_json(ROOT / "pipeline/config.json")["patch"]))
    elif args.command == "export":
        export_tables(ROOT)
    elif args.command == "fetch-tree-source":
        print(fetch_tree_source(ROOT, args.source, args.offline))
    elif args.command == "tree-candidate":
        tree_candidate(ROOT, args.source, args.out, args.offline)
    else:
        arguments = getattr(args, "arguments", [])
        if arguments[:1] == ["--"]:
            arguments = arguments[1:]
        regenerate(ROOT, getattr(args, "refresh_dictionary", False), getattr(args, "activate", False),
                   base_only=args.command == "adapt",
                   operation=args.command if args.command in {"dictionary", "modifiers"} else None,
                   arguments=arguments,
                   reference_audit=args.reference_audit if args.command == "adapt" else None)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"data snapshot: {error}") from error
