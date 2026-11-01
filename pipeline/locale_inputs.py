"""Run the independently pinned language producer on temporary, authenticated inputs.

PoBR owns acquisition and candidate installation. Projection, merging and language
selection belong to the translation repository; no upstream HEAD is resolved here.
"""

import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile

from data_bundle import decode, relative_path
from data_imports import GitInputs, acquire, encoded, git_address, pinned, regular, require, validate_pin, write

ROOT = Path(__file__).resolve().parents[1]
OWNER = "ackness/poe2-zh-cn-stats"
MODULES = {"src/locale-producer.mjs", "src/legacy-translation-merge.mjs", "src/strict-json.mjs"}
OUTPUTS = {f"i18n/zh-CN/{name}.json" for name in (
    "base_items", "skills", "stat_lines", "classes", "words", "passive_names", "mods", "rare_words", "_meta"
)} | {"generated/translation-audit.json"}
RECEIPT = "generated/locale-generation-receipt.json"


def safe(path):
    path = Path(path).absolute()
    require(".." not in path.parts and all(not p.is_symlink() for p in (path, *path.parents)),
            "Unsafe locale input/output directory")
    return path


def validate(lock, version=None):
    require(lock.get("schema") == "pobr-locale-generation-lock/v1", "Unknown locale generation lock")
    require(lock.get("repository") == OWNER, "Language producer must belong to the independent translation owner")
    target = lock["target"]
    require(set(target) == {"game", "game_patch", "realm"} and target["game"] == "poe2"
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", target["game_patch"])
            and target["realm"] in ("international", "china"), "Invalid locale generation target")
    require(version is None or version == target["game_patch"],
            "Locale generation lock targets another patch; provide an explicit independent --locale-lock")
    producer = lock["producer"]
    require(producer.get("entrypoint") == "src/locale-producer.mjs"
            and set(producer["files"]) == MODULES, "Unexpected locale producer closure")
    for name, pin in producer["files"].items():
        git_address(lock, name)
        validate_pin(pin)
    git_address(lock, lock["input_manifest"]["path"])
    validate_pin(lock["input_manifest"])
    return target


def repository(value):
    require(isinstance(value, str) and value.startswith("https://github.com/"), "Expected a GitHub source identity")
    name = value.removeprefix("https://github.com/")
    # git_address validates the owner/repository spelling and immutable revision.
    git_address({"repository": name, "revision": "0" * 40}, "identity.json")
    return name


def input_source(source, owner):
    """Convert a fixed raw URL or producer-relative source into a Git blob address."""
    require(isinstance(source, dict), "Missing locale input source")
    if set(source) == {"url"}:
        match = re.fullmatch(r"https://raw\.githubusercontent\.com/([^/]+/[^/]+)/([a-f0-9]{40})/(.+)", source["url"])
        require(match is not None, "Locale input URL must name an immutable GitHub commit")
        repo, revision, name = match.groups()
    else:
        require(set(source) == {"repository", "revision", "path"}, "Unexpected locale source descriptor")
        repo, revision, name = repository(source["repository"]), source["revision"], source["path"]
        if revision == "producer-commit":
            require(repo == owner["repository"], "Producer-relative source belongs to another owner")
            revision = owner["revision"]
    spec = {"repository": repo, "revision": revision}
    git_address(spec, name)
    return spec, name


