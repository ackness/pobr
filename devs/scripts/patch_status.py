#!/usr/bin/env python3
"""Read-only patch handoff evidence; never a publication or acceptance gate."""

import argparse
from datetime import UTC, datetime
from functools import lru_cache
import http.client
import importlib.util
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tomllib
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from pipeline.imports import data_imports as imports
from pipeline.imports.data_bump import producer, same_pin
from pipeline.runtime import build_data, data_bundle

_spec = importlib.util.spec_from_file_location("patch_release_policy", ROOT / "devs/scripts/release-policy.py")
policy = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = policy
_spec.loader.exec_module(policy)

OWNERS = {"pobr": ("ackness/pobr", "master"), "repoe": ("ackness/repoe", "master"),
          "locale": ("ackness/poe2-zh-cn-stats", "main")}
SITES = ("https://pobr.win", "https://pobr-web.pages.dev")
PATCH = re.compile(r"[0-9]+(?:\.[0-9]+)+")
SHA = re.compile(r"[a-f0-9]{40}")
READY = {"ready", "observed", "not-applicable"}
CI_RULES = {
    ("repoe", "verify.yml"): {
        "verify": ("Check schema, lock, code and offline tests",),
        "pobr-data-adapter": ("Format, lint and test the adapter", "Keystone rules match the published game package"),
    },
    ("repoe", "release-build.yml"): {
        "release:build reproduces the published game package": ("Rebuild selected game packages from pinned inputs",),
    },
    ("locale", "verify.yml"): {
        "verify": ("Run mise run check",),
        "release:build reproduces the locale package": ("Rebuild every declared patch's language package from its pinned source",),
        "release:regenerate rebuilds the published members from pinned inputs": ("Regenerate every recipe's package from its producers' pinned inputs",),
    },
    ("pobr", "ci.yml"): {"Full gate": ("Require every full check",)},
    ("pobr", "pr.yml"): {"master integration / Full gate": ("Require every full check",)},
    ("pobr", "release.yml"): {
        "release identity": (),
        "release validation / Full gate": ("Require every full check",),
        "publish standalone WASM": ("Attach verified assets to version release",),
    },
}


class Unavailable(Exception):
    """Missing evidence, not proof of absence or failure."""


def now():
    return datetime.now(UTC).isoformat(timespec="seconds")


def command_text(args, cwd=ROOT, *, raw=False):
    env = {**os.environ, "GIT_OPTIONAL_LOCKS": "0", "GIT_NO_LAZY_FETCH": "1",
           "GIT_TERMINAL_PROMPT": "0", "GH_PROMPT_DISABLED": "1", "GH_HOST": "github.com"}
    try:
        result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=not raw, timeout=25)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise Unavailable(str(error)) from error
    if result.returncode:
        raise Unavailable(f"{shlex.join(args[:3])}: {result.stderr.strip()[:600] or 'command failed'}")
    return result.stdout


def parse_refs(raw):
    refs = {}
    for line in raw.splitlines():
        fields = line.split()
        if len(fields) != 2 or not SHA.fullmatch(fields[0]) or not fields[1].startswith("refs/"):
            raise Unavailable("Malformed remote refs")
        sha, ref = fields
        if ref in refs and refs[ref] != sha:
            raise Unavailable(f"Conflicting remote ref: {ref}")
        refs[ref] = sha
    for ref in refs:
        if ref.endswith("^{}") and ref[:-3] not in refs:
            raise Unavailable("Remote peeled tag lacks its tag ref")
    return {ref: refs.get(ref + "^{}", sha) for ref, sha in refs.items() if not ref.endswith("^{}")}


def json_object(raw):
    value = data_bundle.decode(raw)
    if not isinstance(value, dict):
        raise ValueError("Expected a JSON object")
    return value


def record(state, detail, **evidence):
    return {"state": state, "detail": detail, "observed_at": now(), **evidence}


def git_read(path, revision, name):
    return command_text(["git", "show", f"{revision}:{name}"], path, raw=True)


def local_checkout(path):
    try:
        head = command_text(["git", "rev-parse", "HEAD"], path).strip()
        dirty = bool(command_text(["git", "status", "--porcelain", "--untracked-files=normal"], path).strip())
        branch = command_text(["git", "branch", "--show-current"], path).strip()
        return record("observed", "Local checkout only; not publication evidence", path=str(path),
                      head=head, branch=branch, dirty=dirty)
    except Unavailable as error:
        return record("unknown", str(error), path=str(path))


