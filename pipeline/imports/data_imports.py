"""Materialize pinned library inputs and project declared runtime display needs.

Downloads are build inputs, never player assets. This first import is a candidate
display package, not a complete game snapshot or source-admission operation.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import zipfile

from pipeline.runtime.data_bundle import decode, relative_path

ROOT = Path(__file__).resolve().parents[2]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def validate_pin(pin):
    require(isinstance(pin, dict) and type(pin.get("bytes")) is int and pin["bytes"] >= 0
            and isinstance(pin.get("sha256"), str)
            and re.fullmatch(r"[a-f0-9]{64}", pin["sha256"]), "Invalid input pin")


def pinned(raw, pin):
    validate_pin(pin)
    require(len(raw) == pin["bytes"] and sha(raw) == pin["sha256"], "Input bytes/SHA256 mismatch")
    return raw


def regular(path):
    path = Path(path).absolute()
    require(all(not parent.is_symlink() for parent in (path, *path.parents)), "Symlink input/output path")
    require(path.is_file(), f"Not a regular input: {path}")
    return path.read_bytes()


def write(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as handle:
        handle.write(raw)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode()


def validate(requirements, lock):
    require(requirements.get("schema") == "pobr-data-requirements/v1"
            and lock.get("schema") == "pobr-data-import-lock/v1", "Unknown data requirements/lock schema")
    require(requirements.get("purpose") == "candidate-runtime-imports", "Unaccepted import purpose")
    target = requirements["target"]
    require(target == lock["target"] and target.get("game") == "poe2"
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", target.get("game_patch", ""))
            and target.get("realm") in ("international", "china"), "Target identity mismatch")
    require(requirements["translation"]["locale"] == lock["locale"]
            and lock["locale"] in ("zh-CN", "zh-TW"), "Locale mismatch")
    require(requirements["repoe"]["usage"] == "text-applicability-only", "Unsupported RePoE usage")
    tables = requirements["repoe"]["tables"]
    require(isinstance(tables, dict) and tables, "No requested tables")
    fields = set()
    for table, request in tables.items():
        require(re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", table)
                and request["key"] == ("HASH32" if table == "Words" else "Id"), "Invalid table key")
        columns = request["display_fields"]
        require(isinstance(columns, list) and columns and len(set(columns)) == len(columns)
                and all(isinstance(f, str) and re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", f) for f in columns),
                "Invalid requested fields")
        fields.update(f"{table}.{field}" for field in columns)
    projection = requirements["translation"]
    require(projection["literals"] == "all-catalog-conflict-participants", "Unsafe literal filtering")
    require(isinstance(projection["scopes"], list)
            and len(set(projection["scopes"])) == len(projection["scopes"])
            and set(projection["scopes"]) <= fields, "Runtime scope was not requested")
    # Reviewed-only supplements are optional; requesting them is what makes the
    # lock's extra compact package and workbench module mandatory, never implicit.
    supplement = projection.get("supplement")
    require(supplement is None or supplement == {"mode": "reviewed-only"},
            "Unsupported reviewed supplement mode")
    require(set(lock["packages"]) == {"repoe", "translation"} | ({"supplement"} if supplement else set()),
            "Missing library package")
    if supplement:
        workbench = lock.get("workbench")
        require(isinstance(workbench, dict)
                and workbench.get("file") == "src/translation-workbench.mjs"
                and re.fullmatch(r"[a-f0-9]{40}", workbench.get("revision") or ""),
                "Invalid workbench module pin")
        validate_pin(workbench)
        require(workbench["repository"] == lock["packages"]["translation"]["repository"],
                "Workbench module repository mismatch")
    else:
        require("workbench" not in lock and "supplement" not in lock["packages"],
                "Unused reviewed supplement pin")
    return fields


def git_address(source, name):
    require(re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9_.-]*/[A-Za-z0-9_-][A-Za-z0-9_.-]*",
                         source["repository"]), "Invalid repository")
    require(re.fullmatch(r"[a-f0-9]{40}", source["revision"]), "Full immutable Git revision required")
    relative_path(name)
    return (f'https://github.com/{source["repository"]}.git', source["revision"], name)


class GitInputs:
    """Fetch only fixed shallow commits and requested blobs, without a checkout."""

    def __init__(self, root, timeout=None):
        require(timeout is None or (type(timeout) in (int, float) and math.isfinite(timeout) and timeout > 0),
                "Invalid Git input timeout")
        self.root = root
        self.repositories = {}
        self.timeout = timeout

    def __call__(self, address, *, max_bytes=None):
        require(max_bytes is None or (type(max_bytes) is int and max_bytes > 0), "Invalid Git blob size limit")
        url, revision, name = address
        key = (url, revision)
        env = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}
        if key not in self.repositories:
            root = self.root / str(len(self.repositories))
            root.mkdir(parents=True)
            def git(*args):
                subprocess.run(["git", "-C", str(root), *args], env=env, check=True,
                               stdout=subprocess.DEVNULL, timeout=self.timeout)
            git("init", "--bare", "--quiet")
            git("remote", "add", "origin", url)
            git("config", "remote.origin.promisor", "true")
            git("config", "remote.origin.partialclonefilter", "blob:none")
            git("fetch", "--quiet", "--no-tags", "--depth=1", "--filter=blob:none", "origin", revision)
            actual = subprocess.check_output(["git", "-C", str(root), "rev-parse", "FETCH_HEAD"],
                                             env=env, timeout=self.timeout)
            require(actual.decode().strip() == revision, "Git revision mismatch")
            self.repositories[key] = root
        if max_bytes is not None:
            size = subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "-s",
                                            f"{revision}:{name}"], env=env, timeout=self.timeout)
            require(int(size) <= max_bytes, "Git metadata blob exceeds size limit")
        return subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "blob",
                                        f"{revision}:{name}"], env=env, timeout=self.timeout)


def acquire(source, name, pin, cache, offline, fetcher):
    address = git_address(source, name)
    validate_pin(pin)
    path = cache / pin["sha256"]
    if path.exists() or path.is_symlink():
        return pinned(regular(path), pin)
    require(not offline, f"Offline cache miss: {name}")
    raw = pinned(fetcher(address), pin)
    # Verify the complete blob before caching. Any interrupted cache write fails
    # its pin on reuse. Declared size is checked without an arbitrary byte cap.
    write(path, raw)
    return raw


def unpack(archive, specification, destination):
    with tempfile.TemporaryFile() as handle:
        handle.write(archive)
        handle.seek(0)
        with zipfile.ZipFile(handle) as package:
            entries = package.infolist()
            names = [entry.filename for entry in entries]
            require(len(names) == len(set(names)) and "manifest.json" in names, "Duplicate/missing archive member")
            for entry in entries:
                relative_path(entry.filename)
                mode = entry.external_attr >> 16
                require(not entry.is_dir() and not stat.S_ISLNK(mode)
                        and stat.S_IFMT(mode) in (0, stat.S_IFREG), "Non-regular archive member")
            require(sum(entry.file_size for entry in entries) == specification["expanded_bytes"],
                    "Expanded archive size mismatch")
            raw = pinned(package.read("manifest.json"), specification["manifest"])
            manifest = decode(raw)
            require(set(names) == {"manifest.json", *manifest["files"]}, "Unlisted/missing archive file")
            for name, pin in manifest["files"].items():
                write(destination / relative_path(name), pinned(package.read(name), pin))
            write(destination / "manifest.json", raw)
    return manifest


def prepare(requirements_path, lock_path, output, cache, offline=False, fetcher=None):
    requirements_raw, lock_raw = regular(requirements_path), regular(lock_path)
    requirements, lock = decode(requirements_raw), decode(lock_raw)
    requested_fields = validate(requirements, lock)
    output, cache = Path(output).absolute(), Path(cache).absolute()
    for path in (output, cache):
        require(".." not in path.parts and path.is_relative_to(ROOT / ".cache")
                and all(not p.is_symlink() for p in (path, *path.parents)), "Use repository cache without symlinks")
    require(not output.exists(), "Fresh import output required")
    cache.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".imports-", dir=output.parent) as temporary:
        stage = Path(temporary) / "output"
        stage.mkdir()
        fetcher = fetcher or GitInputs(Path(temporary) / "git")
        manifests = {}
        for role, spec in lock["packages"].items():
            archive = acquire(spec, spec["path"], spec, cache, offline, fetcher)
            manifests[role] = unpack(archive, spec, stage / role)
        source, catalog = manifests["repoe"], manifests["translation"]
        versions = {"repoe-localized-tables/v2": "poe2-native-catalog-manifest/v2",
                    "repoe-localized-tables/v3": "poe2-native-catalog-manifest/v3"}
        require(source.get("format") in versions
                and catalog.get("schema") == versions[source["format"]], "Unsupported source/catalog format")
        require(not requirements["translation"].get("supplement")
                or catalog.get("schema") == "poe2-native-catalog-manifest/v3",
                "Reviewed supplements require a v3 catalog")
        require(source.get("production_ready") is False and catalog.get("production_ready") is False,
                "Expected frozen candidate inputs")
        require(all(catalog.get(key) == value for key, value in lock["target"].items())
                and catalog["locale"] == lock["locale"], "Catalog target mismatch")
        request = decode(regular(stage / "translation/request.json"))
        require(request["localized"]["sha256"] == lock["packages"]["repoe"]["manifest"]["sha256"],
                "Catalog is bound to another RePoE source")
        tables = decode(regular(stage / "repoe/tables.json"))
        for name, need in requirements["repoe"]["tables"].items():
            require(name in tables and {need["key"], *need["display_fields"]}
                    <= set(tables[name]["physical_fields"]), f"Requested source fields missing: {name}")
        del tables
        rows = decode(regular(stage / "translation/catalog.json"))["records"]
        available = {f'{row["table"]}.{row["field"]}' for row in rows}
        require(requested_fields <= available, "Requested catalog fields missing")
        del rows
        supplement = None
        if requirements["translation"].get("supplement"):
            workbench, package = lock["workbench"], manifests["supplement"]
            files = package.get("files", {})
            require(package.get("schema") == "poe2-reviewed-native-supplements-manifest/v1"
                    and package.get("status") == "candidate" and package.get("production_ready") is False
                    and set(files) == {"supplements.json", "request.json"},
                    "Invalid reviewed supplement package")
            runtime = decode(regular(stage / "supplement/supplements.json"))
            require(runtime.get("schema") == "poe2-reviewed-native-supplements/v1"
                    and runtime.get("status") == "candidate" and runtime.get("production_ready") is False
                    and isinstance(runtime.get("sources"), list) and isinstance(runtime.get("records"), list),
                    "Invalid reviewed supplement runtime")
            verification = catalog["files"].get("verification.json")
            bound = {"game": lock["target"]["game"], "game_patch": lock["target"]["game_patch"],
                     "realm": lock["target"]["realm"], "locale": lock["locale"],
                     "catalog_sha256": catalog["files"]["catalog.json"]["sha256"],
                     "verification": None if verification is None else
                     {"bytes": verification["bytes"], "sha256": verification["sha256"]}}
            require(runtime["sources"] == [bound], "Supplement is bound to another catalog")
            supplement = {"workbench": workbench, "runtime": runtime,
                          "manifest": lock["packages"]["supplement"]["manifest"]}
        library = lock["library"]
        require(library["repository"] == lock["packages"]["translation"]["repository"]
                and library["revision"] == lock["packages"]["translation"]["revision"]
                and {name: pin["sha256"] for name, pin in library["files"].items()} == catalog["producer"],
                "Library producer closure mismatch")
        for name, pin in library["files"].items():
            write(stage / "library" / relative_path(name), acquire(library, name, pin, cache, offline, fetcher))
        if supplement:
            # The workbench module is its own pinned revision, materialized next to
            # the official modules without becoming part of the catalog producer.
            workbench = supplement["workbench"]
            write(stage / "library" / relative_path(workbench["file"]),
                  acquire({"repository": workbench["repository"], "revision": workbench["revision"]},
                          workbench["file"], workbench, cache, offline, fetcher))
        display_request = {"schema": "pobr-native-display-texts-request/v1", **lock["target"],
                           "locale": lock["locale"],
                           "catalog": {"manifest": str(output / "translation/manifest.json"),
                                       **lock["packages"]["translation"]["manifest"]},
                           "library": {"root": str(output / "library"),
                                       "files": {name: pin["sha256"] for name, pin in library["files"].items()}},
                           "projection": {key: requirements["translation"][key] for key in ("scopes", "literals")}}
        if supplement:
            display_request["supplement"] = {
                "manifest": str(output / "supplement/manifest.json"), **supplement["manifest"],
                "module": {key: supplement["workbench"][key]
                           for key in ("file", "bytes", "sha256", "revision")}}
        write(stage / "display-request.json", encoded(display_request))
        require(regular(requirements_path) == requirements_raw and regular(lock_path) == lock_raw,
                "Requirements changed during import")
        receipt = {"schema": "pobr-data-import-receipt/v1", "requirements_sha256": sha(requirements_raw),
                   "lock_sha256": sha(lock_raw), "target": lock["target"], "locale": lock["locale"],
                   "packages": lock["packages"], "library": library,
                   **({"workbench": supplement["workbench"]} if supplement else {}),
                   "requested_fields": sorted(requested_fields), "adoption": "not-adopted",
                   "scope": "Pinned build inputs and declared display projection; not a complete game snapshot."}
        # Completion is the final step; a failed package never replaces prior output.
        output.mkdir()
        for path in stage.iterdir():
            path.rename(output / path.name)
        write(output / "import-receipt.json", encoded(receipt))
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--requirements", type=Path, default=ROOT / "pipeline/runtime/data-requirements.json")
    parser.add_argument("--lock", type=Path, default=ROOT / "pipeline/imports/data-imports.lock.json")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    prepare(args.requirements, args.lock, args.out, args.cache, args.offline)
    subprocess.run(["node", str(ROOT / "pipeline/native/native-display-texts.mjs"), "--request",
                    str(args.out.absolute() / "display-request.json"), "--out",
                    str(args.out.absolute() / "runtime")], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
