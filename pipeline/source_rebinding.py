"""Offline draft source-lock rebinding and deterministic ordinary-input handoff.

No acquisition, decoding, source admission, runtime assembly or publication occurs.
The externally pinned recipe selects the evidence closure; historical records are
preserved verbatim, including unavailable receipt identities and unknown fields.
"""
import argparse
import copy
import io
import json
import stat
from pathlib import Path
import zipfile

import client_sources as client

RECIPE = "pobr-source-rebinding-recipe/v1"
STATUS = "unreviewed-acquisition-draft"


def require(value, message):
    if not value:
        raise ValueError(message)


def canonical(value):
    # Matches the native reader's sorted serde_json object + newline identity.
    return (json.dumps(value, sort_keys=True, separators=(",", ":"),
                       ensure_ascii=False, allow_nan=False) + "\n").encode()


def pin(raw):
    return {"bytes": len(raw), "sha256": client.sha256(raw)}


def portable(name):
    client.relative(name)
    require(not name.startswith("/") and all(
        not part.endswith((".", " ")) and not any(ord(c) < 32 or c in '<>"|?*' for c in part)
        and part.split(".")[0].upper() not in {
            "CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)),
            *(f"LPT{i}" for i in range(1, 10))}
        for part in name.split("/")), f"Nonportable path: {name}")
    return name


def paths(names):
    folded = set()
    for name in names:
        portable(name)
        require(name.casefold() not in folded, "Duplicate/case-colliding path")
        folded.add(name.casefold())
    for name in folded:
        require(not any(p in folded for p in (
            "/".join(name.split("/")[:i]) for i in range(1, len(name.split("/"))))),
            "File/directory path collision")


def read(path):
    path = client.safe_path(path)
    # Regular-file checks and O_NOFOLLOW remain in the established primitive.
    return client.read_file(path, path.stat().st_size)


def pinned(path, expected):
    require(type(expected["bytes"]) is int and expected["bytes"] >= 0, "Invalid byte pin")
    client.digest(expected["sha256"])
    path = client.safe_path(path)
    require(path.stat().st_size == expected["bytes"], f"Byte count mismatch: {path}")
    raw = client.read_file(path, expected["bytes"])
    require(pin(raw) == expected, f"Fingerprint mismatch: {path}")
    return raw


def inventory(root):
    root = client.safe_path(root)
    require(root.is_dir(), "Expected input directory")
    result = {}
    for child in sorted(root.rglob("*")):
        client.safe_path(child)
        require(child.is_dir() or child.is_file(), "Nonregular input entry")
        if child.is_file():
            result[child.relative_to(root).as_posix()] = pin(read(child))
    paths(result)
    return result


def exact_root(root, expected):
    require(inventory(root) == expected, f"Inventory mismatch: {root}")


def absent(path, inputs=()):
    path = client.safe_path(path)
    require(not path.exists(), f"Output already exists: {path}")
    for source in inputs:
        source = client.safe_path(source)
        require(path != source and path not in source.parents and source not in path.parents,
                "Output overlaps an input")
    return path


def scope(resources):
    paths(resources)
    tables = sorted(name for name in resources if name.startswith("Data/Balance/")
                    and name.endswith(".datc64"))
    other = sorted(set(resources) - set(tables))
    return {"resource_count": len(resources), "dat_table_count": len(tables),
            "resources": sorted(resources), "dat_resources": tables, "other_resources": other}


def observations(receipt):
    resources = receipt["resources"]
    if isinstance(resources, list):
        require(len({r["path"] for r in resources}) == len(resources), "Duplicate receipt resource")
        resources = {r["path"]: r for r in resources}
    require(isinstance(resources, dict), "Invalid receipt resources")
    result = {}
    for name, r in resources.items():
        portable(name)
        result[name] = {"bytes": r.get("observed_bytes", r.get("bytes")),
                        "sha256": r.get("observed_sha256", r.get("sha256")),
                        **{k: r[k] for k in ("bundle_url", "uncompressed_offset", "uncompressed_length")}}
    return result


