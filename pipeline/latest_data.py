"""Resolve complete owner publications into immutable PoBR data candidates.

Mutable default branches locate small publication indexes only. Runtime inputs
always retain full Git revisions and byte pins; discovery is not source approval.
"""

import argparse
import copy
from datetime import datetime, timezone
import io
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import zipfile

import build_data
import data_bundle as bundle
import data_imports as imports
import data_snapshot
import ordinary_inputs
from data_imports import encoded, git_address, pinned, regular, require, sha, validate_pin, write


ROOT = Path(__file__).resolve().parents[1]
SCHEMA = "pobr-data-composition/v1"
INDEX_SCHEMA = "owned-data-publication-index/v2"
PUBLICATION_SCHEMA = "owned-data-publication/v2"
RESOLUTION_SCHEMA = "pobr-resolved-data-composition/v1"
MAX_METADATA = 2 * 1024 * 1024
FILES = {
    "build": "pipeline/build-data.lock.json",
    "ordinary": "pipeline/ordinary-inputs.lock.json",
    "ordinary_requirements": "pipeline/ordinary-inputs.requirements.json",
    "display": "pipeline/data-imports.lock.json",
    "requirements": "pipeline/data-requirements.json",
    "decisions": "pipeline/sources.lock.json",
    "locale_generation": "pipeline/locale-generation.lock.json",
    "channels": "pipeline/latest-channels.json",
}
OWNERS = {"ackness/repoe": "master", "ackness/poe2-zh-cn-stats": "main"}
ROLES = {
    "ackness/repoe": {
        "game": ("build", "packages", "game"),
        "crafting": ("build", "crafting"),
        "ordinary": ("ordinary", "packages", "repoe"),
        "display_source": ("display", "packages", "repoe"),
        "source_verifier": ("ordinary", "source_verifier"),
        "test_fixtures": ("build", "test_fixtures"),
    },
    "ackness/poe2-zh-cn-stats": {
        "language": ("build", "packages", "language"),
        "display_catalog": ("display", "packages", "translation"),
        "display_library": ("display", "library"),
        "reviewed_supplement": ("display", "packages", "supplement"),
        "workbench": ("display", "workbench"),
        "support_display": ("build", "support_display"),
        "locale_generation": ("locale_generation",),
    },
}


def pin(raw):
    return {"bytes": len(raw), "sha256": sha(raw)}


def digest(value):
    require(isinstance(value, str) and re.fullmatch(r"[a-f0-9]{64}", value), "Invalid composition digest")


def sequence(value):
    require(type(value) is int and value > 0, "Invalid publication sequence")


def target(value):
    require(isinstance(value, dict) and set(value) == {"game", "game_patch", "realm"}
            and value["game"] == "poe2" and value["realm"] in ("international", "china")
            and isinstance(value["game_patch"], str)
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value["game_patch"]), "Invalid composition target")


def lookup(documents, path):
    value = documents
    for key in path:
        value = value[key]
    return value


def declarations(documents, owner):
    return {role: lookup(documents, path) for role, path in ROLES[owner].items()}


def metadata_pin(value, owner):
    require(isinstance(value, dict), "Missing publication descriptor pin")
    validate_pin(value)
    require(value.get("repository", owner) == owner, "Cross-owner descriptor reference")
    require(value["bytes"] <= MAX_METADATA and value["path"].startswith("publication/")
            and value["path"].endswith(".json"), "Publication metadata exceeds its boundary")
    git_address({"repository": owner, **value}, value["path"])


def publication(document, owner):
    require(isinstance(document, dict) and document.get("schema") == PUBLICATION_SCHEMA
            and document.get("owner") == owner, "Owner publication identity differs")
    target(document["target"])
    sequence(document["sequence"])
    previous = document.get("previous")
    require((document["sequence"] == 1) == (previous is None), "Publication predecessor missing")
    if previous is not None:
        metadata_pin(previous, owner)
    require(isinstance(document.get("components"), dict)
            and set(document["components"]) == set(ROLES[owner]), "Incomplete owner publication roles")
    scope = document.get("scope")
    require(isinstance(scope, dict) and bool(scope), "Publication scope must remain explicit")
    for role, entry in document["components"].items():
        require(isinstance(entry, dict) and set(entry) == {"sequence", "input"}
                and isinstance(entry["input"], dict), f"Invalid publication role: {role}")
        sequence(entry["sequence"])
        value = entry["input"]
        require(value.get("repository") == owner, f"Cross-owner publication input: {role}")
        require(re.fullmatch(r"[a-f0-9]{40}", value.get("revision", "")), "Full role revision required")
        require(value.get("publication") not in ("pending", "local-only"), "Unpublished latest input")
    if owner == "ackness/poe2-zh-cn-stats":
        require(scope.get("complete") == "declared-dependency-closure-only"
                and all(scope.get(key) is False for key in
                        ("freshness_certified", "official_client_certified", "translation_quality_certified")),
                "Language publication scope differs")
        require(set(document.get("bindings", {})) == {"display_source"}
                and set(document.get("evidence", {})) == {"catalog_validation"},
                "Language publication bindings/evidence missing")
        binding = document["bindings"]["display_source"]
        validate_pin(binding)
        require(binding.get("repository") == "ackness/repoe", "Language display source owner differs")
        git_address(binding, binding["path"])
        validate_pin(binding["manifest"])
        evidence = document["evidence"]["catalog_validation"]
        validate_pin(evidence)
        catalog = document["components"]["display_catalog"]["input"]
        require(evidence.get("revision") == catalog["revision"]
                and evidence.get("path") == str(Path(catalog["path"]).parent / "validation.json"),
                "Language catalog validation publication differs")
        git_address({"repository": owner, **evidence}, evidence["path"])
    return document


