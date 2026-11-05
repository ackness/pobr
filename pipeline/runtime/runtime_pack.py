"""Build/verify compact release blocks and adapt them to the existing GameData loader.

This is transport packaging, not a second game-data interpreter. Inner documents
are JSON strings: JS must never round an i64 or rewrite a source float on staging.
"""

import argparse
import io
import json
from pathlib import Path
import re
import tempfile
import zipfile

from pipeline.runtime import data_bundle as bundle
from pipeline.imports.data_imports import pinned, regular, require, write

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "pobr-runtime-release/v1"
BLOCK_SCHEMA = "pobr-runtime-block/v1"
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
    require(isinstance(locale, str) and bundle.LANGUAGE_TAG.fullmatch(locale), "Unsupported pack locale")
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
    if "contract_version" in snapshot:
        # The producer's data-contract declaration; the WASM loader checks it too.
        manifest["contract_version"] = snapshot["contract_version"]
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
    require(isinstance(locale, str) and bundle.LANGUAGE_TAG.fullmatch(locale), "Unsupported pack locale")
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


def build_release(lock_path, data, output, expected_id=None):
    """Assemble the compact release from a prepared owner snapshot.

    The owner game and language publications already contain every runtime file,
    including the selected locale's display texts; nothing is reproduced here.
    The data lock fixes target, locale, the snapshot runtime identity and, once
    adopted, the release identity.
    """
    lock = bundle.decode(regular(lock_path))
    require(lock.get("schema") == "pobr-data-lock/v2", "Release build requires the data lock")
    target, locale = lock["target"], lock["locale"]
    version = target["game_patch"]
    files, runtime_identity = bundle.data_inputs(data, version)
    require(runtime_identity == lock["runtime_identity"], "Prepared data differs from the data lock")
    display_name = f"data/{version}/i18n/{locale}/display_texts.json"
    require(display_name in files, "Prepared snapshot lacks the selected locale display texts")
    release, payloads = assemble(files, version, target, locale, files[display_name])
    inspect_release(payloads)
    expected_id = expected_id or lock.get("release_id")
    require(expected_id is None or release["release_id"] == expected_id, "Unexpected release identity")
    output = fresh_output(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".pack-build-", dir=output.parent) as temp:
        stage = Path(temp) / "result"
        for name, raw in payloads.items():
            write(stage / "player" / name, raw)
        write(stage / "runtime-pack.zip", archive_bytes(payloads))
        write(stage / "build-receipt.json", raw_json({
            "source": "prepared owner snapshot", "data_version": version,
            "source_runtime_identity": runtime_identity, "release_id": release["release_id"],
            "adoption": "not-adopted"}))
        stage.rename(output)
    return {"status": "assembled", "release_id": release["release_id"],
            "runtime_identity": release["runtime_identity"], "source_runtime_identity": runtime_identity}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("build", help="Assemble the compact release from prepared owner data")
    p.add_argument("--lock", type=Path, default=ROOT / "pipeline/data.lock.json")
    p.add_argument("--data", type=Path, default=ROOT / "data")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--expected-id", help="Defaults to the data lock's release_id when recorded")
    for command in ("verify", "materialize"):
        p = sub.add_parser(command)
        p.add_argument("--pack", type=Path, required=True)
        p.add_argument("--expected-id")
        if command == "materialize":
            p.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        result = build_release(args.lock, args.data, args.out, args.expected_id)
    elif args.command == "verify":
        release, _, payloads = read_release(args.pack, args.expected_id)
        result = {"release_id": release["release_id"], "runtime_identity": release["runtime_identity"],
                  "manifest_sha256": bundle.sha256(payloads["manifest.json"])}
    else:
        result = materialize(args.pack, args.out, args.expected_id)
    print(json.dumps(result, ensure_ascii=False))

if __name__ == "__main__":
    main()