def check_receipt(receipt, target):
    for field in ("game", "realm", "game_patch"):
        require(receipt[field] == target[field], f"Mixed receipt source identity: {field}")


def check_evidence(recipe, evidence, source):
    """Join only the declared acquisition closure; never follow opaque old paths."""
    doc = lambda name: client.decode(evidence[name])
    group_spec = recipe["groups"]
    groups = doc(group_spec["evidence"])
    require(set(groups) == set(group_spec["counts"]) == set(group_spec["receipts"]),
            "Resource group keys differ")
    combined = set()
    locations = {}
    outside = {}
    acquired = source["resources"]
    for group, names in groups.items():
        require(type(group_spec["counts"][group]) is int
                and len(names) == len(set(names)) == group_spec["counts"][group], "Wrong group count")
        require(not combined.intersection(names), "Overlapping resource groups")
        combined.update(names)
        receipt = doc(group_spec["receipts"][group])
        check_receipt(receipt, source)
        observed = observations(receipt)
        for name in names:
            require(name in acquired and name in observed and all(
                observed[name][k] == value for k, value in acquired[name].items()
                if k in {"bytes", "sha256", "bundle_url", "uncompressed_offset", "uncompressed_length"}),
                f"Acquisition resource differs: {name}")
            require({"bytes", "sha256"} <= set(acquired[name]), "Missing original raw pin")
            locations[name] = observed[name]
        outside[group] = sorted(set(observed) - set(names))
    require(combined == set(acquired), "Acquisition groups do not cover exact selected resources")
    replacement = recipe["replacement"]
    online, offline = (doc(replacement[key]) for key in ("online", "offline"))
    for r in (online, offline):
        check_receipt(r, source)
        require(r["status"] == STATUS and r["production_ready"] is False,
                "Replacement is not a draft acquisition")
    expected_names = set(groups[replacement["group"]])
    require(set(observations(online)) == set(observations(offline)) == expected_names,
            "Wrong replacement resource scope")
    require(observations(online) == observations(offline), "Replacement offline pins differ")
    require(offline["mode"] == "offline" and offline["network"]["requests"] == 0
            and offline["network"]["body_bytes"] == 0, "Replacement offline evidence used HTTP")
    require(online["mode"] == "online", "Missing online replacement observation")
    selection = evidence[replacement["selection"]]
    require(online["selection"]["sha256"] == offline["selection"]["sha256"] == client.sha256(selection),
            "Replacement selection pin mismatch")
    selection = client.decode(selection)
    check_receipt(selection, source)
    require(set(selection["resources"]) == expected_names, "Replacement selection scope mismatch")
    for name in expected_names:
        for key in ("bundle_url", "uncompressed_offset", "uncompressed_length"):
            require(selection["resources"][name][key] == observations(online)[name][key],
                    "Replacement location differs")
    ranges = doc(replacement["ranges"])
    original = doc(replacement["original_bc"])
    require(ranges["source_patch"] == source["game_patch"], "Mixed range replacement patch")
    require(ranges["original_bc_receipt_sha256"] == client.sha256(evidence[replacement["original_bc"]]),
            "Replacement bound to another BC receipt")
    require(type(replacement["range_count"]) is int
            and len(ranges["records"]) == replacement["range_count"], "Wrong replacement range count")
    require(ranges["offline"] == {"requests": 0, "body_bytes": 0}, "BC offline used HTTP")
    identities = set()
    keys = ("url", "final_url", "start", "length", "total", "etag", "sha256")
    for pair in ranges["records"]:
        old, new = pair["historical_record"], pair["replacement_record"]
        require(old in original["ranges"], "Replaced range absent from original receipt")
        require(old["provenance"]["kind"] == "verified-cache-copy"
                and new["provenance"] == {"kind": "http-range"}, "Relabelled transport evidence")
        identity = tuple(new[k] for k in keys)
        require(identity == tuple(old[k] for k in keys) and identity not in identities,
                "Replacement range identity differs/duplicates")
        client.official_url(new["url"], source["base_url"] + "Bundles2/")
        identities.add(identity)
    return outside, locations


