"""Offline replay of one pinned native-runtime profile; no acquisition or publication."""

import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile

from data_bundle import decode, encoded, relative_path, sha256, walk_error


SCHEMA = "pobr-native-candidate-recipe/v1"
SOURCE_STATUS = "unreviewed-acquisition-draft"
INPUT_PROFILE = "sorceress-spark-level1-q0-v1"
OUTPUT_PROFILE = "spark-single-hit-cast-cost-v1"
DOMAINS = sorted("""base_items cost_types granted_effect_levels granted_effect_stat_sets
granted_effects native_acquisition native_coverage native_input_lock native_normalization
native_reference_index native_rule_requirements native_runtime native_skill_projection
native_tree_match passive_tree passive_tree_meta skill_gems""".split())


def require(condition, message):
    if not condition:
        raise ValueError(message)


def keys(value, expected):
    require(isinstance(value, dict) and set(value) == set(expected),
            f"Expected exactly these keys: {', '.join(expected)}")


def hex_pin(value, size=64):
    require(isinstance(value, str) and re.fullmatch(f"[0-9a-f]{{{size}}}", value),
            "Expected a full lowercase immutable digest")


def checked_path(path, directory=False):
    path = Path(os.path.abspath(path))
    for component in [*reversed(path.parents), path]:
        require(not component.is_symlink(), "Native replay forbids symlink components")
    mode = path.stat().st_mode
    require(stat.S_ISDIR(mode) if directory else stat.S_ISREG(mode),
            "Native replay requires regular files/directories")
    return path


def fingerprint(path):
    with checked_path(path).open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def pinned_bytes(path, expected):
    raw = checked_path(path).read_bytes()
    require(sha256(raw) == expected, "External byte pin mismatch")
    return raw


def inventory(root):
    """All files, including non-JSON files; reject links, special files and empty extras."""
    checked_path(root, directory=True)
    files, directories = {}, set()
    for directory, dirs, names in os.walk(root, followlinks=False, onerror=walk_error):
        for name in dirs:
            path = checked_path(Path(directory) / name, directory=True)
            directories.add(path.relative_to(root).as_posix())
        for name in names:
            path = Path(directory) / name
            files[path.relative_to(root).as_posix()] = fingerprint(path)
    return files, directories


def exact_inventory(root, expected):
    for name, pin in expected.items():
        relative_path(name)
        hex_pin(pin)
    files, directories = inventory(root)
    parents = {p.as_posix() for name in expected for p in Path(name).parents if p != Path(".")}
    require(files == expected and directories == parents, "Artifact inventory/hash mismatch")
    return files


