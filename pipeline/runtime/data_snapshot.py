#!/usr/bin/env python3
"""Seal, promote and bundle validated owner data.

PoBR never produces data: snapshots come from the owner publications (see
build_data / locked_update). Domain candidates are made in ackness/repoe by
tools/pobr-data-adapter."""

import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
from pathlib import Path
import re
import shutil
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
    bundle = sub.add_parser("bundle")
    bundle.add_argument("--out", type=Path, required=True)
    bundle.add_argument("--version")
    check_bundle = sub.add_parser("verify-bundle")
    check_bundle.add_argument("--bundle", type=Path, required=True)
    check_bundle.add_argument("--expected-id", help="Trusted SHA-256 bundle identity from the selected source lock")
    args = parser.parse_args()
    if args.command in {"bundle", "verify-bundle"}:
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
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(f"data snapshot: {error}") from error