def prepare(lock_path, output, cache, version=None, offline=False, fetcher=None):
    lock_path, output, cache = Path(lock_path), safe(output), safe(cache)
    lock_raw = regular(lock_path)
    lock = decode(lock_raw)
    target = validate(lock, version)
    require(not output.exists(), "Fresh locale output required")
    require(not output.is_relative_to(ROOT / "data"), "Generate outside source data; use the candidate adapter")
    output.parent.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".locale-inputs-", dir=output.parent) as temporary:
        stage = Path(temporary)
        fetcher = fetcher or GitInputs(stage / "git")
        reads = {}
        def get(spec, name, pin, destination):
            raw = acquire(spec, name, pin, cache, offline, fetcher)
            write(destination, raw)
            reads[destination] = pin
            return raw
        m = lock["input_manifest"]
        manifest_path = stage / "input-manifest.json"
        manifest = decode(get(lock, m["path"], m, manifest_path))
        require(manifest.get("schema") == "poe2-locale-generation-input/v1"
                and manifest.get("target") == target, "Locale input manifest target/schema mismatch")
        require(isinstance(manifest.get("inputs"), dict) and manifest["inputs"], "Missing locale input inventory")
        expected = manifest.get("expected_files")
        require(isinstance(expected, dict) and set(expected) == OUTPUTS, "Pin every generated locale output")
        for pin in expected.values():
            validate_pin(pin)
        for name, pin in lock["producer"]["files"].items():
            get(lock, name, pin, stage / "producer" / relative_path(name))
        for name, pin in manifest["inputs"].items():
            destination = stage / "inputs" / relative_path(name)
            validate_pin(pin)
            spec, source_path = input_source(pin["source"], lock)
            get(spec, source_path, pin, destination)
        generated = stage / "generated"
        subprocess.run(["node", str(stage / "producer" / lock["producer"]["entrypoint"]),
                        "--manifest", str(manifest_path), "--manifest-sha256", m["sha256"],
                        "--input-dir", str(stage / "inputs"), "--out", str(generated)], check=True)
        receipt = decode(regular(generated / "receipt.json"))
        require(receipt.get("schema") == "poe2-locale-generation-receipt/v1"
                and receipt.get("target") == target and receipt.get("manifest_sha256") == m["sha256"]
                and receipt.get("identity") == manifest.get("identity")
                and receipt.get("profile") == manifest.get("profile")
                and receipt.get("inputs") == manifest["inputs"] and receipt.get("files") == expected,
                "Independent locale receipt does not match its fixed inputs/outputs")
        actual = {p.relative_to(generated).as_posix() for p in generated.rglob("*") if not p.is_dir()}
        require(actual == OUTPUTS | {"receipt.json"}, "Unlisted/missing locale output")
        for name, pin in expected.items():
            pinned(regular(generated / relative_path(name)), pin)
        for path, pin in reads.items():
            pinned(regular(path), pin)
        require(regular(lock_path) == lock_raw, "Locale generation lock changed during generation")
        generated.rename(output)
    return receipt


def install_candidate(prepared, candidate, target):
    """Only the transaction's disposable candidate can receive generated files."""
    candidate = safe(candidate)
    require(os.environ.get("POBR_CANDIDATE") == "1" and os.environ.get("POBR_DATA_ROOT"),
            "Locale installation requires the snapshot candidate transaction")
    require(not candidate.is_relative_to(ROOT / "data"), "Refusing in-place source data installation")
    expected = safe(os.environ["POBR_DATA_ROOT"]) / target["game_patch"]
    require(candidate == expected and candidate.is_dir(), "Locale candidate identity mismatch")
    receipt = decode(regular(Path(prepared) / "receipt.json"))
    require(receipt.get("target") == target and set(receipt.get("files", {})) == OUTPUTS,
            "Invalid prepared locale receipt")
    # Verify the complete result before writing into the disposable candidate.
    files = {name: pinned(regular(Path(prepared) / name), pin) for name, pin in receipt["files"].items()}
    files[RECEIPT] = encoded(receipt)
    for name, raw in files.items():
        path = safe(candidate / relative_path(name))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, default=ROOT / "pipeline/locale-generation.lock.json")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/locale-generation-downloads")
    parser.add_argument("--version")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--check-lock", action="store_true", help="Validate the fixed target/closure without acquisition")
    outputs = parser.add_mutually_exclusive_group()
    outputs.add_argument("--out", type=Path)
    outputs.add_argument("--candidate", type=Path)
    args = parser.parse_args()
    if args.check_lock:
        require(args.out is None and args.candidate is None, "Lock preflight does not generate or install")
        validate(decode(regular(args.lock)), args.version)
        print("Independent locale generation lock matches target")
        return
    require(args.out is not None or args.candidate is not None, "Use --out or --candidate")
    if args.candidate:
        # Check the boundary before any acquisition or producer execution.
        target = validate(decode(regular(args.lock)), args.version)
        require(not safe(args.candidate).is_relative_to(ROOT / "data"), "Refusing in-place source data installation")
        require(os.environ.get("POBR_CANDIDATE") == "1" and os.environ.get("POBR_DATA_ROOT")
                and safe(args.candidate) == safe(os.environ["POBR_DATA_ROOT"]) / target["game_patch"],
                "Use the snapshot candidate transaction for installation")
        args.cache.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".locale-candidate-", dir=args.cache.parent) as temporary:
            output = Path(temporary) / "output"
            receipt = prepare(args.lock, output, args.cache, args.version, args.offline)
            install_candidate(output, args.candidate, target)
    else:
        receipt = prepare(args.lock, args.out, args.cache, args.version, args.offline)
    print(f"Independent language projection: {len(receipt['files'])} generated files; {receipt['profile']}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"locale inputs: {error}") from error