def metadata(original_manifest, original_lock, recipe, evidence):
    require(recipe["schema"] == RECIPE, "Unsupported rebinding recipe")
    require(original_manifest["format"] == "repoe-native-facts/v4"
            and original_manifest["status"] == "partial-candidate"
            and original_manifest["production_ready"] is False, "Expected partial F77-style facts")
    identity = original_manifest["source_identity"]
    require(original_lock["schema_version"] == 1, "Unsupported source lock")
    selected = original_lock["sources"][identity["source"]]
    retired = recipe["retired"]
    require(len({(r["original"]["kind"], r["original"]["sha256"]) for r in retired}) == len(retired),
            "Duplicate retired receipt")
    require(all(any(r["original"] in source["acquisition"]["evidence"]
                    for source in original_lock["sources"].values()) for r in retired),
            "Retired receipt is not an original acquisition identity")
    require(set(recipe["sources"]) == set(original_lock["sources"]), "Source inventory mismatch")
    require(identity["selected_source_sha256"] == client.sha256(canonical(selected)),
            "Original selected source identity mismatch")
    require(identity["acquisition"] == selected["acquisition"], "Original acquisition identity mismatch")
    for k in ("kind", "game", "realm", "game_patch"):
        require(identity[k] == selected[k], f"Mixed source identity: {k}")
    require(identity["raw_resources"] == {
        n: {k: r[k] for k in ("bytes", "sha256")} for n, r in selected["resources"].items()},
        "Manifest raw pins differ from selected source")
    outside, locations = check_evidence(recipe, evidence, selected)
    lock = copy.deepcopy(original_lock)
    by_hash = {client.sha256(raw): name for name, raw in evidence.items()}
    scopes = {}
    for name, source in lock["sources"].items():
        for k in ("game", "realm", "game_patch", "base_url", "decoder"):
            require(source[k] == selected[k], f"Mixed lock source identity: {k}")
        require(all(selected["resources"].get(n) == r for n, r in source["resources"].items()),
                "Ancestor resources are not an exact subset")
        spec = recipe["sources"][name]
        s = scope(source["resources"])
        require(type(spec["resource_count"]) is int and type(spec["dat_table_count"]) is int
                and s["resource_count"] == spec["resource_count"]
                and s["dat_table_count"] == spec["dat_table_count"], "Wrong source scope/count")
        scopes[name] = s
        # BC's old source stored only raw pins for its 38 additions. Add the
        # exact acquisition tuples without replacing any original source field.
        for path, record in source["resources"].items():
            for field, value in locations[path].items():
                record.setdefault(field, value)
        historical = copy.deepcopy(source["acquisition"])
        require(historical["status"] == STATUS, "Original source is not draft")
        active = []
        replaced = []
        for record in historical["evidence"]:
            replacement = next((r for r in recipe["retired"] if r["original"] == record), None)
            if replacement:
                receipt = replacement["replacement"]
                new_hash = client.sha256(evidence[receipt])
                require(new_hash != record["sha256"], "Replacement retains original receipt hash")
                active.append({"kind": replacement["kind"], "sha256": new_hash})
                replaced.append({**replacement, "replacement_pin": pin(evidence[receipt]),
                                 "disposition": "unavailable-original-replaced-not-recovered"})
            else:
                require(record["sha256"] in by_hash, f"Missing receipt pin: {record['kind']}")
                active.append(record)
        for extra in spec["supplements"]:
            active.append({"kind": extra["kind"], "sha256": client.sha256(evidence[extra["evidence"]])})
        source["scope"] = (f"Draft: exactly {s['dat_table_count']} physical DAT tables and "
                           f"{len(s['other_resources'])} other resources ({s['resource_count']} total); "
                           "exact paths in rebinding.resource_scope. Partial source input; "
                           "metadata, locale, foreign-key, gameplay and production closure are not certified.")
        source["acquisition"] = {"status": STATUS, "evidence": active}
        require("rebinding" not in source, "Input has already been rebound")
        source["rebinding"] = {
            "schema": "pobr-source-evidence-binding-draft/v1", "status": "draft-pending-source-acceptance",
            "original_source_sha256": client.sha256(canonical(original_lock["sources"][name])),
            "original_scope": original_lock["sources"][name]["scope"],
            "original_acquisition": historical, "replacements": replaced, "resource_scope": s,
            "evidence_files": {n: {"path": f"evidence/{n}", **pin(raw)} for n, raw in sorted(evidence.items())},
            "source_approval_claimed": False,
        }
    manifest = copy.deepcopy(original_manifest)
    selected = lock["sources"][identity["source"]]
    lock_raw = client.encoded(lock)
    manifest["source_identity"]["source_lock_sha256"] = client.sha256(lock_raw)
    manifest["source_identity"]["selected_source_sha256"] = client.sha256(canonical(selected))
    manifest["source_identity"]["acquisition"] = copy.deepcopy(selected["acquisition"])
    manifest["files"]["source-lock.json"] = pin(lock_raw)
    return manifest, lock, scopes, outside


