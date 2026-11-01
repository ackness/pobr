"""Build/verify compact release blocks and adapt them to the existing GameData loader.

This is transport packaging, not a second game-data interpreter. Inner documents
are JSON strings: JS must never round an i64 or rewrite a source float on staging.
"""

import argparse
import gzip
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile

import data_bundle as bundle
from data_imports import GitInputs, acquire, pinned, regular, require, write

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = "pobr-runtime-release/v1"
BLOCK_SCHEMA = "pobr-runtime-block/v1"
INPUT_SCHEMA = "pobr-runtime-inputs/v1"
LOCK_SCHEMA = "pobr-runtime-input-lock/v1"
HEX = re.compile(r"[0-9a-f]{64}")
SPECIAL = {"overlay/special_mods.json", "generated/special_derived.json",
           "generated/special_vendor.json", "overlay-common/special_mods.json"}


class Number(str):
    """A JSON number token, preserved verbatim through projection."""


def lossless(raw):
    def unique(pairs):
        out = {}
        for key, value in pairs:
            require(key not in out, f"Duplicate JSON key: {key}")
            out[key] = value
        return out

    def invalid(value):
        raise ValueError(f"Non-JSON number: {value}")

    return json.loads(raw, parse_int=Number, parse_float=Number,
                      object_pairs_hook=unique, parse_constant=invalid)


def compact(value):
    if isinstance(value, Number):
        return str(value)
    if isinstance(value, dict):
        return "{" + ",".join(compact(key) + ":" + compact(item)
                              for key, item in sorted(value.items())) + "}"
    if isinstance(value, list):
        return "[" + ",".join(map(compact, value)) + "]"
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def raw_json(value):
    return (compact(value) + "\n").encode("utf-8")


def pin(raw):
    return {"bytes": len(raw), "sha256": bundle.sha256(raw)}


def target_valid(target, version):
    require(isinstance(target, dict) and set(target) == {"game", "game_patch", "realm"}
            and target["game"] == "poe2" and target["game_patch"] == version
            and target["realm"] in ("international", "china"), "Pack target mismatch")


def project(name, raw):
    value = lossless(raw)
    # Only document headers and the three optional SpecialTemplateDef audit
    # members are removed. NativePassiveStats.provenance is validated/consumed
    # at runtime and MUST remain, as do verified/batch/exporter/game_source.
    if isinstance(value, dict) and not name.startswith("i18n/"):
        value.pop("_meta", None)
    if name in SPECIAL:
        for entry in value["entries"]:
            for key in ("vendor_pattern", "source_note", "examples"):
                entry.pop(key, None)
    return raw_json(value)


def identity(version, digests):
    hashes = {name: digest for name, digest in digests.items() if name != "manifest.json"}
    return bundle.sha256(bundle.encoded(["pobr-data-pack-v1", 3, version, hashes]))


def release_document(target, locale, files, blocks):
    version = target["game_patch"]
    payload = {"schema": SCHEMA, "version": version, "target": target, "locale": locale,
               "runtime_identity": identity(version, files), "files": sorted(files),
               "digests": files, "blocks": blocks}
    return {**payload, "release_id": bundle.sha256(bundle.encoded(payload))}


def block_kind(name):
    if name == "manifest.json" or name.startswith("i18n/"):
        return "locale"
    return "core" if name.startswith("base/") else "rules"


