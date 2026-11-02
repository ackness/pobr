"""Temporarily compose pinned external game/language data for builds and real tests.

The source tree owns rules and locks. Data owners publish the two parts; PoBR
never creates a data Git ref. Downloads are hash-addressed, disposable inputs.
"""

import argparse
import json
from pathlib import Path
import re
import shutil
import tempfile

import data_bundle as bundle
from data_imports import GitInputs, acquire, encoded, pinned, regular, require, unpack, write

ROOT = Path(__file__).resolve().parents[1]
LOCK_SCHEMA = "pobr-build-data-lock/v2"
LEGACY_LOCK_SCHEMA = "pobr-build-data-lock/v1"
LOCK_SCHEMAS = (LEGACY_LOCK_SCHEMA, LOCK_SCHEMA)
PART_SCHEMA = "pobr-build-data-part/v1"
OWNERS = {"game": "ackness/repoe", "language": "ackness/poe2-zh-cn-stats"}


def safe_directory(path):
    path = Path(path).absolute()
    require(".." not in path.parts and all(not p.is_symlink() for p in (path, *path.parents)),
            "Unsafe data directory")
    return path


def member_kind(name, version):
    parts = bundle.relative_path(name).parts
    if len(parts) == 3 and parts[:2] == ("data", "overlay-common") and parts[-1].endswith(".json"):
        return "game"
    require(len(parts) >= 4 and parts[:2] == ("data", version)
            and parts[2] in bundle.LAYERS and parts[-1].endswith(".json"),
            f"Invalid external runtime member: {name}")
    return "language" if parts[2] == "i18n" else "game"


def validate(lock):
    from data_imports import validate_pin
    require(lock.get("schema") in LOCK_SCHEMAS, "Unknown build data lock")
    if lock["schema"] == LOCK_SCHEMA:
        import crafting_inputs
        require(isinstance(lock.get("crafting"), dict), "The v2 build lock requires crafting facts")
        crafting_inputs.validate(lock["crafting"])
    else:
        require("crafting" not in lock, "Crafting projection requires the v2 build lock")
    target = lock["target"]
    require(set(target) == {"game", "game_patch", "realm"} and target["game"] == "poe2"
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", target["game_patch"])
            and target["realm"] in ("international", "china"), "Invalid build target")
    validate_pin(lock["snapshot_manifest"])
    require(re.fullmatch(r"[a-f0-9]{64}", lock["runtime_identity"]), "Invalid build runtime identity")
    require(set(lock["packages"]) == set(OWNERS), "Both independent data owners are required")
    for kind, spec in lock["packages"].items():
        require(spec.get("format", "zip") in ("zip", "locale-git-files"), "Unknown data package format")
        validate_pin(spec["manifest"])
        if spec.get("format", "zip") == "zip":
            validate_pin(spec)
            require(type(spec["expanded_bytes"]) is int and spec["expanded_bytes"] > 0,
                    "Invalid expanded data size")
        else:
            require(kind == "language", "Git file projection is language-only")
        bundle.relative_path(spec["path"])
        require(spec.get("publication") in ("pending", "local-only", "published"), "Invalid data publication status")
        if spec["publication"] in ("published", "local-only"):
            require(spec.get("repository") == OWNERS[kind], "Data package repository violates ownership")
            from data_imports import git_address
            git_address(spec, spec["path"])
        else:
            require(spec.get("repository") is None and spec.get("revision") is None,
                    "Unpublished data must not invent a remote pin")
    require(isinstance(lock.get("shared_files"), dict), "Missing pinned local shared rules")
    for name, expected in lock["shared_files"].items():
        require(len(bundle.relative_path(name).parts) == 1 and name.endswith(".json"), "Invalid shared rule name")
        validate_pin(expected)
    return target["game_patch"]


