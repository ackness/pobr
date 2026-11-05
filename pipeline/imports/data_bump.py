"""Move the data lock to an owner's GitHub Release: `mise run data:bump <owner> <tag> [<owner> <tag> ...]`.

For each owner/tag the Release's `SHA256SUMS` names the package asset and
`receipt.json`; both are downloaded, verified against it and cached by
content. The receipt supplies the producer commit and data-contract version
and, when it pins the package (`package`), must pin exactly this zip and its
manifest. The lock entry for that owner is replaced, the snapshot pin, runtime
identity and compact release identity are recomputed from the verified bytes,
the lock is written, and `data:prepare` installs it.

A language Release whose `SHA256SUMS` also lists the support display
(`support-display.zh-CN.json` and `supportDescriptions.zh-CN.json`) moves the
lock's `support_display` to it, verified against the new target and the game
package's trade catalog. A kept support display must still match that catalog.
On a new target the test fixtures (patch-independent API samples) stay and
their entry records the target they were published for. Nothing else in the
lock changes.

Bump both owners in one call when a new game patch changes the target.
"""

import argparse
import io
import json
import os
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
from pipeline.imports import display_assets as display
from pipeline.imports.data_imports import encoded, regular, require

ROOT = Path(__file__).resolve().parents[2]
ROLES = {owner: kind for kind, owner in build_data.OWNERS.items()}
ALIASES = {"repoe": "ackness/repoe", "game": "ackness/repoe",
           "poe2-zh-cn-stats": "ackness/poe2-zh-cn-stats", "locale": "ackness/poe2-zh-cn-stats",
           "language": "ackness/poe2-zh-cn-stats"}
SUMS, RECEIPT = "SHA256SUMS", "receipt.json"
# The only receipt schema that may omit its `package` pin (zip + root manifest):
# RePoE's v1 receipts, e.g. game/0.5.5.5-1, which pins neither `package` nor
# `release_build` and stays lockable. Every other schema, a missing or unknown
# one included, must carry it; the translation repository's
# `poe2-locale-release-receipt/v1` always has.
RECEIPTS_WITHOUT_PACKAGE = frozenset({"repoe-release-receipt/v1"})
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
    require(found.get("repository") == owner, "receipt.json must name this owner as producer repository")
    require(found.get("dirty") is False, "receipt.json must record a clean producer checkout (dirty: false)")
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
    receipt = bundle.decode(receipt_raw)
    commit, contract = producer(receipt, owner)
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
        check_receipt(receipt, receipt_raw, spec, part, Path(temporary) / "part")
    shown = support_display(source, sums, receipt, part["target"], cache, fetcher) if ROLES[owner] == "language" else None
    return spec, part, shown


def support_display(source, sums, receipt, target, cache, fetcher):
    """The support display a language Release carries, as a lock entry; None when it carries none.

    Its catalog binding is checked once the candidate is prepared (`check_display`).
    """
    listed = {display.MANIFEST, display.ASSET} & set(sums)
    if not listed:
        require(receipt.get("support_display") is None,
                f"receipt.json pins a support display, but SHA256SUMS lists neither {display.MANIFEST} "
                f"nor {display.ASSET}")
        return None
    require(len(listed) == 2, f"SHA256SUMS lists {sorted(listed)} without "
                              f"{sorted({display.MANIFEST, display.ASSET} - listed)}")
    manifest_raw = fetch_verified(imports.release_address(source, display.MANIFEST), sums[display.MANIFEST],
                                  cache, fetcher, imports.MAX_METADATA)
    manifest = bundle.decode(manifest_raw)
    require(isinstance(manifest, dict) and isinstance(manifest.get("catalog_sha256"), str)
            and re.fullmatch(r"[a-f0-9]{64}", manifest["catalog_sha256"]),
            f"{display.MANIFEST} lacks a trade catalog SHA-256")
    asset_pin = display.check_manifest(manifest, target, manifest["catalog_sha256"])
    require(asset_pin["sha256"] == sums[display.ASSET], f"{display.MANIFEST} pins another {display.ASSET}")
    asset_raw = fetch_verified(imports.release_address(source, display.ASSET), sums[display.ASSET],
                               cache, fetcher, max(1, asset_pin["bytes"]))
    require(pin(asset_raw) == asset_pin, f"{display.ASSET} differs from {display.MANIFEST}")
    display.check_asset(asset_raw, manifest)
    entry = {"repository": source["repository"], "tag": source["tag"], "manifest": pin(manifest_raw),
             "asset": asset_pin, "catalog_sha256": manifest["catalog_sha256"],
             "source_data_version": manifest["provenance"]["generation_data_version"]}
    # A receipt that pins the support display (optional) must pin these bytes.
    pinned = receipt.get("support_display")
    if pinned is not None:
        require(isinstance(pinned, dict)
                and all(isinstance(pinned.get(k), dict) and {f: pinned[k].get(f) for f in ("bytes", "sha256")} == v
                        for k, v in (("manifest", entry["manifest"]), ("asset", entry["asset"]))),
                "receipt.json pins another support display")
    return entry