def transition(before, after):
    require(after["sequence"] == before["sequence"] + 1, "Nonconsecutive owner publication")
    require(after["owner"] == before["owner"] and after["target"]["game"] == before["target"]["game"]
            and after["target"]["realm"] == before["target"]["realm"], "Publication channel changed")
    for role, old in before["components"].items():
        new = after["components"][role]
        changed = old["input"] != new["input"]
        require(new["sequence"] == old["sequence"] + int(changed),
                f"Publication role regressed, forked or advanced without new input: {role}")
    require(any(before.get(key) != after.get(key) for key in ("target", "components", "bindings", "evidence", "scope")),
            "Empty owner publication advance")


def documents_valid(documents):
    build, ordinary, display = (documents[name] for name in ("build", "ordinary", "display"))
    require(build.get("schema") == build_data.LOCK_SCHEMA, "Latest requires the complete v2 build lock")
    build_data.validate(build)
    require(ordinary.get("schema") == "pobr-ordinary-input-lock/v2", "Latest requires v2 ordinary inputs")
    ordinary_inputs.plan(documents["ordinary_requirements"], ordinary)
    require(set(ordinary["packages"]) == {"repoe"}, "Unexpected ordinary owners")
    require(len(documents["ordinary_requirements"]["domains"]) == 6, "Incomplete ordinary domain profile")
    imports.validate(documents["requirements"], display)
    require(documents["requirements"]["translation"].get("supplement") == {"mode": "reviewed-only"},
            "Latest must preserve reviewed-only supplements")
    wanted = build["target"]
    require(all(documents[name]["target"] == wanted for name in
                ("ordinary", "ordinary_requirements", "display", "requirements", "locale_generation")),
            "Composition targets differ")
    require(build["packages"]["game"]["publication"] == build["packages"]["language"]["publication"]
            == build["crafting"]["publication"] == ordinary["packages"]["repoe"]["publication"] == "published",
            "Latest requires published game, language, crafting and ordinary inputs")
    require("support_display" in build and "test_fixtures" in build
            and "source_verifier" in ordinary, "Required existing composition capability missing")
    channel_config(documents["channels"])
    return wanted


def channel_config(config):
    require(config.get("schema") == "pobr-latest-data-channels/v1"
            and config.get("channels") == {owner: {"branch": branch, "index": "publication/index.json"}
                                           for owner, branch in OWNERS.items()}, "Untrusted latest channels")
    observer = config["official_observer"]
    require(observer.get("repository") == "ackness/repoe" and observer.get("module") == "RePoE.client_version"
            and re.fullmatch(r"[a-f0-9]{40}", observer.get("revision", ""))
            and set(observer["files"]) == {"RePoE/__init__.py", "RePoE/source_io.py", "RePoE/client_version.py"},
            "Invalid independent official observer closure")
    for name, expected in observer["files"].items():
        git_address(observer, name)
        validate_pin(expected)
        require(expected["bytes"] <= MAX_METADATA, "Oversized observer module")
    require(type(config["max_observation_age_seconds"]) is int
            and 0 < config["max_observation_age_seconds"] <= 86400, "Invalid official observation age")


