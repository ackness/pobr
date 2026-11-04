"""Move the data lock to an owner's GitHub Release: `mise run data:bump <owner> <tag> [<owner> <tag> ...]`.

For each owner/tag the Release's `SHA256SUMS` names the package asset and
`receipt.json`; both are downloaded, verified against it and cached by
content. The receipt supplies the producer commit and data-contract version.
The lock entry for that owner is replaced, the snapshot pin, runtime identity
and compact release identity are recomputed from the verified bytes, the lock
is written, and `data:prepare` installs it. Nothing else in the lock changes.

Bump both owners in one call when a new game patch changes the target.
"""

import argparse
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import zipfile

from pipeline.runtime import build_data
from pipeline.runtime import data_bundle as bundle
from pipeline.runtime import runtime_pack
from pipeline.imports import data_imports as imports
from pipeline.imports.data_imports import encoded, regular, require

ROOT = Path(__file__).resolve().parents[2]
ROLES = {owner: kind for kind, owner in build_data.OWNERS.items()}
ALIASES = {"repoe": "ackness/repoe", "game": "ackness/repoe",
           "poe2-zh-cn-stats": "ackness/poe2-zh-cn-stats", "locale": "ackness/poe2-zh-cn-stats",
           "language": "ackness/poe2-zh-cn-stats"}
SUMS, RECEIPT = "SHA256SUMS", "receipt.json"
# A package asset has no pin until this command records one; bound the download.
MAX_PACKAGE = 1024 * 1024 * 1024


def pin(raw):
    return {"bytes": len(raw), "sha256": imports.sha(raw)}


def owner_name(value):
    owner = ALIASES.get(value, value)
    require(owner in ROLES, f"Unknown data owner {value!r}; use one of {sorted({*ALIASES, *ROLES})}")
    return owner


def fetch_verified(address, digest, cache, fetcher, max_bytes):
    """Download (or reuse from cache) one asset whose SHA-256 SHA256SUMS declares."""
    path = cache / digest
    if path.exists() or path.is_symlink():
        raw = regular(path)
        require(imports.sha(raw) == digest, f"Corrupt cache entry for {address.asset}")
        return raw
    raw = fetcher(address, max_bytes=max_bytes)
    require(imports.sha(raw) == digest, f"{address.asset} does not match SHA256SUMS")
    imports.write(path, raw)
    return raw


def choose_asset(sums, asset):
    if asset is not None:
        require(asset in sums, f"SHA256SUMS does not list {asset}")
        return asset
    packages = sorted(name for name in sums if name.endswith(".zip"))
    require(len(packages) == 1, f"Release must list exactly one .zip package, found {packages}; pass --asset")
    return packages[0]


def producer(receipt, owner):
    """Producer commit and contract version from the owner's build receipt."""
    found = receipt.get("producer") if isinstance(receipt, dict) else None
    require(isinstance(found, dict), "receipt.json lacks a producer")
    require(found.get("repository", owner) == owner, "receipt.json names another producer repository")
    require(found.get("dirty") is not True, "receipt.json records a dirty producer checkout")
    commit = found.get("commit")
    require(isinstance(commit, str) and re.fullmatch(r"[a-f0-9]{40}", commit), "receipt.json lacks a producer commit")
    contract = found.get("contract")
    version = contract.get("version") if isinstance(contract, dict) else receipt.get("contract_version")
    require(version is None or type(version) is int, "Invalid receipt contract version")
    return commit, version


def resolve(owner, tag, cache, fetcher, asset=None):
    """Verify one Release and return its lock entry and part manifest."""
    source = {"repository": owner, "tag": tag}
    sums = imports.parse_sums(fetcher(imports.release_address(source, SUMS), max_bytes=imports.MAX_METADATA))
    asset = choose_asset(sums, asset)
    require(RECEIPT in sums, f"SHA256SUMS does not list {RECEIPT}")
    receipt_raw = fetch_verified(imports.release_address(source, RECEIPT), sums[RECEIPT], cache, fetcher,
                                 imports.MAX_METADATA)
    commit, contract = producer(bundle.decode(receipt_raw), owner)
    archive = fetch_verified(imports.release_address(source, asset), sums[asset], cache, fetcher, MAX_PACKAGE)
    with tempfile.TemporaryDirectory(prefix=".bump-part-", dir=cache.parent) as temporary:
        # Read the embedded manifest first: unpack checks it against the entry.
        with zipfile.ZipFile(io.BytesIO(archive)) as package:
            manifest_raw = package.read("manifest.json")
            expanded = sum(entry.file_size for entry in package.infolist())
        spec = {"repository": owner, "tag": tag, "asset": asset, **pin(archive),
                "manifest": pin(manifest_raw), "expanded_bytes": expanded,
                "receipt": pin(receipt_raw), "producer_commit": commit,
                "contract_version": contract}
        part = imports.unpack(archive, spec, Path(temporary) / "part")
    require(part.get("schema") == build_data.PART_SCHEMA and part.get("kind") == ROLES[owner],
            f"{asset} is not a {ROLES[owner]} part")
    return spec, part