def same_pin(found, expected):
    return isinstance(found, dict) and {k: found.get(k) for k in ("bytes", "sha256")} == {
        k: expected[k] for k in ("bytes", "sha256")}


def check_receipt(receipt, raw, spec, part, unpacked):
    """The receipt must describe this package, not merely sit next to it."""
    commit, contract = spec["producer_commit"], spec["contract_version"]
    require(receipt.get("target") == part.get("target"), "receipt.json target differs from the package target")
    package = receipt.get("package")
    require(package is not None or receipt.get("schema") in RECEIPTS_WITHOUT_PACKAGE,
            f"receipt.json ({receipt.get('schema')!r}) lacks its package pin")
    if package is not None:
        # Both owners' release tools pin the zip and its root manifest in the
        # receipt (the root manifest itself stays commit-independent).
        require(isinstance(package, dict), "receipt.json package pin is malformed")
        require(same_pin(package, spec), "receipt.json pins another package zip")
        require(same_pin(package.get("manifest"), spec["manifest"]), "receipt.json pins another package manifest")
        require(package.get("expanded_bytes", spec["expanded_bytes"]) == spec["expanded_bytes"],
                "receipt.json records another expanded package size")
        require(package.get("asset", spec["asset"]) == spec["asset"], "receipt.json names another package asset")
    built = part.get("release_build")
    if built is not None:
        # A freshly built root manifest pins the receipt it was built with.
        require(isinstance(built, dict) and isinstance(built.get("receipt"), dict)
                and {k: built["receipt"].get(k) for k in ("bytes", "sha256")} == pin(raw),
                "Package manifest pins another receipt.json")
        require(built.get("producer", {}).get("commit", commit) == commit,
                "Package manifest names another producer commit")
    if part["kind"] == "game":
        name = f'data/{part["target"]["game_patch"]}/manifest.json'
        require(name in part["files"], "Game part lacks the snapshot manifest")
        snapshot = bundle.decode(regular(unpacked / name))
        if "contract_version" in snapshot:
            require(snapshot["contract_version"] == contract,
                    f"Snapshot declares contract {snapshot['contract_version']!r}; receipt.json records {contract!r}")


def candidate(lock, releases):
    """The lock with each released owner entry replaced; identities still to compute."""
    original_target = lock["target"]
    lock = json.loads(json.dumps(lock))
    for owner, (spec, part, shown) in releases.items():
        lock["packages"][ROLES[owner]] = spec
        lock["target"] = part["target"]
        if ROLES[owner] == "game":
            snapshot = f'data/{part["target"]["game_patch"]}/manifest.json'
            require(snapshot in part["files"], "Game part lacks the snapshot manifest")
            lock["snapshot_manifest"] = part["files"][snapshot]
        if shown is not None:
            lock["support_display"] = shown
    targets = {json.dumps(part["target"], sort_keys=True) for _, part, _ in releases.values()}
    require(len(targets) == 1, "Released parts name different targets")
    if lock["target"] != original_target:
        missing = sorted(set(build_data.OWNERS.values()) - set(releases))
        require(not missing, f"A new target {lock['target']['game_patch']} needs every owner bumped; also bump {missing}")
        # A support display names one target; the language Release must carry the new one.
        adopted = any(shown is not None for _, _, shown in releases.values())
        require("support_display" not in lock or adopted,
                f"A new target {lock['target']['game_patch']} needs a new support display: the language Release "
                f"must list {display.MANIFEST} and {display.ASSET} in its SHA256SUMS")
        if "test_fixtures" in lock:
            lock["test_fixtures"].setdefault("target", original_target)
    return lock