def observation(directory, config, now=None):
    channel_config(config)
    directory = build_data.safe_directory(directory)
    raw = regular(directory / "observation.json")
    record = bundle.decode(raw)
    require(record.get("schema") == "repoe-client-version-observation/v1"
            and record.get("status") == "endpoint-version-observation"
            and record["endpoint"]["host"] == "patch.pathofexile2.com"
            and record["endpoint"]["port"] == 13060 and record["handshake_hex"] == "0107",
            "Invalid official observer receipt")
    expected_code = {name: spec["sha256"] for name, spec in config["official_observer"]["files"].items()}
    require(record["producer"]["module"] == "RePoE.client_version"
            and record["producer"]["code_sha256"] == expected_code, "Observer producer differs")
    require(record["response"].get("path") == "response.bin", "Unexpected observer response path")
    pinned(regular(directory / "response.bin"), record["response"])
    version = record["version"]
    require(isinstance(version, str) and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", version)
            and record["cdn"] == f"https://patch-poe2.poecdn.com/{version}/", "Observer version/CDN differs")
    observed = datetime.fromisoformat(record["observed_at_utc"].replace("Z", "+00:00"))
    require(observed.tzinfo is not None, "Observer time lacks timezone")
    age = ((now or datetime.now(timezone.utc)) - observed).total_seconds()
    require(-300 <= age <= config["max_observation_age_seconds"], "Official observation is stale or from the future")
    return {"version": version, "observed_at_utc": record["observed_at_utc"], "receipt": pin(raw),
            "response": record["response"], "scope": record["scope"]}


def observe_official(config, output, cache, fetcher):
    """Run only the pinned upstream observer, never a PoBR protocol duplicate."""
    channel_config(config)
    output, cache = build_data.safe_directory(output), build_data.safe_directory(cache)
    spec = config["official_observer"]
    require(not output.exists(), "Fresh official observation output required")
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="pobr-official-observer-") as temporary:
        library = Path(temporary).resolve()
        for name, expected in spec["files"].items():
            write(library / name, imports.acquire(spec, name, expected, cache, False, fetcher))
        env = {key: value for key, value in os.environ.items() if not key.startswith("PYTHON")
               and key not in ("GH_TOKEN", "GITHUB_TOKEN", "DATA_SYNC_TOKEN")}
        subprocess.run([sys.executable, "-B", "-S", "-m", spec["module"], "--out", str(output)],
                       cwd=library, env=env, stdout=sys.stderr, check=True, timeout=12)
        for name, expected in spec["files"].items():
            pinned(regular(library / name), expected)
    return observation(output, config)


def read_composition(root):
    raw = regular(Path(root) / "pipeline/data-composition.json")
    composition = bundle.decode(raw)
    require(composition.get("schema") == SCHEMA, "Unknown data composition schema")
    sequence(composition["generation"])
    if composition.get("previous_id") is not None:
        digest(composition["previous_id"])
    require((composition["generation"] == 1) == (composition.get("previous_id") is None),
            "Composition predecessor missing")
    require(set(composition["files"]) == set(FILES), "Incomplete composition file closure")
    documents, payloads = {}, {}
    for key, name in FILES.items():
        spec = composition["files"][key]
        require(spec.get("path") == name, "Composition path differs")
        payloads[name] = pinned(regular(Path(root) / name), spec)
        documents[key] = bundle.decode(payloads[name])
    wanted = documents_valid(documents)
    require(regular(Path(root) / "data/CURRENT") == (wanted["game_patch"] + "\n").encode(),
            "Composition CURRENT differs")
    require(composition["target"] == wanted and composition["locale"] == documents["display"]["locale"],
            "Composition target/locale differs")
    require(composition["expected"] == {key: documents["build"][key]
                                        for key in ("snapshot_manifest", "runtime_identity")},
            "Composition full runtime expectations differ")
    require(set(composition["owner_publications"]) == set(OWNERS), "Both owner publications required")
    for owner, entry in composition["owner_publications"].items():
        sequence(entry["sequence"])
        metadata_pin(entry["descriptor"], owner)
        require(re.fullmatch(r"[a-f0-9]{40}", entry["index_revision"]), "Immutable index revision required")
    return composition, documents, payloads, raw


def resolve_head(owner, branch):
    require(OWNERS.get(owner) == branch, "Untrusted publication channel")
    output = subprocess.check_output(["git", "ls-remote", "--exit-code", "--refs",
                                     f"https://github.com/{owner}.git", "refs/heads/" + branch],
                                    timeout=30, env={**os.environ, "GIT_TERMINAL_PROMPT": "0"})
    require(len(output) <= 1024, "Oversized publication ref response")
    parts = output.decode().strip().split()
    require(len(parts) == 2 and re.fullmatch(r"[a-f0-9]{40}", parts[0])
            and parts[1] == "refs/heads/" + branch, "Publication ref resolution differs")
    return parts[0]