def assemble(files, version, target, locale, display):
    target_valid(target, version)
    require(locale in ("zh-CN", "zh-TW"), "Unsupported pack locale")
    snapshot = bundle.decode(files[f"data/{version}/manifest.json"])
    require(locale in snapshot["languages"], "Selected locale missing from snapshot")
    selected = {}
    prefix = f"data/{version}/"
    for name, raw in sorted(files.items()):
        if name.startswith(prefix):
            name = name.removeprefix(prefix)
        elif name.startswith("data/overlay-common/"):
            name = name.removeprefix("data/")
        else:
            continue
        if name == "manifest.json" or (name.startswith("i18n/") and (
                not name.startswith(f"i18n/{locale}/") or name.endswith("/_meta.json"))):
            continue
        selected[name] = project(name, raw)
    text = lossless(display)
    require(text.get("schema") == "pobr-native-display-texts/v1"
            and all(text.get(key) == target[key] for key in ("game_patch", "realm"))
            and text.get("locale") == locale, "Display target/locale mismatch")
    selected[f"i18n/{locale}/display_texts.json"] = raw_json(text)
    # A new schema-3 inventory seals the projected bytes. Source identities and
    # ownership are retained once in build evidence, never masquerade as these.
    inventory = {name: bundle.sha256(raw) for name, raw in sorted(selected.items())
                 if not name.startswith("overlay-common/")}
    manifest = {key: snapshot[key] for key in ("schema_version", "poe_version", "domains")}
    manifest.update(languages=[locale], files=inventory)
    bundle.validate_declarations(manifest, inventory)
    selected["manifest.json"] = raw_json(manifest)
    digests = {name: bundle.sha256(raw) for name, raw in sorted(selected.items())}
    blocks, payloads = [], {}
    for kind in ("core", "rules", "locale"):
        members = {name: raw.decode("utf-8") for name, raw in selected.items() if block_kind(name) == kind}
        raw = raw_json({"schema": BLOCK_SCHEMA, "target": target, "kind": kind, "files": members})
        name = f"blocks/{kind}.json"
        blocks.append({"path": name, **pin(raw), "files": sorted(members)})
        payloads[name] = raw
    release = release_document(target, locale, digests, blocks)
    payloads["manifest.json"] = raw_json(release)
    return release, payloads


def inspect_release(payloads, expected_id=None):
    release = bundle.decode(payloads["manifest.json"])
    require(release.get("schema") == SCHEMA, "Unknown runtime release schema")
    version, locale, target = release["version"], release["locale"], release["target"]
    require(isinstance(version, str) and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", version), "Invalid pack version")
    target_valid(target, version)
    require(locale in ("zh-CN", "zh-TW"), "Unsupported pack locale")
    require(expected_id is None or release.get("release_id") == expected_id, "Unexpected release identity")
    digests, blocks = release["digests"], release["blocks"]
    require(isinstance(digests, dict) and all(isinstance(v, str) and HEX.fullmatch(v) for v in digests.values()),
            "Invalid pack digests")
    require(len(blocks) == 3 and [b["path"] for b in blocks] == [f"blocks/{k}.json" for k in ("core", "rules", "locale")],
            "Invalid runtime block list")
    require(set(payloads) == {"manifest.json", *(b["path"] for b in blocks)}, "Unlisted/missing release file")
    files = {}
    for spec in blocks:
        raw = pinned(payloads[spec["path"]], spec)
        block = bundle.decode(raw)
        kind = spec["path"].split("/")[1].removesuffix(".json")
        require(set(block) == {"schema", "target", "kind", "files"}
                and block["schema"] == BLOCK_SCHEMA and block["target"] == target and block["kind"] == kind,
                "Mixed runtime block target/kind")
        require(isinstance(block["files"], dict) and sorted(block["files"]) == spec["files"], "Block file inventory mismatch")
        for name, text in block["files"].items():
            bundle.relative_path(name)
            require(name == "manifest.json" or (name.endswith(".json") and name.split("/")[0]
                    in {"base", "overlay", "generated", "overlay-common", "i18n"}), "Invalid pack file path")
            require(name not in files and block_kind(name) == kind and isinstance(text, str), "Duplicate/misplaced pack file")
            raw = text.encode("utf-8")
            require(bundle.sha256(raw) == digests.get(name), f"Pack file SHA256 mismatch: {name}")
            lossless(raw)
            files[name] = raw
    require(release == release_document(target, locale, digests, blocks) and set(files) == set(digests),
            "Runtime release identity/inventory mismatch")
    snapshot = bundle.decode(files["manifest.json"])
    inventory = {name: digest for name, digest in digests.items()
                 if name != "manifest.json" and not name.startswith("overlay-common/")}
    require(snapshot.get("schema_version") == 3 and snapshot.get("poe_version") == version
            and snapshot.get("languages") == [locale] and snapshot.get("files") == inventory,
            "Runtime snapshot identity/inventory mismatch")
    bundle.validate_declarations(snapshot, inventory)
    require(all(not name.startswith("i18n/") or name.startswith(f"i18n/{locale}/") for name in files),
            "Mixed runtime locale")
    return release, files


def read_release(source, expected_id=None):
    source = Path(source).absolute()
    require(all(not p.is_symlink() for p in (source, *source.parents)), "Symlink release path")
    if source.is_dir():
        paths = bundle.json_paths(source)
        require(set(paths) == {"manifest.json", "blocks/core.json", "blocks/rules.json", "blocks/locale.json"},
                "Unlisted/missing release file")
        # Reject non-JSON extras as well.
        actual = {p.relative_to(source).as_posix() for p in source.rglob("*") if p.is_file()}
        require(actual == set(paths), "Unlisted release file")
        payloads = {name: bundle.read_file(source, name) for name in paths}
    else:
        with zipfile.ZipFile(io.BytesIO(regular(source))) as archive:
            names = archive.namelist()
            require(len(names) == len(set(names)), "Duplicate archive member")
            payloads = {}
            for entry in archive.infolist():
                bundle.relative_path(entry.filename)
                require(entry.filename in {"manifest.json", "blocks/core.json", "blocks/rules.json", "blocks/locale.json"}
                        and not entry.is_dir() and ((entry.external_attr >> 16) & 0o170000) in (0, 0o100000),
                        "Invalid release archive member")
                payloads[entry.filename] = archive.read(entry)
    release, files = inspect_release(payloads, expected_id)
    return release, files, payloads


def archive_bytes(payloads):
    handle = io.BytesIO()
    with zipfile.ZipFile(handle, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, raw in sorted(payloads.items()):
            entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, raw, compresslevel=9)
    return handle.getvalue()


def fresh_output(output):
    output = Path(output).absolute()
    require(".." not in output.parts and all(not p.is_symlink() for p in (output, *output.parents)),
            "Unsafe output path")
    data = ROOT / "data"
    require(not output.is_relative_to(data) and not data.is_relative_to(output), "Output must be outside published data")
    require(not output.exists(), f"Fresh output required: {output}")
    return output


def materialize(source, output, expected_id=None):
    release, files, _ = read_release(source, expected_id)
    output = fresh_output(output)
    require(not output.is_relative_to(Path(source).resolve()), "Output must be separate from input pack")
    version = release["version"]
    legacy = {f"data/{version}/{name}": raw for name, raw in files.items() if not name.startswith("overlay-common/")}
    legacy.update({f"data/{name}": raw for name, raw in files.items() if name.startswith("overlay-common/")})
    legacy["data/CURRENT"] = (version + "\n").encode()
    receipt = bundle.document(version, legacy, release["runtime_identity"])
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".materialize-", dir=output.parent) as temp:
        stage = Path(temp) / "result"
        for name, raw in legacy.items():
            write(stage / name, raw)
        write(stage / "bundle.json", raw_json(receipt))
        bundle.verify(stage, receipt["bundle_id"])
        stage.rename(output)
    return receipt