def recipe_document(raw):
    recipe = decode(raw)
    keys(recipe, ["schema", "target", "facts", "descriptions", "official_tree", "compiler"])
    require(recipe["schema"] == SCHEMA, "Unsupported native candidate recipe version")
    target = recipe["target"]
    keys(target, ["game", "realm", "game_patch", "input_profile", "output_profile", "source_status", "partial"])
    require(target == {"game": "poe2", "realm": "INTERNATIONAL", "game_patch": target["game_patch"],
                       "input_profile": INPUT_PROFILE, "output_profile": OUTPUT_PROFILE,
                       "source_status": SOURCE_STATUS, "partial": True}
            and target["partial"] is True, "Unsupported native target/profile/status")
    require(isinstance(target["game_patch"], str)
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", target["game_patch"]), "Invalid target patch")
    for name in ["facts", "descriptions"]:
        keys(recipe[name], ["manifest_sha256", "source_lock_sha256"])
        for pin in recipe[name].values():
            hex_pin(pin)
    keys(recipe["official_tree"], ["sha256", "game_patch"])
    hex_pin(recipe["official_tree"]["sha256"])
    require(recipe["official_tree"]["game_patch"] is None, "Official tree patch must remain unknown")
    keys(recipe["compiler"], ["binary_sha256", "revision_declaration", "implementation_sha256"])
    for key, value in recipe["compiler"].items():
        hex_pin(value, 40 if key == "revision_declaration" else 64)
    return recipe


def package(root, pins, target, kind):
    raw = pinned_bytes(root / "manifest.json", pins["manifest_sha256"])
    manifest = decode(raw)
    require(isinstance(manifest, dict), "Invalid producer manifest")
    identity = manifest.get("source_identity")
    require(isinstance(identity, dict), "Missing producer source identity")
    require(manifest.get("format") == {"facts": "repoe-native-facts/v4",
                                      "descriptions": "repoe-native-descriptions/v1"}[kind]
            and manifest.get("status") == "partial-candidate"
            and manifest.get("production_ready") is False, "Unsupported producer format/status")
    require(all(identity.get(key) == value for key, value in {
        "game": target["game"], "realm": "international", "game_patch": target["game_patch"],
        "source_lock_sha256": pins["source_lock_sha256"],
    }.items()) and isinstance(identity.get("acquisition"), dict)
            and identity["acquisition"].get("status") == target["source_status"],
            "Producer source identity/status differs from recipe")
    files = manifest.get("files")
    require(isinstance(files, dict) and files and "manifest.json" not in files,
            "Invalid producer inventory")
    expected = {"manifest.json": pins["manifest_sha256"]}
    for name, descriptor in files.items():
        keys(descriptor, ["bytes", "sha256"])
        relative = relative_path(name)
        require(type(descriptor["bytes"]) is int and descriptor["bytes"] >= 0,
                "Invalid producer payload size")
        require(checked_path(root / relative).stat().st_size == descriptor["bytes"],
                "Producer payload size mismatch")
        expected[name] = descriptor["sha256"]
    require(expected.get("source-lock.json") == pins["source_lock_sha256"],
            "Producer source lock differs from recipe")
    exact_inventory(root, expected)
    # Physical decoding, FK joins, source/decoder contracts and domain semantics
    # remain exclusively the established Rust adapter's responsibility.
    return identity["acquisition"]


def verify_runtime(runtime, recipe, acquisition):
    """Check the sealed envelope and its cross-links, not a second semantic decoder."""
    manifest = decode(checked_path(runtime / "manifest.json").read_bytes())
    keys(manifest, ["schema_version", "poe_version", "languages", "domains", "files"])
    target = recipe["target"]
    require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 4
            and manifest["poe_version"] == target["game_patch"] and manifest["languages"] == []
            and manifest["domains"] == {"base": DOMAINS, "overlay": [], "generated": []},
            "Runtime manifest target/domains mismatch")
    files = manifest["files"]
    keys(files, [f"base/{name}.json" for name in DOMAINS])
    hashes = exact_inventory(runtime, {**files,
        "manifest.json": fingerprint(runtime / "manifest.json"),
        "assembly-receipt.json": fingerprint(runtime / "assembly-receipt.json")})
    documents = {name: decode((runtime / name).read_bytes()) for name in hashes}
    lock = documents["base/native_input_lock.json"]
    compiler = {k: v for k, v in recipe["compiler"].items() if k != "binary_sha256"}
    require(isinstance(lock, dict) and lock.get("format") == "pobr-native-runtime-input-lock/v1"
            and all(lock.get(key) == target[key] for key in ["game", "realm", "game_patch"])
            and lock.get("facts") == recipe["facts"] and lock.get("descriptions") == recipe["descriptions"]
            and lock.get("compiler") == compiler and lock.get("profile") == target["input_profile"]
            and isinstance(lock.get("official_tree"), dict)
            and all(lock["official_tree"].get(k) == v for k, v in recipe["official_tree"].items())
            and lock["official_tree"].get("fields") == ["coordinates"], "Runtime input lock mismatch")
    contract = documents["base/native_runtime.json"]
    require(isinstance(contract, dict) and type(contract.get("schema_version")) is int
            and contract["schema_version"] == 1
            and all(contract.get(key) == target[key] for key in
                    ["game", "realm", "game_patch", "input_profile", "output_profile"])
            and contract.get("compiler_revision") == compiler["revision_declaration"]
            and contract.get("descriptions_manifest_sha256") == recipe["descriptions"]["manifest_sha256"]
            and contract.get("allowed_source_manifests") == [recipe["facts"]["manifest_sha256"]],
            "Runtime contract target mismatch")
    hex_pin(contract.get("assembly_key"))
    for field, name in [("source_lock_sha256", "native_input_lock"), ("coverage_sha256", "native_coverage"),
                        ("reference_index_sha256", "native_reference_index")]:
        require(contract.get(field) == files[f"base/{name}.json"], "Runtime contract sidecar mismatch")
    require(contract.get("acquisition") == {"status": SOURCE_STATUS,
            "evidence_sha256": files["base/native_acquisition.json"]}
            and documents["base/native_acquisition.json"] == acquisition, "Runtime acquisition mismatch")
    coverage = documents["base/native_coverage.json"]
    require(isinstance(coverage, dict) and coverage.get("format") == "pobr-native-runtime-coverage/v1"
            and coverage.get("production_ready") is False and coverage.get("calculation_admitted") is False
            and coverage.get("source_status") == SOURCE_STATUS, "Runtime coverage/status mismatch")
    receipt = documents["assembly-receipt.json"]
    keys(receipt, ["format", "production_ready", "calculation_admitted", "assembly_key", "compiler",
                   "manifest_sha256", "input_lock_sha256", "scope"])
    require(receipt["format"] == "pobr-native-runtime-assembly-receipt/v1"
            and receipt["production_ready"] is False and receipt["calculation_admitted"] is False
            and receipt["compiler"] == compiler and receipt["assembly_key"] == contract["assembly_key"]
            and receipt["manifest_sha256"] == hashes["manifest.json"]
            and receipt["input_lock_sha256"] == files["base/native_input_lock.json"],
            "Invalid assembly receipt")
    return hashes, receipt