class Publications:
    def __init__(self, fetcher):
        self.fetcher = fetcher
        self.payloads = {}

    def get(self, owner, revision, path, expected=None):
        address = git_address({"repository": owner, "revision": revision}, path)
        if address not in self.payloads:
            raw = (self.fetcher(address, max_bytes=MAX_METADATA) if isinstance(self.fetcher, imports.GitInputs)
                   else self.fetcher(address))
            require(len(raw) <= MAX_METADATA, "Owner publication metadata is oversized")
            self.payloads[address] = raw
        raw = self.payloads[address]
        if expected is not None:
            pinned(raw, expected)
        return bundle.decode(raw)

    def descriptor(self, owner, spec):
        metadata_pin(spec, owner)
        document = publication(self.get(owner, spec["revision"], spec["path"], spec), owner)
        if owner == "ackness/poe2-zh-cn-stats":
            evidence = document["evidence"]["catalog_validation"]
            validation = self.get(owner, evidence["revision"], evidence["path"], evidence)
            require(validation.get("format") == "windows-fk-validation/v1"
                    and validation.get("manifest_sha256") == document["components"]["display_catalog"]["input"]["manifest"]["sha256"]
                    and validation.get("independent_fk_recomputed") is True
                    and validation.get("boundaries") == {"observed_patch": None, "observed_realm": None,
                         "official_client_semantics_verified": False, "production_ready": False},
                    "Language catalog validation evidence differs")
        return document

    def latest(self, owner, current, wanted, heads=resolve_head):
        revision = heads(owner, OWNERS[owner])
        index = self.get(owner, revision, "publication/index.json")
        require(index.get("schema") == INDEX_SCHEMA
                and index.get("channel") == {key: wanted[key] for key in ("game", "realm")},
                "Publication index channel/schema differs")
        sequence(index["sequence"])
        latest = self.descriptor(owner, index["head"])
        require(latest["sequence"] == index["sequence"], "Index/publication sequence differs")
        entry = {"index_revision": revision, "sequence": index["sequence"], "descriptor": index["head"]}
        if current is not None:
            require(index["sequence"] >= current["sequence"], "Owner publication rollback")
            previous = self.descriptor(owner, current["descriptor"])
            require(previous["sequence"] == current["sequence"], "Adopted owner sequence differs")
            cursor, pointer, children = latest, index["head"], []
            while cursor["sequence"] > current["sequence"]:
                require(len(children) < 128, "Publication history needs an explicit checkpoint")
                children.append(cursor)
                pointer = cursor["previous"]
                parent = self.descriptor(owner, pointer)
                require(parent["sequence"] == cursor["sequence"] - 1, "Nonconsecutive publication predecessor")
                cursor = parent
            require(pointer == current["descriptor"], "Owner publication history fork")
            for child in reversed(children):
                transition(cursor, child)
                cursor = child
            # Index source-code commits may advance without a new data release.
            if entry["descriptor"] == current["descriptor"]:
                entry = current
        return entry, latest


def bind_publications(documents, publications):
    for owner in OWNERS:
        require(publications[owner]["target"] == documents["build"]["target"], "Owner target mismatch")
        actual = {role: spec["input"] for role, spec in publications[owner]["components"].items()}
        require(actual == declarations(documents, owner), "Owner publication and consumer declarations differ")
    require(publications["ackness/poe2-zh-cn-stats"]["bindings"]["display_source"]
            == documents["display"]["packages"]["repoe"], "Language display source binding differs")


def composition_for(documents, payloads, publications, previous=None):
    wanted = documents_valid(documents)
    return {"schema": SCHEMA, "generation": 1 if previous is None else previous["generation"] + 1,
            "previous_id": None if previous is None else sha(encoded(previous)), "target": wanted,
            "locale": documents["display"]["locale"], "owner_publications": publications,
            "files": {key: {"path": name, **pin(payloads[name])} for key, name in FILES.items()},
            "expected": {key: documents["build"][key] for key in ("snapshot_manifest", "runtime_identity")}}


def bootstrap(root, fetcher, heads=resolve_head):
    root = Path(root)
    payloads = {name: regular(root / name) for name in FILES.values()}
    documents = {key: bundle.decode(payloads[name]) for key, name in FILES.items()}
    wanted = documents_valid(documents)
    require(regular(root / "data/CURRENT") == (wanted["game_patch"] + "\n").encode(), "Bootstrap CURRENT differs")
    library, entries, descriptors = Publications(fetcher), {}, {}
    revisions = {owner: heads(owner, branch) for owner, branch in OWNERS.items()}
    for owner in OWNERS:
        entries[owner], descriptors[owner] = library.latest(owner, None, wanted,
                                                         lambda name, branch: revisions[name])
    bind_publications(documents, descriptors)
    return composition_for(documents, payloads, entries)


def verify_composition(root, fetcher):
    """Verify recorded immutable publications; do not discover a newer channel."""
    composition, documents, _, raw = read_composition(root)
    library, descriptors = Publications(fetcher), {}
    for owner, adopted in composition["owner_publications"].items():
        entry, descriptors[owner] = library.latest(owner, adopted, composition["target"],
                                                    lambda name, branch: composition["owner_publications"][name]["index_revision"])
        require(entry == adopted, "Recorded publication index differs from composition")
    bind_publications(documents, descriptors)
    return {"schema": "pobr-immutable-composition-verification/v1", "status": "verified-immutable-declarations",
            "composition_id": sha(raw), "owner_publications": composition["owner_publications"],
            "freshness": "not-checked", "adoption": "unchanged"}