def frozen_inputs(source, recipe_path):
    source = Path(source)
    receipt = bundle.verify(source)
    version = receipt["data_version"]
    files, source_id = bundle.snapshot_inputs(source, version)
    recipe_raw = regular(recipe_path)
    recipe = bundle.decode(recipe_raw)
    require(recipe.get("schema") == "pobr-remaining-domain-integration/v1"
            and recipe["runtime_identity"] == source_id and recipe["bundle_id"] == receipt["bundle_id"]
            and recipe["snapshot_manifest_sha256"] == bundle.sha256(files[f"data/{version}/manifest.json"]),
            "Frozen recipe identity mismatch")
    snapshot = bundle.decode(files[f"data/{version}/manifest.json"])
    require(set(recipe["files"]) == set(snapshot["files"]), "Frozen recipe inventory mismatch")
    owners = {}
    for name, entry in recipe["files"].items():
        require(entry["sha256"] == snapshot["files"][name], f"Recipe hash mismatch: {name}")
        # The already sealed bundle supplies bytes; historical absolute paths
        # in the recipe are evidence, never input fallbacks.
        owners[name] = {key: value for key, value in entry.items() if key != "path"}
    shared = {name.removeprefix("data/overlay-common/"): {"sha256": bundle.sha256(raw)}
              for name, raw in files.items() if name.startswith("data/overlay-common/")}
    require(shared == recipe["shared_files"], "Recipe shared-rule mismatch")
    evidence = {"schema": INPUT_SCHEMA, "version": version, "source_bundle_id": receipt["bundle_id"],
                "source_runtime_identity": source_id, "source_manifest_sha256": recipe["snapshot_manifest_sha256"],
                "recipe": pin(recipe_raw), "owners": owners, "shared_files": shared,
                "producer_receipts": {k: {"sha256": v["sha256"]} for k, v in recipe["producer_receipts"].items()},
                "language_receipt_sha256": recipe["language_receipt_sha256"],
                "scope": recipe["scope"], "adoption": "not-adopted"}
    return files, evidence