def validate_tables(root, manifest):
    resources = manifest["source_identity"]["raw_resources"]
    found = set()
    for name in manifest["files"]:
        if not name.startswith("tables/"):
            continue
        table = client.decode(read(root / name))
        raw = table["raw_resource"]
        require(raw in resources and raw not in found, "Table raw resource mismatch")
        require(name == f"tables/{table['table']}.json"
                and raw == f"Data/Balance/{table['table']}.datc64", "Table name/resource mismatch")
        require(table["validation"]["rows"] == len(table["rows"]), "Table row count mismatch")
        found.add(raw)
    require(found == {n for n in resources if n.endswith(".datc64")}, "Wrong physical table scope")


def create(source, recipe_path, recipe_sha256, output):
    source = client.safe_path(source)
    recipe_raw = read(recipe_path)
    require(client.sha256(recipe_raw) == client.digest(recipe_sha256), "Recipe pin mismatch")
    recipe = client.decode(recipe_raw)
    manifest_raw = pinned(source / "manifest.json", recipe["original"]["manifest"])
    lock_raw = pinned(source / "source-lock.json", recipe["original"]["lock"])
    original = client.decode(manifest_raw)
    require(original["source_identity"]["source_lock_sha256"] == client.sha256(lock_raw), "Original lock pin mismatch")
    require(original["files"]["source-lock.json"] == pin(lock_raw), "Original lock descriptor mismatch")
    exact_root(source, {"manifest.json": pin(manifest_raw), **original["files"]})
    validate_tables(source, original)
    paths(recipe["evidence"])
    evidence = {name: pinned(Path(item["path"]), {k: item[k] for k in ("bytes", "sha256")})
                for name, item in recipe["evidence"].items()}
    output = absent(output, [source, recipe_path, *(Path(v["path"]) for v in recipe["evidence"].values())])
    portable_recipe = copy.deepcopy(recipe)
    for item in portable_recipe["evidence"].values():
        del item["path"]
    new, lock, scopes, outside = metadata(original, client.decode(lock_raw), portable_recipe, evidence)
    files = {"provenance/original-manifest.json": manifest_raw,
             "provenance/original-source-lock.json": lock_raw,
             "facts/manifest.json": client.encoded(new), "facts/source-lock.json": client.encoded(lock),
             **{f"evidence/{name}": raw for name, raw in evidence.items()}}
    payload_pins = {n: d for n, d in original["files"].items() if n != "source-lock.json"}
    proof = {"schema": "pobr-source-rebinding-draft/v1", "status": "draft-pending-source-acceptance",
             "production_ready": False, "source_approval_claimed": False,
             "recipe": portable_recipe, "resource_scopes": scopes,
             "distinct_acquired_resources": len(original["source_identity"]["raw_resources"]),
             "receipt_resources_outside_selected_groups": outside,
             "payload_byte_equivalence": payload_pins,
             "excluded_domains": recipe["excluded_domains"]}
    files["rebinding.json"] = client.encoded(proof)
    output.mkdir(parents=True)
    expanded = output / "input"
    for name, raw in sorted(files.items()):
        client.write_new(expanded / name, raw)
    for name, descriptor in payload_pins.items():
        client.write_new(expanded / "facts" / name, pinned(source / name, descriptor))
    entries = inventory(expanded)
    index = client.encoded({"schema": "pobr-ordinary-source-input/v1", "status": "draft",
                            "files": entries, "facts_root": "facts", "production_ready": False})
    client.write_new(expanded / "input-manifest.json", index)
    archive = output / "ordinary-input.zip"
    with archive.open("xb") as stream, zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED,
                                                       compresslevel=9) as z:
        for name in sorted([*entries, "input-manifest.json"]):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | 0o644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, read(expanded / name), compresslevel=9)
    handoff = {"schema": "pobr-ordinary-source-handoff/v1", "status": "draft-pending-source-acceptance",
               "production_ready": False, "archive": {"path": archive.name, **pin(read(archive))},
               "input_manifest": {"path": "input-manifest.json", **pin(index)},
               "expanded_bytes": sum(v["bytes"] for v in entries.values()) + len(index),
               "expanded_files": len(entries) + 1, "facts_root": "facts",
               "facts_manifest": pin(files["facts/manifest.json"]),
               "source_lock": pin(files["facts/source-lock.json"]),
               "original": recipe["original"], "excluded_domains": recipe["excluded_domains"]}
    client.write_new(output / "handoff.json", client.encoded(handoff))
    verify(archive, handoff)
    return handoff