def discover(root, fetcher, heads=resolve_head, *, official=None):
    current, documents, payloads, raw = read_composition(root)
    library, entries, descriptors, scope_preserved = Publications(fetcher), {}, {}, True
    revisions = {owner: heads(owner, branch) for owner, branch in OWNERS.items()}
    for owner in OWNERS:
        entries[owner], descriptors[owner] = library.latest(owner, current["owner_publications"][owner],
                                                         current["target"], lambda name, branch: revisions[name])
        before = library.descriptor(owner, current["owner_publications"][owner]["descriptor"])
        scope_preserved = scope_preserved and before["scope"] == descriptors[owner]["scope"]
        require({role: value["input"] for role, value in before["components"].items()}
                == declarations(documents, owner), "Adopted owner publication differs from its locks")
    changed = entries != current["owner_publications"]
    same_target = all(d["target"] == current["target"] for d in descriptors.values())
    fresh = official is not None
    waiting = fresh and any(d["target"]["game_patch"] != official["version"] for d in descriptors.values())
    status = "waiting-for-owner-publication" if waiting else ("update-available" if changed else "no-new-publication")
    return {"schema": "pobr-latest-data-status/v1", "status": status,
            "adopted_composition": sha(raw), "target": current["target"], "published_frontier": entries,
            "same_target": same_target, "scope_preserved": scope_preserved,
            "metadata_auto_eligible": same_target and scope_preserved,
            "auto_eligible": same_target and scope_preserved and fresh and not waiting,
            "official_observation": official, "freshness": "observed" if fresh else "not-checked",
            "adoption": "not-adopted",
            "scope": "Latest indexed complete publications only; upstream generation and game semantics are separate"}, library, descriptors


def archive_metadata(spec, cache, fetcher):
    """Read only the authenticated small manifest, not the expanded source tables."""
    raw = imports.acquire(spec, spec["path"], spec, cache, False, fetcher)
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        require(len(archive.namelist()) == len(set(archive.namelist())), "Duplicate publication members")
        require(archive.getinfo("manifest.json").file_size <= MAX_METADATA, "Oversized publication manifest")
        manifest_raw = pinned(archive.read("manifest.json"), spec["manifest"])
        require(len(manifest_raw) <= MAX_METADATA, "Oversized publication manifest")
        return bundle.decode(manifest_raw), raw


def build_expectations(build, cache, fetcher):
    """Derive full expectations from the owner's declared snapshot/shared hashes.

    Preparation still verifies every actual runtime byte before this candidate can
    be adopted. A new language cannot be paired with an old full snapshot manifest.
    """
    game = build["packages"]["game"]
    manifest, archive_raw = archive_metadata(game, cache, fetcher)
    wanted, version = build["target"], build["target"]["game_patch"]
    require(manifest.get("schema") == build_data.PART_SCHEMA and manifest.get("kind") == "game"
            and manifest.get("target") == wanted, "Game publication target/schema differs")
    snapshot_path = f"data/{version}/manifest.json"
    require(f"data/{version}/overlay/trade_crafting_sources.json" not in manifest["files"],
            "Latest game part contains projected crafting fallback")
    with zipfile.ZipFile(io.BytesIO(archive_raw)) as archive:
        spec = manifest["files"][snapshot_path]
        validate_pin(spec)
        require(spec["bytes"] <= MAX_METADATA, "Oversized full snapshot manifest")
        require(archive.getinfo(snapshot_path).file_size == spec["bytes"], "Full snapshot size differs")
        snapshot_raw = pinned(archive.read(snapshot_path), spec)
    snapshot = bundle.decode(snapshot_raw)
    require(snapshot.get("schema_version") == 3 and snapshot.get("poe_version") == version,
            "Latest full snapshot target/schema differs")
    bundle.validate_declarations(snapshot, snapshot["files"])
    for name, value in snapshot["files"].items():
        bundle.relative_path(name)
        digest(value)
    language = build["packages"]["language"]
    if language.get("format") == "locale-git-files":
        value = imports.acquire(language, language["path"], language["manifest"], cache, False, fetcher)
        require(len(value) <= MAX_METADATA, "Oversized language manifest")
        locale = bundle.decode(value)
        require(locale.get("schema") == "poe2-retained-locale-publication/v1"
                and locale.get("target") == wanted, "Latest language target/schema differs")
        for name, spec in locale["files"].items():
            bundle.relative_path(name)
            validate_pin(spec)
        language_files = {name: spec["sha256"] for name, spec in locale["files"].items()
                          if name.startswith("i18n/")}
    else:
        locale, _ = archive_metadata(language, cache, fetcher)
        require(locale.get("schema") == build_data.PART_SCHEMA and locale.get("kind") == "language"
                and locale.get("target") == wanted, "Latest language target/schema differs")
        prefix = f"data/{version}/"
        require(all(name.startswith(prefix + "i18n/") for name in locale["files"]), "Cross-owner language files")
        for name, spec in locale["files"].items():
            bundle.relative_path(name)
            validate_pin(spec)
        language_files = {name.removeprefix(prefix): spec["sha256"] for name, spec in locale["files"].items()}
    require(language_files == {name: value for name, value in snapshot["files"].items() if name.startswith("i18n/")},
            "consumer-data-divergence: latest language does not match full game snapshot")
    hashes = dict(snapshot["files"])
    shared = {}
    for name, spec in manifest["files"].items():
        bundle.relative_path(name)
        validate_pin(spec)
        if name.startswith("data/overlay-common/"):
            shared[name.removeprefix("data/")] = spec["sha256"]
        elif name != snapshot_path:
            require(build_data.member_kind(name, version) == "game", "Cross-owner game member")
            relative = name.removeprefix(f"data/{version}/")
            require(snapshot["files"].get(relative) == spec["sha256"], "Game part/snapshot hash differs")
    for name, spec in build["shared_files"].items():
        key = "overlay-common/" + name
        require(key not in shared, "External/local shared ownership intersection")
        shared[key] = spec["sha256"]
    hashes.update(shared)
    return {"snapshot_manifest": pin(snapshot_raw),
            "runtime_identity": sha(bundle.encoded(["pobr-data-pack-v1", 3, version, hashes]))}, shared