def create(source, recipe_path, display_path, output, target, locale):
    output = fresh_output(output)
    require(not output.is_relative_to(Path(source).resolve()), "Output must be separate from input bundle")
    files, evidence = frozen_inputs(source, recipe_path)
    display = regular(display_path)
    release, payloads = assemble(files, evidence["version"], target, locale, display)
    inspect_release(payloads, release["release_id"])
    archive = archive_bytes(payloads)
    sizes = {name: {**pin(raw), "gzip9_bytes": len(gzip.compress(raw, compresslevel=9, mtime=0))}
             for name, raw in sorted(payloads.items())}
    evidence.update(display_input=pin(display), release_id=release["release_id"],
                    runtime_identity=release["runtime_identity"], player_files=sizes,
                    player_totals={"bytes": sum(s["bytes"] for s in sizes.values()),
                                   "gzip9_bytes": sum(s["gzip9_bytes"] for s in sizes.values())},
                    cli_archive=pin(archive),
                    measurement="Local raw bytes and independent gzip level 9; no HTTP timing or transfer measurement")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".runtime-pack-", dir=output.parent) as temp:
        stage = Path(temp) / "result"
        for name, raw in payloads.items():
            write(stage / "player" / name, raw)
        write(stage / "runtime-pack.zip", archive)
        write(stage / "build-receipt.json", raw_json(evidence))
        stage.rename(output)
    return release


def export_inputs(source, recipe, output):
    """Make a portable Git blob without copying any source/verification cache."""
    output = fresh_output(output)
    require(not output.is_relative_to(Path(source).resolve()), "Output must be separate from input bundle")
    files, evidence = frozen_inputs(source, recipe)
    manifest_raw = raw_json({**evidence, "files": {name: pin(raw) for name, raw in sorted(files.items())}})
    archive = archive_bytes({**files, "manifest.json": manifest_raw})
    lock = {"schema": LOCK_SCHEMA, "publication": "pending", "repository": None, "revision": None,
            "path": f"handoff/runtime-{evidence['version']}/core-inputs.zip", **pin(archive),
            "manifest": pin(manifest_raw), "expanded_bytes": sum(map(len, files.values())) + len(manifest_raw),
            "source_manifest_sha256": evidence["source_manifest_sha256"],
            "source_runtime_identity": evidence["source_runtime_identity"],
            "recipe": evidence["recipe"]}
    output.mkdir(parents=True)
    write(output / "core-inputs.zip", archive)
    write(output / "runtime-inputs.lock.json", raw_json(lock))
    return lock


def git_handoff(artifact_dir):
    """A local bare Git artifact, with no checkout, remote or publication."""
    artifact_dir = Path(artifact_dir).absolute()
    lock = bundle.decode(regular(artifact_dir / "runtime-inputs.lock.json"))
    archive = pinned(regular(artifact_dir / "core-inputs.zip"), lock)
    bare = artifact_dir / "handoff.git"
    require(not bare.exists(), "Fresh local Git handoff required")
    subprocess.run(["git", "init", "--quiet", "--bare", str(bare)], check=True)
    env = {**os.environ, "GIT_AUTHOR_NAME": "PoBR runtime packaging", "GIT_AUTHOR_EMAIL": "runtime@pobr.invalid",
           "GIT_COMMITTER_NAME": "PoBR runtime packaging", "GIT_COMMITTER_EMAIL": "runtime@pobr.invalid",
           "GIT_AUTHOR_DATE": "2000-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2000-01-01T00:00:00Z"}

    def git(*args, raw=None):
        return subprocess.check_output(["git", "--git-dir", str(bare), *args], input=raw, env=env).decode().strip()

    oid = git("hash-object", "-w", "--stdin", raw=archive)
    parts = list(bundle.relative_path(lock["path"]).parts)
    tree = git("mktree", raw=f"100644 blob {oid}\t{parts.pop()}\n".encode())
    for part in reversed(parts):
        tree = git("mktree", raw=f"040000 tree {tree}\t{part}\n".encode())
    revision = git("commit-tree", tree, raw=b"Freeze complete PoBR runtime packaging inputs\n")
    git("update-ref", "refs/heads/runtime-inputs", revision)
    result = {"schema": "pobr-local-git-handoff/v1", "revision": revision, "tree": tree,
              "blob": oid, "ref": "refs/heads/runtime-inputs", "path": lock["path"],
              "sha256": lock["sha256"], "publication": "local-only", "repository": None}
    write(artifact_dir / "git-handoff.json", raw_json(result))
    return result