def replay(root, recipe_path, expected_recipe_sha256, facts, source_lock, descriptions, official_tree, compiler, out):
    hex_pin(expected_recipe_sha256)
    raw = pinned_bytes(recipe_path, expected_recipe_sha256)
    recipe = recipe_document(raw)
    facts, descriptions = checked_path(facts, True), checked_path(descriptions, True)
    source_lock, official_tree, compiler = map(checked_path, [source_lock, official_tree, compiler])
    out = Path(os.path.abspath(out))
    checked_path(out.parent, True)  # Explicit existing parent; never create an input directory.
    require(not out.exists() and not out.is_symlink(), "Output must be new and absent")
    protected = [root / "data", facts, descriptions, source_lock.parent, official_tree.parent, compiler.parent]
    # Compare filesystem identities too: macOS can accept DATA as an alias of
    # data, while Path.is_relative_to compares case-sensitive path spellings.
    require(not any(parent.samefile(p) for p in protected if p.exists() for parent in out.parents),
            "Output must be outside production data and input/tool directories")
    code = [root / "pipeline" / name for name in ["data_snapshot.py", "native_candidate.py", "data_bundle.py"]]
    implementation = {p.relative_to(root).as_posix(): fingerprint(p) for p in code}

    def verify_inputs():
        require(pinned_bytes(recipe_path, expected_recipe_sha256) == raw, "Recipe changed during replay")
        acquisition = {name: package(path, recipe[name], recipe["target"], name)
                       for name, path in [("facts", facts), ("descriptions", descriptions)]}
        require(fingerprint(source_lock) == recipe["facts"]["source_lock_sha256"], "External source lock mismatch")
        require(fingerprint(official_tree) == recipe["official_tree"]["sha256"], "Official tree byte pin mismatch")
        require(fingerprint(compiler) == recipe["compiler"]["binary_sha256"], "Compiler binary byte pin mismatch")
        require({p.relative_to(root).as_posix(): fingerprint(p) for p in code} == implementation,
                "Orchestrator implementation changed during replay")
        return acquisition

    acquisition = verify_inputs()  # Every external byte pin passes before executable invocation.
    out.mkdir()  # Reserve once; retain incomplete output on failure, never overwrite it.
    with (out / "recipe.json").open("xb") as file:
        file.write(raw)
    subprocess.run([str(compiler), "--native-runtime", str(facts),
        "--expected-manifest-sha256", recipe["facts"]["manifest_sha256"], "--source-lock", str(source_lock),
        "--descriptions", str(descriptions), "--descriptions-manifest-sha256", recipe["descriptions"]["manifest_sha256"],
        "--official-tree", str(official_tree), "--official-tree-sha256", recipe["official_tree"]["sha256"],
        "--compiler-revision", recipe["compiler"]["revision_declaration"], "--out", str(out / "runtime")],
        cwd=root, env={}, check=True)
    runtime_hashes, assembly = verify_runtime(out / "runtime", recipe, acquisition)
    require(verify_inputs() == acquisition, "Producer acquisition changed during replay")
    files = {"recipe.json": expected_recipe_sha256,
             **{f"runtime/{name}": pin for name, pin in runtime_hashes.items()}}
    exact_inventory(out, files)
    receipt = {"schema": "pobr-native-candidate-orchestration/v1", "status": "complete",
               "recipe_sha256": expected_recipe_sha256, "target": recipe["target"],
               "production_ready": False, "calculation_admitted": False,
               "compiler": {**recipe["compiler"], "clean_build_attested": False},
               "orchestrator": implementation, "assembly_key": assembly["assembly_key"], "files": files}
    # Publish the completion marker atomically, exclusively and last, outside the
    # sealed runtime namespace. No receipt exists after any earlier failure.
    with tempfile.NamedTemporaryFile(dir=out.parent, prefix=".native-receipt-") as file:
        file.write(encoded(receipt) + b"\n")
        file.flush()
        os.fsync(file.fileno())
        os.link(file.name, out / "orchestration-receipt.json")
    return receipt