def ordinary_expectations(documents, cache, fetcher):
    lock, request = documents["ordinary"], documents["ordinary_requirements"]
    manifest, _ = archive_metadata(lock["packages"]["repoe"], cache, fetcher)
    require(manifest.get("schema") == "repoe-data-publication/v1"
            and manifest.get("target") == lock["target"], "Ordinary publication target/schema differs")
    for unit, spec in lock["inputs"].items():
        if unit in lock["policies"]:
            continue
        for name in spec["files"]:
            key = f"{unit}/{name}"
            require(key in manifest["files"], "New ordinary publication lacks a declared member")
            validate_pin(manifest["files"][key])
            spec["files"][name] = manifest["files"][key]
    for job in request["domains"]:
        for flag, checksum in ordinary_inputs.PROFILES[job["domain"]][0].items():
            if checksum:
                spec = lock["inputs"][job["options"][flag]["input"]]
                name = "manifest.json" if flag == "--supplement" else spec["path"]
                job["options"][checksum] = spec["files"][name]["sha256"]


def propose(root, output, fetcher, heads=resolve_head, *, official=None, cache=None):
    root, output = Path(root), build_data.safe_directory(output)
    require(not output.exists() and not output.is_relative_to(root / "data"), "Fresh candidate outside data required")
    status, library, descriptors = discover(root, fetcher, heads, official=official)
    require(status["status"] != "waiting-for-owner-publication", "Official version lacks complete owner publications")
    require(status["same_target"], "New patch requires compatible requirements and source decisions")
    current, documents, payloads, old_raw = read_composition(root)
    require(sha(old_raw) == status["adopted_composition"], "Adopted composition changed during discovery")
    if status["status"] == "no-new-publication":
        return status
    proposed = copy.deepcopy(documents)
    for owner in OWNERS:
        for role, path in ROLES[owner].items():
            parent = proposed
            for key in path[:-1]:
                parent = parent[key]
            parent[path[-1]] = descriptors[owner]["components"][role]["input"]
    cache = build_data.safe_directory(cache or root / ".cache/data-import-downloads")
    cache.mkdir(parents=True, exist_ok=True)
    if any(proposed["build"][key] != documents["build"][key] for key in ("packages", "crafting")):
        expected, shared = build_expectations(proposed["build"], cache, fetcher)
        _, previous_shared = build_expectations(documents["build"], cache, fetcher)
        require(shared == previous_shared, "Shared data changed; explicit migration review required")
        proposed["build"].update(expected)
    if proposed["ordinary"]["packages"] != documents["ordinary"]["packages"]:
        ordinary_expectations(proposed, cache, fetcher)
    documents_valid(proposed)
    bind_publications(proposed, descriptors)
    # The candidate Full CI authenticates all input bytes, runs existing policy
    # bindings and checks actual full/compact consumers before any adoption.
    proposed_raw = {name: encoded(proposed[key]) for key, name in FILES.items()}
    for key, name in FILES.items():
        if proposed[key] == documents[key]:
            proposed_raw[name] = payloads[name]
    composition = composition_for(proposed, proposed_raw, status["published_frontier"], current)
    composition["previous_id"] = sha(old_raw)
    proposed_raw["pipeline/data-composition.json"] = encoded(composition)
    proposed_raw["data/CURRENT"] = regular(root / "data/CURRENT")
    inventory = {name: pin(value) for name, value in proposed_raw.items()}
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".latest-candidate-", dir=output.parent) as temporary:
        stage = Path(temporary) / "candidate"
        for name, value in proposed_raw.items():
            write(stage / name, value)
        evidence = {}
        for address, value in library.payloads.items():
            name = "evidence/" + sha(encoded(address))
            write(stage / name, value)
            evidence[name] = {"address": list(address), **pin(value)}
        result = {"schema": RESOLUTION_SCHEMA, "previous_id": sha(old_raw),
                  "composition_id": sha(proposed_raw["pipeline/data-composition.json"]),
                  "files": inventory, "evidence": evidence,
                  "auto_eligible": status["auto_eligible"],
                  "official_observation": official,
                  "status": "resolved-not-validated", "adoption": "not-adopted"}
        read_composition(stage)
        require(regular(root / "pipeline/data-composition.json") == old_raw
                and all(regular(root / name) == value for name, value in payloads.items()),
                "Adopted composition changed during resolution")
        write(stage / "resolution.json", encoded(result))
        stage.rename(output)
    return {**status, "candidate": str(output), "candidate_composition": result["composition_id"]}