def import_pack(requirements_path, lock_path, imports_path, output, cache, offline=False,
                allow_unpublished=False, fetcher=None, ordinary_path=None, local_parts=None, shared_root=None):
    requirements = bundle.decode(regular(requirements_path))
    lock = bundle.decode(regular(lock_path))
    from build_data import LOCK_SCHEMA as BUILD_LOCK_SCHEMA
    config = requirements["runtime_pack"]
    require(config["schema"] == SCHEMA, "Unknown required runtime pack")
    require(config.get("blocks", ["core", "rules", "locale"]) == ["core", "rules", "locale"],
            "Unsupported required runtime blocks")
    require(lock.get("schema") in (LOCK_SCHEMA, BUILD_LOCK_SCHEMA), "Unknown runtime input lock")
    if lock["schema"] == LOCK_SCHEMA:
        require(lock.get("repository") != "ackness/pobr", "PoBR cannot store external runtime data")
        from data_imports import validate_pin
        validate_pin(lock)
        validate_pin(lock["manifest"])
        require(type(lock["expanded_bytes"]) is int and lock["expanded_bytes"] > 0
                and HEX.fullmatch(lock["source_manifest_sha256"])
                and HEX.fullmatch(lock["source_runtime_identity"]), "Invalid portable core lock")
        validate_pin(lock["recipe"])
        if lock.get("publication") == "pending":
            require(lock.get("repository") is None and lock.get("revision") is None, "Pending lock has Git address")
            require(allow_unpublished, "Core runtime Git artifact is unpublished; publish the pinned core-inputs.zip and lock its repository/revision")
            return {"status": "pending-core-publication", "expected_artifact": {k: lock[k] for k in ("path", "bytes", "sha256")},
                    "adoption": "not-adopted"}
        require(lock.get("publication") == "published", "Invalid runtime publication status")
        require(not local_parts, "Local split packages need a build data lock")
    output = fresh_output(output)
    cache = Path(cache).absolute()
    require(".." not in cache.parts and all(not p.is_symlink() for p in (cache, *cache.parents)), "Unsafe cache path")
    cache.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".pack-import-", dir=output.parent) as temp:
        temp = Path(temp)
        fetcher = fetcher or GitInputs(temp / "git")
        if lock["schema"] == BUILD_LOCK_SCHEMA:
            from build_data import prepare
            require(lock["target"] == requirements["target"], "Build data target mismatch")
            receipt = prepare(lock_path, temp / "input", cache, shared_root or ROOT / "data/overlay-common", offline=offline,
                              local_parts=local_parts, fetcher=fetcher)
            version = receipt["data_version"]
            files, _ = bundle.snapshot_inputs(temp / "input", version)
            manifest = {"schema": BUILD_LOCK_SCHEMA, "bundle": receipt}
        else:
            archive = acquire(lock, lock["path"], lock, cache, offline, fetcher)
            from data_imports import unpack
            manifest = unpack(archive, lock, temp / "input")
            require(manifest.get("schema") == INPUT_SCHEMA
                    and manifest["source_manifest_sha256"] == lock["source_manifest_sha256"]
                    and manifest["source_runtime_identity"] == lock["source_runtime_identity"]
                    and manifest["recipe"] == lock["recipe"], "Portable core input identity mismatch")
            version = manifest["version"]
            files, source_id = bundle.snapshot_inputs(temp / "input", version)
            require(source_id == lock["source_runtime_identity"]
                    and bundle.sha256(files[f"data/{version}/manifest.json"]) == lock["source_manifest_sha256"],
                    "Portable core snapshot mismatch")
        ordinary_evidence = None
        if "ordinary" in config:
            from runtime_ordinary import overlay
            files, ordinary_evidence = overlay(files, version, requirements["target"], config["ordinary"],
                                               Path(requirements_path).parent, ordinary_path)
        else:
            require(ordinary_path is None, "Ordinary input was not declared")
        imports = Path(imports_path)
        display = regular(imports / "runtime/display_texts.json")
        display_receipt = bundle.decode(regular(imports / "runtime/receipt.json"))
        pinned(display, display_receipt["files"]["display_texts.json"])
        import_receipt = bundle.decode(regular(imports / "import-receipt.json"))
        require(import_receipt["requirements_sha256"] == bundle.sha256(regular(requirements_path))
                and import_receipt["target"] == requirements["target"]
                and import_receipt["locale"] == requirements["translation"]["locale"], "Display import requirements mismatch")
        release, payloads = assemble(files, version, requirements["target"], requirements["translation"]["locale"], display)
        inspect_release(payloads)
        stage = temp / "result"
        for name, raw in payloads.items():
            write(stage / "player" / name, raw)
        write(stage / "runtime-pack.zip", archive_bytes(payloads))
        evidence = {"core_input_lock": lock, "source": manifest,
              "display_input": pin(display), "import_receipt": pin(regular(imports / "import-receipt.json")),
              "release_id": release["release_id"], "adoption": "not-adopted"}
        if ordinary_evidence is not None:
            evidence["ordinary"] = ordinary_evidence
        write(stage / "build-receipt.json", raw_json(evidence))
        stage.rename(output)
    return {"status": "assembled", "release_id": release["release_id"], "adoption": "not-adopted"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    create_parser = sub.add_parser("create")
    for name in ("bundle", "recipe", "display", "out"):
        create_parser.add_argument(f"--{name}", type=Path, required=True)
    create_parser.add_argument("--requirements", type=Path, default=ROOT / "pipeline/data-requirements.json")
    for command in ("verify", "materialize"):
        p = sub.add_parser(command)
        p.add_argument("--pack", type=Path, required=True)
        p.add_argument("--expected-id")
        if command == "materialize":
            p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("export-inputs")
    for name in ("bundle", "recipe", "out"):
        p.add_argument(f"--{name}", type=Path, required=True)
    p = sub.add_parser("import")
    p.add_argument("--requirements", type=Path, default=ROOT / "pipeline/data-requirements.json")
    p.add_argument("--lock", type=Path, default=ROOT / "pipeline/build-data.lock.json")
    p.add_argument("--imports", type=Path, required=True)
    p.add_argument("--ordinary", type=Path, help="Completed declared ordinary outputs (or CI output artifact)")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    p.add_argument("--offline", action="store_true")
    p.add_argument("--allow-unpublished", action="store_true")
    p.add_argument("--game", type=Path, help="Explicit local game package")
    p.add_argument("--language", type=Path, help="Explicit local language package")
    p = sub.add_parser("git-handoff")
    p.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "create":
        request = bundle.decode(regular(args.requirements))
        result = create(args.bundle, args.recipe, args.display, args.out, request["target"], request["translation"]["locale"])
        result = {"release_id": result["release_id"], "runtime_identity": result["runtime_identity"]}
    elif args.command == "verify":
        release, _, payloads = read_release(args.pack, args.expected_id)
        result = {"release_id": release["release_id"], "runtime_identity": release["runtime_identity"],
                  "manifest_sha256": bundle.sha256(payloads["manifest.json"])}
    elif args.command == "materialize":
        result = materialize(args.pack, args.out, args.expected_id)
    elif args.command == "export-inputs":
        result = export_inputs(args.bundle, args.recipe, args.out)
    elif args.command == "git-handoff":
        result = git_handoff(args.artifacts)
    else:
        result = import_pack(args.requirements, args.lock, args.imports, args.out, args.cache,
                             args.offline, args.allow_unpublished, ordinary_path=args.ordinary,
                             local_parts={k: p for k in ("game", "language") if (p := getattr(args, k)) is not None})
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