def check_display(lock, prepared, draft, cache, fetcher, adopted):
    """The support display must match the prepared trade catalog; an adopted one is imported in full."""
    spec = lock["support_display"]
    require(isinstance(spec.get("catalog_sha256"), str), "support_display lacks its catalog_sha256")
    version = lock["target"]["game_patch"]
    try:
        catalog = bundle.sha256(bundle.read_file(Path(prepared), f"data/{version}/overlay/trade_catalog.json"))
    except (OSError, KeyError, ValueError):
        catalog = None
    require(catalog == spec["catalog_sha256"],
            f"support_display is bound to trade catalog {spec['catalog_sha256'][:12]}…, but the bumped game package "
            f"carries {(catalog or 'none')[:12]}…; the language owner must publish a support display for it "
            f"({display.MANIFEST} in a locale/<patch>-<n> Release) and bump that Release too")
    if adopted:
        display.prepare(draft, prepared, cache, fetcher=fetcher)


def summary(lock):
    result = {"target": lock["target"], "snapshot_manifest": lock["snapshot_manifest"]["sha256"],
              "runtime_identity": lock["runtime_identity"], "release_id": lock.get("release_id"),
              "packages": {kind: {k: spec.get(k) for k in ("repository", "tag", "asset", "path", "sha256")
                                  if spec.get(k) is not None}
                           for kind, spec in lock["packages"].items()}}
    shown = lock.get("support_display")
    if isinstance(shown, dict):
        result["support_display"] = {k: v for k, v in (("repository", shown.get("repository")),
                                                       ("tag", shown.get("tag")),
                                                       ("sha256", (shown.get("asset") or {}).get("sha256")))
                                     if v is not None}
    return result


def member_diff(before, after):
    changed = sorted(n for n in before.keys() & after.keys() if before[n] != after[n])
    return {"changed": changed, "added": sorted(after.keys() - before.keys()),
            "removed": sorted(before.keys() - after.keys())}


def replace_atomically(path, raw):
    """Write `raw` beside `path`, then rename over it: readers see old or new bytes, never a mix."""
    path = Path(path)
    with tempfile.NamedTemporaryFile(prefix=f".{path.name}.", dir=path.parent, delete=False) as handle:
        handle.write(raw)
        temporary = Path(handle.name)
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


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
        check_display(new, prepared, draft, cache, fetcher,
                      adopted=new.get("support_display") != lock.get("support_display"))
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
    version_changed = new["target"]["game_patch"] != lock["target"]["game_patch"]
    replace_atomically(lock_path, encoded(new))
    if version_changed and data_root is not None:
        replace_atomically(Path(data_root) / "CURRENT", (new["target"]["game_patch"] + "\n").encode())
    result = {"schema": "pobr-data-bump/v1", "before": summary(lock), "after": summary(new),
              "identity_changed": new["runtime_identity"] != lock["runtime_identity"]}
    if before_files is not None:
        # Member paths include the version directory, so a new patch lists every file as moved.
        result["members"] = member_diff(before_files, after_files)
    return result


def adopt(lock_path, pairs, cache, data_root, *, asset=None, fetcher=None, install=None):
    """Bump, then install; a failed install restores the previous lock and CURRENT bytes."""
    lock_path, current = Path(lock_path), Path(data_root) / "CURRENT"
    previous = {path: regular(path) for path in (lock_path, current) if path.exists()}
    result = bump(lock_path, pairs, cache, Path(data_root) / "overlay-common", fetcher=fetcher, asset=asset,
                  data_root=data_root)
    if install is not None:
        try:
            install()
        except BaseException:
            for path, raw in previous.items():
                replace_atomically(path, raw)
            print("data:bump: data:prepare failed; restored the previous lock and CURRENT", file=sys.stderr)
            raise
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
    def install():
        subprocess.run([sys.executable, "-m", "pipeline.runtime.build_data", "--lock", str(args.lock),
                        "--install", str(args.data_root), "--cache", str(args.cache)],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    result = adopt(args.lock, pairs, args.cache, args.data_root, asset=args.asset,
                   install=None if args.no_prepare else install)
    print(encoded(result).decode(), end="")
    if not args.no_prepare:
        print("data:prepare: installed the bumped lock", file=sys.stderr)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"data:bump: {error}") from error
