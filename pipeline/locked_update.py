"""Prepare an exact independent-owner candidate; select it only by explicit request."""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

import build_data
import data_bundle as bundle
import data_snapshot
from data_imports import encoded, regular, require


ROOT = Path(__file__).resolve().parents[1]


def fingerprint(path):
    path = build_data.safe_directory(path)
    if not path.exists():
        return {}
    require(path.is_dir(), "Expected a data directory")
    result = {}
    for item in path.rglob("*"):
        build_data.safe_directory(item)
        require(item.is_file() or item.is_dir(), "Nonregular data input")
        if item.is_file():
            result[item.relative_to(path).as_posix()] = bundle.sha256(regular(item))
    return result


def validate_candidate(data_root, version):
    """Keep the upgrade's existing version, loader and parity gates."""
    env = {**os.environ, "POBR_DATA_ROOT": str(data_root), "POBR_DATA_VERSION": version}
    # Preserve the upgrade path's existing bypass for an unreliable local wrapper.
    env["CARGO_BUILD_RUSTC_WRAPPER"] = ""
    env.pop("POBR_BLESS_PINS", None)
    for command in (["bash", "devs/scripts/driver.sh", "versions"],
                    ["cargo", "test", "--locked", "--quiet", "-p", "pobr-gamedata", "--lib", "--tests"],
                    ["cargo", "test", "--locked", "--quiet", "-p", "pobr-build", "--test", "parity", "parity_no_regression"]):
        subprocess.run(command, cwd=ROOT, env=env, stdout=sys.stderr, check=True)


def activate(prepared, data_root, expected_current, validator=None):
    """Select a verified bundle without downloading or changing shared rules.

    Cross-version patch migration is separate; only a patch already belonging
    to the target is preserved. Reuse local maintenance's publication/rollback.
    """
    prepared = build_data.safe_directory(prepared)
    data_root = build_data.safe_directory(data_root)
    require(not prepared.is_relative_to(data_root) and not data_root.is_relative_to(prepared),
            "Candidate and installed data overlap")
    expected_current = data_snapshot.patch_version(expected_current)
    expected_marker = (expected_current + "\n").encode()
    with data_snapshot.workspace_lock(data_root.parent):
        require(regular(data_root / "CURRENT") == expected_marker, "Expected CURRENT changed")
        receipt = bundle.verify(prepared)
        version = receipt["data_version"]
        # Require a usable installed previous snapshot for rollback.
        bundle.snapshot_inputs(data_root.parent, expected_current)
        shared = fingerprint(data_root / "overlay-common")
        wanted_shared = {name.removeprefix("data/overlay-common/"): digest
                         for name, digest in receipt["files"].items()
                         if name.startswith("data/overlay-common/")}
        require(shared == wanted_shared, "Shared rules differ; review and migrate them separately")
        prefix = f"data/{version}/"
        wanted = {name.removeprefix(prefix): digest for name, digest in receipt["files"].items()
                  if name.startswith(prefix)}
        require(fingerprint(prepared / "data" / version) == wanted, "Candidate has unlisted or changed files")
        history = [path for path in data_root.iterdir()
                   if path.is_dir() and path.name != version
                   and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", path.name)]
        watched = {data_root / name for name in (expected_current, version, "overlay-common")} | set(history)
        before = {path: fingerprint(path) for path in watched}
        target_patch = fingerprint(data_root / version / "patch")
        def check_installed():
            require(all(fingerprint(path) == value for path, value in before.items())
                    and regular(data_root / "CURRENT") == expected_marker,
                    "Installed data changed before selection")
        cache = data_root.parent / ".cache"
        with tempfile.TemporaryDirectory(prefix="locked-update-", dir=cache) as temporary:
            stage = Path(temporary) / "data"
            shutil.copytree(prepared / "data" / version, stage / version)
            require(fingerprint(stage / version) == wanted, "Candidate changed while copying")
            patch = data_root / version / "patch"
            if patch.exists():
                shutil.copytree(patch, stage / version / "patch")
            shutil.copytree(prepared / "data/overlay-common", stage / "overlay-common")
            (stage / "CURRENT").write_text(version + "\n", encoding="utf-8")
            if (ROOT / "examples").is_dir():
                (stage.parent / "examples").symlink_to((ROOT / "examples").resolve(), target_is_directory=True)
            for path in history:
                (stage / path.name).symlink_to(path, target_is_directory=True)
            require(bundle.snapshot_inputs(stage.parent, version)[1] == receipt["runtime_identity"],
                    "Staged runtime identity differs")
            check_installed()
            (validator or validate_candidate)(stage, version)
            require(regular(stage / "CURRENT") == (version + "\n").encode(), "Validator changed staged CURRENT")
            require(bundle.snapshot_inputs(stage.parent, version)[1] == receipt["runtime_identity"],
                    "Validator changed the staged runtime")
            staged = {name: value for name, value in fingerprint(stage / version).items()
                      if name in wanted or (not name.startswith("patch/")
                                            and name not in data_snapshot.MAINTENANCE_FILES)}
            require(staged == wanted and fingerprint(stage / version / "patch") == target_patch,
                    "Validator changed the target patch or added unlisted runtime files")
            check_installed()
            bundle.verify(prepared, receipt["bundle_id"])
            data_snapshot.promote(stage, data_root, version, activate=True)
    return {"schema": "pobr-locked-data-selection/v1", "previous": expected_current,
            "selected": version, "bundle_id": receipt["bundle_id"],
            "runtime_identity": receipt["runtime_identity"],
            "user_patch": "Target's local patch preserved; cross-version migration not inferred",
            "scope": "Explicit local selection only; source approval, deployment and golden updates are separate"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, required=True, help="Exact independent game/language build lock")
    parser.add_argument("--out", type=Path, required=True, help="Fresh complete candidate; retained after selection")
    parser.add_argument("--patch", help="Optional assertion of the lock target, never a relabeling input")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    parser.add_argument("--data-root", type=Path, default=ROOT / "data")
    parser.add_argument("--shared", type=Path, help="Pinned rules; defaults to data-root/overlay-common")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--activate", action="store_true", help="Explicitly select the candidate locally")
    parser.add_argument("--expected-current", help="Required for selection; rejects a changed marker")
    for kind in (*build_data.OWNERS, "crafting"):
        parser.add_argument("--" + kind, type=Path, help="Explicit local owner input; all pins still apply")
    args = parser.parse_args()
    require(args.activate == (args.expected_current is not None), "Selection needs --activate and --expected-current together")
    data_root, output = (build_data.safe_directory(path) for path in (args.data_root, args.out))
    require(not output.is_relative_to(data_root) and not data_root.is_relative_to(output),
            "Candidate and installed data overlap")
    lock = bundle.decode(regular(args.lock))
    version = build_data.validate(lock)
    require(args.patch is None or args.patch == version, "Requested patch differs from the independent lock")
    if args.activate:
        require(regular(data_root / "CURRENT") == (args.expected_current + "\n").encode(), "Expected CURRENT changed")
    local = {kind: value for kind in (*build_data.OWNERS, "crafting") if (value := getattr(args, kind)) is not None}
    receipt = build_data.prepare(args.lock, output, args.cache, args.shared or data_root / "overlay-common",
                                 args.offline, local)
    result = {"candidate": receipt, "selection": None}
    if args.activate:
        result["selection"] = activate(output, data_root, args.expected_current)
    print(encoded(result).decode(), end="")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"locked data update: {error}") from error
