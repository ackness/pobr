"""Resolve complete owner publications into an immutable PoBR data-lock candidate.

`pipeline/data.lock.json` is PoBR's only data declaration. Mutable owner default
branches locate small publication indexes only; the lock keeps each consumed
artifact's full Git revision, byte pin and the owner tag that keeps it alive.
Discovery is not source approval.
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

from pipeline.runtime import build_data
from pipeline.runtime import data_bundle as bundle
from pipeline.imports import data_imports as imports
from pipeline.runtime import data_snapshot
from pipeline.imports.data_imports import encoded, git_address, pinned, regular, require, sha, validate_pin, write


ROOT = Path(__file__).resolve().parents[2]
SCHEMA = build_data.DATA_LOCK_SCHEMA
LOCK = "pipeline/data.lock.json"
CHANNELS = "pipeline/latest/latest-channels.json"
CANDIDATE_FILES = (LOCK, "data/CURRENT")
INDEX_SCHEMA = "owned-data-publication-index/v2"
PUBLICATION_SCHEMA = "owned-data-publication/v2"
RESOLUTION_SCHEMA = "pobr-resolved-data-lock/v1"
MAX_METADATA = 2 * 1024 * 1024
OWNERS = {"ackness/repoe": "master", "ackness/poe2-zh-cn-stats": "main"}
# Only the roles PoBR consumes; an owner publication may declare more.
ROLES = {
    "ackness/repoe": {
        "game": ("packages", "game"),
        "test_fixtures": ("test_fixtures",),
    },
    "ackness/poe2-zh-cn-stats": {
        "language": ("packages", "language"),
        "support_display": ("support_display",),
    },
}
# Roles PoBR validates: the consumed ones plus the language catalog binding.
# Owners may add or retire any other role without a PoBR change.
CHECKED = {
    "ackness/repoe": {*ROLES["ackness/repoe"], "display_source"},
    "ackness/poe2-zh-cn-stats": {*ROLES["ackness/poe2-zh-cn-stats"], "display_catalog"},
}


def pin(raw):
    return {"bytes": len(raw), "sha256": sha(raw)}


def digest(value):
    require(isinstance(value, str) and re.fullmatch(r"[a-f0-9]{64}", value), "Invalid data lock digest")


def sequence(value):
    require(type(value) is int and value > 0, "Invalid publication sequence")


def target(value):
    require(isinstance(value, dict) and set(value) == {"game", "game_patch", "realm"}
            and value["game"] == "poe2" and value["realm"] in ("international", "china")
            and isinstance(value["game_patch"], str)
            and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value["game_patch"]), "Invalid data lock target")


def lookup(documents, path):
    value = documents
    for key in path:
        value = value[key]
    return value


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
            and CHECKED[owner] <= set(document["components"]), "Incomplete owner publication roles")
    scope = document.get("scope")
    require(isinstance(scope, dict) and bool(scope), "Publication scope must remain explicit")
    for role in sorted(CHECKED[owner]):
        entry = document["components"][role]
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
        require("display_catalog" in document["components"], "Language catalog publication missing")
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
    for role in sorted(CHECKED[before["owner"]]):
        old, new = before["components"][role], after["components"].get(role)
        require(new is not None, f"Owner publication dropped a role: {role}")
        changed = old["input"] != new["input"]
        require(new["sequence"] == old["sequence"] + int(changed),
                f"Publication role regressed, forked or advanced without new input: {role}")
    require(any(before.get(key) != after.get(key) for key in ("target", "components", "bindings", "evidence", "scope")),
            "Empty owner publication advance")


def untagged(spec):
    return {key: value for key, value in spec.items() if key != "tag"}


def tagged(spec, tag):
    """Keep the tag next to the revision it names."""
    out = {}
    for key, value in spec.items():
        out[key] = value
        if key == "revision":
            out["tag"] = tag
    return out


def declarations(lock, owner):
    return {role: untagged(lookup(lock, path)) for role, path in ROLES[owner].items()}


def lock_valid(lock):
    require(lock.get("schema") == SCHEMA, "Latest requires the data lock")
    build_data.validate(lock)
    require(all(lookup(lock, path).get("publication") == "published"
                for roles in ROLES.values() for path in roles.values()),
            "Latest requires published owner inputs")
    return lock["target"]


def read_lock(root):
    raw = regular(Path(root) / LOCK)
    lock = bundle.decode(raw)
    wanted = lock_valid(lock)
    require(regular(Path(root) / "data/CURRENT") == (wanted["game_patch"] + "\n").encode(),
            "Data lock CURRENT differs")
    return lock, raw


def channels(root):
    config = bundle.decode(regular(Path(root) / CHANNELS))
    channel_config(config)
    return config


def resolve_tags(owner):
    """Map each owner `handoff/*` tag to the commit it names (peeled)."""
    require(owner in OWNERS, "Untrusted owner repository")
    output = subprocess.check_output(["git", "ls-remote", "--tags", f"https://github.com/{owner}.git",
                                      "refs/tags/handoff/*"],
                                     timeout=30, env={**os.environ, "GIT_TERMINAL_PROMPT": "0"})
    require(len(output) <= MAX_METADATA, "Oversized owner tag listing")
    tags = {}
    for line in output.decode().splitlines():
        commit, ref = line.split("\t")
        require(re.fullmatch(r"[a-f0-9]{40}", commit) and ref.startswith("refs/tags/"), "Invalid owner tag listing")
        name = ref.removeprefix("refs/tags/")
        if name.endswith("^{}"):
            tags[name[:-3]] = commit
        else:
            tags.setdefault(name, commit)
    return tags


def tag_for(revision, tags):
    names = sorted(name for name, commit in tags.items() if commit == revision and name.startswith("handoff/"))
    require(len(names) == 1, f"Owner revision {revision} needs exactly one handoff/* tag")
    return names[0]


def verify_tags(lock, tags=resolve_tags):
    for owner, roles in ROLES.items():
        found = tags(owner)
        for path in roles.values():
            spec = lookup(lock, path)
            require(found.get(spec["tag"]) == spec["revision"],
                    f"Owner tag {spec['tag']} does not name its pinned revision")


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


def bind_publications(lock, publications):
    for owner in OWNERS:
        require(publications[owner]["target"] == lock["target"], "Owner target mismatch")
        actual = {role: publications[owner]["components"][role]["input"] for role in ROLES[owner]}
        require(actual == declarations(lock, owner), "Owner publication and data lock differ")
    source = publications["ackness/repoe"]["components"].get("display_source")
    require(source is not None
            and publications["ackness/poe2-zh-cn-stats"]["bindings"]["display_source"] == source["input"],
            "Language display source binding differs")


def bootstrap(root, fetcher, heads=resolve_head):
    """Record the current owner frontiers for a lock whose inputs already match them."""
    lock, _ = read_lock(root)
    library, entries, descriptors = Publications(fetcher), {}, {}
    revisions = {owner: heads(owner, branch) for owner, branch in OWNERS.items()}
    for owner in OWNERS:
        entries[owner], descriptors[owner] = library.latest(owner, None, lock["target"],
                                                         lambda name, branch: revisions[name])
    bind_publications(lock, descriptors)
    return {**lock, "owner_publications": entries}


def verify_composition(root, fetcher, tags=resolve_tags):
    """Verify recorded immutable publications and owner tags; do not discover a newer channel."""
    lock, raw = read_lock(root)
    library, descriptors = Publications(fetcher), {}
    for owner, adopted in lock["owner_publications"].items():
        entry, descriptors[owner] = library.latest(owner, adopted, lock["target"],
                                                    lambda name, branch: lock["owner_publications"][name]["index_revision"])
        require(entry == adopted, "Recorded publication index differs from the data lock")
    bind_publications(lock, descriptors)
    verify_tags(lock, tags)
    return {"schema": "pobr-immutable-data-lock-verification/v1", "status": "verified-immutable-declarations",
            "lock_id": sha(raw), "owner_publications": lock["owner_publications"], "tags": "verified",
            "freshness": "not-checked", "adoption": "unchanged"}


def discover(root, fetcher, heads=resolve_head, *, official=None):
    current, raw = read_lock(root)
    library, entries, descriptors, scope_preserved = Publications(fetcher), {}, {}, True
    revisions = {owner: heads(owner, branch) for owner, branch in OWNERS.items()}
    for owner in OWNERS:
        entries[owner], descriptors[owner] = library.latest(owner, current["owner_publications"][owner],
                                                         current["target"], lambda name, branch: revisions[name])
        before = library.descriptor(owner, current["owner_publications"][owner]["descriptor"])
        scope_preserved = scope_preserved and before["scope"] == descriptors[owner]["scope"]
        require({role: before["components"][role]["input"] for role in ROLES[owner]}
                == declarations(current, owner), "Adopted owner publication differs from the data lock")
    changed = entries != current["owner_publications"]
    same_target = all(d["target"] == current["target"] for d in descriptors.values())
    fresh = official is not None
    waiting = fresh and any(d["target"]["game_patch"] != official["version"] for d in descriptors.values())
    status = "waiting-for-owner-publication" if waiting else ("update-available" if changed else "no-new-publication")
    return {"schema": "pobr-latest-data-status/v1", "status": status,
            "adopted_lock": sha(raw), "target": current["target"], "published_frontier": entries,
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


def build_expectations(lock, cache, fetcher):
    """Derive full expectations from the owner's declared snapshot/shared hashes.

    Preparation still verifies every actual runtime byte before this candidate can
    be adopted. A new language cannot be paired with an old full snapshot manifest.
    """
    game = lock["packages"]["game"]
    manifest, archive_raw = archive_metadata(game, cache, fetcher)
    wanted, version = lock["target"], lock["target"]["game_patch"]
    require(manifest.get("schema") == build_data.PART_SCHEMA and manifest.get("kind") == "game"
            and manifest.get("target") == wanted, "Game publication target/schema differs")
    snapshot_path = f"data/{version}/manifest.json"
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
    language = lock["packages"]["language"]
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
    for name, spec in lock["shared_files"].items():
        key = "overlay-common/" + name
        require(key not in shared, "External/local shared ownership intersection")
        shared[key] = spec["sha256"]
    hashes.update(shared)
    return {"snapshot_manifest": pin(snapshot_raw),
            "runtime_identity": sha(bundle.encoded(["pobr-data-pack-v1", 3, version, hashes]))}, shared


def compact_release(lock, root, cache, fetcher):
    """Prepare the candidate's complete data and return its compact release identity.

    This authenticates every runtime byte the candidate names, so a proposal
    carries an exact release_id instead of leaving it for a later manual change.
    """
    from pipeline.runtime import runtime_pack
    version, locale = lock["target"]["game_patch"], lock["locale"]
    with tempfile.TemporaryDirectory(prefix="pobr-latest-release-") as temporary:
        temporary = Path(temporary).resolve()
        write(temporary / "data.lock.json", encoded(lock))
        build_data.prepare(temporary / "data.lock.json", temporary / "prepared", cache,
                           shared_root=Path(root) / "data/overlay-common", fetcher=fetcher)
        files, runtime_identity = bundle.snapshot_inputs(temporary / "prepared", version)
        require(runtime_identity == lock["runtime_identity"], "Prepared candidate runtime identity differs")
        display = files[f"data/{version}/i18n/{locale}/display_texts.json"]
        release, _ = runtime_pack.assemble(files, version, lock["target"], locale, display)
        return release["release_id"]


def propose(root, output, fetcher, heads=resolve_head, *, official=None, cache=None, tags=resolve_tags):
    root, output = Path(root), build_data.safe_directory(output)
    require(not output.exists() and not output.is_relative_to(root / "data"), "Fresh candidate outside data required")
    status, library, descriptors = discover(root, fetcher, heads, official=official)
    require(status["status"] != "waiting-for-owner-publication", "Official version lacks complete owner publications")
    require(status["same_target"], "New patch requires compatible requirements and source decisions")
    current, old_raw = read_lock(root)
    require(sha(old_raw) == status["adopted_lock"], "Adopted data lock changed during discovery")
    if status["status"] == "no-new-publication":
        return status
    proposed, found = copy.deepcopy(current), {}
    for owner, roles in ROLES.items():
        for role, path in roles.items():
            value = descriptors[owner]["components"][role]["input"]
            parent = lookup(proposed, path[:-1])
            if untagged(parent[path[-1]]) == value:
                continue
            if owner not in found:
                found[owner] = tags(owner)
            parent[path[-1]] = tagged(value, tag_for(value["revision"], found[owner]))
    cache = build_data.safe_directory(cache or root / ".cache/data-import-downloads")
    cache.mkdir(parents=True, exist_ok=True)

    def runtime_inputs(lock):
        return [untagged(lock["packages"]["game"]), untagged(lock["packages"]["language"])]

    if runtime_inputs(proposed) != runtime_inputs(current):
        expected, shared = build_expectations(proposed, cache, fetcher)
        _, previous_shared = build_expectations(current, cache, fetcher)
        require(shared == previous_shared, "Shared data changed; explicit migration review required")
        proposed.update(expected)
        proposed["release_id"] = compact_release(proposed, root, cache, fetcher)
    proposed.update(generation=current["generation"] + 1, previous_id=sha(old_raw),
                    owner_publications=status["published_frontier"])
    lock_valid(proposed)
    bind_publications(proposed, descriptors)
    # The candidate Full CI authenticates all input bytes and checks the actual
    # full and compact consumers before any adoption.
    proposed_raw = {LOCK: encoded(proposed), "data/CURRENT": regular(root / "data/CURRENT")}
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
                  "lock_id": sha(proposed_raw[LOCK]), "files": inventory, "evidence": evidence,
                  "auto_eligible": status["auto_eligible"], "official_observation": official,
                  "status": "resolved-not-validated", "adoption": "not-adopted"}
        read_lock(stage)
        require(regular(root / LOCK) == old_raw, "Adopted data lock changed during resolution")
        write(stage / "resolution.json", encoded(result))
        stage.rename(output)
    return {**status, "candidate": str(output), "candidate_lock": result["lock_id"]}


def verify_candidate(candidate):
    candidate = build_data.safe_directory(candidate)
    receipt = bundle.decode(regular(candidate / "resolution.json"))
    require(receipt.get("schema") == RESOLUTION_SCHEMA and receipt.get("status") == "resolved-not-validated"
            and receipt.get("adoption") == "not-adopted", "Invalid latest candidate receipt")
    allowed = set(CANDIDATE_FILES)
    require(set(receipt["files"]) == allowed, "Candidate metadata closure differs")
    for name, expected in receipt["files"].items():
        pinned(regular(candidate / name), expected)
    lock, raw = read_lock(candidate)
    require(sha(raw) == receipt["lock_id"] and lock["previous_id"] == receipt["previous_id"],
            "Candidate lock/predecessor differs")
    payloads = {}
    for name, spec in receipt["evidence"].items():
        address = spec["address"]
        require(isinstance(address, list) and len(address) == 3
                and name == "evidence/" + sha(encoded(address)), "Candidate evidence address differs")
        payloads[tuple(address)] = pinned(regular(candidate / name), spec)
    library = Publications(lambda address: payloads[address])
    publications = {owner: library.descriptor(owner, entry["descriptor"])
                    for owner, entry in lock["owner_publications"].items()}
    require(all(entry["sequence"] == publications[owner]["sequence"]
                for owner, entry in lock["owner_publications"].items()), "Data lock owner sequence differs")
    bind_publications(lock, publications)
    actual = set()
    for path in candidate.rglob("*"):
        build_data.safe_directory(path)
        require(path.is_file() or path.is_dir(), "Nonregular candidate member")
        if path.is_file():
            actual.add(path.relative_to(candidate).as_posix())
    require(actual == allowed | set(receipt["evidence"]) | {"resolution.json"},
            "Unlisted candidate metadata/evidence")
    return receipt


def recheck(candidate, root, fetcher, heads=resolve_head, *, official, tags=resolve_tags):
    """Recheck freshness and the adopted baseline; CI success is checked by the host."""
    receipt = verify_candidate(candidate)
    current, current_raw = read_lock(root)
    proposed, _ = read_lock(candidate)
    require(sha(current_raw) == receipt["previous_id"], "Expected data lock changed")
    require(proposed["generation"] == current["generation"] + 1, "Candidate generation differs")
    status, _, _ = discover(root, fetcher, heads, official=official)
    require(status["status"] == "update-available" and status["auto_eligible"],
            "Candidate is not eligible for automatic adoption")
    require(all(all(proposed["owner_publications"][owner][key] == status["published_frontier"][owner][key]
                        for key in ("sequence", "descriptor")) for owner in OWNERS),
            "Newer owner publication supersedes the candidate")
    # Reconstruct the authorized metadata rather than trusting a PR's receipt.
    with tempfile.TemporaryDirectory(prefix="pobr-latest-recheck-") as temporary:
        rebuilt = Path(temporary).resolve() / "candidate"
        propose(root, rebuilt, fetcher, heads=lambda owner, branch: proposed["owner_publications"][owner]["index_revision"],
                official=official, tags=tags)
        regenerated = verify_candidate(rebuilt)
        require(regenerated["files"] == receipt["files"], "Candidate differs from resolved owner declarations")
    return {"status": "fresh-candidate-not-adopted", "lock_id": receipt["lock_id"],
            "previous_id": receipt["previous_id"], "official_observation": official}


def apply_candidate(candidate, root, expected_lock, *, replace=os.replace):
    """Stage only the frozen data lock, with cooperative CAS and rollback.

    This prepares a normal Git change. It neither selects installed data nor
    proves runtime validation or authorizes a default-branch merge.
    """
    candidate, root = (build_data.safe_directory(path) for path in (candidate, root))
    require(not candidate.is_relative_to(root / "data"), "Candidate overlaps installed data")
    digest(expected_lock)
    receipt = verify_candidate(candidate)
    require(receipt["previous_id"] == expected_lock, "Candidate predecessor differs")
    payloads = {name: pinned(regular(candidate / name), spec) for name, spec in receipt["files"].items()}
    with data_snapshot.workspace_lock(root):
        current, old_raw = read_lock(root)
        require(sha(old_raw) == expected_lock, "Expected data lock changed")
        proposed = bundle.decode(payloads[LOCK])
        require(proposed["generation"] == current["generation"] + 1, "Candidate generation differs")
        originals = {LOCK: old_raw, "data/CURRENT": regular(root / "data/CURRENT")}
        backup = Path(tempfile.mkdtemp(prefix="latest-metadata-", dir=root / ".cache"))
        changed = []
        try:
            for name, raw in originals.items():
                write(backup / "original" / name, raw)
            # The lock last: readers reject a partially changed closure.
            for name in ("data/CURRENT", LOCK):
                require(regular(root / name) == originals[name], "Metadata changed while staging")
                if payloads[name] == originals[name]:
                    continue
                write(backup / "new" / name, payloads[name])
                changed.append(name)
                replace(backup / "new" / name, root / name)
            require(all(regular(root / name) == raw for name, raw in payloads.items()),
                    "Metadata changed during staging")
            read_lock(root)
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
    return {"status": "metadata-staged-not-adopted", "lock_id": receipt["lock_id"],
            "previous_id": expected_lock, "runtime_selection": "unchanged"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("bootstrap", "check", "resolve", "verify", "apply", "recheck", "verify-composition"))
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--expected-lock")
    parser.add_argument("--observation", type=Path, help="Existing fresh pinned observer receipt directory")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    args = parser.parse_args()
    if args.action == "verify":
        result = verify_candidate(args.root)
    elif args.action == "apply":
        require(args.candidate is not None and args.expected_lock is not None,
                "Apply needs --candidate and --expected-lock")
        result = apply_candidate(args.candidate, args.root, args.expected_lock)
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
                config = channels(args.root)
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