def verify_candidate(candidate):
    candidate = build_data.safe_directory(candidate)
    receipt = bundle.decode(regular(candidate / "resolution.json"))
    require(receipt.get("schema") == RESOLUTION_SCHEMA and receipt.get("status") == "resolved-not-validated"
            and receipt.get("adoption") == "not-adopted", "Invalid latest candidate receipt")
    allowed = {*FILES.values(), "pipeline/data-composition.json", "data/CURRENT"}
    require(set(receipt["files"]) == allowed, "Candidate metadata closure differs")
    for name, expected in receipt["files"].items():
        pinned(regular(candidate / name), expected)
    comp, documents, _, raw = read_composition(candidate)
    require(sha(raw) == receipt["composition_id"] and comp["previous_id"] == receipt["previous_id"],
            "Candidate composition/predecessor differs")
    require(regular(candidate / "data/CURRENT") == (comp["target"]["game_patch"] + "\n").encode(),
            "Candidate CURRENT differs")
    payloads = {}
    for name, spec in receipt["evidence"].items():
        address = spec["address"]
        require(isinstance(address, list) and len(address) == 3
                and name == "evidence/" + sha(encoded(address)), "Candidate evidence address differs")
        payloads[tuple(address)] = pinned(regular(candidate / name), spec)
    library = Publications(lambda address: payloads[address])
    publications = {owner: library.descriptor(owner, entry["descriptor"])
                    for owner, entry in comp["owner_publications"].items()}
    require(all(entry["sequence"] == publications[owner]["sequence"]
                for owner, entry in comp["owner_publications"].items()), "Composition owner sequence differs")
    bind_publications(documents, publications)
    actual = set()
    for path in candidate.rglob("*"):
        build_data.safe_directory(path)
        require(path.is_file() or path.is_dir(), "Nonregular candidate member")
        if path.is_file():
            actual.add(path.relative_to(candidate).as_posix())
    require(actual == allowed | set(receipt["evidence"]) | {"resolution.json"},
            "Unlisted candidate metadata/evidence")
    return receipt


def recheck(candidate, root, fetcher, heads=resolve_head, *, official):
    """Recheck freshness and the adopted baseline; CI success is checked by the host."""
    receipt = verify_candidate(candidate)
    current, _, _, current_raw = read_composition(root)
    proposed, _, _, _ = read_composition(candidate)
    require(sha(current_raw) == receipt["previous_id"], "Expected composition changed")
    require(proposed["generation"] == current["generation"] + 1, "Candidate generation differs")
    status, _, _ = discover(root, fetcher, heads, official=official)
    require(status["status"] == "update-available" and status["auto_eligible"],
            "Candidate is not eligible for automatic adoption")
    require(all(all(proposed["owner_publications"][owner][key] == status["published_frontier"][owner][key]
                        for key in ("sequence", "descriptor")) for owner in OWNERS),
            "Newer owner publication supersedes the candidate")
    # Reconstruct the authorized metadata rather than trusting a PR's receipt.
    # This also refuses policy/source/scope changes and modified expectations.
    with tempfile.TemporaryDirectory(prefix="pobr-latest-recheck-") as temporary:
        rebuilt = Path(temporary).resolve() / "candidate"
        propose(root, rebuilt, fetcher, heads=lambda owner, branch: proposed["owner_publications"][owner]["index_revision"],
                official=official)
        regenerated = verify_candidate(rebuilt)
        require(regenerated["files"] == receipt["files"], "Candidate differs from resolved owner declarations")
    return {"status": "fresh-candidate-not-adopted", "composition_id": receipt["composition_id"],
            "previous_id": receipt["previous_id"], "official_observation": official}


