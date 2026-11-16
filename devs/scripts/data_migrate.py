#!/usr/bin/env python3
"""Collect data migration measurements and review-only drafts; never accept them."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "devs/scripts"))
sys.path.insert(0, str(ROOT))
import config_relevance
from pipeline.runtime import data_bundle
from pipeline.runtime.data_snapshot import workspace_lock


PIN_NAMES = {
    "minions.raised_zombie", "minions.spectre_cocoon3",
    "parser_rules.handler_fallbacks", "parser_rules.section_counts",
    "skill_overrides.stored_uses_count",
}
PIN_FIXTURE = "crates/pobr-gamedata/tests/fixtures/repository-test-pins.json"
CONFIG = "web/src/lib/configGameRelevance.json"
COVERAGE = "devs/ci/parse-coverage-baseline.json"
SNAPSHOT = "examples/output-snapshot.json"
CORPORA = ("examples/demo-bd-test/builds", "examples/reference-builds/tuned/builds")
COUNTS = ("total", "parsed", "unsupported", "err")


def read_json(path):
    return data_bundle.decode(path.read_bytes())


def write_json(path, value, *, sort_keys=True, ensure_ascii=False):
    with path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(value, ensure_ascii=ensure_ascii, sort_keys=sort_keys, indent=2,
                                allow_nan=False) + "\n")


def game_digest(manifest):
    game = {key: value for key, value in manifest.items() if key != "languages"}
    game["files"] = {key: value for key, value in game["files"].items()
                     if not key.startswith("i18n/")}
    return data_bundle.sha256(data_bundle.encoded(game))


def output_directory(root, requested):
    base = root / ".cache/data-migrate"
    name = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-") + uuid.uuid4().hex[:8]
    raw = Path(requested) if requested else base / name
    if ".." in raw.parts:
        raise ValueError("output path cannot contain '..'")
    destination = Path(os.path.abspath(raw if raw.is_absolute() else root / raw))
    if destination.parent != base:
        raise ValueError("--out must be a fresh direct child of .cache/data-migrate")
    for path in (root / ".cache", base, destination):
        if path.is_symlink():
            raise ValueError(f"symlink output path: {path}")
    base.mkdir(parents=True, exist_ok=True)
    destination.mkdir()
    for name in ("measurements", "drafts", "logs"):
        (destination / name).mkdir()
    return destination


def source_fingerprints(root):
    paths = {root / name for name in (
        "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "pipeline/data.lock.json", "devs/reference-sources.json",
        "data/CURRENT", PIN_FIXTURE, CONFIG, COVERAGE, SNAPSHOT,
        "devs/scripts/config_relevance.py", "devs/scripts/data_migrate.py",
    )}
    for directory in ("crates", "tools/precompile-mods", "examples"):
        paths.update(path for path in (root / directory).rglob("*")
                     if path.is_file() and (path.suffix in {".rs", ".toml", ".json"}
                                           or path.name in {"code.txt", "decoded.xml"}))
    return {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(paths) if path.is_file()}


def differences(before, after):
    return [{"key": key, "before": before.get(key), "after": after.get(key)}
            for key in sorted(before.keys() | after.keys()) if before.get(key) != after.get(key)]


def pin_draft(root, raw, version, digest):
    if (raw.get("schema") != "pobr-repository-pins/v1" or raw.get("data_version") != version
            or raw.get("game_manifest_sha256") != digest
            or not isinstance(raw.get("pins"), dict) or set(raw["pins"]) != PIN_NAMES):
        raise ValueError("missing pin measurements or mismatched game manifest identity")
    pins = raw["pins"]
    for name in ("parser_rules.handler_fallbacks", "skill_overrides.stored_uses_count"):
        if type(pins[name]) is not int or pins[name] < 0:
            raise ValueError(f"invalid pin count: {name}")
    if not isinstance(pins["parser_rules.section_counts"], dict) or not pins["parser_rules.section_counts"]:
        raise ValueError("missing parser section measurements")
    for key, value in pins["parser_rules.section_counts"].items():
        if key in {"unsupported", "unsupported_pobr_extra"}:
            valid = isinstance(value, list) and all(isinstance(item, str) for item in value)
        else:
            valid = type(value) is int and value >= 0
        if not valid:
            raise ValueError(f"invalid parser section measurement: {key}")
    for name, keys in (("minions.raised_zombie", {"damage", "life"}),
                       ("minions.spectre_cocoon3", {"armour", "life"})):
        value = pins[name]
        if (not isinstance(value, dict) or set(value) != keys
                or any(type(number) not in (int, float) or not math.isfinite(number)
                       for number in value.values())):
            raise ValueError(f"invalid minion measurement: {name}")
    fixture = read_json(root / PIN_FIXTURE)
    before = fixture.get(version, {})
    fixture[version] = {"game_manifest_sha256": digest, "pins": pins}
    return fixture, {"binding_changed": before.get("game_manifest_sha256") != digest,
                     "differences": differences(before.get("pins", {}), pins)}


def coverage_draft(root, raw, version):
    if raw.get("_meta", {}).get("schema") != "parse-coverage/v1":
        raise ValueError("missing fresh parse coverage report")
    summary = raw["summary"]
    if any(type(summary.get(key)) is not int or summary[key] < 0 for key in COUNTS):
        raise ValueError("invalid coverage counts")
    ratio = summary.get("coverage_ratio")
    if (not summary["total"] or summary["total"] != sum(summary[key] for key in COUNTS[1:])
            or type(ratio) not in (float, int) or not math.isfinite(ratio)
            or abs(ratio - summary["parsed"] / summary["total"]) > 0.00000051):
        raise ValueError("inconsistent coverage summary")
    baseline = read_json(root / COVERAGE)
    regression = ratio + 0.0000005 < baseline["coverage_ratio"]
    draft = {**baseline, "patch": version, **{key: summary[key] for key in (*COUNTS, "coverage_ratio")}}
    return draft, {"before": baseline, "after": summary, "regression": regression,
                   "note_requires_review": True}


def snapshot_changes(before, after):
    changed = []
    for build in sorted(before.keys() | after.keys()):
        old, new = before.get(build, {}), after.get(build, {})
        for stat in sorted(old.keys() | new.keys()):
            if old.get(stat) != new.get(stat):
                changed.append({"build": build, "stats": [stat], "before": old.get(stat),
                                "after": new.get(stat), "attribution": "", "reason": ""})
    return changed


def validate_snapshot(snapshot):
    if snapshot.get("schema") != "pobr-output-snapshot/v1" or not snapshot.get("engine_version"):
        raise ValueError("invalid L2 snapshot identity")
    builds = snapshot["builds"]
    if not isinstance(builds, dict) or not builds:
        raise ValueError("missing L2 builds")
    for stats in builds.values():
        if not isinstance(stats, dict) or not stats or any(
            not isinstance(value, str) or not re.fullmatch(r"(?:-?\d+\.\d{6}|-?Infinity)", value)
            for value in stats.values()
        ):
            raise ValueError("invalid L2 values, including NaN")


def validate_l2(root, raw, candidate, identity):
    before = read_json(root / SNAPSHOT)
    for snapshot in (before, candidate):
        validate_snapshot(snapshot)
    expected_builds = {path.parent.relative_to(root).as_posix() for corpus in CORPORA
                       for path in (root / corpus).glob("*/code.txt")}
    if any(not list((root / corpus).glob("*/code.txt")) for corpus in CORPORA):
        raise ValueError("missing L2 corpus")
    if set(candidate["builds"]) != expected_builds:
        raise ValueError("incomplete L2 collection")
    identity_keys = ("runtime_identity", "engine_version")
    changed = snapshot_changes(before["builds"], candidate["builds"])
    if (candidate["runtime_identity"] != identity or raw.get("schema") != "pobr-output-diff/v1"
            or raw.get("before") != {key: before[key] for key in identity_keys}
            or raw.get("after") != {key: candidate[key] for key in identity_keys}
            or raw.get("differences") != changed):
        raise ValueError("L2 identities or empty attribution rows differ from fresh snapshot")
    return {"before": raw["before"], "after": raw["after"], "builds": len(expected_builds),
            "changed_stats": len(changed), "attribution_required": bool(changed)}


def collect(root, out, skip, run=subprocess.run):
    lock = read_json(root / "pipeline/data.lock.json")
    version = lock["target"]["game_patch"]
    if (root / "data/CURRENT").read_text().strip() != version:
        raise ValueError("data/CURRENT differs from the data lock; run data:prepare")
    files, identity = data_bundle.data_inputs(root / "data", version)
    if identity != lock["runtime_identity"]:
        raise ValueError("prepared runtime identity differs from the lock; run data:prepare")
    manifest = data_bundle.decode(files[f"data/{version}/manifest.json"])
    digest = game_digest(manifest)
    fingerprints = source_fingerprints(root)
    revision = run(["git", "rev-parse", "HEAD"], cwd=root, check=True,
                   capture_output=True, text=True).stdout.strip()
    status = run(["git", "status", "--porcelain"], cwd=root, check=True,
                 capture_output=True, text=True).stdout
    inputs = {"schema": "pobr-data-migration-inputs/v1", "head": revision,
              "git_status": status,
              "lock": lock, "data_version": version, "runtime_identity": identity,
              "game_manifest_sha256": digest, "source_sha256": fingerprints,
              "runtime_files_sha256": {name: data_bundle.sha256(raw) for name, raw in files.items()}}
    write_json(out / "inputs.json", inputs)
    report = {"schema": "pobr-data-migration/v1", "status": "INCOMPLETE", "accepted": False,
              "data_version": version, "runtime_identity": identity, "steps": {},
              "manual_review": [
                  "Review every draft; this command never applies or accepts it.",
                  "Explain coverage changes and update baseline notes; a decrease is never approved here.",
                  "Attribute every L2 row before adopting the freshly measured snapshot.",
                  "Check inline data assertions, WASM translation canaries and inherited fallbacks.",
                  "For a new patch family, check tree_version and compatibility labels; do not guess them.",
                  "Keep PoB2 references and historical Web fixtures bound to their original identities.",
                  "Run data:accept and exact-head full CI after reviewed edits.",
              ]}

    def command(name, args, env):
        with (out / "logs" / f"{name}.log").open("x", encoding="utf-8") as log:
            result = run(["mise", "exec", "--", *args], cwd=root, env=env,
                         stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise ValueError(f"{name} exited {result.returncode}; see logs/{name}.log")

    with tempfile.TemporaryDirectory(prefix="work-", dir=out) as temporary:
        work = Path(temporary)
        for name, raw in files.items():
            path = work / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        data = work / "data" / version
        env = {key: value for key, value in os.environ.items() if not key.startswith("POBR_")}
        env.update({"POBR_DATA_ROOT": str(work / "data"), "POBR_DATA_VERSION": version,
                    "CARGO_TARGET_DIR": str(root / "target"), "CARGO_NET_OFFLINE": "true"})

        def pins():
            path = out / "measurements/pins.json"
            command("pins", ["cargo", "test", "--locked", "-q", "-p", "pobr-build", "--test", "skills",
                             "repository_migration_pins", "--", "--ignored", "--nocapture"],
                    {**env, "POBR_MIGRATION_PINS": str(path)})
            draft, summary = pin_draft(root, read_json(path), version, digest)
            write_json(out / "drafts/repository-test-pins.json", draft)
            return summary

        def config():
            draft = config_relevance.web_manifest(data)
            before = read_json(root / CONFIG)
            summary = {"before_version": before["data_version"], "after_version": version,
                       "rule_changed": before.get("rule") != draft.get("rule"),
                       "differences": differences(before["excluded"], draft["excluded"])}
            write_json(out / "drafts/configGameRelevance.json", draft, sort_keys=False, ensure_ascii=True)
            return summary

        def coverage():
            if not list((root / CORPORA[0]).glob("*/decoded.xml")):
                raise ValueError("missing decoded XML coverage corpus")
            coverage_root = work / "coverage"
            for name, raw in files.items():
                path = coverage_root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(raw)
            (coverage_root / "examples").symlink_to(root / "examples", target_is_directory=True)
            coverage_data = coverage_root / "data" / version
            command("coverage", ["cargo", "run", "--locked", "--quiet", "-p", "precompile-mods", "--",
                                 "--data", str(coverage_data), "--report"], env)
            raw = read_json(coverage_data / "generated/parse-coverage.json")
            write_json(out / "measurements/parse-coverage.json", raw)
            draft, summary = coverage_draft(root, raw, version)
            if not summary["regression"]:
                write_json(out / "drafts/parse-coverage-baseline.json", draft)
            return summary

        def l2():
            path = out / "measurements/l2.json"
            candidate_path = work / "output-snapshot.json"
            command("l2", ["cargo", "test", "--locked", "-q", "-p", "pobr-build", "--test", "parity",
                           "output_snapshot::snapshot_update", "--", "--exact", "--ignored", "--nocapture"],
                    {**env, "POBR_SNAPSHOT_MODE": "report", "POBR_SNAPSHOT_REPORT": str(path),
                     "POBR_SNAPSHOT_LOCK": str(root / "pipeline/data.lock.json"),
                     "POBR_OUTPUT_SNAPSHOT": str(root / SNAPSHOT),
                     "POBR_MIGRATION_SNAPSHOT": str(candidate_path)})
            raw, candidate = read_json(path), read_json(candidate_path)
            summary = validate_l2(root, raw, candidate, identity)
            write_json(out / "drafts/output-snapshot.json", candidate)
            lines = ["| Build | Stat | Before | After | Attribution | Reason |",
                     "| --- | --- | --- | --- | --- | --- |"]
            for row in raw["differences"]:
                values = [row["build"], row["stats"][0], row["before"], row["after"], "", ""]
                lines.append("| " + " | ".join(str(value).replace("|", "\\|")
                                              if value is not None else "(absent)"
                                              for value in values) + " |")
            (out / "measurements/l2.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
            return summary

        for name, action in (("pins", pins), ("config", config), ("coverage", coverage), ("l2", l2)):
            print(f"data:migrate — {name}", flush=True)
            if name in skip:
                report["steps"][name] = {"status": "SKIPPED"}
                continue
            try:
                report["steps"][name] = {"status": "COLLECTED", **action()}
            except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
                report["steps"][name] = {"status": "FAILED", "error": str(error)}

    _, final_identity = data_bundle.data_inputs(root / "data", version)
    if source_fingerprints(root) != fingerprints or final_identity != identity:
        report["input_error"] = "inputs changed during collection; discard these drafts and rerun"
    elif all(step["status"] == "COLLECTED" for step in report["steps"].values()):
        report["status"] = ("REVIEW_REQUIRED" if report["steps"]["coverage"]["regression"]
                            else "READY_FOR_REVIEW")
    write_json(out / "report.json", report)
    (out / "README.md").write_text(
        f"# Data migration: {report['status']}\n\n"
        "Nothing was accepted or applied. Inspect report.json and inputs.json, then compare drafts/ "
        "with the corresponding tracked files. Failed or skipped measurements are incomplete. "
        "Coverage notes and every L2 attribution/reason require human review. "
        "Retain historical identities. Run data:accept and full CI after reviewed edits.\n",
        encoding="utf-8")
    return report


def main(argv=None, root=ROOT, run=subprocess.run):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", help="fresh direct child of .cache/data-migrate (default: unique run)")
    parser.add_argument("--skip", action="append", choices=("pins", "coverage", "l2"), default=[],
                        help="skip a measurement; result is INCOMPLETE and exits nonzero")
    args = parser.parse_args(argv)
    out = None
    try:
        out = output_directory(root, args.out)
        with workspace_lock(root):
            report = collect(root, out, set(args.skip), run)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        if out:
            write_json(out / "failure.json", {"status": "INCOMPLETE", "accepted": False, "error": str(error)})
        print(f"data:migrate: INCOMPLETE: {error}", file=sys.stderr)
        return 1
    print(f"data:migrate: {report['status']} — {out} (not accepted)", flush=True)
    return 0 if report["status"] == "READY_FOR_REVIEW" else 1


if __name__ == "__main__":
    raise SystemExit(main())