def verify(archive, handoff, output=None):
    """Verify before optional extraction; caller must authenticate handoff bytes."""
    require(handoff["schema"] == "pobr-ordinary-source-handoff/v1"
            and handoff["production_ready"] is False
            and handoff["status"] == "draft-pending-source-acceptance", "Unsupported handoff")
    archive_raw = pinned(archive, {k: handoff["archive"][k] for k in ("bytes", "sha256")})
    with zipfile.ZipFile(io.BytesIO(archive_raw)) as z:
        members = z.infolist()
        paths([m.filename for m in members])
        for member in members:
            require(not member.is_dir() and stat.S_ISREG(member.external_attr >> 16)
                    and member.orig_filename == member.filename
                    and not member.flag_bits & 1 and member.compress_type == zipfile.ZIP_DEFLATED,
                    "Nonregular/encrypted/unsupported archive member")
        index_pin = {k: handoff["input_manifest"][k] for k in ("bytes", "sha256")}
        require(z.getinfo("input-manifest.json").file_size == index_pin["bytes"], "Index size mismatch")
        index_raw = z.read("input-manifest.json")
        require(pin(index_raw) == index_pin, "Input manifest pin mismatch")
        index = client.decode(index_raw)
        require(index["schema"] == "pobr-ordinary-source-input/v1" and index["production_ready"] is False,
                "Unsupported input manifest")
        expected = {**index["files"], "input-manifest.json": index_pin}
        require(len(members) == len(expected) == handoff["expanded_files"]
                and set(m.filename for m in members) == set(expected), "Archive inventory mismatch")
        require(sum(p["bytes"] for p in expected.values()) == handoff["expanded_bytes"], "Expanded byte count mismatch")
        for m in members:
            require(m.file_size == expected[m.filename]["bytes"], "Archive member size mismatch")
        documents = {}
        for name, descriptor in expected.items():
            raw = z.read(name)
            require(pin(raw) == descriptor, f"Archive fingerprint mismatch: {name}")
            # Retain only small envelope/evidence records; table bytes are streamed again for extraction.
            if not name.startswith("facts/") or name in {"facts/manifest.json", "facts/source-lock.json"}:
                documents[name] = raw
        proof = client.decode(documents["rebinding.json"])
        require(proof["schema"] == "pobr-source-rebinding-draft/v1"
                and proof["production_ready"] is False and proof["source_approval_claimed"] is False
                and proof["status"] == "draft-pending-source-acceptance", "Invalid draft boundary")
        recipe = proof["recipe"]
        original_raw = documents["provenance/original-manifest.json"]
        old_lock_raw = documents["provenance/original-source-lock.json"]
        require(pin(original_raw) == handoff["original"]["manifest"] == recipe["original"]["manifest"]
                and pin(old_lock_raw) == handoff["original"]["lock"] == recipe["original"]["lock"],
                "Original identity mismatch")
        evidence = {name: documents[f"evidence/{name}"] for name in recipe["evidence"]}
        require(all(pin(raw) == recipe["evidence"][name] for name, raw in evidence.items()), "Evidence pin mismatch")
        original = client.decode(original_raw)
        require(original["source_identity"]["source_lock_sha256"] == client.sha256(old_lock_raw)
                and original["files"]["source-lock.json"] == pin(old_lock_raw), "Original lock pin mismatch")
        new, lock, scopes, outside = metadata(original, client.decode(old_lock_raw), recipe, evidence)
        require(documents["facts/manifest.json"] == client.encoded(new)
                and documents["facts/source-lock.json"] == client.encoded(lock), "Rebinding metadata differs")
        require(pin(documents["facts/manifest.json"]) == handoff["facts_manifest"]
                and pin(documents["facts/source-lock.json"]) == handoff["source_lock"], "Candidate pin mismatch")
        payloads = {n: d for n, d in original["files"].items() if n != "source-lock.json"}
        require(proof["payload_byte_equivalence"] == payloads
                and proof["resource_scopes"] == scopes
                and proof["receipt_resources_outside_selected_groups"] == outside
                and proof["excluded_domains"] == handoff["excluded_domains"] == recipe["excluded_domains"]
                and proof["distinct_acquired_resources"] == len(original["source_identity"]["raw_resources"]),
                "Rebinding scope/equivalence proof mismatch")
        require(all(expected.get(f"facts/{n}") == d for n, d in payloads.items()), "Altered payload")
        require(set(expected) == {"input-manifest.json", "rebinding.json", "facts/manifest.json",
                "facts/source-lock.json", "provenance/original-manifest.json", "provenance/original-source-lock.json",
                *(f"facts/{n}" for n in payloads), *(f"evidence/{n}" for n in evidence)}, "Undeclared portable closure")
        if output is not None:
            output = absent(output, [archive])
            output.mkdir(parents=True)
            for name in sorted(expected):
                client.write_new(output / name, z.read(name))
    return handoff


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    create_parser = sub.add_parser("create")
    create_parser.add_argument("--source", type=Path, required=True)
    create_parser.add_argument("--recipe", type=Path, required=True)
    create_parser.add_argument("--recipe-sha256", required=True)
    create_parser.add_argument("--out", type=Path, required=True)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--archive", type=Path, required=True)
    verify_parser.add_argument("--handoff", type=Path, required=True)
    verify_parser.add_argument("--handoff-sha256", required=True)
    verify_parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "create":
            result = create(args.source, args.recipe, args.recipe_sha256, args.out)
        else:
            raw = read(args.handoff)
            require(client.sha256(raw) == client.digest(args.handoff_sha256), "Handoff pin mismatch")
            result = verify(args.archive, client.decode(raw), args.out)
        print(json.dumps(result, sort_keys=True))
    except (ValueError, KeyError, TypeError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"source-rebinding: {error}\n")


if __name__ == "__main__":
    main()
