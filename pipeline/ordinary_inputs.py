"""Acquire declared inputs and run the existing ordinary adapter sequentially.

This produces draft domain candidates, never a complete/adopted runtime release.
"""
import argparse
import copy
from pathlib import Path
import re
import stat
import subprocess
import zipfile

from data_bundle import decode
from data_imports import (GitInputs, acquire, encoded, git_address, pinned, regular,
                          require, sha, unpack, validate_pin, write)
from source_rebinding import absent, inventory, paths, pin, portable, verify


COMMON = {"--codec-source": "--codec-source-sha256", "--codec-policy": "--codec-policy-sha256"}
SUPPLEMENT = {"--supplement": "--supplement-manifest-sha256"}
# Deliberately bounded command profiles, not arbitrary adapter/shell arguments.
PROFILES = {
    "stat-descriptions": ({**COMMON, **SUPPLEMENT, "--item-descriptions": None},
                          {"overlay/stat_descriptions.json"}),
    "passive-tree": ({**COMMON, **SUPPLEMENT, "--tree-reference": None, "--tree-descriptions": None},
                     {"base/passive_tree.json", "base/passive_tree_meta.json"}),
    "base-items": ({**COMMON, **SUPPLEMENT}, {"base/base_items.json", "overlay/base_item_overrides.json"}),
    "minions": ({**COMMON, **SUPPLEMENT}, {"overlay/minions.json"}),
    "spectres": ({**COMMON, **SUPPLEMENT}, {"overlay/spectres.json"}),
    "skill-overrides": ({**COMMON, "--branch-codec": "--branch-codec-sha256"},
                        {"overlay/skill_overrides.json"}),
}
ARCHIVE_INPUT, HANDOFF_INPUT = "ordinary-input.zip", "handoff.json"
POLICY_TARGETS = {"passive-tree": ["base_policy", "target", "facts_manifest_sha256"],
                  **{name: ["target", "facts_manifest_sha256"]
                     for name in ("minions", "spectres", "skill-overrides")}}


def load(path):
    return decode(regular(path))