class Sources:
    """Only bounded reads. No fetch, downloads to disk, build, CI dispatch or publish."""

    def __init__(self, offline=False):
        self.offline = offline
        self.download = imports.ReleaseInputs(base="https://github.com", transport="https", timeout=15, attempts=1)

    def online(self):
        if self.offline:
            raise Unavailable("Offline: remote evidence was not queried")

    def refs(self, owner):
        self.online()
        repository, branch = OWNERS[owner]
        return parse_refs(command_text(["git", "ls-remote", f"https://github.com/{repository}.git",
                                        f"refs/heads/{branch}", "refs/tags/*"]))

    def gh(self, owner, *args):
        self.online()
        return data_bundle.decode(command_text(["gh", *args, "-R", OWNERS[owner][0]]))

    @lru_cache(maxsize=None)
    def release(self, owner, tag):
        return self.gh(owner, "release", "view", tag, "--json",
                       "tagName,isDraft,isPrerelease,publishedAt,url,assets")

    @lru_cache(maxsize=None)
    def metadata(self, owner, tag, name):
        self.online()
        try:
            return self.download(imports.ReleaseAsset(OWNERS[owner][0], tag, name), max_bytes=imports.MAX_METADATA)
        except (OSError, ValueError) as error:
            raise Unavailable(f"Cannot read {tag}/{name}: {error}") from error

    @lru_cache(maxsize=None)
    def runs(self, owner, workflow, sha):
        return self.gh(owner, "run", "list", "--workflow", workflow, "--commit", sha, "--limit", "10",
                       "--json", "databaseId,headSha,headBranch,event,status,conclusion,url,createdAt,updatedAt,attempt")

    @lru_cache(maxsize=None)
    def run(self, owner, number):
        return self.gh(owner, "run", "view", str(number), "--json",
                       "databaseId,headSha,headBranch,event,status,conclusion,url,jobs,attempt")

    def observe(self, path):
        self.online()
        # RePoE owns this stdlib-only observer. Use this mise-selected Python,
        # not an owner build/setup task that might install or change files.
        return json_object(command_text([sys.executable, "-B", "-m", "RePoE.client.version"], path))

    def site(self, base):
        self.online()
        request = Request(base + "/data/manifest.json", headers={"Cache-Control": "no-cache", "User-Agent": "pobr-patch-status"})
        try:
            with urlopen(request, timeout=15) as response:
                raw = response.read(imports.MAX_METADATA + 1)
        except (OSError, http.client.HTTPException) as error:
            raise Unavailable(f"Site request failed: {error}") from error
        if len(raw) > imports.MAX_METADATA:
            raise ValueError("Site manifest exceeds metadata limit")
        return json_object(raw)


def safely(function, *args):
    try:
        return function(*args), None
    except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
        return None, str(error)


def ordered_tags(prefix, patch, refs, candidates=()):
    pattern = re.compile(re.escape(f"{prefix}/{patch}-") + r"([1-9][0-9]*)")
    tags = [ref.removeprefix("refs/tags/") for ref in refs if ref.startswith("refs/tags/")]
    tags.extend(candidates)
    matches = {(int(match[1]), tag) for tag in tags if (match := pattern.fullmatch(tag))}
    return [tag for _, tag in sorted(matches, reverse=True)]


def select_tag(prefix, patch, refs, candidates=()):
    return next(iter(ordered_tags(prefix, patch, refs, candidates)), None)


def candidates(path, owner, patch):
    """Inspect editable declarations only; never treat local files as published."""
    result = {}
    directory = path / "release"
    paths = directory.glob(f"{patch}*") if owner == "repoe" else directory.glob(f"{patch}*.json")
    for candidate in sorted(paths):
        name = candidate.name if owner == "repoe" else candidate.stem
        match = re.fullmatch(re.escape(patch) + r"(?:-([1-9][0-9]*))?", name)
        if not match:
            continue
        tag = f"{'game' if owner == 'repoe' else 'locale'}/{patch}-{match[1] or '1'}"
        source = candidate / "inputs.lock.json" if owner == "repoe" else candidate
        value, error = safely(lambda: json_object(source.read_bytes()))
        result[tag] = {"path": str(source), "document": value, "error": error}
    return result


def declaration_evidence(path, owner, tag, value, receipt):
    """A dirty or stale checkout must never invalidate immutable publication evidence."""
    if not receipt:
        return record("unknown", "Published receipt unavailable; producer declaration not checked")
    patch = tag.split("/")[1].rsplit("-", 1)[0]
    pattern = (rf"release/{re.escape(patch)}(?:-[1-9][0-9]*)?/inputs\.lock\.json" if owner == "repoe"
               else rf"release/{re.escape(patch)}(?:-[1-9][0-9]*)?\.json")
    configs = receipt.get("producer", {}).get("config", {})
    matches = [name for name in configs if re.fullmatch(pattern, name)]
    if len(matches) != 1:
        return record("unknown", "Receipt does not identify one producer declaration")
    name = matches[0]
    try:
        raw = git_read(path, value["commit"], name)
        imports.pinned(raw, configs[name])
        document = json_object(raw)
        if document.get("target") != receipt.get("target"):
            raise ValueError("Producer declaration target differs from receipt")
        expected = document.get("expect", {}).get("package") or document.get("published")
        if not expected:
            raise Unavailable("Producer declaration has no package expect/published pin")
        imports.validate_pin(expected)
        asset = f"pobr-{'game' if owner == 'repoe' else 'locale'}-{patch}.zip"
        if not same_pin(value["assets"].get(asset), expected):
            raise ValueError("Producer declaration pins different package bytes")
        return record("ready", "Declaration bytes and package pin match at the receipt producer commit", path=name,
                      commit=value["commit"], package=expected)
    except Unavailable as error:
        return record("unknown", str(error), path=name)
    except (OSError, ValueError, KeyError, TypeError) as error:
        return record("blocked", str(error), path=name)


