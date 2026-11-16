"""Migration collection contracts; fake Cargo commands, real identity validation."""

import contextlib
import copy
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import data_migrate as migrate


class MigrationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.version = "0.5.5.8"
        self.data = self.root / "data" / self.version
        payload = b'{"fixture":true}'
        self.manifest = {"schema_version": 3, "poe_version": self.version,
                         "domains": {"base": ["stats"]}, "languages": [],
                         "files": {"base/stats.json": migrate.data_bundle.sha256(payload)}}
        self.put(f"data/{self.version}/base/stats.json", payload)
        self.put(f"data/{self.version}/manifest.json", self.manifest)
        self.put("data/CURRENT", self.version)
        _, self.identity = migrate.data_bundle.data_inputs(self.root / "data", self.version)
        self.put("pipeline/data.lock.json", {"target": {"game_patch": self.version},
                                             "runtime_identity": self.identity})
        self.digest = migrate.game_digest(self.manifest)
        self.pins = {"schema": "pobr-repository-pins/v1", "data_version": self.version,
                     "game_manifest_sha256": self.digest,
                     "pins": {"minions.raised_zombie": {"damage": 0.75, "life": 0.7},
                              "minions.spectre_cocoon3": {"armour": 0.4, "life": 2.2},
                              "parser_rules.handler_fallbacks": 3,
                              "parser_rules.section_counts": {"forms": 95},
                              "skill_overrides.stored_uses_count": 6736}}
        self.put(migrate.PIN_FIXTURE, {"old": {"pins": {"keep": True}}})
        self.config = {"data_version": self.version, "excluded": {}}
        self.put(migrate.CONFIG, {"data_version": "old", "excluded": {"removed": {}}})
        self.coverage = {"_meta": {"schema": "parse-coverage/v1"},
                         "summary": {"total": 10, "parsed": 8, "unsupported": 2,
                                     "err": 0, "coverage_ratio": 0.8}}
        self.put(migrate.COVERAGE, {"_meta": {"note": "reviewed note"}, "patch": "old",
                                   **self.coverage["summary"]})
        builds = {}
        for corpus in migrate.CORPORA:
            self.put(f"{corpus}/sample/code.txt", "code")
            self.put(f"{corpus}/sample/decoded.xml", "<xml/>")
            builds[f"{corpus}/sample"] = {"Life": "1.000000"}
        self.snapshot = {"schema": "pobr-output-snapshot/v1", "runtime_identity": "old",
                         "engine_version": "1", "builds": builds}
        self.put(migrate.SNAPSHOT, self.snapshot)
        self.candidate = {**copy.deepcopy(self.snapshot), "runtime_identity": self.identity}
        self.calls = []
        self.fail = None

    def put(self, relative, value):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(value, bytes):
            path.write_bytes(value)
        else:
            path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        return path

    def runner(self, command, **kwargs):
        if command[0] == "git":
            return subprocess.CompletedProcess(command, 0, stdout="head\n")
        self.calls.append((command, kwargs))
        env = kwargs["env"]
        kind = "pins" if "POBR_MIGRATION_PINS" in env else "l2" if "POBR_SNAPSHOT_MODE" in env else "coverage"
        if kind == self.fail:
            return subprocess.CompletedProcess(command, 7)
        if kind == "pins":
            migrate.write_json(Path(env["POBR_MIGRATION_PINS"]), self.pins)
        elif kind == "coverage":
            directory = Path(command[command.index("--data") + 1])
            directory.joinpath("generated").mkdir(exist_ok=True)
            migrate.write_json(directory / "generated/parse-coverage.json", self.coverage)
        else:
            migrate.write_json(Path(env["POBR_MIGRATION_SNAPSHOT"]), self.candidate)
            report = {"schema": "pobr-output-diff/v1",
                      "before": {key: self.snapshot[key] for key in ("runtime_identity", "engine_version")},
                      "after": {key: self.candidate[key] for key in ("runtime_identity", "engine_version")},
                      "differences": migrate.snapshot_changes(self.snapshot["builds"], self.candidate["builds"])}
            migrate.write_json(Path(env["POBR_SNAPSHOT_REPORT"]), report)
        return subprocess.CompletedProcess(command, 0)

    def collect(self, *args):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()), \
                patch.object(migrate.config_relevance, "web_manifest", return_value=self.config):
            result = migrate.main(["--out", ".cache/data-migrate/run", *args], self.root, self.runner)
        path = self.root / ".cache/data-migrate/run/report.json"
        return result, migrate.read_json(path) if path.exists() else None

    def test_collects_live_measurements_without_touching_inputs(self):
        before = migrate.source_fingerprints(self.root)
        result, report = self.collect()
        self.assertEqual(result, 0)
        self.assertEqual(report["status"], "READY_FOR_REVIEW")
        self.assertFalse(report["accepted"])
        self.assertEqual(before, migrate.source_fingerprints(self.root))
        out = self.root / ".cache/data-migrate/run"
        pins = migrate.read_json(out / "drafts/repository-test-pins.json")
        self.assertEqual(pins["old"], {"pins": {"keep": True}})
        self.assertEqual(pins[self.version]["pins"], self.pins["pins"])
        self.assertEqual(migrate.read_json(out / "drafts/parse-coverage-baseline.json")["_meta"]["note"], "reviewed note")
        self.assertEqual(report["steps"]["l2"]["changed_stats"], 0)
        self.assertEqual(len(self.calls), 3)
        for command, kwargs in self.calls:
            self.assertEqual(command[:3], ["mise", "exec", "--"])
            self.assertEqual(kwargs["env"]["CARGO_TARGET_DIR"], str(self.root / "target"))
            self.assertIn("--locked", command)
        self.assertFalse(list(out.glob("work-*")))

    def test_l2_changes_keep_empty_attribution_and_reason(self):
        build = next(iter(self.candidate["builds"]))
        self.candidate["builds"][build]["Life"] = "2.000000"
        result, report = self.collect()
        self.assertEqual(result, 0)
        self.assertEqual(report["steps"]["l2"]["changed_stats"], 1)
        diff = migrate.read_json(self.root / ".cache/data-migrate/run/measurements/l2.json")
        self.assertEqual(diff["differences"][0]["attribution"], "")
        self.assertEqual(diff["differences"][0]["reason"], "")

    def test_coverage_decline_requires_review_not_acceptance(self):
        self.coverage["summary"].update(parsed=7, unsupported=3, coverage_ratio=0.7)
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["status"], "REVIEW_REQUIRED")
        self.assertTrue(report["steps"]["coverage"]["regression"])
        self.assertEqual(migrate.read_json(self.root / migrate.COVERAGE)["coverage_ratio"], 0.8)
        self.assertFalse((self.root / ".cache/data-migrate/run/drafts/parse-coverage-baseline.json").exists())
        self.assertTrue((self.root / ".cache/data-migrate/run/measurements/parse-coverage.json").exists())

    def test_skip_is_incomplete_and_nonzero(self):
        result, report = self.collect("--skip", "pins", "--skip", "coverage", "--skip", "l2")
        self.assertEqual(result, 1)
        self.assertEqual(report["status"], "INCOMPLETE")
        self.assertFalse(self.calls)

    def test_command_failure_preserves_partial_evidence(self):
        self.fail = "pins"
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["steps"]["pins"]["status"], "FAILED")
        self.assertEqual(report["steps"]["l2"]["status"], "COLLECTED")

    def test_missing_measurement_is_not_successful(self):
        original = self.runner
        def missing(command, **kwargs):
            if "POBR_MIGRATION_PINS" in kwargs.get("env", {}):
                return subprocess.CompletedProcess(command, 0)
            return original(command, **kwargs)
        self.runner = missing
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["steps"]["pins"]["status"], "FAILED")

    def test_wrong_pin_identity_is_incomplete(self):
        self.pins["game_manifest_sha256"] = "wrong"
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["steps"]["pins"]["status"], "FAILED")

    def test_missing_pin_family_is_incomplete(self):
        del self.pins["pins"]["minions.raised_zombie"]
        self.assertEqual(self.collect()[0], 1)

    def test_nonobject_pin_measurement_leaves_incomplete_report(self):
        self.pins = []
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["status"], "INCOMPLETE")
        self.assertEqual(report["steps"]["pins"]["status"], "FAILED")

    def test_nonobject_coverage_summary_leaves_incomplete_report(self):
        self.coverage["summary"] = []
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["status"], "INCOMPLETE")
        self.assertEqual(report["steps"]["coverage"]["status"], "FAILED")

    def test_invalid_section_count_is_incomplete(self):
        self.pins["pins"]["parser_rules.section_counts"]["forms"] = "95"
        self.assertEqual(self.collect()[0], 1)

    def test_config_failure_does_not_publish_a_draft(self):
        self.put(migrate.CONFIG, {})
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertEqual(report["steps"]["config"]["status"], "FAILED")
        self.assertFalse((self.root / ".cache/data-migrate/run/drafts/configGameRelevance.json").exists())

    def test_runtime_identity_mismatch_refuses_all_commands(self):
        self.put("pipeline/data.lock.json", {"target": {"game_patch": self.version}, "runtime_identity": "wrong"})
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertIsNone(report)
        self.assertFalse(self.calls)

    def test_modified_runtime_bytes_refuse_collection(self):
        self.put(f"data/{self.version}/base/stats.json", b'{}')
        self.assertEqual(self.collect()[0], 1)
        self.assertFalse(self.calls)

    def test_wrong_current_refuses_collection(self):
        self.put("data/CURRENT", "0.0.0")
        self.assertEqual(self.collect()[0], 1)
        self.assertFalse(self.calls)

    def test_language_only_does_not_change_game_digest(self):
        localized = copy.deepcopy(self.manifest)
        localized["languages"] = ["zh-CN"]
        localized["files"]["i18n/zh-CN/stats.json"] = "new"
        self.assertEqual(migrate.game_digest(localized), self.digest)
        localized["files"]["base/stats.json"] = "changed"
        self.assertNotEqual(migrate.game_digest(localized), self.digest)

    def test_l2_wrong_identity_is_incomplete(self):
        self.candidate["runtime_identity"] = "wrong"
        self.assertEqual(self.collect()[0], 1)

    def test_l2_missing_build_is_incomplete(self):
        del self.candidate["builds"][next(iter(self.candidate["builds"]))]
        self.assertEqual(self.collect()[0], 1)

    def test_l2_nan_is_incomplete(self):
        self.candidate["builds"][next(iter(self.candidate["builds"]))]["Life"] = "NaN"
        self.assertEqual(self.collect()[0], 1)

    def test_input_change_during_collection_invalidates_report(self):
        original = self.runner
        def mutating(command, **kwargs):
            result = original(command, **kwargs)
            if "POBR_MIGRATION_PINS" in kwargs.get("env", {}):
                self.put("examples/demo-bd-test/builds/sample/code.txt", "changed")
            return result
        self.runner = mutating
        result, report = self.collect()
        self.assertEqual(result, 1)
        self.assertIn("input_error", report)

    def test_existing_output_is_never_overwritten(self):
        self.assertEqual(self.collect()[0], 0)
        saved = (self.root / ".cache/data-migrate/run/report.json").read_bytes()
        self.assertEqual(self.collect()[0], 1)
        self.assertEqual((self.root / ".cache/data-migrate/run/report.json").read_bytes(), saved)

    def test_unsafe_output_paths_are_rejected(self):
        for path in ("examples/out", ".cache/data-migrate", ".cache/data-migrate/../other",
                     ".cache/data-migrate/nested/out", str(self.root.parent / "outside")):
            with self.subTest(path=path), self.assertRaises(ValueError):
                migrate.output_directory(self.root, path)

    def test_symlink_output_parents_are_rejected(self):
        target = self.root / "other"
        target.mkdir()
        (self.root / ".cache").symlink_to(target, target_is_directory=True)
        with self.assertRaises(ValueError):
            migrate.output_directory(self.root, None)

    def test_host_data_and_bless_overrides_are_not_inherited(self):
        with patch.dict(os.environ, {"POBR_DATA_ROOT": "/other/data", "POBR_BLESS_PINS": "1",
                                     "POBR_SNAPSHOT_MODE": "record", "CARGO_TARGET_DIR": "/other/target"}):
            self.assertEqual(self.collect()[0], 0)
        for _, kwargs in self.calls:
            self.assertNotIn("POBR_BLESS_PINS", kwargs["env"])
            self.assertNotEqual(kwargs["env"]["POBR_DATA_ROOT"], "/other/data")
            self.assertNotEqual(kwargs["env"].get("POBR_SNAPSHOT_MODE"), "record")

    def test_inconsistent_coverage_counts_are_incomplete(self):
        self.coverage["summary"]["total"] = 20
        self.assertEqual(self.collect()[0], 1)


if __name__ == "__main__":
    unittest.main()