def plan(request, lock):
    require(request.get("schema") == "pobr-ordinary-requirements/v1"
            and lock.get("schema") in ("pobr-ordinary-input-lock/v1", "pobr-ordinary-input-lock/v2"), "Unsupported schema")
    fields = {"schema", "target", "publication", "package", "inputs"} if lock["schema"].endswith("/v1") else {
        "schema", "target", "packages", "policies", "inputs"}
    require(set(request) == {"schema", "target", "domains", "retained"}
            and set(lock) == fields, "Unknown request/lock fields")
    target = request["target"]
    require(target == lock["target"] and set(target) == {"game", "game_patch", "realm"}
            and target["game"] == "poe2" and target["realm"] in ("international", "china")
            and isinstance(target["game_patch"], str)
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", target["game_patch"]), "Target identity mismatch")
    require(isinstance(request["retained"], str) and request["retained"].strip(), "Missing retained scope")
    require(isinstance(request["domains"], list) and request["domains"], "No declared domains")
    require(isinstance(lock["inputs"], dict) and lock["inputs"], "No declared inputs")
    files = {}
    for key, spec in lock["inputs"].items():
        portable(key)
        require(set(spec) in ({"path", "files"}, {"path", "files", "policy_rebinding"})
                and isinstance(spec["files"], dict) and spec["files"], "Invalid input unit")
        require(spec["path"] == "." or spec["path"] in spec["files"], "Missing unit entry point")
        for name, identity in spec["files"].items():
            portable(name)
            require(set(identity) == {"bytes", "sha256"}, "Invalid file pin")
            validate_pin(identity)
            files[f"{key}/{name}"] = identity
    paths([*files, "manifest.json"])
    seen, referenced = set(), {ARCHIVE_INPUT, HANDOFF_INPUT}
    require(referenced <= lock["inputs"].keys(), "Missing source archive/handoff")
    for key in referenced:
        require(lock["inputs"][key]["path"] == key, "Invalid source archive/handoff entry point")
    for job in request["domains"]:
        require(isinstance(job, dict) and set(job) == {"domain", "options", "selected_facts"}, "Unknown job fields")
        selected = job["selected_facts"]
        require(isinstance(selected, list) and len(selected) > 1 and "source-lock.json" in selected,
                "Missing selected fact paths")
        paths(selected)
        domain, options = job["domain"], job["options"]
        require(isinstance(domain, str) and domain in PROFILES and domain not in seen, "Unknown or duplicate domain")
        seen.add(domain)
        profile = PROFILES[domain][0]
        require(isinstance(options, dict) and set(options) == set(profile) | {v for v in profile.values() if v},
                f"Missing/unsupported {domain} arguments")
        for flag, digest_flag in profile.items():
            ref = options[flag]
            require(isinstance(ref, dict) and set(ref) == {"input"} and isinstance(ref["input"], str)
                    and ref["input"] in lock["inputs"], "Invalid input reference")
            referenced.add(ref["input"])
            spec = lock["inputs"][ref["input"]]
            if "policy_rebinding" in spec:
                rebind = spec["policy_rebinding"]
                require(flag == "--codec-policy" and isinstance(rebind, dict)
                        and set(rebind) == {"original", "pointer"}
                        and rebind["original"] in spec["files"] and rebind["original"] != spec["path"]
                        and rebind["pointer"] == POLICY_TARGETS.get(domain), "Invalid policy rebinding")
            require((spec["path"] == ".") == (flag == "--supplement"), "File/directory argument mismatch")
            if digest_flag:
                name = "manifest.json" if flag == "--supplement" else spec["path"]
                require(name in spec["files"] and options[digest_flag] == spec["files"][name]["sha256"],
                        "Argument pin differs from declared input")
            if flag in {"--item-descriptions", "--tree-descriptions"}:
                require(spec["path"] == "descriptions.json" and "manifest.json" in spec["files"],
                        "Description sibling manifest missing")
    require(referenced == lock["inputs"].keys(), "Undeclared/unreferenced input unit")
    if lock["schema"].endswith("/v2"):
        from ordinary_policy import SOURCES
        require(set(lock["policies"]) == {key for key in lock["inputs"] if key.endswith("/codec-policy")},
                "Every policy must be locally materialized")
        for key, declaration in lock["policies"].items():
            require(key in SOURCES and set(declaration) == {"source"}, "Unknown policy materializer declaration")
            validate_pin(declaration["source"])
        require(isinstance(lock["packages"], dict) and lock["packages"], "Missing owner packages")
        for name, package in lock["packages"].items():
            portable(name)
            require(package["repository"] in ("ackness/repoe", "ackness/poe2-zh-cn-stats"), "External input repository violates ownership")
            git_address(package, package["path"])
            validate_pin(package)
            validate_pin(package["manifest"])
            require(package["publication"] in ("pending", "local-only", "published")
                    and type(package["expanded_bytes"]) is int and package["expanded_bytes"] > 0,
                    "Invalid external package declaration")
    else:
        require(lock["publication"] in ("pending", "published"), "Invalid publication state")
        if lock["publication"] == "published":
            package = lock["package"]
            require(isinstance(package, dict), "Missing published package")
            require(package.get("repository") != "ackness/pobr", "PoBR cannot store external ordinary data")
            git_address(package, package["path"])
            validate_pin(package)
            validate_pin(package["manifest"])
            require(type(package["expanded_bytes"]) is int and package["expanded_bytes"] > 0, "Invalid expansion size")
        else:
            require(lock["package"] is None, "Pending package must not claim a Git identity")
    return files


def identity(document, target):
    require(isinstance(document, dict) and all(document.get(k) == v for k, v in target.items()),
            "Source game/patch/realm mismatch")


def preflight(request, lock, inputs, source):
    handoff = load(inputs / HANDOFF_INPUT / HANDOFF_INPUT)
    verify(inputs / ARCHIVE_INPUT / ARCHIVE_INPUT, handoff, source)
    manifest = load(source / "facts/manifest.json")
    require(manifest.get("format") == "repoe-native-facts/v4"
            and manifest.get("status") == "partial-candidate" and manifest.get("production_ready") is False,
            "Unexpected facts format/status")
    identity(manifest["source_identity"], request["target"])
    resolved = {key: inputs / key / spec["path"] for key, spec in lock["inputs"].items()}
    for job in request["domains"]:
        require(set(job["selected_facts"]) <= manifest["files"].keys(), "Selected fact missing from manifest")
        options = job["options"]
        policy = load(resolved[options["--codec-policy"]["input"]])
        pointer = POLICY_TARGETS.get(job["domain"])
        if pointer:
            parent = policy
            for key in pointer[:-1]:
                parent = parent[key]
            require(parent[pointer[-1]] == handoff["facts_manifest"]["sha256"]
                    and parent["game_patch"] == request["target"]["game_patch"], "Policy target manifest mismatch")
        spec = lock["inputs"][options["--codec-policy"]["input"]]
        if "policy_rebinding" in spec:
            original = load(inputs / options["--codec-policy"]["input"] / spec["policy_rebinding"]["original"])
            restored = copy.deepcopy(policy)
            before, after = original, restored
            for key in pointer[:-1]:
                before, after = before[key], after[key]
            require(before[pointer[-1]] == handoff["original"]["manifest"]["sha256"], "Policy original source mismatch")
            after[pointer[-1]] = before[pointer[-1]]
            require(restored == original, "Policy rebinding changed non-target content")
        for flag in ("--supplement", "--item-descriptions", "--tree-descriptions"):
            if flag not in options:
                continue
            key = options[flag]["input"]
            spec, root = lock["inputs"][key], inputs / key
            meta = load(root / "manifest.json")
            identity(meta["source_identity"], request["target"])
            expected_format = ("repoe-item-inheritance/v1" if job["domain"] == "base-items"
                               else "repoe-native-tables/v1") if flag == "--supplement" else "repoe-native-descriptions/v1"
            require(meta.get("format") == expected_format and meta.get("production_ready") is False,
                    "Unsupported supplemental manifest")
            # A CSD consumer reads only descriptions+manifest; its other provenance
            # files are not implicitly imported. Selected table units are complete.
            if flag == "--supplement":
                require(set(spec["files"]) == {"manifest.json", *meta["files"]}, "Incomplete supplement inventory")
            for name, expected in spec["files"].items():
                if name != "manifest.json":
                    require(meta["files"].get(name) == expected, "Supplement manifest/file pin mismatch")
            if flag != "--supplement":
                require(policy.get("descriptions_sha256") == spec["files"]["descriptions.json"]["sha256"]
                        and policy.get("descriptions_manifest_sha256") == spec["files"]["manifest.json"]["sha256"],
                        "Policy description pin mismatch")
    return handoff, manifest, resolved


def pack(request, lock, local, output):
    files = plan(request, lock)
    local, output = Path(local).absolute(), absent(output, [local])
    # Authenticate all files before publishing the archive; no expanded copy.
    for name, expected in files.items():
        pinned(regular(local / name), expected)
    manifest = encoded({"schema": "pobr-ordinary-declared-inputs/v1", "target": request["target"], "files": files})
    output.mkdir(parents=True)
    archive = output / "ordinary-producer-inputs.zip"
    with archive.open("xb") as handle, zipfile.ZipFile(handle, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for name in sorted([*files, "manifest.json"]):
            raw = manifest if name == "manifest.json" else pinned(regular(local / name), files[name])
            entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | 0o644) << 16
            z.writestr(entry, raw, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    result = {"path": archive.name, **pin(regular(archive)), "manifest": pin(manifest),
              "expanded_bytes": sum(p["bytes"] for p in files.values()) + len(manifest)}
    write(output / "package.json", encoded(result))
    return result


def check_output(job, output, facts, manifest, handoff, lock):
    receipt_raw = regular(output / "producer-receipt.json")
    receipt = decode(receipt_raw)
    target = {"game": lock["target"]["game"], "realm": lock["target"]["realm"],
              "version": lock["target"]["game_patch"]}
    require(receipt.get("format") == "pobr-ordinary-facts-receipt/v1"
            and receipt.get("status") == "domain-candidate" and receipt.get("complete_runtime") is False
            and receipt.get("adoption") == "not-adopted" and receipt.get("target") == target,
            "Producer receipt target/status mismatch")
    actual_input = receipt["input"]
    require(actual_input["manifest_sha256"] == handoff["facts_manifest"]["sha256"]
            and actual_input["source_identity"] == manifest["source_identity"]
            and Path(actual_input["root"]) == facts.resolve(), "Producer receipt input mismatch")
    selected = actual_input["selected_payloads"]
    require(set(selected) == set(job["selected_facts"]) and selected.get("source-lock.json") == handoff["source_lock"]
            and all(manifest["files"].get(name) == value for name, value in selected.items()),
            "Producer selected input mismatch")
    files = receipt["files"]
    paths(files)
    for value in files.values():
        validate_pin(value)
    require(inventory(output) == {**files, "producer-receipt.json": pin(receipt_raw)}, "Producer output inventory mismatch")
    require(files.get("source-manifest.json") == handoff["facts_manifest"]
            and files.get("source-lock.json") == handoff["source_lock"], "Producer source sidecars mismatch")
    marker = load(output / "_domain_candidate.json")
    require(marker.get("schema") == "pobr-domain-candidate/v1"
            and marker.get("domain") == "ordinary-" + job["domain"]
            and marker.get("version") == target["version"]
            and marker.get("source_id") == manifest["source_identity"]["source"]
            and marker.get("source_manifest_sha256") == handoff["facts_manifest"]["sha256"], "Producer domain mismatch")
    business = {name for name in files if name.startswith(("base/", "overlay/"))}
    require(business == PROFILES[job["domain"]][1], "Missing/unexpected domain output")
    # Check exact byte-preserved source/policy archives independently of exit status.
    archived = {"--codec-policy": "codec-policy.json", "--codec-source": {
        "stat-descriptions": "legacy-stat-descriptions.json", "passive-tree": "legacy-passive-tree.json",
        "base-items": "legacy-base-item-overrides.json", "minions": "legacy-actors.json",
        "spectres": "legacy-actors.json", "skill-overrides": "legacy-skill-overrides.json"}[job["domain"]]}
    if job["domain"] == "stat-descriptions":
        archived["--codec-policy"] = "description-policy.json"
    if "--branch-codec" in job["options"]:
        archived["--branch-codec"] = "branch-codec.json"
    for flag, name in archived.items():
        spec = lock["inputs"][job["options"][flag]["input"]]
        require(files.get(name) == spec["files"][spec["path"]], "Producer archived argument mismatch")
    return {"receipt": pin(receipt_raw), "adapter": receipt["adapter"],
            "outputs": {name: files[name] for name in sorted(business)}}


def run(request, lock, output, local=None, adapter=None, cache=None, offline=False, fetcher=None, local_packages=None):
    files = plan(request, lock)
    local = Path(local).absolute() if local else None
    output = absent(output, [p for p in (local, Path(adapter) if adapter else None) if p is not None])
    require(local is not None or lock["schema"].endswith("/v2") or lock["publication"] == "published", "Pending Git publication")
    executable = Path(adapter).absolute() if adapter else None
    binary = pin(regular(executable)) if executable else None
    output.mkdir(parents=True)
    inputs = output / "inputs"
    external = {}
    if local:
        for name, expected in files.items():
            write(inputs / name, pinned(regular(local / name), expected))
    else:
        cache = Path(cache).absolute() if cache else output / "cache"
        require(all(not p.is_symlink() for p in (cache, *cache.parents)), "Symlink cache")
        cache.mkdir(parents=True, exist_ok=True)
        fetcher = fetcher or GitInputs(output / "git")
        if lock["schema"].endswith("/v2"):
            from ordinary_policy import TREE_BINDINGS, materialize
            local_packages = local_packages or {}
            require(set(local_packages) <= lock["packages"].keys(), "Unknown local owner package")
            expected_external = {name: expected for name, expected in files.items()
                                 if not any(name.startswith(key + "/") for key in lock["policies"])}
            for key, spec in lock["packages"].items():
                if key in local_packages:
                    raw = pinned(regular(local_packages[key]), spec)
                else:
                    require(spec["publication"] == "published", "Pending external ordinary package")
                    raw = acquire(spec, spec["path"], spec, cache, offline, fetcher)
                root = output / "packages" / key
                manifest = unpack(raw, spec, root)
                require(manifest.get("schema") == "repoe-data-publication/v1"
                        and manifest.get("target") == request["target"], "External ordinary package target/schema mismatch")
                for name, expected in manifest["files"].items():
                    require(name not in external, "Duplicate owner input member")
                    require(name in expected_external or name == TREE_BINDINGS
                            or name == "publication-provenance/ordinary-manifest.json", "Unrecognized external ordinary member")
                    if name in expected_external:
                        require(expected == expected_external[name], "External ordinary input pin differs")
                    external[name] = expected
                    write(inputs / name, pinned(regular(root / name), expected))
            require(set(expected_external) <= set(external) and ("passive-tree/codec-policy" not in lock["policies"] or TREE_BINDINGS in external), "Missing external ordinary members")
            materialize(lock, inputs)
        else:
            spec = lock["package"]
            raw = acquire(spec, spec["path"], spec, cache, offline, fetcher)
            manifest = unpack(raw, spec, inputs)
            require(manifest == {"schema": "pobr-ordinary-declared-inputs/v1", "target": request["target"], "files": files},
                    "Declared input archive mismatch")
    handoff, manifest, resolved = preflight(request, lock, inputs, output / "source")
    evidence = {"schema": "pobr-ordinary-production/v1", "target": request["target"],
                "requirements": pin(encoded(request)), "lock": pin(encoded(lock)),
                "facts_manifest": handoff["facts_manifest"], "source_lock": handoff["source_lock"],
                "production_ready": False, "adoption": "not-adopted", "retained": request["retained"]}
    if executable is None:
        evidence.update(status="verified-inputs-only", domains=[j["domain"] for j in request["domains"]])
        write(output / "verified-inputs.json", encoded(evidence))
        return evidence
    results = {}
    for job in request["domains"]:
        pinned(regular(executable), binary)
        args = [str(executable), "--ordinary-facts", str(output / "source/facts"),
                "--facts-manifest-sha256", handoff["facts_manifest"]["sha256"],
                "--version", request["target"]["game_patch"], "--ordinary-domain", job["domain"],
                "--out", str(output / job["domain"])]
        for flag, value in job["options"].items():
            args += [flag, str(resolved[value["input"]]) if isinstance(value, dict) else value]
        log = output / "logs" / job["domain"]
        log.mkdir(parents=True)
        with (log / "stdout.txt").open("xb") as stdout, (log / "stderr.txt").open("xb") as stderr:
            process = subprocess.run(args, stdout=stdout, stderr=stderr, check=False)
        write(log / "execution.json", encoded({"command": args, "exit_code": process.returncode, "binary": binary}))
        require(process.returncode == 0, f"Producer failed: {job['domain']}; see {log}")
        results[job["domain"]] = check_output(job, output / job["domain"], output / "source/facts", manifest, handoff, lock)
        pinned(regular(executable), binary)
    # Inputs must still be the bytes authenticated before the first producer.
    for name, expected in {**files, **external}.items():
        pinned(regular(inputs / name), expected)
    for name, expected in manifest["files"].items():
        pinned(regular(output / "source/facts" / portable(name)), expected)
    pinned(regular(output / "source/facts/manifest.json"), handoff["facts_manifest"])
    for job in request["domains"]:
        require(results[job["domain"]] == check_output(job, output / job["domain"], output / "source/facts",
                                                     manifest, handoff, lock), "Completed domain changed")
    evidence.update(status="completed-declared-domains", binary=binary, domains=results)
    write(output / "completed.json", encoded(evidence))
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["plan", "pack", "verify", "execute"])
    parser.add_argument("--requirements", default="pipeline/ordinary-inputs.requirements.json")
    parser.add_argument("--lock", default="pipeline/ordinary-inputs.lock.json")
    parser.add_argument("--local", type=Path, help="Explicit portable input layout; never follows author-machine paths")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--adapter", type=Path, help="Already built ordinary adapter executable")
    parser.add_argument("--cache", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--package", action="append", default=[], help="Explicit local owner input: key=archive.zip")
    args = parser.parse_args()
    try:
        request, lock = load(args.requirements), load(args.lock)
        if args.action == "plan":
            result = {"target": request["target"], "files": plan(request, lock)}
        else:
            require(args.out is not None, "--out required")
            if args.action == "pack":
                require(args.local is not None, "--local required for pack")
                result = pack(request, lock, args.local, args.out)
            else:
                require(args.action != "execute" or args.adapter is not None, "--adapter required")
                result = run(request, lock, args.out, args.local,
                             args.adapter if args.action == "execute" else None, args.cache, args.offline,
                             local_packages=dict(item.split("=", 1) for item in args.package))
        print(encoded(result).decode(), end="")
    except (ValueError, KeyError, TypeError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"ordinary-inputs: {error}\n")


if __name__ == "__main__":
    main()