def prepare(lock_path, output, cache, shared_root=ROOT / "data/overlay-common",
            offline=False, local_parts=None, fetcher=None):
    """Verify both owners before publishing a complete, exact schema-3 bundle.

    Explicit local parts authenticate pending handoffs only. No path search or
    tracked snapshot fallback occurs. Offline mode only uses authenticated cache.
    """
    lock_raw = regular(lock_path)
    lock = bundle.decode(lock_raw)
    version = validate(lock)
    output, cache = safe_directory(output), safe_directory(cache)
    require(not output.exists(), f"Fresh data output required: {output}")
    require(not output.is_relative_to(ROOT / "data"), "Prepare outside source data; use --install")
    local_parts = local_parts or {}
    require(set(local_parts) <= set(OWNERS) | ({"crafting"} if lock["schema"] == LOCK_SCHEMA else set()),
            "Unknown local data part")
    cache.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".build-data-", dir=output.parent) as temporary:
        temp = Path(temporary)
        fetcher = fetcher or GitInputs(temp / "git")
        files = {}
        for kind, spec in lock["packages"].items():
            part = temp / kind
            if spec.get("format", "zip") == "locale-git-files":
                # Retained language publication is already a pinned Git tree.
                # Authenticate all evidence, but project only exact i18n bytes.
                from data_bundle import relative_path
                from data_imports import validate_pin
                local = Path(local_parts[kind]) if kind in local_parts else None
                require(local is not None or spec["publication"] == "published",
                        "Unpublished language package: explicit local handoff required")
                def get(name, expected):
                    return pinned(regular(local / relative_path(name)), expected) if local is not None else acquire(
                        spec, (Path(spec["path"]).parent / name).as_posix(), expected, cache, offline, fetcher)
                manifest_raw = pinned(regular(local / "manifest.json"), spec["manifest"]) if local is not None else acquire(
                    spec, spec["path"], spec["manifest"], cache, offline, fetcher)
                manifest = bundle.decode(manifest_raw)
                require(manifest.get("schema") == "poe2-retained-locale-publication/v1"
                        and manifest.get("target") == lock["target"], "Retained language target/schema mismatch")
                for name, expected in manifest["files"].items():
                    relative_path(name)
                    validate_pin(expected)
                    require(name.startswith(("i18n/", "evidence/")) and name.endswith(".json"),
                            "Unrecognized retained language member")
                    raw = get(name, expected)
                    if name.startswith("i18n/"):
                        destination = f"data/{version}/{name}"
                        require(member_kind(destination, version) == kind, "Cross-owner language member")
                        require(destination not in files, "Duplicate external data member")
                        files[destination] = raw
            else:
                if kind in local_parts:
                    archive = pinned(regular(local_parts[kind]), spec)
                else:
                    require(spec["publication"] == "published", f"Unpublished {kind} package: explicit local handoff required")
                    archive = acquire(spec, spec["path"], spec, cache, offline, fetcher)
                manifest = unpack(archive, spec, part)
                require(manifest.get("schema") == PART_SCHEMA and manifest.get("kind") == kind
                        and manifest.get("target") == lock["target"], "External data part target/kind mismatch")
                require(manifest["files"], "Empty external data part")
                for name in manifest["files"]:
                    if name == f"data/{version}/manifest.json":
                        require(kind == "game", "Snapshot manifest belongs to game package")
                    else:
                        require(member_kind(name, version) == kind, "Cross-owner data member")
                    require(name not in files, "Duplicate external data member")
                    files[name] = regular(part / name)
        crafting_receipt = None
        if lock["schema"] == LOCK_SCHEMA:
            import crafting_inputs
            name = f"data/{version}/overlay/trade_crafting_sources.json"
            require(name not in files, "Game part contains PoBR-projected crafting data")
            files[name], crafting_receipt = crafting_inputs.materialize(
                lock["crafting"], lock["target"], cache, temp / "crafting", offline, fetcher,
                local_archive=local_parts.get("crafting"))
        snapshot_path = f"data/{version}/manifest.json"
        snapshot = bundle.decode(pinned(files[snapshot_path], lock["snapshot_manifest"]))
        require(snapshot.get("schema_version") == 3 and snapshot.get("poe_version") == version,
                "External snapshot version/schema mismatch")
        expected = {snapshot_path, *(f"data/{version}/{name}" for name in snapshot["files"])}
        version_files = {n for n in files if not n.startswith("data/overlay-common/")}
        require(version_files == expected, "Combined data part inventory differs from snapshot")
        files["data/CURRENT"] = (version + "\n").encode()
        for name, expected in lock["shared_files"].items():
            destination = f"data/overlay-common/{name}"
            require(destination not in files, "Duplicate external/local shared member")
            raw = bundle.read_file(Path(shared_root), name)
            files[destination] = pinned(raw, expected)
        stage = temp / "result"
        for name, raw in files.items():
            write(stage / name, raw)
        _, runtime_identity = bundle.snapshot_inputs(stage, version)
        require(runtime_identity == lock["runtime_identity"], "Prepared runtime identity mismatch")
        receipt = bundle.document(version, files, runtime_identity)
        write(stage / "bundle.json", encoded(receipt))
        bundle.verify(stage, receipt["bundle_id"])
        require(regular(lock_path) == lock_raw, "Build data lock changed during prepare")
        if crafting_receipt is not None:
            crafting_inputs.record(cache, lock_raw, crafting_receipt)
        stage.rename(output)
    return receipt