def candidate(lock, releases):
    """The lock with each released owner entry replaced; identities still to compute."""
    lock = json.loads(json.dumps(lock))
    for owner, (spec, part) in releases.items():
        lock["packages"][ROLES[owner]] = spec
        lock["target"] = part["target"]
        if ROLES[owner] == "game":
            snapshot = f'data/{part["target"]["game_patch"]}/manifest.json'
            require(snapshot in part["files"], "Game part lacks the snapshot manifest")
            lock["snapshot_manifest"] = part["files"][snapshot]
    targets = {json.dumps(part["target"], sort_keys=True) for _, part in releases.values()}
    require(len(targets) == 1, "Released parts name different targets")
    return lock


def summary(lock):
    return {"target": lock["target"], "snapshot_manifest": lock["snapshot_manifest"]["sha256"],
            "runtime_identity": lock["runtime_identity"], "release_id": lock.get("release_id"),
            "packages": {kind: {k: spec.get(k) for k in ("repository", "tag", "asset", "path", "sha256")
                                if spec.get(k) is not None}
                         for kind, spec in lock["packages"].items()}}


def member_diff(before, after):
    changed = sorted(n for n in before.keys() & after.keys() if before[n] != after[n])
    return {"changed": changed, "added": sorted(after.keys() - before.keys()),
            "removed": sorted(before.keys() - after.keys())}


def bump(lock_path, pairs, cache, shared, fetcher=None, asset=None, data_root=None):
    """Write the bumped lock (and CURRENT on a target change); return the identity diff."""
    lock_raw = regular(lock_path)
    lock = bundle.decode(lock_raw)
    build_data.validate(lock)
    cache = build_data.safe_directory(cache)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".data-bump-", dir=cache.parent) as temporary:
        temp = Path(temporary)
        fetcher = fetcher or imports.OwnerInputs(temp / "git")
        releases = {}
        for owner, tag in pairs:
            require(owner not in releases, f"{owner} named twice")
            releases[owner] = resolve(owner, tag, cache, fetcher, asset)
        new = candidate(lock, releases)
        new.update(runtime_identity="0" * 64, release_id=None)
        build_data.validate(new)
        draft = temp / "draft.lock.json"
        draft.write_bytes(encoded(new))
        prepared = temp / "prepared"
        receipt = build_data.prepare(draft, prepared, cache, shared, fetcher=fetcher, expect_identity=False)
        new["runtime_identity"] = receipt["runtime_identity"]
        draft.unlink()
        draft.write_bytes(encoded(new))
        if lock.get("release_id") is not None:
            # The compact release identity is recorded once adopted; keep recording it.
            new["release_id"] = runtime_pack.build_release(draft, prepared / "data", temp / "pack")["release_id"]
        before_files = None
        if data_root is not None:
            try:
                before_files = {name: bundle.sha256(raw) for name, raw in
                                bundle.data_inputs(data_root, lock["target"]["game_patch"])[0].items()}
            except (OSError, KeyError, TypeError, ValueError):
                before_files = None
        after_files = receipt["files"]
    require(regular(lock_path) == lock_raw, "Data lock changed during bump")
    Path(lock_path).write_bytes(encoded(new))
    version_changed = new["target"]["game_patch"] != lock["target"]["game_patch"]
    if version_changed and data_root is not None:
        (Path(data_root) / "CURRENT").write_text(new["target"]["game_patch"] + "\n", encoding="utf-8")
    result = {"schema": "pobr-data-bump/v1", "before": summary(lock), "after": summary(new),
              "identity_changed": new["runtime_identity"] != lock["runtime_identity"]}
    if before_files is not None:
        # Member paths include the version directory, so a new patch lists every file as moved.
        result["members"] = member_diff(before_files, after_files)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("release", nargs="+", help="<owner> <tag> pairs, e.g. repoe game/0.5.5.5-1")
    parser.add_argument("--asset", help="Package asset name when SHA256SUMS lists several .zip files")
    parser.add_argument("--lock", type=Path, default=build_data.DATA_LOCK)
    parser.add_argument("--cache", type=Path, default=imports.download_cache())
    parser.add_argument("--data-root", type=Path, default=ROOT / "data")
    parser.add_argument("--no-prepare", action="store_true", help="Only rewrite the lock; skip data:prepare")
    args = parser.parse_args()
    require(len(args.release) % 2 == 0, "Give <owner> <tag> pairs")
    pairs = [(owner_name(args.release[i]), args.release[i + 1]) for i in range(0, len(args.release), 2)]
    require(args.asset is None or len(pairs) == 1, "--asset applies to a single owner")
    result = bump(args.lock, pairs, args.cache, args.data_root / "overlay-common", asset=args.asset,
                  data_root=args.data_root)
    print(encoded(result).decode(), end="")
    if not args.no_prepare:
        subprocess.run([sys.executable, "-m", "pipeline.runtime.build_data", "--lock", str(args.lock),
                        "--install", str(args.data_root), "--cache", str(args.cache)],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        print("data:prepare: installed the bumped lock", file=sys.stderr)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"data:bump: {error}") from error