def local_declaration_evidence(candidate, value, receipt):
    if not candidate:
        return record("unknown", "No matching local declaration")
    if candidate.get("error"):
        return record("blocked", candidate["error"], path=candidate["path"])
    try:
        document = candidate["document"]
        expected = document.get("expect", {}).get("package") or document.get("published")
        if not expected:
            raise ValueError("Local declaration has no package expect/published pin")
        imports.validate_pin(expected)
        if not receipt:
            return record("unknown", "Local package pin exists; publication not verified", path=candidate["path"], package=expected)
        package = receipt["package"]
        if document.get("target") != receipt.get("target") or not same_pin(package, expected):
            raise ValueError("Local declaration differs from published receipt (not used to judge publication)")
        return record("ready", "Local target/package pin matches published receipt", path=candidate["path"], package=expected)
    except (OSError, ValueError, KeyError, TypeError) as error:
        return record("blocked", str(error), path=candidate["path"])


def asset_pin(asset):
    digest = asset.get("digest")
    if not isinstance(digest, str) or not digest.startswith("sha256:"):
        raise Unavailable(f"No SHA-256 metadata for {asset.get('name')}")
    pin = {"bytes": asset.get("size"), "sha256": digest.removeprefix("sha256:")}
    imports.validate_pin(pin)
    if asset.get("state") != "uploaded":
        raise ValueError(f"Asset is not uploaded: {asset.get('name')}")
    return pin


def release_evidence(sources, owner, tag, refs, expected=None, receipt=True):
    if not tag:
        return record("unknown" if refs is None else "pending",
                      "Remote lookup unavailable; no local candidate" if refs is None else "No target tag or local declaration found"), None
    sha = refs.get(f"refs/tags/{tag}") if refs is not None else None
    if refs is not None and not sha:
        return record("pending", "Local candidate only; no remote tag", tag=tag), None
    if refs is None:
        return record("unknown", "Remote tag lookup unavailable", tag=tag), None
    try:
        release = sources.release(owner, tag)
        evidence = {"tag": tag, "commit": sha, "url": release.get("url"), "published_at": release.get("publishedAt")}
        if release.get("tagName") != tag:
            raise ValueError("Release names another tag")
        if release.get("isDraft") is not False or release.get("isPrerelease") is not False or not release.get("publishedAt"):
            return record("pending", "Draft/prerelease or publication time missing", **evidence), None
        rows = release.get("assets")
        if not isinstance(rows, list):
            raise ValueError("Release has no asset metadata")
        assets = {item["name"]: item for item in rows}
        if len(assets) != len(rows):
            raise ValueError("Duplicate Release asset names")
        sums_asset = assets.get("SHA256SUMS")
        if not sums_asset:
            raise ValueError("Release lacks SHA256SUMS")
        sums_raw = sources.metadata(owner, tag, "SHA256SUMS")
        imports.pinned(sums_raw, asset_pin(sums_asset))
        sums = imports.parse_sums(sums_raw)
        required = ({f"pobr-{'game' if owner == 'repoe' else 'locale'}-{tag.split('/')[1].rsplit('-', 1)[0]}.zip", "receipt.json"}
                    if receipt else set(expected or {}) or {name for name in assets if name.endswith(".zip")})
        if not required or not required <= assets.keys() or not required <= sums.keys():
            raise ValueError(f"Required Release assets/checksums missing: {sorted(required)}")
        pins = {}
        for name in required:
            pin = asset_pin(assets[name])
            if sums[name] != pin["sha256"]:
                raise ValueError(f"SHA256SUMS disagrees with Release asset metadata: {name}")
            pins[name] = pin
        for name, pin in (expected or {}).items():
            if name not in pins or not same_pin(pins[name], pin):
                raise ValueError(f"Published asset differs from declared pin: {name}")
        document = None
        if receipt:
            raw = sources.metadata(owner, tag, "receipt.json")
            imports.pinned(raw, pins["receipt.json"])
            document = json_object(raw)
            schemas = {"repoe": {"repoe-release-receipt/v1", "repoe-release-receipt/v2"},
                       "locale": {"poe2-locale-release-receipt/v1"}}
            if document.get("schema") not in schemas[owner]:
                raise Unavailable(f"Unsupported receipt schema: {document.get('schema')}")
            commit, contract = producer(document, OWNERS[owner][0])
            if commit != sha:
                raise ValueError("Receipt producer does not match the remote tag commit")
            package_name = next(name for name in required if name.endswith(".zip"))
            if not same_pin(document.get("package"), pins[package_name]):
                raise ValueError("Receipt package pin differs from Release asset metadata")
            evidence.update(producer_commit=commit, contract_version=contract)
        return record("ready", "Published asset metadata/checksum bytes agree; ZIP not downloaded",
                      assets=pins, **evidence), document
    except Unavailable as error:
        return record("unknown", str(error), tag=tag, commit=sha), None
    except (OSError, ValueError, KeyError, TypeError) as error:
        return record("blocked", str(error), tag=tag, commit=sha), None