def apply_candidate(candidate, root, expected_composition, *, replace=os.replace):
    """Stage only frozen small metadata, with cooperative CAS and rollback.

    This prepares a normal Git change. It neither selects installed data nor
    proves runtime validation or authorizes a default-branch merge.
    """
    candidate, root = (build_data.safe_directory(path) for path in (candidate, root))
    require(not candidate.is_relative_to(root / "data"), "Candidate overlaps installed data")
    digest(expected_composition)
    receipt = verify_candidate(candidate)
    require(receipt["previous_id"] == expected_composition, "Candidate predecessor differs")
    payloads = {name: pinned(regular(candidate / name), spec) for name, spec in receipt["files"].items()}
    with data_snapshot.workspace_lock(root):
        current, _, originals, old_raw = read_composition(root)
        require(sha(old_raw) == expected_composition, "Expected composition changed")
        proposed = bundle.decode(payloads["pipeline/data-composition.json"])
        require(proposed["generation"] == current["generation"] + 1, "Candidate generation differs")
        originals.update({"pipeline/data-composition.json": old_raw, "data/CURRENT": regular(root / "data/CURRENT")})
        backup = Path(tempfile.mkdtemp(prefix="latest-metadata-", dir=root / ".cache"))
        changed = []
        try:
            for name, raw in originals.items():
                write(backup / "original" / name, raw)
            # Composition last: readers reject a partially changed closure.
            names = sorted(set(payloads) - {"pipeline/data-composition.json"}) + ["pipeline/data-composition.json"]
            for name in names:
                require(regular(root / name) == originals[name], "Metadata changed while staging")
                if payloads[name] == originals[name]:
                    continue
                write(backup / "new" / name, payloads[name])
                changed.append(name)
                replace(backup / "new" / name, root / name)
            require(all(regular(root / name) == raw for name, raw in payloads.items()),
                    "Metadata changed during staging")
            read_composition(root)
        except BaseException:
            for name in reversed(changed):
                installed = regular(root / name)
                require(installed in (payloads[name], originals[name]),
                        f"Rollback conflict; original metadata retained at {backup}")
                if installed != originals[name]:
                    os.replace(backup / "original" / name, root / name)
            import shutil
            shutil.rmtree(backup)
            raise
        else:
            import shutil
            shutil.rmtree(backup)
    return {"status": "metadata-staged-not-adopted", "composition_id": receipt["composition_id"],
            "previous_id": expected_composition, "runtime_selection": "unchanged"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("bootstrap", "check", "resolve", "verify", "apply", "recheck", "verify-composition"))
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--expected-composition")
    parser.add_argument("--observation", type=Path, help="Existing fresh pinned observer receipt directory")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    args = parser.parse_args()
    if args.action == "verify":
        result = verify_candidate(args.root)
    elif args.action == "apply":
        require(args.candidate is not None and args.expected_composition is not None,
                "Apply needs --candidate and --expected-composition")
        result = apply_candidate(args.candidate, args.root, args.expected_composition)
    else:
        with tempfile.TemporaryDirectory(prefix="pobr-latest-git-") as temporary:
            fetcher = imports.GitInputs(Path(temporary), timeout=30)
            if args.action == "verify-composition":
                result = verify_composition(args.root, fetcher)
            elif args.action == "bootstrap":
                require(args.out is not None, "Bootstrap needs an explicit fresh output")
                result = bootstrap(args.root, fetcher)
                write(args.out, encoded(result))
            else:
                config = read_composition(args.root)[1]["channels"]
                if args.observation is not None:
                    official = observation(args.observation, config)
                else:
                    official = observe_official(config, Path(temporary).resolve() / "observation", args.cache, fetcher)
                if args.action == "recheck":
                    require(args.candidate is not None, "Recheck needs --candidate")
                    result = recheck(args.candidate, args.root, fetcher, official=official)
                elif args.action == "resolve":
                    require(args.out is not None, "Resolve needs a fresh candidate output")
                    result = propose(args.root, args.out, fetcher, official=official, cache=args.cache)
                else:
                    result = discover(args.root, fetcher, official=official)[0]
    print(encoded(result).decode(), end="")
    if result.get("status") == "waiting-for-owner-publication":
        raise SystemExit(2)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(f"latest data: {error}") from error