def install(prepared, data_root, keep_current=False):
    """Install only the ignored version directory; never rewrite CURRENT/rules.

    Existing files are accepted only if the complete bundle matches. A partial,
    corrupt or different version fails and is left intact for diagnosis.
    """
    receipt = bundle.verify(prepared)
    version = receipt["data_version"]
    data_root = safe_directory(data_root)
    current = regular(data_root / "CURRENT")
    require(keep_current or current == (version + "\n").encode(), "Install CURRENT mismatch")
    # Verify both local policies and externally retained shared files against
    # the exact prepared identity; ownership remains in the input manifests.
    expected_shared = {name.removeprefix("data/overlay-common/"): digest
                       for name, digest in receipt["files"].items() if name.startswith("data/overlay-common/")}
    existing_shared = set(bundle.json_paths(data_root / "overlay-common"))
    require(existing_shared <= set(expected_shared), "Install has unlisted shared rules")
    for name in existing_shared:
        require(bundle.sha256(bundle.read_file(data_root / "overlay-common", name)) == expected_shared[name],
                "Install shared rules differ")
    for name in expected_shared.keys() - existing_shared:
        write(data_root / "overlay-common" / name, bundle.read_file(Path(prepared) / "data/overlay-common", name))
    destination = data_root / version
    if destination.exists():
        from data_snapshot import MAINTENANCE_FILES
        safe_directory(destination)
        entries = list(destination.rglob("*"))
        require(all(not p.is_symlink() and (p.is_dir() or p.is_file()) for p in entries),
                "Unsafe installed data entry")
        # These local layers are explicitly excluded from the sealed runtime.
        # Reusing pinned data must not delete a user's patch or derived cache.
        wanted = {name.removeprefix(f"data/{version}/") for name in receipt["files"]
                  if name.startswith(f"data/{version}/")}
        actual = {p.relative_to(destination).as_posix() for p in entries if p.is_file()}
        local_only = {name for name in actual - wanted
                      if name.startswith("patch/") or name in MAINTENANCE_FILES}
        actual -= local_only
        require(actual == wanted and all(
            bundle.sha256(regular(data_root / name.removeprefix("data/"))) == digest
            for name, digest in receipt["files"].items() if not (keep_current and name == "data/CURRENT")), "Existing installed data differs")
    else:
        with tempfile.TemporaryDirectory(prefix=".install-data-", dir=data_root) as temporary:
            stage = Path(temporary) / version
            shutil.copytree(Path(prepared) / "data" / version, stage)
            prefix = f"data/{version}/"
            for name, expected in receipt["files"].items():
                if name.startswith(prefix):
                    require(bundle.sha256(regular(stage / name.removeprefix(prefix))) == expected,
                            "Prepared data changed during install")
            stage.rename(destination)
    require(regular(data_root / "CURRENT") == current, "CURRENT changed during install")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, default=ROOT / "pipeline/build-data.lock.json")
    parser.add_argument("--out", type=Path, help="Fresh complete bundle; omitted for temporary prepare/install")
    parser.add_argument("--install", type=Path, help="Install the ignored version directory into an existing data root")
    parser.add_argument("--keep-current", action="store_true", help="Install a fixed rollback/test version without selecting it")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    parser.add_argument("--shared", type=Path, default=ROOT / "data/overlay-common")
    parser.add_argument("--game", type=Path, help="Explicit local game ZIP for an unpublished handoff")
    parser.add_argument("--language", type=Path, help="Explicit local language ZIP for an unpublished handoff")
    parser.add_argument("--crafting", type=Path, help="Explicit local crafting ZIP for an unpublished handoff")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    require(args.out is not None or args.install is not None, "Use --out or --install")
    require(not args.keep_current or args.install is not None, "--keep-current needs --install")
    local = {kind: path for kind in (*OWNERS, "crafting") if (path := getattr(args, kind)) is not None}
    def run(output):
        result = prepare(args.lock, output, args.cache, args.shared, args.offline, local)
        import display_assets
        display = display_assets.prepare(args.lock, output, args.cache, args.offline)
        fixtures = display_assets.prepare_test_fixtures(args.lock, args.cache, args.offline)
        if args.install:
            install(output, args.install, args.keep_current)
            if not args.keep_current:
                display_assets.install(display)
                display_assets.install_test_fixtures(fixtures)
        return result
    if args.out:
        result = run(args.out)
    else:
        # Expanded game/language parts are deleted on success and failure.
        args.cache.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".prepared-data-", dir=args.cache.parent) as temporary:
            result = run(Path(temporary) / "bundle")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