def ci_evidence(sources, owner, workflow, sha, branch=None):
    if not sha:
        return record("unknown", "No exact commit available", workflow=workflow), None
    try:
        runs = sources.runs(owner, workflow, sha)
        exact = [item for item in runs if item.get("headSha") == sha
                 and item.get("event") in {"push", "schedule", "workflow_dispatch"}
                 and (branch is None or item.get("headBranch") == branch)]
        if not exact:
            return record("unknown", "No direct exact-SHA run found (PR merge runs are not substituted)",
                          workflow=workflow, commit=sha), None
        selected = max(exact, key=lambda item: item["databaseId"])
        run = sources.run(owner, selected["databaseId"])
        evidence = {"workflow": workflow, "commit": sha, "run_id": run.get("databaseId"),
                    "attempt": run.get("attempt"), "url": run.get("url")}
        if (run.get("headSha") != sha or run.get("event") not in {"push", "schedule", "workflow_dispatch"}
                or (branch is not None and run.get("headBranch") != branch)):
            raise Unavailable("Run identity changed or does not describe this commit/ref")
        if run.get("status") != "completed":
            return record("pending", "Exact-SHA run is not complete", **evidence), None
        if run.get("conclusion") != "success":
            return record("blocked", f"Exact-SHA run conclusion: {run.get('conclusion')}", **evidence), None
        jobs = {job["name"]: job for job in run.get("jobs", [])}
        for name, required_steps in CI_RULES[(owner, workflow)].items():
            job = jobs.get(name)
            if not job:
                raise Unavailable(f"Required job evidence absent: {name}")
            if job.get("status") != "completed" or job.get("conclusion") != "success":
                return record("blocked", f"Required job did not succeed: {name}", **evidence), None
            steps = {step["name"]: step for step in job.get("steps", [])}
            for step_name in required_steps:
                step = steps.get(step_name)
                if not step:
                    raise Unavailable(f"Required step evidence absent: {step_name}")
                if step.get("status") != "completed" or step.get("conclusion") != "success":
                    return record("blocked", f"Required step did not run successfully: {step_name}", **evidence), None
        return record("ready", "Exact-SHA workflow and required jobs/steps succeeded", **evidence), run
    except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
        return record("unknown", str(error), workflow=workflow, commit=sha), None


def master_ci_evidence(sources, sha):
    """The reusable Full gate also runs under push and release workflows."""
    observations = []
    for workflow in ("ci.yml", "pr.yml", "release.yml"):
        proof, _ = ci_evidence(sources, "pobr", workflow, sha)
        observations.append(proof)
        if proof["state"] == "ready":
            return proof
    return aggregate(observations, "No verified Full gate for remote master in the inspected workflows")


def site_evidence(sources, base, lock):
    try:
        manifest = sources.site(base)
        version, files, digests = (manifest.get(key) for key in ("version", "files", "digests"))
        if not isinstance(files, list) or not isinstance(digests, dict) or len(files) != len(set(files)) or set(files) != set(digests):
            raise ValueError("Malformed browser manifest inventory")
        if not isinstance(version, str) or not PATCH.fullmatch(version) or "manifest.json" not in digests:
            raise ValueError("Malformed browser data version")
        for name, digest in digests.items():
            imports.validate_pin({"bytes": 0, "sha256": digest})
            data_bundle.relative_path(name)
            if name != "manifest.json" and name.split("/")[0] not in {*data_bundle.LAYERS, "overlay-common"}:
                raise ValueError(f"Unexpected production data layer: {name}")
        hashes = {name: digest for name, digest in digests.items() if name != "manifest.json"}
        identity = data_bundle.sha256(data_bundle.encoded(["pobr-data-pack-v1", 3, version, hashes]))
        evidence = {"url": base + "/data/manifest.json", "data_version": version, "runtime_identity": identity,
                    "app_version": None}
        if not lock:
            return record("unknown", "Site manifest observed, but release-tag lock is unavailable", **evidence)
        if version != lock["target"]["game_patch"] or identity != lock["runtime_identity"]:
            return record("blocked", "Served manifest identity differs from the release-tag lock", **evidence)
        return record("observed", "Served data manifest matches tag lock; app version and payload bytes not verified", **evidence)
    except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
        return record("unknown", str(error), url=base + "/data/manifest.json")


def aggregate(items, detail):
    state = next((state for state in ("blocked", "unknown", "pending") if any(item["state"] == state for item in items)), "ready")
    return record(state, detail, checks=items)


def read_lock(path, revision=None):
    raw = (path / "pipeline/data.lock.json").read_bytes() if revision is None else git_read(path, revision, "pipeline/data.lock.json")
    lock = json_object(raw)
    build_data.validate(lock)
    return lock


def make_report(paths, patch=None, sources=None):
    sources = sources or Sources()
    started = now()
    repositories, refs = {}, {}
    for owner, path in paths.items():
        remote, error = safely(sources.refs, owner)
        refs[owner] = remote
        repositories[owner] = {"repository": OWNERS[owner][0], "local": local_checkout(path),
                               "remote": record("unknown", error) if error else record(
                                   "observed", "Live remote refs; no fetch", head=remote.get(f"refs/heads/{OWNERS[owner][1]}"))}
    local_lock, lock_error = safely(read_lock, paths["pobr"])
    observation, observation_error = safely(sources.observe, paths["repoe"])
    observed_patch = observation.get("version") if observation else None
    if observed_patch is not None and (not isinstance(observed_patch, str) or not PATCH.fullmatch(observed_patch)):
        observation_error, observed_patch = "Invalid RePoE version observation", None
    patch = patch or observed_patch or (local_lock["target"]["game_patch"] if local_lock else None)
    if not isinstance(patch, str) or not PATCH.fullmatch(patch):
        raise ValueError("Pass a numeric patch; neither the observer nor the local data lock supplied one")
    stages = []

    def stage(name, value, next_action):
        stages.append({"id": name, **value, "next": next_action})

    stage("official", record("observed", "Endpoint advertisement only; not a release date or source certification",
                             advertised_patch=observed_patch, cdn=observation.get("cdn"), matches_target=observed_patch == patch)
          if observed_patch else record("unknown", observation_error or "No official observation"),
          f"In RePoE: mise run client:version; selected target remains {patch}")
    published, receipts, declarations = {}, {}, {}
    for owner, prefix in (("repoe", "game"), ("locale", "locale")):
        local = candidates(paths[owner], owner, patch)
        tags = ordered_tags(prefix, patch, refs[owner] or {})
        alternatives = []
        value, receipt = None, None
        for tag in tags:
            candidate_value, candidate_receipt = release_evidence(sources, owner, tag, refs[owner])
            if candidate_receipt and candidate_receipt.get("target") != {"game": "poe2", "game_patch": patch, "realm": "international"}:
                candidate_value["state"], candidate_value["detail"] = "blocked", "Receipt target is not the selected international patch"
                candidate_receipt = None
            if candidate_value["state"] == "ready":
                value, receipt = candidate_value, candidate_receipt
                break
            alternatives.append(candidate_value)
        if value is None:
            value = alternatives[0] if alternatives else release_evidence(
                sources, owner, select_tag(prefix, patch, {}, local), refs[owner])[0]
        selected = value.get("tag")
        declaration = local.get(selected, {})
        declarations[owner] = declaration
        value["newer_tags"] = alternatives if receipt else alternatives[1:]
        value["local_declaration"] = local_declaration_evidence(declaration, value, receipt)
        value["local_candidates"] = [{"tag": tag, "remote_tag_exists": None if refs[owner] is None else f"refs/tags/{tag}" in refs[owner],
                                      **{key: val for key, val in item.items() if key != "document"}}
                                     for tag, item in local.items()]
        value["producer_declaration"] = declaration_evidence(paths[owner], owner, selected, value, receipt)
        published[owner], receipts[owner] = value, receipt

    # A new contract tag is not required for every patch. Check the adapter's
    # actual pin, not the highest documentation revision of the contract.
    try:
        adapter = tomllib.loads((paths["repoe"] / "tools/pobr-data-adapter/Cargo.toml").read_text())
        dependency = adapter["dependencies"]["pobr-data"]
        tag = dependency.get("tag")
        if dependency.get("git", "").removesuffix(".git") != "https://github.com/ackness/pobr" or not re.fullmatch(r"contract/[0-9]+\.[0-9]+", tag or ""):
            raise ValueError("Adapter does not pin an immutable PoBR contract tag")
        contract = build_data.contract_version(paths["pobr"] / "crates/pobr-data/src/lib.rs")
        if int(tag.split("/")[1].split(".")[0]) != contract:
            raise ValueError("Adapter contract major differs from this PoBR checkout")
        if refs["pobr"] is None:
            raise Unavailable("Contract tag lookup unavailable")
        value = record("ready" if f"refs/tags/{tag}" in refs["pobr"] else "pending",
                       "Adapter contract pin (documentation revisions need not match the newest tag)",
                       tag=tag, commit=refs["pobr"].get(f"refs/tags/{tag}"), contract_version=contract)
    except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
        value = record("unknown", str(error))
    value["scope"] = "current local adapter dependency"
    receipt_contract = (receipts["repoe"] or {}).get("producer", {}).get("contract")
    if isinstance(receipt_contract, dict):
        tag, commit, version = (receipt_contract.get(key) for key in ("tag", "commit", "version"))
        if refs["pobr"] is None:
            published_contract = record("unknown", "Published contract tag lookup unavailable", **receipt_contract)
        elif (receipt_contract.get("repository") != OWNERS["pobr"][0]
              or not isinstance(tag, str) or not re.fullmatch(r"contract/[0-9]+\.[0-9]+", tag)
              or type(version) is not int or int(tag.split("/")[1].split(".")[0]) != version
              or refs["pobr"].get(f"refs/tags/{tag}") != commit):
            published_contract = record("blocked", "Published receipt contract does not match its remote tag", **receipt_contract)
        else:
            published_contract = record("ready", "Published game receipt contract tag/commit match remote", **receipt_contract)
        value = {**published_contract, "local_adapter": value}
    else:
        value["detail"] += "; no published game contract receipt available"
    stage("contract", value, "Check the published game receipt contract and local adapter dependency; do not tag unchanged contracts")

    input_specs = ((receipts["repoe"] or {}).get("inputs", {}).get("packages")
                   or (declarations["repoe"].get("document") or {}).get("packages", {}))
    input_pins = {}
    for item in input_specs.values() if isinstance(input_specs, dict) else ():
        if (isinstance(item, dict) and item.get("repository") == OWNERS["repoe"][0]
                and str(item.get("tag", "")).startswith(f"inputs/{patch}-") and "revision" not in item):
            input_pins.setdefault(item["tag"], {})[item["asset"]] = item
    if not input_pins:
        latest = select_tag("inputs", patch, refs["repoe"] or {})
        if latest:
            input_pins[latest] = {}
    inputs = [release_evidence(sources, "repoe", tag, refs["repoe"], pins, receipt=False)[0]
              for tag, pins in sorted(input_pins.items())]
    stage("inputs", aggregate(inputs, "Input Release metadata; embedded export receipts are not inspected") if inputs else
          record("unknown" if refs["repoe"] is None else "pending", "No target inputs Release found"),
          "In RePoE follow RELEASING.md: export from clean master, then review release:inputs (no automatic publication)")
    stage("game", published["repoe"], "In RePoE: review mise run release:build -- --fetch --check and release:publish dry-run")
    stage("locale", published["locale"], "In locale owner: review release:build -- --fetch --check and release:regenerate -- --fetch")

    main_sha = (refs["pobr"] or {}).get("refs/heads/master")
    main_lock, main_error = safely(read_lock, paths["pobr"], main_sha) if main_sha else (None, "Remote master SHA unavailable")
    desired = {"game": published["repoe"], "language": published["locale"]}
    adoption = record("unknown", main_error) if not main_lock else record(
        "pending", "Remote master has not adopted the selected package pair", commit=main_sha,
        target=main_lock["target"], packages=main_lock["packages"], runtime_identity=main_lock["runtime_identity"])
    if main_lock and all(value["state"] == "ready" for value in desired.values()):
        try:
            for role, value in desired.items():
                spec = main_lock["packages"][role]
                if spec["tag"] != value["tag"] or spec.get("producer_commit") != value["commit"]:
                    raise ValueError(f"Remote master still pins a different {role} tag/producer")
                for name, pin in ((spec["asset"], spec), ("receipt.json", spec["receipt"])):
                    if not same_pin(value["assets"].get(name), pin):
                        raise ValueError(f"Remote master pins different {role} bytes: {name}")
            if main_lock["target"]["game_patch"] != patch:
                raise ValueError("Remote master target differs")
            adoption["state"], adoption["detail"] = "ready", "Remote master lock matches selected published packages and receipt pins"
        except (ValueError, KeyError, TypeError) as error:
            adoption["detail"] = str(error)
    if main_lock and not all(value["state"] == "ready" for value in desired.values()):
        if any(value["state"] == "unknown" for value in desired.values()):
            adoption["state"], adoption["detail"] = "unknown", "Publication evidence incomplete; cannot compare selected pair with remote master"
        else:
            adoption["state"], adoption["detail"] = "pending", "Selected published pair is not available for adoption"
    adoption["local_lock"] = {"error": lock_error, "target": local_lock.get("target") if local_lock else None,
                              "differs_from_master": local_lock != main_lock if local_lock and main_lock else None}
    bump = (f"mise run data:bump ackness/repoe {published['repoe']['tag']} "
            f"ackness/poe2-zh-cn-stats {published['locale']['tag']}"
            if published['repoe'].get('tag') and published['locale'].get('tag') else "wait for both owner Releases")
    stage("adoption", adoption, f"In PoBR: data:prepare; {bump}; migrate consumers, data:accept, then a reviewed data PR")

    ci = []
    for owner, workflows in (("repoe", ("verify.yml", "release-build.yml")), ("locale", ("verify.yml",))):
        for workflow in workflows:
            proof, _ = ci_evidence(sources, owner, workflow, published[owner].get("commit"))
            ci.append({"repository": OWNERS[owner][0], **proof})
    ci.append({"repository": OWNERS["pobr"][0], **master_ci_evidence(sources, main_sha)})
    stage("ci", aggregate(ci, "Producer-commit and remote-master CI evidence; never substitute another SHA"),
          "Inspect exact-SHA workflow jobs; if necessary request full CI only after review/authorization")

    app_tags = []
    for ref in refs["pobr"] or {}:
        if ref.startswith("refs/tags/v"):
            tag = ref.removeprefix("refs/tags/")
            try:
                version = policy.tag_version(tag)
                if version.is_production():
                    app_tags.append((version.precedence(), tag))
            except policy.PolicyError:
                pass
    app_tag = max(app_tags)[1] if app_tags else None
    app_sha = (refs["pobr"] or {}).get(f"refs/tags/{app_tag}")
    app_lock, app_error = safely(read_lock, paths["pobr"], app_sha) if app_sha else (None, "No stable application tag available")
    app = record("unknown", app_error, tag=app_tag, commit=app_sha)
    release_run = None
    if app_lock:
        proof, release_run = ci_evidence(sources, "pobr", "release.yml", app_sha, app_tag)
        try:
            release = sources.release("pobr", app_tag)
            required = {f"pobr-wasm-{app_tag}-web.tar.gz", "wasm-size.json", "wasm-size.md", "SHA256SUMS"}
            assets = {item["name"]: item for item in release["assets"]}
            if (release.get("tagName") != app_tag or release.get("isDraft") is not False
                    or release.get("isPrerelease") is not False or not release.get("publishedAt")):
                raise ValueError("Application Release is not a stable publication")
            if not required <= assets.keys() or any(assets[name].get("state") != "uploaded" for name in required):
                raise ValueError("Application Release lacks uploaded required assets")
            pair_matches = main_lock is not None and app_lock == main_lock and adoption["state"] == "ready"
            pair_unknown = main_lock is None or adoption["state"] == "unknown"
            app = record(proof["state"] if pair_matches else "unknown" if pair_unknown else "pending",
                         "Highest stable tag carries the adopted lock" if pair_matches else
                         "Selected adoption evidence unavailable; application lock comparison is unknown" if pair_unknown else
                         "Highest stable tag does not carry the selected adopted lock",
                         tag=app_tag, commit=app_sha, url=release.get("url"), published_at=release.get("publishedAt"),
                         runtime_identity=app_lock["runtime_identity"], ci=proof,
                         assets_checked="required names and uploaded state only; asset bytes/digests not verified")
        except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
            app = record("unknown", str(error), tag=app_tag, commit=app_sha, ci=proof)
    stage("release", app, "Follow PoBR RELEASING.md §4: separate version PR, local e2e release receipt, then authorized immutable tag")

    deployment = record("unknown", "No completed exact-tag release run available", tag=app_tag)
    if release_run:
        jobs = [job for job in release_run.get("jobs", []) if job.get("name") == "deploy stable v0 to Cloudflare Pages"]
        if len(jobs) == 1:
            job = jobs[0]
            steps = [step for step in job.get("steps", []) if step.get("name", "").startswith("Run cloudflare/wrangler-action@")]
            done = (job.get("status") == "completed" and job.get("conclusion") == "success" and len(steps) == 1
                    and steps[0].get("status") == "completed" and steps[0].get("conclusion") == "success")
            deployment = record("ready" if done else "pending", "Actual deployment step succeeded" if done else "Deployment step skipped, failed or unavailable",
                                tag=app_tag, commit=app_sha, run_id=release_run["databaseId"], attempt=release_run.get("attempt"),
                                url=release_run.get("url"), completed_at=job.get("completedAt"))
    selected_lock = app_lock is not None and main_lock == app_lock and adoption["state"] == "ready"
    comparison_unknown = app_lock is None or main_lock is None or adoption["state"] == "unknown"
    if not selected_lock:
        deployment["observed_deployment_state"] = deployment["state"]
        deployment["state"] = "unknown" if comparison_unknown else "pending"
        deployment["detail"] = ("Selected adoption comparison unavailable; latest-app deployment observed separately"
                                if comparison_unknown else "Deployment evidence is for the latest app, not the selected adopted package pair")
    stage("deployment", deployment, "Inspect the exact-tag release workflow's deployment step, not just its aggregate conclusion")
    sites = [site_evidence(sources, base, app_lock) for base in SITES]
    site_summary = aggregate(sites, "Live manifest comparison only; no app-version endpoint or browser/strict-audit verification")
    if not selected_lock and site_summary["state"] == "ready":
        site_summary["state"] = "unknown" if comparison_unknown else "pending"
        site_summary["detail"] = ("Sites match the latest app lock; selected adoption comparison unavailable"
                                  if comparison_unknown else "Observed sites match the latest app lock, not the selected adopted package pair")
    stage("sites", site_summary,
          f"Open both sites and check {app_tag or 'the release version'}; run audit:display --release <tag> --strict on a real build corpus")

    consistency = []
    for owner, before in refs.items():
        after, error = safely(sources.refs, owner)
        if error:
            consistency.append(record("unknown", f"{owner}: final ref check unavailable: {error}"))
        elif before != after:
            consistency.append(record("unknown", f"{owner}: refs changed during collection; rerun before using the report"))
        else:
            consistency.append(record("ready", f"{owner}: remote refs unchanged during collection"))
    stable = aggregate(consistency, "Rechecked remote refs after collection")
    if stable["state"] != "ready":
        stages.insert(0, {"id": "consistency", **stable, "next": "Rerun patch:status when remote refs are stable; do not use mixed observations"})
    # Official discovery is informational when an explicit/historical target is
    # inspected. Unknown discovery must not hide a later actionable handoff.
    first = next((item for item in stages if item["id"] != "official" and item["state"] not in READY), None)
    attention = []
    for item in stages:
        for key in ("producer_declaration", "local_declaration", "local_adapter"):
            evidence = item.get(key)
            if evidence and evidence["state"] not in READY:
                attention.append({"stage": item["id"], "kind": key, **evidence})
        attention.extend({"stage": item["id"], "kind": "newer_tag", **evidence}
                         for evidence in item.get("newer_tags", []))
    return {"schema": "pobr-patch-status/v1", "started_at": started, "observed_at": now(),
            "target": {"game": "poe2", "game_patch": patch, "realm": "international"},
            "repositories": repositories, "stages": stages, "consistency": stable, "attention": attention,
            "selected_packages": {owner: published[owner].get("tag") for owner in ("repoe", "locale")},
            "first_incomplete": first["id"] if first else None,
            "next": first["next"] if first else stages[-1]["next"],
            "scope": "Workflow metadata observations, not approval to publish or proof of source, game semantics, ZIP reproducibility or complete live acceptance.",
            "unverified": ["ZIP payload bytes, embedded export receipts and owner publication policies (including D40)", "L2 attribution and local e2e release receipt",
                           "Live application version, browser behavior and release strict display audit"]}


def render(report):
    print(f"Patch {report['target']['game_patch']} ({report['target']['realm']}) — {report['observed_at']}")
    for item in report["stages"]:
        print(f"  {item['id']:12} {item['state'].upper():10} {item['detail']}")
        for check in item.get("checks", []):
            label = check.get("tag") or check.get("repository") or check.get("url") or check.get("workflow", "")
            print(f"    {check['state'].upper():10} {label}: {check['detail']}")
        if item.get("tag"):
            print(f"    {item['tag']} @ {item.get('commit') or 'commit not verified'}")
        if item.get("advertised_patch"):
            print(f"    endpoint: {item['advertised_patch']} (matches target: {item['matches_target']})")
        for candidate in item.get("local_candidates", []):
            if candidate["remote_tag_exists"] is not True:
                print(f"    local candidate (not publication): {candidate['tag']} — {candidate['error'] or candidate['path']}")
        for newer in item.get("newer_tags", []):
            print(f"    newer tag {newer['tag']}: {newer['state'].upper()} — {newer['detail']}")
        for key in ("local_declaration", "producer_declaration", "local_adapter"):
            evidence = item.get(key)
            if evidence:
                print(f"    {key}: {evidence['state'].upper()} — {evidence['detail']}")
        if item.get("assets_checked"):
            print(f"    assets: {item['assets_checked']}")
    print(f"\nFirst incomplete: {report['first_incomplete'] or 'none in the implemented metadata checks'}")
    print(f"Next: {report['next']}")
    if report["attention"]:
        print("\nAttention (separate from the selected published pair):")
        for item in report["attention"]:
            print(f"  {item['stage']}/{item['kind']} {item['state'].upper()}: {item['detail']}")
    print("\nNot verified: " + "; ".join(report["unverified"]))
    for owner, repository in report["repositories"].items():
        local = repository["local"]
        if local["state"] == "unknown" or local.get("dirty"):
            print(f"Local {owner}: {local['detail'] if local['state'] == 'unknown' else 'dirty working tree; not publication evidence'}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("patch", nargs="?", help="Numeric target (default: official observation, then local lock)")
    parser.add_argument("--pobr", type=Path, default=ROOT, help="PoBR checkout to inspect")
    parser.add_argument("--repoe", type=Path, help="RePoE checkout (default: sibling of the primary PoBR checkout)")
    parser.add_argument("--locale", type=Path, help="Locale checkout (default: sibling poe2-zh-cn-stats)")
    parser.add_argument("--offline", action="store_true", help="No network or official observer; remote states remain unknown")
    parser.add_argument("--json", action="store_true", help="Print pobr-patch-status/v1 JSON to stdout; never writes a report file")
    args = parser.parse_args(argv)
    if args.patch is not None and not PATCH.fullmatch(args.patch):
        parser.error("patch must be a dotted numeric version")
    try:
        common = Path(command_text(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"], args.pobr).strip())
        workspace = common.parent.parent
        paths = {"pobr": args.pobr.resolve(), "repoe": (args.repoe or workspace / "repoe").resolve(),
                 "locale": (args.locale or workspace / "poe2-zh-cn-stats").resolve()}
        report = make_report(paths, args.patch, Sources(args.offline))
    except (Unavailable, OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"patch:status: {error}\n")
    if args.json:
        print(imports.encoded(report).decode(), end="")
    else:
        render(report)
    return 0  # A status report is not a gate, including when evidence is missing.


if __name__ == "__main__":
    raise SystemExit(main())
