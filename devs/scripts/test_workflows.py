"""Check script failure propagation and working-tree preservation without Cargo."""

import base64
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
from unittest.mock import patch
import zlib


REPO = Path(__file__).resolve().parents[2]


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="pobr script tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        scripts = ["devs/scripts/regen-check.sh", "devs/scripts/cache-report.py",
                   "devs/scripts/driver.sh", ".claude/skills/run-pobr/driver.sh"]
        if (REPO / ".agents/skills/run-pobr/driver.sh").is_file():
            scripts.append(".agents/skills/run-pobr/driver.sh")
        for rel in scripts:
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / rel, dest)
        self.write("Cargo.toml", "")
        self.write("pipeline/config.json", '{"patch":"test"}')
        self.write("data/CURRENT", "test")
        self.write("data/test/base/passive_tree.json", "[]")
        self.write("data/test/generated/special_derived.json", "{}")
        self.write("data/test/generated/special_vendor.json", "{}")
        self.write("data/overlay-common/special_mods.json", "{}")
        self.write("examples/demo-bd-test/builds/sample/decoded.xml", "<Build/>")
        self.write("data/test/generated/parsed_mods.json", "canonical")
        self.write("data/test/generated/parse-coverage.json", '{"coverage_ratio": 1.0}')
        self.write("devs/ci/parse-coverage-baseline.json", '{"coverage_ratio": 1.0}')
        self.write("bin/cargo", '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["CALLS"], "a", encoding="utf-8") as out:
    out.write(json.dumps(args) + "\\n")
if args == ["nextest", "--version"]:
    sys.exit(0 if os.environ.get("HAS_NEXTEST") else 1)
if args[:1] == ["metadata"]:
    print(json.dumps({"target_directory": str(pathlib.Path.cwd() / "target")}))
    sys.exit(0)
if os.environ.get("FAIL_COMMAND") == args[0]:
    sys.exit(17)
if os.environ.get("FAIL_TEST") and args[0] == "test":
    sys.exit(19)
if "--gem-quality" in args and os.environ.get("FAIL_QUALITY"):
    sys.exit(29)
if "pobr-data-adapter" in args:
    dest = pathlib.Path(args[args.index("--out") + 1]) / "test/generated"
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "special_derived.json").write_text("{}", encoding="utf-8")
    if "--gem-quality" in args:
        quality = dest.parent / "overlay/gem_quality_stats.json"
        quality.parent.mkdir(parents=True, exist_ok=True)
        quality.write_text("{}", encoding="utf-8")
if "precompile-mods" in args:
    dest = pathlib.Path(args[args.index("--data") + 1])
    assert dest != pathlib.Path(os.environ["SOURCE_DATA"])
    assert (dest / "generated/special_vendor.json").is_file()
    assert (dest.parent / "overlay-common/special_mods.json").is_file()
    assert (dest.parent.parent / "examples/demo-bd-test/builds/sample/decoded.xml").is_file()
    (dest / "generated/parsed_mods.json").write_text("canonical", encoding="utf-8")
    if os.environ.get("FAIL_REGEN"):
        sys.exit(23)
    (dest / "generated/parse-coverage.json").write_text('{"coverage_ratio": 1.0}', encoding="utf-8")
''')
        (self.root / "bin/cargo").chmod(0o755)
        self.write("bin/gh", '''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["CALLS"], "a", encoding="utf-8") as out:
    out.write(json.dumps(["gh", *sys.argv[1:]]) + "\\n")
sys.exit(int(os.environ.get("GH_EXIT", "0")))
''')
        (self.root / "bin/gh").chmod(0o755)
        (self.root / "tmp").mkdir()
        self.env = {
            **os.environ,
            "PATH": f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}",
            "CALLS": str(self.root / "calls.jsonl"),
            "SOURCE_DATA": str(self.root / "data/test"),
            "TMPDIR": str(self.root / "tmp"),
            "POBR_PATCH": "test",
        }
        for key in ["HAS_NEXTEST", "FAIL_COMMAND", "FAIL_TEST", "FAIL_REGEN", "FAIL_QUALITY", "GH_EXIT"]:
            self.env.pop(key, None)

    def write(self, rel, text):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def run_script(self, script, *args):
        return subprocess.run(
            ["bash", str(self.root / script), *args], cwd=self.root,
            env=self.env, capture_output=True, text=True, timeout=10,
        )

    def assert_regen_preserves_tree(self, expected):
        before = {p.relative_to(self.root): p.read_bytes()
                  for p in (self.root / "data").rglob("*") if p.is_file()}
        result = self.run_script("devs/scripts/regen-check.sh")
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        after = {p.relative_to(self.root): p.read_bytes()
                 for p in (self.root / "data").rglob("*") if p.is_file()}
        self.assertEqual(before, after)
        self.assertEqual(list((self.root / "tmp").iterdir()), [])

    def test_regen_success_preserves_sources(self):
        self.assert_regen_preserves_tree(0)

    def test_quality_regeneration_uses_receipt_and_preserves_sources(self):
        self.write("pipeline/tables/English/GrantedEffectQualityStats.json", "[]")
        self.write("pipeline/gem-quality/test.json", "{}")
        self.write("data/test/overlay/gem_quality_stats.json", "{}")
        self.assert_regen_preserves_tree(0)
        quality = [args for args in self.calls() if "--gem-quality" in args]
        self.assertEqual(len(quality), 1)
        self.assertEqual(quality[0][quality[0].index("--quality-source") + 1],
                         str(self.root / "pipeline/gem-quality/test.json"))

    def test_quality_failure_aborts_full_regeneration_before_other_generators(self):
        shutil.copyfile(REPO / "pipeline/regen-all.sh", self.root / "pipeline/regen-all.sh")
        shutil.copyfile(REPO / "pipeline/data_snapshot.py", self.root / "pipeline/data_snapshot.py")
        self.write("pipeline/config.json", '{"patch":"9.1"}')
        self.write("data/CURRENT", "9.1\n")
        self.write("data/9.1/overlay/gem_quality_stats.json", "previous quality data")
        self.env["FAIL_QUALITY"] = "1"
        result = self.run_script("pipeline/regen-all.sh")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(len(self.calls()), 1)
        self.assertIn("--gem-quality", self.calls()[0])
        self.assertEqual((self.root / "data/9.1/overlay/gem_quality_stats.json").read_text(),
                         "previous quality data")

    def test_regen_drift_preserves_uncommitted_artifact(self):
        self.write("data/test/generated/parsed_mods.json", "uncommitted edit")
        self.assert_regen_preserves_tree(1)

    def test_regen_partial_failure_preserves_uncommitted_artifact(self):
        self.env["FAIL_REGEN"] = "1"
        self.write("data/test/generated/parsed_mods.json", "uncommitted edit")
        self.assert_regen_preserves_tree(23)

    def test_regen_dereferences_output_symlinks(self):
        artifact = self.root / "data/test/generated/parsed_mods.json"
        artifact.unlink()
        self.write("original-output", "uncommitted edit")
        artifact.symlink_to(self.root / "original-output")
        self.assert_regen_preserves_tree(1)
        self.assertEqual((self.root / "original-output").read_text(), "uncommitted edit")

    def test_smoke_stops_on_first_failure(self):
        self.env["FAIL_TEST"] = "1"
        result = self.run_script("devs/scripts/driver.sh", "smoke")
        self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
        calls = (self.root / "calls.jsonl").read_text().splitlines()
        self.assertEqual(len(calls), 1)
        self.assertEqual(json.loads(calls[0])[0], "test")

    def test_targeted_arguments_are_forwarded_without_splitting(self):
        args = ["-p", "pobr-build", "--test", "codec", "build_code::", "--", "--exact"]
        result = self.run_script("devs/scripts/driver.sh", "test", *args)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads((self.root / "calls.jsonl").read_text()), ["test", *args])

    def test_empty_target_does_not_run_workspace(self):
        result = self.run_script("devs/scripts/driver.sh", "test")
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.root / "calls.jsonl").exists())

    def test_short_commands_select_only_requested_suite_and_preserve_filter(self):
        result = self.run_script("devs/scripts/driver.sh", "verify", "build", "skills", "a filter with spaces")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [
            ["test", "-p", "pobr-build", "--test", "skills", "a filter with spaces"],
            ["fmt", "--all", "--check"],
            ["clippy", "-p", "pobr-build", "--test", "skills", "--", "-D", "warnings"],
        ])

    def test_verify_does_not_lint_after_test_failure(self):
        self.env["FAIL_TEST"] = "1"
        result = self.run_script("devs/scripts/driver.sh", "verify", "core", "lib")
        self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["test", "-p", "pobr-core", "--lib"]])

    def test_timings_compiles_only_the_selected_suite(self):
        result = self.run_script("devs/scripts/driver.sh", "timings", "pobr-build", "parity")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [
            ["test", "--no-run", "--timings", "-p", "pobr-build", "--test", "parity"],
        ])

    def test_build_defaults_to_application_only(self):
        result = self.run_script("devs/scripts/driver.sh", "build")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["build", "-p", "pobr-cli"]])

    def test_skill_sweep_forwards_options_without_running_cargo_in_driver(self):
        self.write("devs/scripts/skill-sweep.py", "import json, sys; print(json.dumps(sys.argv[1:]))\n")
        result = self.run_script("devs/scripts/driver.sh", "skills:sweep", "--jobs", "4",
                                 "--json", "report with spaces.json")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads(result.stdout), ["--jobs", "4", "--json", "report with spaces.json"])
        self.assertFalse((self.root / "calls.jsonl").exists())

    def test_cache_report_counts_files_without_building_or_following_symlinks(self):
        self.write("target/debug/incremental/session/object.o", "cache")
        self.write("target/debug/deps/test-binary", "binary")
        self.write(".cache/report.json", "local report")
        self.write("outside/secret", "must not be counted")
        (self.root / "target/debug/linked").symlink_to(self.root / "outside", target_is_directory=True)
        result = self.run_script("devs/scripts/driver.sh", "cache")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("debug/incremental", result.stdout)
        self.assertRegex(result.stdout, r"\s2  target\n")
        self.assertNotIn("secret", result.stdout)
        self.assertNotIn("linked", result.stdout)
        self.assertEqual(self.calls(), [["metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"]])
        self.assertEqual((self.root / "outside/secret").read_text(encoding="utf-8"), "must not be counted")
        self.assertTrue((self.root / "target/debug/incremental/session/object.o").is_file())

    def test_cache_report_propagates_metadata_failure(self):
        # Substitute a failing metadata tool, keeping Cargo diagnostics visible.
        self.write("bin/cargo", "#!/bin/sh\necho 'metadata failure' >&2\nexit 17\n")
        result = self.run_script("devs/scripts/driver.sh", "cache")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("metadata failure", result.stderr)

    def test_short_commands_require_explicit_targets(self):
        for args in [["test", "core"], ["lint", "core"], ["timings"],
                     ["verify", "core"], ["verify", "core", "--workspace"],
                     ["verify", "core", "lib", "--workspace"], ["ci"],
                     ["ci", "feature/x", "--unknown"], ["compare"], ["compare", "--refresh"],
                     ["lookup"], ["lookup", "--level", "20"]]:
            with self.subTest(args=args):
                result = self.run_script("devs/scripts/driver.sh", *args)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertFalse((self.root / "calls.jsonl").exists())

    def test_ci_uses_explicit_remote_ref_and_propagates_failure(self):
        self.env["GH_EXIT"] = "31"
        result = self.run_script("devs/scripts/driver.sh", "ci", "feature/ci-workflow")
        self.assertEqual(result.returncode, 31, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["gh", "workflow", "run", "ci.yml", "--ref", "feature/ci-workflow"]])
        self.assertNotIn("CI requested", result.stdout)

    def test_ci_requests_e2e_only_when_asked(self):
        result = self.run_script("devs/scripts/driver.sh", "ci", "feature/ci-workflow", "--e2e")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["gh", "workflow", "run", "ci.yml", "--ref", "feature/ci-workflow",
                                         "-f", "e2e=true"]])

    def calls(self):
        return [json.loads(line) for line in
                (self.root / "calls.jsonl").read_text(encoding="utf-8").splitlines()]

    @unittest.skipUnless((REPO / ".agents/skills/run-pobr/driver.sh").is_file(),
                         "Optional local agent alias is not present in fresh checkouts")
    def test_agent_alias_preserves_arguments_and_failure(self):
        self.env["FAIL_TEST"] = "1"
        args = ["-p", "pobr-build", "--test", "skills", "a filter with spaces"]
        result = self.run_script(".agents/skills/run-pobr/driver.sh", "test", *args)
        self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["test", *args]])

    def test_lint_only_checks_requested_targets(self):
        args = ["-p", "pobr-build", "--lib", "--test", "skills"]
        result = self.run_script("devs/scripts/driver.sh", "lint", *args)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [
            ["fmt", "--all", "--check"], ["clippy", *args, "--", "-D", "warnings"],
        ])

    def test_lint_stops_on_format_failure(self):
        self.env["FAIL_COMMAND"] = "fmt"
        result = self.run_script("devs/scripts/driver.sh", "lint", "-p", "pobr-build")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [["fmt", "--all", "--check"]])

    def test_empty_commands_do_not_trigger_implicit_checks(self):
        for entry in [".claude", ".agents"]:
            if not (self.root / entry / "skills/run-pobr/driver.sh").is_file():
                continue
            for args in [[], ["lint"], ["test"]]:
                with self.subTest(entry=entry, args=args):
                    result = self.run_script(f"{entry}/skills/run-pobr/driver.sh", *args)
                    self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                    self.assertFalse((self.root / "calls.jsonl").exists())

    def test_full_uses_nextest_and_keeps_doctests(self):
        self.env["HAS_NEXTEST"] = "1"
        result = self.run_script("devs/scripts/driver.sh", "full")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [
            ["fmt", "--all", "--check"],
            ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
            ["nextest", "--version"],
            ["nextest", "run", "--workspace"],
            ["test", "--workspace", "--doc"],
            ["run", "-p", "lint-i18n"],
        ])

    def test_full_falls_back_only_when_nextest_is_unavailable(self):
        result = self.run_script("devs/scripts/driver.sh", "full")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls()[2:], [
            ["nextest", "--version"], ["test", "--workspace"], ["run", "-p", "lint-i18n"],
        ])

    def test_full_stops_at_failure_without_retrying_tests(self):
        for command, has_nextest in [("fmt", "1"), ("clippy", "1"),
                                     ("nextest", "1"), ("test", "1"), ("test", "")]:
            with self.subTest(command=command, has_nextest=has_nextest):
                (self.root / "calls.jsonl").unlink(missing_ok=True)
                self.env["FAIL_COMMAND"] = command
                self.env["HAS_NEXTEST"] = has_nextest
                result = self.run_script("devs/scripts/driver.sh", "full")
                self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
                calls = self.calls()
                self.assertEqual(calls[-1][0], command)
                self.assertEqual(sum(call[0] == command and "--version" not in call
                                     for call in calls), 1)


class ModifierWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="pobr modifier workflow ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ["pipeline", "data/test/generated", "bin"]:
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / "pipeline/refresh-modifiers.sh", self.root / "pipeline/refresh-modifiers.sh")
        (self.root / "data/CURRENT").write_text("test", encoding="utf-8")
        self.snapshot = self.root / "data/test/generated/modifier-audit.json"
        self.snapshot.write_text("previous snapshot", encoding="utf-8")
        self.cargo = self.root / "bin/cargo"
        self.cargo.write_text('''#!/usr/bin/env python3
import os, pathlib, sys
args = sys.argv[1:]
if "--check" in args:
    sys.exit(int(os.environ.get("CHECK_EXIT", "0")))
assert "--audit" in args, "Offline audit must not extract or precompile"
assert pathlib.Path(args[args.index("--baseline") + 1]).read_text() == "previous snapshot"
out = pathlib.Path(args[args.index("--audit") + 1])
out.write_text("candidate snapshot", encoding="utf-8")
sys.exit(int(os.environ.get("AUDIT_EXIT", "0")))
''', encoding="utf-8")
        self.cargo.chmod(0o755)

    def run_audit(self, code, check_code=0):
        return subprocess.run(["bash", "pipeline/refresh-modifiers.sh", "--offline"],
            cwd=self.root, env={**os.environ, "PATH": f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}",
                "AUDIT_EXIT": str(code), "CHECK_EXIT": str(check_code)}, capture_output=True, text=True, timeout=15)

    def test_failed_audit_preserves_previous_snapshot_and_exposes_report(self):
        result = self.run_audit(17)
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertEqual(self.snapshot.read_text(encoding="utf-8"), "previous snapshot")
        self.assertEqual((self.root / ".cache/modifier-audit/test/current.json").read_text(encoding="utf-8"), "candidate snapshot")

    def test_invalid_rules_stop_before_audit_and_publication(self):
        result = self.run_audit(0, check_code=19)
        self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
        self.assertEqual(self.snapshot.read_text(encoding="utf-8"), "previous snapshot")
        self.assertFalse((self.root / ".cache/modifier-audit/test/current.json").exists())

    def test_successful_audit_publishes_snapshot_without_extracting(self):
        result = self.run_audit(0)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.snapshot.read_text(encoding="utf-8"), "candidate snapshot")


class VersionPromotionTests(unittest.TestCase):
    def test_updated_dictionary_is_sealed_before_audit_reads_candidate(self):
        spec = importlib.util.spec_from_file_location("snapshot_test", REPO / "pipeline/data_snapshot.py")
        snapshot = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(snapshot)
        with tempfile.TemporaryDirectory(prefix="pobr dictionary manifest ") as directory:
            root = Path(directory)
            version = "1.2.3"
            relative = "i18n/zh-CN/stat_lines.json"
            data = root / "data" / version
            target = data / relative
            target.parent.mkdir(parents=True)
            target.write_text('[{"src":"旧词条","en":"Old line"}]\n', encoding="utf-8")
            (root / "pipeline").mkdir()
            (root / "pipeline/config.json").write_text(json.dumps({"patch": version}), encoding="utf-8")
            (root / "data/CURRENT").write_text(version + "\n", encoding="utf-8")
            snapshot.manifest(data)
            updated = '[{"src":"新词条","en":"New line"}]\n'
            audited = []

            def run(command, cwd, env):
                candidate = Path(env["POBR_DATA_ROOT"]) / version
                if command[0] == "node":
                    (candidate / relative).write_text(updated, encoding="utf-8")
                elif "pipeline/refresh-modifiers.sh" in command:
                    manifest = snapshot.read_json(candidate / "manifest.json")
                    self.assertEqual(manifest["files"][relative], snapshot.digest(candidate / relative))
                    self.assertEqual((candidate / relative).read_text(encoding="utf-8"), updated)
                    audited.append(candidate)

            with patch.object(snapshot, "run", run):
                snapshot.regenerate(root, operation="dictionary")
            self.assertEqual(len(audited), 1)
            self.assertEqual(target.read_text(encoding="utf-8"), updated)
            self.assertEqual(snapshot.read_json(data / "manifest.json")["files"][relative], snapshot.digest(target))

    def test_candidate_checks_gate_promotion_without_editing_rust(self):
        for failure in ["dictionary", "audit", "versions", "gamedata", "parity", None]:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory(prefix="pobr version gate ") as directory:
                root = Path(directory)
                def write(rel, text):
                    destination = root / rel
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_text(text, encoding="utf-8")
                    return destination
                write("pipeline/bump-version.sh", (REPO / "pipeline/bump-version.sh").read_text(encoding="utf-8"))
                write("pipeline/data_snapshot.py", (REPO / "pipeline/data_snapshot.py").read_text(encoding="utf-8"))
                write("pipeline/regen-all.sh", '''#!/usr/bin/env bash
mkdir -p "$POBR_DATA_ROOT/1.2.4/base"
printf '[]\\n' > "$POBR_DATA_ROOT/1.2.4/base/skill_gems.json"
''').chmod(0o755)
                write("pipeline/refresh-modifiers.sh", f"#!/usr/bin/env bash\nexit {17 if failure == 'audit' else 0}\n")
                # Simulate resuming after a different failed candidate.
                write("pipeline/config.json", '{"patch": "1.2.99"}')
                write("pipeline/gem-quality/1.2.4.json", "{}")
                write("pipeline/tree/data.json", "{}")
                write("data/CURRENT", "1.2.3\n")
                rust = 'pub const DATA_VERSION: &str = include_str!("../../../data/CURRENT").trim_ascii();\n'
                write("crates/pobr-data/src/lib.rs", rust)
                write("devs/scripts/driver.sh", f"#!/usr/bin/env bash\nexit {17 if failure == 'versions' else 0}\n")
                write("bin/cargo", f'''#!/usr/bin/env python3
import sys
args = sys.argv[1:]
failure = {failure!r}
sys.exit(17 if (failure == "gamedata" and "pobr-gamedata" in args) or (failure == "parity" and "parity_no_regression" in args) else 0)
''').chmod(0o755)
                (root / "web/node_modules").mkdir(parents=True)
                write("bin/pnpm", "#!/usr/bin/env bash\nexit 0\n").chmod(0o755)
                write("bin/node", f"#!/usr/bin/env bash\nexit {19 if failure == 'dictionary' else 0}\n").chmod(0o755)
                write("bin/curl", '''#!/usr/bin/env python3
import pathlib, sys
pathlib.Path(sys.argv[sys.argv.index("-o") + 1]).write_text("{}", encoding="utf-8")
''').chmod(0o755)
                result = subprocess.run(["bash", "pipeline/bump-version.sh", "--patch", "1.2.4", "--skip-download"],
                    cwd=root, env={**os.environ, "PATH": f"{root / 'bin'}{os.pathsep}{os.environ['PATH']}"},
                    capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode == 0, failure is None, result.stdout + result.stderr)
                self.assertEqual((root / "data/CURRENT").read_text(encoding="utf-8"),
                                 "1.2.3\n" if failure else "1.2.4\n")
                self.assertEqual((root / "crates/pobr-data/src/lib.rs").read_text(encoding="utf-8"), rust)
                self.assertEqual(json.loads((root / "pipeline/config.json").read_text(encoding="utf-8"))["patch"], "1.2.4")
                if failure:
                    expected = {"dictionary": "gen-zh-cn.mjs", "audit": "refresh-modifiers.sh",
                                "versions": "driver.sh", "gamedata": "pobr-gamedata", "parity": "parity_no_regression"}
                    self.assertIn(expected[failure], result.stderr)


class VendorRepositoryIsolationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="pobr vendor isolation ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.real_git = shutil.which("git")
        assert self.real_git is not None
        for rel in ["devs/scripts/driver.sh", "pipeline/bump-version.sh"]:
            self.write(rel, (REPO / rel).read_text(encoding="utf-8"))
        self.write("data/CURRENT", "1.2.3\n")
        self.sha = "a" * 40
        self.write("data/1.2.3/overlay/mod_parser_rules.json",
                   json.dumps({"_meta": {"vendor_commit": self.sha}}))
        self.write("pipeline/config.json", '{"patch": "1.2.3"}\n')
        self.write("vendor/.pob2-version.txt", "previous\n")
        self.write("bin/git", '''#!/usr/bin/env python3
import os, pathlib, subprocess, sys
real = os.environ["REAL_GIT"]
args = sys.argv[1:]
if "fetch" in args:
    vendor = args[args.index("-C") + 1]
    root = subprocess.check_output([real, "-C", vendor, "rev-parse", "--show-toplevel"], text=True).strip()
    pathlib.Path(os.environ["FETCH_ROOTS"]).write_text(root, encoding="utf-8")
    sys.exit(47)
os.execv(real, [real, *args])
''').chmod(0o755)
        self.write("bin/curl", '''#!/usr/bin/env python3
import pathlib, sys
pathlib.Path(sys.argv[sys.argv.index("-o") + 1]).write_text("{}", encoding="utf-8")
''').chmod(0o755)
        subprocess.run([self.real_git, "init", "--quiet"], cwd=self.root, check=True)
        subprocess.run([self.real_git, "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                        "-c", "commit.gpgsign=false", "commit", "--quiet", "--allow-empty", "-m", "parent"],
                       cwd=self.root, check=True)
        subprocess.run([self.real_git, "remote", "add", "origin", "https://example.invalid/parent.git"],
                       cwd=self.root, check=True)
        self.parent_head = subprocess.check_output([self.real_git, "rev-parse", "HEAD"], cwd=self.root, text=True).strip()
        self.env = {**os.environ, "PATH": f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}",
                    "REAL_GIT": self.real_git, "FETCH_ROOTS": str(self.root / "fetch-root")}

    def write(self, rel, text):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def assert_failed_fetch_is_isolated(self, result):
        vendor = self.root / "vendor/PathOfBuilding-PoE2"
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((vendor / ".git").is_dir())
        self.assertEqual(Path((self.root / "fetch-root").read_text(encoding="utf-8")).resolve(),
                         vendor.resolve())
        self.assertEqual((self.root / "vendor/.pob2-version.txt").read_text(encoding="utf-8"), "previous\n")
        self.assertEqual(subprocess.check_output([self.real_git, "rev-parse", "HEAD"],
                                                 cwd=self.root, text=True).strip(), self.parent_head)
        self.assertEqual(subprocess.check_output([self.real_git, "remote", "get-url", "origin"],
                                                 cwd=self.root, text=True).strip(), "https://example.invalid/parent.git")

    def test_driver_vendor_initializes_own_repo_and_propagates_fetch_failure(self):
        result = subprocess.run(["bash", "devs/scripts/driver.sh", "vendor"], cwd=self.root,
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assert_failed_fetch_is_isolated(result)
        self.assertNotIn("OK:", result.stdout)

    def test_bump_vendor_initializes_own_repo_and_does_not_promote_on_fetch_failure(self):
        result = subprocess.run(["bash", "pipeline/bump-version.sh", "--patch", "1.2.4",
                                 "--skip-download", "--vendor-sha", self.sha], cwd=self.root,
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assert_failed_fetch_is_isolated(result)
        self.assertEqual((self.root / "data/CURRENT").read_text(encoding="utf-8"), "1.2.3\n")
        self.assertNotIn("vendor →", result.stdout)

    def test_driver_preserves_existing_vendor_changes(self):
        vendor = self.root / "vendor/PathOfBuilding-PoE2"
        vendor.mkdir()
        subprocess.run([self.real_git, "init", "--quiet"], cwd=vendor, check=True)
        (vendor / "keep.txt").write_text("original\n", encoding="utf-8")
        subprocess.run([self.real_git, "add", "keep.txt"], cwd=vendor, check=True)
        subprocess.run([self.real_git, "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                        "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "vendor"],
                       cwd=vendor, check=True)
        (vendor / "keep.txt").write_text("user edit\n", encoding="utf-8")
        result = subprocess.run(["bash", "devs/scripts/driver.sh", "vendor"], cwd=self.root,
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((vendor / "keep.txt").read_text(encoding="utf-8"), "user edit\n")
        self.assertFalse((self.root / "fetch-root").exists())
        self.assertEqual((self.root / "vendor/.pob2-version.txt").read_text(encoding="utf-8"), "previous\n")


class ReferenceBuildSelectionTests(unittest.TestCase):
    """The selection never depends on a calculation, and never repeats a character."""

    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            "fetch_reference_builds", REPO / "devs/scripts/fetch-reference-builds.py")
        self.fetch = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.fetch)
        self.rows = [{"account": account, "character": name}
                     for account, name in (("a", "one"), ("b", "two"), ("c", "three"))]

    def test_first_and_last_skip_committed_characters(self):
        taken = {("a", "one"), ("c", "three")}
        self.assertEqual(self.fetch.choose(self.rows, set(), "first")["account"], "a")
        self.assertEqual(self.fetch.choose(self.rows, set(), "last")["account"], "c")
        self.assertEqual(self.fetch.choose(self.rows, taken, "first")["account"], "b")
        self.assertEqual(self.fetch.choose(self.rows, taken, "last")["account"], "b")
        self.assertIsNone(self.fetch.choose(self.rows, taken | {("b", "two")}, "first"))

    def test_base_classes_are_not_ascendancies(self):
        self.assertEqual(self.fetch.ascendancies(["Witch", "Lich", "Monk", "Invoker"]),
                         ["Invoker", "Lich"])

    def test_committed_characters_are_read_from_every_set(self):
        with tempfile.TemporaryDirectory() as temp:
            examples = Path(temp)
            for corpus, account in (("one/builds/x", "a"), ("two/nested/builds/y", "b")):
                folder = examples / corpus
                folder.mkdir(parents=True)
                (folder / "meta.json").write_text(json.dumps(
                    {"source": {"account": account, "character": "name"}}), encoding="utf-8")
            (examples / "one/builds/z").mkdir()
            (examples / "one/builds/z/meta.json").write_text("{}", encoding="utf-8")
            self.assertEqual(self.fetch.committed_characters(examples),
                             {("a", "name"), ("b", "name")})


def breakdown_line(mod_type, value, kind=None, ident=None, text=None, slot=None, active=True):
    """One engine breakdown line (`BreakdownMod`)."""
    return {"mod_type": mod_type, "value": value, "active": active, "source_text": text,
            "origin_kind": kind, "origin_id": ident, "slot": slot}


def oracle_record(name, mod_type, value, source, tags=None):
    """One line of the PoB2 oracle's tabulated lists."""
    record = {"queryName": name, "name": name, "type": mod_type, "rawValue": value,
              "evalValue": value, "source": source}
    if tags:
        record["tags"] = tags
    return record


class CompareBuildTests(unittest.TestCase):
    """The engine's and PoB2's lines meet on one key per source, and only what
    differs between them is reported."""

    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            "compare_build", REPO / "devs/scripts/compare-build.py")
        self.compare = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.compare)

    def test_action_rate_uses_hit_speed_when_present(self):
        entry = {"stat": "ActionRate", "value": 4.0, "pob_key": "Speed"}
        entries = {"ActionRate": entry}
        for hit_speed in (4.0, 0.0):
            entry["value"] = hit_speed
            output = {"Speed": 1.5, "HitSpeed": hit_speed}
            self.assertEqual(self.compare.disagreeing_stats(entries, output), [])
            self.assertIn("(HitSpeed)", self.compare.stat_header("ActionRate", entry, output))
            self.assertTrue(any(row.endswith("HitSpeed")
                                for row in self.compare.factor_rows(entries, output)))
            self.assertEqual(self.compare.reference_value({"mainOutput": output}, "Speed"), hit_speed)
        entry["value"] = 1.5
        for output in ({"Speed": 1.5}, {"Speed": 1.5, "HitSpeed": None}):
            self.assertEqual(self.compare.disagreeing_stats(entries, output), [])
            self.assertIn("(Speed)", self.compare.stat_header("ActionRate", entry, output))

    def test_selected_minion_totals_appear_in_comparison(self):
        entry = {"stat": "Minion:TotalDPS", "value": 200, "pob_key": "Minion:TotalDPS"}
        oracle = {"mainOutput": {"TotalDPS": 0}, "minionOutput": {"TotalDPS": 210}}
        report = {"breakdowns": [entry]}
        text = self.compare.render("minion", report, oracle, "<PathOfBuilding2/>",
                                   ["Minion:TotalDPS"], "asked")
        self.assertIn("PoB2 210 (Minion:TotalDPS)", text)
        self.assertEqual(self.compare.reference_value(oracle, "Minion:TotalDPS"), 210)

    def test_sources_of_both_sides_meet_on_one_key(self):
        slots = {"23": "item:gloves", "13": "flask:charm1"}
        pairs = [
            ("Tree:12345", {"origin_kind": "PassiveNode", "origin_id": "node.12345"}),
            ("Tree:777", {"origin_kind": "AscendancyNode", "origin_id": "node.777"}),
            ("Item:23:Kraken Grasp, Sirenscale Gloves",
             {"origin_kind": "ItemAffix", "origin_id": "item.gloves.explicit", "slot": "gloves"}),
            ("Item:23:Kraken Grasp, Sirenscale Gloves",
             {"origin_kind": "ItemImplicit", "origin_id": "item.gloves.implicit"}),
            ("Item:13:Thawing Charm", {"origin_kind": "Flask", "origin_id": "flask.charm1"}),
            ("Skill:IceShotPlayer",
             {"origin_kind": "SkillGem", "origin_id": "skill.active_skill_chill_effect_+%_final"}),
            ("Skill:IceShotPlayer",
             {"origin_kind": "SkillGem", "origin_id": "skill.IceShotPlayer.set2.base_number_of_projectiles"}),
            ("Skill:SupportFooPlayer",
             {"origin_kind": "SupportGem", "origin_id": "SupportFooPlayer.support_foo_damage_+%_final"}),
            ("Skill:SupportFooPlayer",
             {"origin_kind": "SupportGem", "origin_id": "support.SupportFooPlayer.manaMultiplier"}),
            ("Skill:ArchmagePlayer", {"origin_kind": "Buff", "origin_id": "aura.ArchmagePlayer"}),
            ("Strength", {"origin_kind": "CharacterBase", "origin_id": "base.attr_derived"}),
            ("Base", {"origin_kind": "GameConstant", "origin_id": "game.constant"}),
            ("Quest:Kill the boss: Act 1", {"origin_kind": "CampaignReward", "origin_id": "campaign.life"}),
            ("Config", {"origin_kind": "ConfigOption", "origin_id": "config.conditionMoving"}),
        ]
        for pob2, pobr in pairs:
            with self.subTest(pob2=pob2, pobr=pobr):
                self.assertEqual(self.compare.pob2_source_key(pob2, slots),
                                 self.compare.pobr_source_key(pobr, "IceShotPlayer"))
        # The jewels meet as one source: PoB2 names them, the engine numbers them.
        self.assertEqual(self.compare.pob2_source_key("Item:2:Prism of Belief, Diamond", slots), "jewel")
        self.assertEqual(self.compare.pobr_source_key(
            {"origin_kind": "Jewel", "origin_id": "jewel.2"}, "IceShotPlayer"), "jewel")
        self.assertEqual(self.compare.pobr_source_key(
            {"origin_kind": "Config", "origin_id": "config.custom"}, "IceShotPlayer"),
            self.compare.pob2_source_key("Custom", slots))
        # A line without an origin is keyed by its text.
        self.assertEqual(self.compare.pobr_source_key({"source_text": "+8 to Strength"}, "IceShotPlayer"),
                         "text:+8 to Strength")

    def test_items_are_keyed_by_their_slot_in_the_active_set(self):
        xml = ('<PathOfBuilding2><Items activeItemSet="2">'
               '<ItemSet id="1"><Slot name="Gloves" itemId="5"/></ItemSet>'
               '<ItemSet id="2" useSecondWeaponSet="true">'
               '<Slot name="Weapon 1 Swap" itemId="2"/><Slot name="Body Armour" itemId="3"/>'
               '<Slot name="Ring 2" itemId="0"/><Slot name="Gloves Jewel Socket 1" itemId="4"/>'
               '<Slot name="Charm 2" itemId="6"/><Slot name="Arm 1" itemId="7"/>'
               '</ItemSet></Items></PathOfBuilding2>')
        self.assertEqual(self.compare.item_slot_keys(xml), {
            "2": "item:weapon1", "3": "item:bodyarmour", "6": "flask:charm2", "7": "item:arm1"})

    def test_lines_are_compared_by_type_and_source(self):
        line = self.compare.Line
        pobr = [
            line("BASE", "tree:1", 100.0, "a"),
            line("INC", "item:gloves", 10.0, "b"),
            line("INC", "text:8% increased maximum Life", 8.0, "c"),
            line("INC", "tree:9", 7.0, "d", active=False),
            line("MORE", "skill:S", 10.0, "e"),
            line("MORE", "skill:S", 10.0, "f"),
            line("INC", "tree:5", 3.0, "g"),
        ]
        pob2 = [
            line("BASE", "tree:1", 100.0, "A"),
            line("INC", "item:gloves", 15.0, "B"),
            line("INC", "custom", 8.0, "C"),
            line("INC", "tree:9", 7.0, "D"),
            # Two 10% more lines are 21% more.
            line("MORE", "skill:S", 21.0, "E"),
        ]
        result = self.compare.compare(pobr, pob2)
        self.assertEqual([(group.mod_type, group.key) for group in result.only_pob2], [("INC", "tree:9")])
        # The engine has that line and does not count it.
        self.assertEqual([found.label for found in result.inactive[("INC", "tree:9")]], ["d"])
        self.assertEqual([(group.mod_type, group.key) for group in result.only_pobr], [("INC", "tree:5")])
        self.assertEqual([(ours.key, ours.value, theirs.value) for ours, theirs in result.different],
                         [("item:gloves", 10.0, 15.0)])
        self.assertEqual([(ours.key, theirs.key) for ours, theirs in result.renamed],
                         [("text:8% increased maximum Life", "custom")])
        totals = self.compare.sums(pobr)
        self.assertEqual((totals["BASE"], totals["INC"]), (100.0, 21.0))
        self.assertAlmostEqual(totals["MORE"], 1.21)

    def test_build_codes_decode_with_or_without_padding(self):
        xml = '<PathOfBuilding2><Build level="90" className="Ranger"/></PathOfBuilding2>'
        code = base64.urlsafe_b64encode(zlib.compress(xml.encode("utf-8"))).decode("ascii")
        for variant in (code, code.rstrip("="), code[:10] + "\n" + code[10:],
                        code.replace("-", "+").replace("_", "/")):
            with self.subTest(variant=variant):
                self.assertEqual(self.compare.decode_code(variant), xml)

    def report(self, folder, *arguments):
        """The comparison of the build in `folder` with its two saved outputs."""
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = self.compare.main([str(folder), *arguments,
                                        "--engine-json", str(folder / "engine.json"),
                                        "--oracle-json", str(folder / "oracle.json")])
        self.assertEqual(status, 0)
        return output.getvalue()

    def test_a_build_is_compared_stat_by_stat_from_both_outputs(self):
        xml = ('<PathOfBuilding2><Build level="99"/><Items activeItemSet="1">'
               '<ItemSet id="1"><Slot name="Gloves" itemId="23"/></ItemSet></Items></PathOfBuilding2>')
        engine = {
            "data_version": "test",
            "build": {"class_name": "Huntress", "ascendancy_name": "Spirit Walker", "level": 99},
            "unsupported_modifiers": [], "missing_items": [],
            "reference": {"tolerance": 0.05, "compared": 10, "agreeing": 8, "differing": [
                {"stat": "Life", "pob_key": "Life", "pobr": 4000, "reference": 4500, "ratio": 0.889},
                {"stat": "TotalDPS", "pob_key": "TotalDPS", "pobr": 1000, "reference": 1200,
                 "ratio": 0.833}]},
            "main_skill": {"skill_id": "IceShotPlayer"},
            "breakdowns": [
                {"stat": "Life", "value": 4000, "pob_key": "Life", "reference": 4500, "breakdown": {
                    "base_total": 300, "inc_total": 60, "more_total": 1.0, "formula": None, "mods": [
                        breakdown_line("BASE", 100, "PassiveNode", "node.12345", "+100 to maximum Life"),
                        breakdown_line("INC", 10, "ItemAffix", "item.gloves.explicit",
                                       "10% increased maximum Life", "gloves"),
                        breakdown_line("INC", 50, text="50% increased maximum Life"),
                        breakdown_line("INC", 7, "PassiveNode", "node.999",
                                       "7% increased maximum Life while X", active=False),
                    ]}},
                {"stat": "TotalDPS", "value": 1000, "pob_key": "TotalDPS", "reference": 1200,
                 "breakdown": None},
                {"stat": "TotalHitAvg", "value": 500, "pob_key": "AverageHit", "breakdown": {
                    "base_total": 0, "inc_total": 0, "more_total": 1, "mods": [], "formula": None,
                    "damage": {"sources": [], "damage_effect": 1.0, "shared_mods": [
                        breakdown_line("INC", 120, "PassiveNode", "node.2", "120% increased Damage")],
                        "rows": [{"damage_type": "Cold", "base_min": 10, "base_max": 20, "increased": 150,
                                  "more": 1.0, "min": 30, "max": 60, "avg": 45, "mods": [
                                      breakdown_line("INC", 30, "PassiveNode", "node.1",
                                                     "30% increased Cold Damage")]}]}}},
                {"stat": "ActionRate", "value": 2.5, "pob_key": "Speed", "breakdown": None},
            ]}
        oracle = {
            "mainOutput": {"Life": 4400, "TotalDPS": 1150, "AverageHit": 560, "Speed": 2.5},
            "skillInfo": {"grantedEffectId": "IceShotPlayer", "activeGemName": "Ice Shot"},
            "components": {"Cold": {"Min": 33, "Max": 66, "HitAverage": 50}},
            "extraModList": {
                "BASE": {"Life": [oracle_record("Life", "BASE", 100, "Tree:12345")]},
                "INC": {"Life": [oracle_record("Life", "INC", 15, "Item:23:Kraken Grasp, Sirenscale Gloves"),
                                 oracle_record("Life", "INC", 50, "Item:2:Prism of Belief, Diamond"),
                                 oracle_record("Life", "INC", 7, "Tree:999", ["Condition:X"])]},
                "MORE": {}, "OVERRIDE": {}},
            "damageModList": {"INC": {
                "Damage": [oracle_record("Damage", "INC", 120, "Tree:2")],
                "ColdDamage": [oracle_record("ColdDamage", "INC", 30, "Tree:1")],
                "ElementalDamage": [oracle_record("ElementalDamage", "INC", 25, "Tree:3")]},
                "MORE": {}},
            "critModList": {"INC": {}, "MORE": {}, "BASE": {}},
        }
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp) / "build"
            folder.mkdir()
            (folder / "code.txt").write_text(base64.urlsafe_b64encode(
                zlib.compress(xml.encode("utf-8"))).decode("ascii"), encoding="utf-8")
            (folder / "engine.json").write_text(json.dumps(engine), encoding="utf-8")
            (folder / "oracle.json").write_text(json.dumps(oracle), encoding="utf-8")

            # Without stats: the reference rows that differ.
            text = self.report(folder)
            self.assertIn("Reference rows that differ (on the reference's basis, tolerance 5%): 2 of 10",
                          text)
            self.assertIn("== Life: PoBR 4,000 · PoB2 4,400 (Life) · PoBR/PoB2 0.909 · "
                          "code reference 4,500", text)
            self.assertIn("item:gloves: PoBR +10% · PoB2 +15%", text)
            self.assertIn("PoBR has it, inactive: +7%  7% increased maximum Life while X", text)
            self.assertIn("PoBR text:50% increased maximum Life · PoB2 jewel", text)
            self.assertIn("-- Cold: PoBR 30–60 (average 45; base 10–20) · PoB2 33–66 (average 50)", text)
            self.assertIn("+25%  tree:3  Tree:3  [ElementalDamage]", text)

            # A PoB2 output name asks for the stat it names.
            text = self.report(folder, "Speed")
            self.assertIn("== ActionRate: PoBR 2.5 · PoB2 2.5 (Speed)", text)
            self.assertNotIn("Reference rows", text)

            # Without reference rows: the panel stats where the two disagree.
            del engine["reference"]
            (folder / "engine.json").write_text(json.dumps(engine), encoding="utf-8")
            self.assertIn("comparing the panel stats where PoBR and PoB2 disagree by more than 5%: "
                          "Life, TotalDPS, TotalHitAvg", self.report(folder))


LOOKUP_DATA = {
    "base/granted_effects.json": [
        {"id": "IceShotPlayer", "is_support": False, "active_skill": "ice_shot", "cast_time": 1000,
         "additional_stat_set_ids": ["IceShotShardPlayer"], "cost_types": [0],
         "skill_types": ["Attack", "Projectile", "Cold"]}],
    "base/granted_effect_levels.json": {"IceShotPlayer": [
        {"level": 1, "cost_amounts": [6], "attack_speed_multiplier": -10.0},
        {"level": 20, "cost_amounts": [31], "attack_speed_multiplier": -10.0}]},
    "base/cost_types.json": [{"id": "Mana", "divisor": 1}],
    "base/granted_effect_stat_sets.json": [{"effect_id": "IceShotPlayer", "sets": [{
        "set_id": "IceShotPlayer", "base_effectiveness": 1.0,
        "constant_stats": [{"stat": "active_skill_chill_effect_+%_final", "value": 50.0}],
        "implicit_stats": ["base_is_projectile"],
        "levels": [{"gem_level": 1, "damage_multiplier": 0.95, "stats": []},
                   {"gem_level": 20, "damage_multiplier": 1.3599999999999999,
                    "stats": [{"stat": "base_number_of_projectiles", "value": 1.0}]}]}]}],
    "base/skill_gems.json": [
        {"id": "Metadata/Items/Gem/SkillGemIceShot", "gem_effect_id": "IceShot",
         "granted_effect_id": "IceShotPlayer", "gem_tags": ["attack", "projectile", "cold"]}],
    "base/stats.json": [{"id": "base_number_of_projectiles", "is_local": False},
                        {"id": "support_rigwald_damage_+%_final_in_weapon_set_two", "is_local": False}],
    "base/base_items.json": [
        {"id": "Metadata/Items/Rings/Ring2", "name": "Ruby Ring", "item_class": "Ring", "drop_level": 1,
         "width": 1, "height": 1, "tags": ["ring"], "implicits": ["FireResistImplicitRing1"]},
        {"id": "Metadata/Items/SoulCores/TalismanFox", "name": "Fox Idol", "item_class": "SoulCore",
         "drop_level": 65, "width": 1, "height": 1, "tags": ["idol"], "implicits": []}],
    "base/mods.json": [{"id": "FireResistImplicitRing1",
                        "stats": [{"stat_id": "base_fire_damage_resistance_%", "min": 20, "max": 30}]}],
    "base/passive_tree.json": [
        {"skill": 1143, "id": "spell_criticals7", "name": "Spell Critical Damage", "kind": "normal",
         "stats": ["15% increased [CriticalDamageBonus|Critical Spell Damage Bonus]"],
         "connections": [50084],
         "variants": [{"class": "Druid", "variant_skill": 57726, "name": "Life Regeneration",
                       "stats": ["Regenerate 0.2% of maximum Life per second"]}]}],
    "overlay/uniques.json": {"uniques": [
        {"name": "Astramentis", "base": "Stellar Amulet", "item_type": "amulet",
         "raw": "Astramentis\nStellar Amulet\nImplicits: 1\n{tags:attribute}+(5-7) to all Attributes\n"
                "{variant:2}{tags:attribute}+(50-100) to all Attributes\n"}]},
    "overlay/runes.json": {"runes": [{"name": "Fox Idol", "slots": {"body armour": {
        "kind": "Idol", "lines": ["Idols socketed in this item gain the benefits of their Bonded modifiers",
                                  "Bonded: +5% to Quality of all Skills"]}}}]},
    "overlay/stat_descriptions.json": {"scopes": {"stat_descriptions": {
        "single": {"base_number_of_projectiles": ["1 additional\nProjectile"]}}}},
    "overlay/skill_stat_map.json": {
        "global": {"base_number_of_projectiles": {"mods": [
            {"kind": "mod", "name": "ProjectileCount", "mod_type": "BASE"}]}},
        "per_stat_set": {"IceShotPlayer": {"1": {"base_number_of_projectiles": {"mods": []}}}}},
}


class TriageTests(unittest.TestCase):
    """`mise run triage` files each reference row that differs by what the
    pinned PoB2 says today, and gathers what explains the engine's rows."""

    def setUp(self):
        spec = importlib.util.spec_from_file_location("triage", REPO / "devs/scripts/triage.py")
        self.triage = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.triage)

    def test_beyond_pob2_aggregates_rule_impact_without_conflating_agreement(self):
        results = []
        for changed in [2, 0, 3]:
            result = self.triage.analyse("build", "set", {
                "reference": {"compared": 10, "differing": []},
                "beyond_pob2": [
                    {"rule_id": "absent_rule", "exporter": "absent", "changed_rows": changed},
                    {"rule_id": "different_rule", "exporter": "differs", "changed_rows": 0},
                ],
            }, {}, "<PathOfBuilding/>", [])
            results.append(result)
        text = self.triage.render(results, [], "test", 3)
        self.assertIn("absent_rule [absent]: 3 present builds; 2 changed builds, 5 changed rows", text)
        self.assertIn("different_rule [differs]: 3 present builds; 0 changed builds, 0 changed rows", text)

    def test_rows_are_filed_by_what_pob2_says_today(self):
        classify = self.triage.classify
        self.assertEqual(classify(100.0, 120.0, 101.0), "reference")
        self.assertEqual(classify(100.0, 120.0, 119.0), "engine")
        self.assertEqual(classify(100.0, 120.0, 150.0), "both")
        self.assertEqual(classify(100.0, 120.0, None), "no oracle")

    def test_known_differences_name_a_build_and_its_rows(self):
        known = [{"build": "deadeye-ice-shot", "stats": ["TotalDPS"], "reason": "Rhoa"}]
        name = "ninja-2026-10-11-ranger-deadeye-ice-shot-0123456789ab"
        self.assertEqual(self.triage.known_reason(known, name, "TotalDPS"), "Rhoa")
        self.assertIsNone(self.triage.known_reason(known, name, "Life"))
        self.assertEqual(self.triage.short(name), "ranger-deadeye-ice-shot")
        for entry in self.triage.load_known():
            with self.subTest(build=entry.get("build")):
                self.assertTrue(entry["build"] and entry["stats"] and entry["reason"])

    def test_lines_meet_across_rolls_and_places(self):
        template = self.triage.template
        self.assertEqual(template("Tree:5332: 22% Chance to gain a Charge"),
                         template("19% Chance to gain a Charge"))
        self.assertEqual(template("Config: +1.5% to all Resistances"), "#% to all Resistances")

    def test_the_holdout_set_is_not_triaged(self):
        with self.assertRaises(SystemExit) as raised:
            self.triage.build_dirs(["examples/reference-builds/holdout/builds"])
        self.assertIn("mise run accuracy", str(raised.exception))

    def test_skills_meet_by_id_and_pob2_name(self):
        key = self.triage.skill_key
        self.assertEqual(key("SummonSkeletalReaversPlayer"), key("Skeletal Reaver Minion"))
        self.assertEqual(key("SummonSpectrePlayer"), key("Spectre: Quill Crab"))
        self.assertEqual(key("SummonBeastPlayer"), key("Companion: Quadrilla"))

    def test_the_ledger_of_lines_is_gathered_across_builds(self):
        usage = lambda reads, lines, conditions=(): {
            "reads": {name: [] for name in reads}, "lines": lines,
            "conditions": list(conditions), "gem_stats": []}
        line = lambda status, text, *names: {
            "status": status, "text": text, "source_id": "item.ring1.explicit",
            "holder": "Ruby Ring", "reason": "rule r", "modifiers": [
                {"name": name, "status": "unread"} for name in names]}
        first = self.triage.ledger_findings(usage(
            ["Damage"],
            [line("unread", "16% increased Rarity of Items found", "LootRarity"),
             line("unread", "10% increased Spell Damage", "SpellDamage"),
             line("unmodelled", "60% increased Effect of Suffixes"),
             line("no_effect", "Config", "Condition:TargetingBrandedEnemy")],
            [{"condition": "OnConsecratedGround", "config_option": None,
              "modifiers": [{"source": "PassiveNode:node.1 | line"}]},
             {"condition": "LowLife", "config_option": "conditionLowLife", "modifiers": []}]))
        second = self.triage.ledger_findings(usage(
            ["SpellDamage"], [line("unmodelled", "58% increased Effect of Suffixes")]))
        self.assertEqual(first["unmodelled"][0]["source"], "item.ring1.explicit (Ruby Ring)")
        results = [{"short": "a", "rows": [{"class": "engine"}], "ledger": first},
                   {"short": "b", "rows": [], "ledger": second}]
        unmodelled = self.triage.gather_lines(results, "unmodelled")
        self.assertEqual(len(unmodelled), 1)
        self.assertEqual(unmodelled[0][1]["builds"], {"a", "b"})
        self.assertEqual(unmodelled[0][1]["actionable"], {"a"})
        # A name no build reads comes before one another build reads.
        self.assertEqual([(name, entry["read_elsewhere"])
                          for name, entry in self.triage.gather_unread(results)],
                         [("LootRarity", False), ("SpellDamage", True)])
        # Only the conditions nothing sets: one a config option sets is the user's choice.
        self.assertEqual([name for name, _ in self.triage.gather_conditions(results)],
                         ["OnConsecratedGround"])
        self.assertEqual(self.triage.ledger_findings(None)["unread"], [])

    def test_spirit_is_compared_skill_by_skill(self):
        reserved = lambda *rows: {"breakdown": {"mods": [
            {"value": value, "origin_kind": "SkillGem", "origin_id": "spirit." + skill}
            for skill, value in rows]}}
        entries = {"Spirit": {"value": 100.0},
                   "SkillSpiritReservationBase": reserved(("SummonSkeletalReaversPlayer", 30.0),
                                                          ("BlinkPlayer", 60.0))}
        oracle = {"mainOutput": {"Spirit": 130.0}, "spiritReservedBreakdown": [
            {"skillName": "Skeletal Reaver Minion", "total": 25},
            {"skillName": "Blink", "total": 60},
            {"skillName": "Trinity", "total": 100}]}
        self.assertEqual(
            [(found["kind"], found["key"], found["pobr"], found["pob2"])
             for found in self.triage.spirit_leads(entries, oracle)],
            [("total", "Spirit", 100.0, 130.0),
             ("different", "Skeletal Reaver Minion", 30.0, 25.0),
             ("only PoB2", "Trinity", None, 100.0)])

        # PoB2 also reserves the skill a reservation-free amulet grants: known.
        xml = ("<PathOfBuilding2><Items><Item id=\"1\">Rarity: UNIQUE\nVoid\nLament Amulet\n"
               "Grants Skill: Level 20 Cast on Minion Death\n</Item></Items></PathOfBuilding2>")
        report = {"reference": {"compared": 1, "differing": [
            {"stat": "SpiritUnreserved", "pobr": -26, "reference": -56, "pob_key": "SpiritUnreserved"},
            {"stat": "SpiritUnreservedPercent", "pobr": -26, "reference": -56,
             "pob_key": "SpiritUnreservedPercent"}]},
            "breakdowns": [{"stat": "SkillSpiritReservationBase",
                            **reserved(("MetaCastOnMinionDeathPlayer", 30.0))}]}
        oracle = {"mainOutput": {"SpiritUnreserved": -56, "SpiritUnreservedPercent": -56},
                  "spiritReservedBreakdown": [{"skillName": "Cast on Minion Death", "total": 60}]}
        result = self.triage.analyse("build", "set", report, oracle, xml, [])
        self.assertEqual([(row["stat"], row["class"]) for row in result["rows"]],
                         [("SpiritUnreserved", "known"), ("SpiritUnreservedPercent", "known")])

    def test_spirit_matches_the_granted_effects_game_display_name(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / "data/test/base"
            base.mkdir(parents=True)
            (root / "data/CURRENT").write_text("test", encoding="utf-8")
            # Neither the effect id nor the gem's metadata name is a display name.
            (base / "skill_gems.json").write_text(json.dumps([
                {"id": "Metadata/Items/Gems/SkillGemInternalTrigger",
                 "granted_effect_id": "MetaCastOnCritPlayer"}]), encoding="utf-8")
            (base / "granted_effects.json").write_text(json.dumps([
                {"id": "MetaCastOnCritPlayer", "displayed_name": "Cast on Critical"}]),
                encoding="utf-8")
            entries = {"SkillSpiritReservationBase": {"breakdown": {"mods": [
                {"origin_id": "spirit.MetaCastOnCritPlayer", "value": 91}]}}}
            oracle = {"spiritReservedBreakdown": [
                {"skillName": "Cast on Critical", "total": 85}]}
            with patch.object(self.triage, "REPO", root):
                found = self.triage.spirit_leads(entries, oracle)
            self.assertEqual([(row["kind"], row["key"], row["pobr"], row["pob2"])
                              for row in found],
                             [("different", "Cast on Critical", 91, 85)])

    def test_leads_are_gathered_across_builds(self):
        lead = self.triage.lead
        results = [
            {"short": "a", "rows": [], "unsupported": ["Tree:1: 10% increased Damage"], "leads": [
                dict(lead("Life", "only PoB2", "INC", "tree:1", pob2=5.0), stat="Life")]},
            {"short": "b", "rows": [{"class": "engine"}], "unsupported": ["12% increased Damage"],
             "leads": [dict(lead("Life", "only PoB2", "INC", "tree:1", pob2=5.0), stat="Life"),
                       dict(lead("Mana", "only PoBR", "BASE", "item:ring1", pobr=9.0), stat="Mana")]},
        ]
        gathered = self.triage.gather_leads(results)
        self.assertEqual([(key, sorted(entry["builds"])) for key, entry in gathered],
                         [(("only PoB2", "INC", "tree:1"), ["a", "b"]),
                          (("only PoBR", "BASE", "item:ring1"), ["b"])])
        [(template, entry)] = self.triage.gather_unsupported(results)
        self.assertEqual((template, sorted(entry["builds"]), sorted(entry["actionable"])),
                         ("#% increased Damage", ["a", "b"], ["b"]))


class LookupTests(unittest.TestCase):
    """`mise run lookup` prints the records an id or a name has in the data the
    engine reads, and the names that contain it when nothing is exact."""

    def setUp(self):
        spec = importlib.util.spec_from_file_location("lookup", REPO / "devs/scripts/lookup.py")
        self.lookup = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.lookup)
        temp = tempfile.TemporaryDirectory(prefix="pobr lookup tests ")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "CURRENT").write_text("test\n", encoding="utf-8")
        for relative, content in LOOKUP_DATA.items():
            path = self.root / "test" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(content), encoding="utf-8")
        # The engine's own stat meanings, with the stat on line 2.
        self.skill_stats = self.root / "skill_stats.rs"
        self.skill_stats.write_text(
            'fn rules() {\n    "support_rigwald_damage_+%_final_in_weapon_set_two" => more(),\n}\n',
            encoding="utf-8")

    def lookup_output(self, *arguments, version=None):
        output = io.StringIO()
        with patch.dict(os.environ, {"POBR_DATA_ROOT": str(self.root)}), \
                patch.object(self.lookup, "SKILL_STATS_RS", self.skill_stats), \
                contextlib.redirect_stdout(output):
            os.environ.pop("POBR_DATA_VERSION", None)
            if version:
                os.environ["POBR_DATA_VERSION"] = version
            status = self.lookup.main(list(arguments))
        return status, output.getvalue()

    def test_a_skill_is_found_by_its_effect_or_gem_id(self):
        for query in ("IceShotPlayer", "iceshot", "SkillGemIceShot"):
            with self.subTest(query=query):
                status, text = self.lookup_output(query)
                self.assertEqual(status, 0)
                self.assertTrue(text.startswith("data test\n"), text)
                self.assertIn("== granted effect IceShotPlayer", text)
                self.assertIn("gem Metadata/Items/Gem/SkillGemIceShot · tags attack, projectile, cold", text)
                self.assertIn("constant: active_skill_chill_effect_+%_final = 50", text)
                self.assertIn("implicit: base_is_projectile", text)
                self.assertIn("per level: base_number_of_projectiles", text)
        status, text = self.lookup_output("IceShotPlayer", "--level", "20")
        self.assertIn("level 20: cost 31 Mana, attack_speed_multiplier -10", text)
        self.assertIn("level 20: damage multiplier 1.36; base_number_of_projectiles = 1", text)
        self.assertIn("level 30: no row", self.lookup_output("IceShotPlayer", "--level", "30")[1])

    def test_a_stat_shows_its_description_meaning_and_carriers(self):
        status, text = self.lookup_output("base_number_of_projectiles")
        self.assertEqual(status, 0)
        self.assertIn("== stat base_number_of_projectiles", text)
        self.assertIn("description: stat_descriptions: 1 additional Projectile", text)
        self.assertIn('stat map, global: [{"kind": "mod", "name": "ProjectileCount"', text)
        self.assertIn("stat map, per stat set: IceShotPlayer/1", text)
        self.assertIn("in stat sets: IceShotPlayer", text)
        status, text = self.lookup_output("support_rigwald_damage_+%_final_in_weapon_set_two")
        self.assertIn("description: none in overlay/stat_descriptions.json", text)
        self.assertIn(f"engine meaning: {self.skill_stats}:2", text)

    def test_a_base_item_shows_its_implicits_and_what_it_grants_socketed(self):
        text = self.lookup_output("Ruby Ring")[1]
        self.assertIn("== base item Ruby Ring (Metadata/Items/Rings/Ring2)", text)
        self.assertIn("implicit FireResistImplicitRing1: base_fire_damage_resistance_% 20–30", text)
        text = self.lookup_output("fox idol")[1]
        self.assertIn("implicits: none", text)
        self.assertIn("socketed in body armour (Idol): Idols socketed in this item gain the benefits of "
                      "their Bonded modifiers / Bonded: +5% to Quality of all Skills", text)

    def test_a_passive_node_is_found_by_number_name_or_variant(self):
        for query in ("1143", "spell critical damage", "spell_criticals7", "57726"):
            with self.subTest(query=query):
                status, text = self.lookup_output(query)
                self.assertEqual(status, 0)
                self.assertIn("== passive node 1143 Spell Critical Damage (spell_criticals7, normal)", text)
                self.assertIn("   15% increased Critical Spell Damage Bonus\n", text)
                self.assertIn("as Druid (57726): Life Regeneration — Regenerate 0.2% of maximum Life "
                              "per second", text)

    def test_a_unique_shows_its_item_text(self):
        text = self.lookup_output("astramentis")[1]
        self.assertIn("== unique Astramentis (Stellar Amulet, amulet)", text)
        self.assertIn("   +(5-7) to all Attributes\n", text)
        self.assertIn("   {variant:2}+(50-100) to all Attributes\n", text)

    def test_nothing_exact_lists_the_names_that_contain_the_query(self):
        status, text = self.lookup_output("shot")
        self.assertEqual(status, 1)
        self.assertIn("the names that contain it (1)", text)
        self.assertIn("granted effect IceShotPlayer", text)
        status, text = self.lookup_output("nothing like it")
        self.assertEqual(status, 1)
        self.assertIn("nothing is named 'nothing like it'", text)

    def test_the_data_version_is_the_engines(self):
        self.assertTrue(self.lookup_output("Ruby Ring", version="test")[1].startswith("data test\n"))
        with self.assertRaises(SystemExit):
            self.lookup_output("Ruby Ring", version="missing")
        self.assertEqual(self.lookup_output("Ruby Ring", "--version", "test")[0], 0)


class GapsTests(unittest.TestCase):
    """Small game-text fixtures exercise the queue and research joins without Cargo."""

    def setUp(self):
        spec = importlib.util.spec_from_file_location("gaps", REPO / "devs/scripts/gaps.py")
        self.gaps = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.gaps)
        temp = tempfile.TemporaryDirectory(prefix="pobr gaps tests ")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.directory = self.root / "data/test"
        self.directory.mkdir(parents=True)
        self.data = self.gaps.lookup.Data(self.directory)

    @staticmethod
    def entry(sources, status="unsupported", vendor="unsupported"):
        return {"sources": sources, "status": status, "vendor_status": vendor, "mod_names": []}

    @staticmethod
    def build(name, *lines):
        return {"build": name, "set": "worked", "path": f"worked/{name}", "usage": {"lines": list(lines)}}

    def test_templates_group_rolls_markup_and_placeholders(self):
        template = self.gaps.template
        expected = "#% increased Effect of Charms"
        for text in ("10% increased Effect of [Charms]", "Tree:42: 20% increased Effect of Charms",
                     "(10-20)% increased Effect of Charms", "{0}% increased Effect of Charms"):
            self.assertEqual(template(text), expected)
        self.assertEqual(template("+{0:+d} to [Life|maximum Life]"), "# to maximum Life")
        self.assertEqual(template("+# to maximum Life"), "# to maximum Life")
        self.assertEqual(self.gaps.words("CharmEffect") & self.gaps.words("Effect of Charms"), {"charm", "effect"})

    def test_ranking_counts_builds_then_player_sources_then_source_count(self):
        entry = self.entry
        audit = {"entries": {
            "10% increased Effect of Charms": entry(["unique:Charm", "translation:charm"]),
            "20% increased Effect of Charms": entry(["unique:Other Charm"], vendor="parsed"),
            "17% increased Effect of Charms": entry(["translation:other charm"]),
            "Many sources": entry(["passive:1", "passive:2", "mods:equipment:x"]),
            "One unique": entry(["unique:One"]),
            "Two uniques": entry(["unique:Two", "unique:Three"]),
            "Only translation": entry(["translation:ignore"]),
            "Parsed": entry(["passive:3"], "parsed", "parsed"),
            "Recognised": entry(["passive:4"], "recognized_empty", "parsed"),
            "Cosmetic": entry(["unique:Cosmetic"], "no_effect"),
        }}
        line = lambda text, fate: {"text": text, "status": fate}
        builds = [self.build("a", line("30% increased Effect of Charms", "unparsed"),
                             line("30% increased Effect of Charms", "unparsed")),
                  self.build("b", line("40% increased Effect of Charms", "unmodelled")),
                  self.build("c", line("Parsed", "unread"), line("Imported only", "no_modifier"))]
        rows = self.gaps.ranked_queue(audit, builds)
        self.assertEqual(rows[0]["template"], "#% increased Effect of Charms")
        self.assertEqual(rows[0]["affected_builds"], 2)
        self.assertEqual(rows[0]["source_count"], 4)
        self.assertEqual(rows[0]["class"], "pobr_gap")
        self.assertEqual(len(rows[0]["variants"]), 3)
        self.assertEqual(len(rows[0]["builds"][0]["lines"]), 2)
        self.assertEqual(rows[0]["ledger_variants"], ["30% increased Effect of Charms", "40% increased Effect of Charms"])
        self.assertNotIn("Only translation", [row["template"] for row in rows])
        # Equal build counts: unique before passive; more sources break the tie.
        zero_builds = [r["template"] for r in rows if r["affected_builds"] == 0]
        self.assertLess(zero_builds.index("Two uniques"), zero_builds.index("One unique"))
        self.assertLess(zero_builds.index("One unique"), zero_builds.index("Many sources"))
        self.assertNotIn("Imported only", [row["template"] for row in rows])
        self.assertEqual(next(r for r in rows if r["template"] == "Recognised")["class"], "recognized_empty")
        uniques = self.gaps.ranked_queue(audit, builds, "unique")
        self.assertTrue(all("unique" in row["source_kinds"] for row in uniques))
        self.assertEqual([r["rank"] for r in uniques], list(range(1, len(uniques) + 1)))

    def test_gaps_use_reader_union_and_keep_all_reasons_on_mixed_lines(self):
        def mod(name, status, **extra):
            return {"name": name, "status": status, **extra}
        def line(text, *mods):
            return {"text": text, "status": "partial", "modifiers": list(mods)}
        gate = {"state": "slow applies", "config_option": None}
        archon = {"state": "Archon active", "config_option": "conditionAffectedByArchon"}
        lines = [
            line("Onslaught", mod("CastSpeed", "applied"), mod("AttackSpeed", "unread")),
            line("Slows", mod("UnaffectedBySlows", "inactive", read_gate=gate)),
            line("Archon", mod("ArchonEffect", "inactive", read_gate=archon)),
            line("Conditional", mod("Damage", "inactive", reason="Condition EnemyFrozen not met")),
            line("Mixed", mod("CastSpeed", "applied"), mod("Missing", "unread"),
                 mod("ArchonEffect", "inactive", read_gate=archon)),
            line("Unknown state", mod("Damage", "inactive", reason="Condition Missing not met")),
            # A config condition elsewhere must not excuse a different unmet requirement.
            line("Wrong flags", mod("Damage", "inactive", reason="needs Attack")),
            line("Enemy condition", mod("Damage", "inactive", reason="Condition enemy.Frozen not met")),
            line("Alternatives", mod("Damage", "inactive", reason="none of Conditions EnemyFrozen, Missing met")),
            {"text": "No effect", "status": "no_effect", "modifiers": [mod("LootRarity", "unread")]},
            {"text": "Unmodelled", "status": "unmodelled"},
            {"text": "Unparsed", "status": "unparsed"},
            {"text": "No modifier", "status": "no_modifier"},
            {"text": "Left out", "status": "inactive", "reason": "no recorded consumer"},
        ]
        a = self.build("spell", *lines)
        a["usage"]["conditions"] = [{"condition": "EnemyFrozen", "config_option": "conditionEnemyFrozen"}]
        b = self.build("attack")
        b["usage"]["reads"] = {"AttackSpeed": ["INC AttackSpeed"]}
        audit = {"entries": {row["text"]: self.entry(["unique:Example"]) for row in lines}}
        builds = [a, b]
        rows = self.gaps.ranked_queue(audit, builds)
        self.assertEqual({row["template"] for row in rows}, {
            "Mixed", "Unknown state", "Alternatives", "Unmodelled", "Unparsed", "No modifier", "Left out"})
        complete = self.gaps.ranked_queue(audit, builds, include_all=True)
        buckets = {row["template"]: row["builds"][0]["buckets"] for row in complete}
        self.assertEqual(buckets["Onslaught"], ["unused"])
        self.assertEqual(buckets["Slows"], ["state-gated"])
        self.assertEqual(buckets["Archon"], ["config-gated"])
        self.assertEqual(buckets["Conditional"], ["config-gated"])
        self.assertEqual(buckets["Enemy condition"], ["config-gated"])
        self.assertEqual(buckets["Mixed"], ["dropped", "config-gated"])
        self.assertEqual(buckets["Wrong flags"], ["skill-mismatch"])
        self.assertEqual(self.gaps.bucket_counts(complete), {
            "dropped": 7, "config-gated": 4, "state-gated": 1, "skill-mismatch": 1, "unused": 1, "handled": 1})
        # The other build need not carry this template to prove its reader exists.
        self.assertEqual(builds, [a, b])
        self.assertNotIn("buckets", lines[0])

    def test_config_gates_include_known_false_and_negated_conditions(self):
        lines = [{"text": text, "status": "inactive", "modifiers": [
            {"name": "EnergyShieldRechargeFaster", "status": "inactive", "reason": reason}]
        } for text, reason in [
            ("Known state", "Condition AffectedByArchon not met"),
            ("Negated state", "Condition AffectedByArchon holds (applies only without it)"),
            ("Player state", "Condition player.AffectedByArchon not met"),
        ]]
        build = self.build("a", *lines)
        # Derived and explicitly false conditions do not appear in unset conditions.
        build["usage"]["condition_configs"] = {"AffectedByArchon": "conditionAffectedByArchon"}
        audit = {"entries": {line["text"]: self.entry(["passive:1"]) for line in lines}}
        self.assertEqual(self.gaps.ranked_queue(audit, [build]), [])
        rows = self.gaps.ranked_queue(audit, [build], include_all=True)
        self.assertTrue(all(row["builds"][0]["buckets"] == ["config-gated"] for row in rows))

    def test_thin_ice_hit_reports_the_config_gate(self):
        text = "50% increased Damage with Hits against [Frozen] Enemies"
        line = {"text": text, "status": "inactive", "modifiers": [
            {"name": "Damage", "status": "inactive", "reason": "Condition EnemyFrozen not met"}]}
        build = self.build("frostbolt", line)
        build["usage"]["condition_configs"] = {"EnemyFrozen": "conditionEnemyFrozen"}
        audit = {"entries": {text: self.entry(["passive:Thin Ice"], "parsed", "parsed")}}
        self.assertEqual(self.gaps.ranked_queue(audit, [build]), [])
        rows = self.gaps.ranked_queue(audit, [build], include_all=True)
        self.assertEqual(rows[0]["builds"][0]["buckets"], ["config-gated"])
        self.assertEqual(self.gaps.bucket_counts(rows)["dropped"], 0)

    def test_affected_by_family_uses_build_state_metadata(self):
        lines = []
        states = ["AffectedByHeraldofAsh", "AffectedByHeraldofThunder", "AffectedByOtherBuff"]
        for name, state in zip(["FireDamage", "LightningDamage", "Damage"], states):
            lines.append({"text": state, "status": "inactive", "modifiers": [
                {"name": name, "status": "inactive", "reason": f"Condition {state} not met"}]})
        build = self.build("self-cast", *lines)
        build["usage"]["condition_states"] = states
        # Build state takes precedence even if a configuration entry also exists.
        build["usage"]["condition_configs"] = {states[0]: "condition" + states[0]}
        audit = {"entries": {line["text"]: self.entry(["passive:Coming Calamity"], "parsed", "parsed")
                             for line in lines}}
        self.assertEqual(self.gaps.ranked_queue(audit, [build]), [])
        rows = self.gaps.ranked_queue(audit, [build], include_all=True)
        for row in rows:
            line = row["builds"][0]["lines"][0]
            self.assertEqual(line["buckets"], ["state-gated"])
            self.assertEqual(line["bucket_reasons"], [f"condition set from the build: {line['text']}"])

    def test_skill_mismatches_are_visible_without_hiding_conditions_or_gaps(self):
        mismatches = ["needs Attack", "needs Spell", "needs Melee", "needs Attack, Melee",
                      "SkillType Triggered", "SkillType Attack or Spell", "not SkillType Triggered",
                      "needs a keyword of Bleed", "needs every keyword of Hit, Cold"]
        gaps = ["Condition NothingSetsThis not met", "Condition Attack not met",
                "Multiplier Bleed is 0, needs at least 1", "Stat Strength is 0, needs at least 50",
                "its distance ramp needs a configured enemy distance",
                "DamageType Cold (the query is Fire)", "SkillName Fireball (the skill is Frostbolt)"]
        lines = [{"text": reason, "status": "inactive", "modifiers": [
            {"name": "Damage", "status": "inactive", "reason": reason}]} for reason in mismatches + gaps]
        lines.append({"text": "Mixed skill and missing model", "status": "partial", "modifiers": [
            {"name": "Damage", "status": "inactive", "reason": "needs Attack"},
            {"name": "Missing", "status": "unread"}]})
        build = self.build("spell", *lines)
        audit = {"entries": {line["text"]: self.entry(["unique:Example"], "parsed", "parsed")
                             for line in lines}}
        rows = self.gaps.ranked_queue(audit, [build])
        self.assertEqual({row["template"] for row in rows},
                         {self.gaps.template(reason) for reason in gaps} | {"Mixed skill and missing model"})
        complete = self.gaps.ranked_queue(audit, [build], include_all=True)
        for row in complete:
            if row["template"] in mismatches:
                self.assertEqual(row["builds"][0]["buckets"], ["skill-mismatch"])
            elif row["template"] == "Mixed skill and missing model":
                self.assertEqual(row["builds"][0]["buckets"], ["dropped", "skill-mismatch"])
        self.assertEqual(self.gaps.bucket_counts(complete)["skill-mismatch"], len(mismatches) + 1)

    def test_build_and_pass_conditions_are_state_gated_without_hiding_unknowns(self):
        states = ["MajorityRedSocketedSupports", "MajorityGreenSocketedSupports",
                  "MajorityBlueSocketedSupports", "UsingBow", "CriticalStrike", "WeaponSet2"]
        def line(text, reason, status="inactive"):
            return {"text": text, "status": status, "modifiers": [
                {"name": "Damage", "status": status, "reason": reason}]}
        lines = [line(state, f"Condition {state} not met") for state in states]
        lines += [line("Unknown", "Condition NothingSetsThis not met"),
                  line("Mixed unknown", "none of Conditions UsingBow, NothingSetsThis met"),
                  line("Wrong flags", "needs Attack"),
                  line("Unread", None, "unread"),
                  line("Already applied", None, "applied"),
                  line("Negated", "Condition UsingBow holds (applies only without it)"),
                  line("Player state", "Condition player.UsingBow not met"),
                  line("Config and state", "none of Conditions UsingBow, EnemyFrozen met"),
                  line("Derived state takes precedence", "Condition CriticalStrike not met")]
        build = self.build("spell", *lines)
        build["usage"]["condition_states"] = states
        build["usage"]["condition_configs"] = {"EnemyFrozen": "conditionEnemyFrozen", "CriticalStrike": "criticalStrikeOption"}
        audit = {"entries": {row["text"]: self.entry(["passive:1"], "parsed", "parsed") for row in lines}}
        rows = self.gaps.ranked_queue(audit, [build])
        self.assertEqual({row["template"] for row in rows}, {"Unknown", "Mixed unknown", "Unread"})
        complete = {row["template"]: row for row in self.gaps.ranked_queue(audit, [build], include_all=True)}
        for state in states:
            row = complete[self.gaps.template(state)]
            self.assertEqual(row["builds"][0]["buckets"], ["state-gated"])
            self.assertEqual(row["builds"][0]["lines"][0]["bucket_reasons"],
                             [f"condition set from the build: {state}"])
        self.assertEqual(complete["Config and state"]["builds"][0]["buckets"], ["config-gated", "state-gated"])
        self.assertEqual(complete["Derived state takes precedence"]["builds"][0]["buckets"], ["state-gated"])
        self.assertEqual(complete["Already applied"]["builds"][0]["buckets"], ["handled"])
        self.assertEqual(complete["Negated"]["builds"][0]["buckets"], ["state-gated"])
        self.assertEqual(complete["Player state"]["builds"][0]["buckets"], ["state-gated"])
        card = dict(complete["UsingBow"], stat_matches=[], source_details=[], candidate_names=[], nearby_rules=[])
        self.assertIn("condition set from the build: UsingBow", self.gaps.render_card(card))
        self.assertNotIn("bucket_reasons", lines[0])

    def test_reverse_stat_match_checks_all_compound_members_in_base(self):
        self.data.files = {
            "base/stats.json": [{"id": "life"}, {"id": "min_fire"}],
            "overlay/stat_descriptions.json": {"scopes": {"stat_descriptions": {
                "single": {"life": ["+1 to maximum Life"], "mana": ["+1 to maximum Mana"]},
                "compound": {"min_fire": {"member_stats": ["min_fire", "max_fire"],
                                           "template": "Adds {0} to {1} Fire Damage"}},
            }}},
        }
        matches = self.gaps.reverse_stats(self.data, ["+87 to maximum Life", "Adds 10 to 20 Fire Damage"])
        self.assertEqual([s for m in matches for s in m["stats"]], [
            {"id": "life", "base_exists": True}, {"id": "min_fire", "base_exists": True},
            {"id": "max_fire", "base_exists": False}])
        self.assertEqual(self.gaps.reverse_stats(self.data, ["+10 to maximum Energy Shield"]), [])

    def test_generated_config_and_gem_labels_do_not_enter_the_game_text_queue(self):
        audit = {"entries": {"Real game text": self.entry(["unique:Example"])}}
        builds = [self.build("a", {"text": "Boss", "source_kind": "EnemyConfig", "status": "unread"},
                             {"text": "Config", "source_kind": "Config", "status": "unread"},
                             {"text": "skill stat (1)", "source_kind": "SkillGem", "status": "unread"},
                             {"text": "Real game text", "source_kind": "Config", "status": "unread"})]
        rows = self.gaps.ranked_queue(audit, builds)
        self.assertEqual([row["template"] for row in rows], ["Real game text"])
        self.assertEqual(rows[0]["affected_builds"], 1)

    def test_research_retains_charm_implicit_node_name_and_all_ledger_fates(self):
        text = "20% increased Effect of Charms"
        self.data.files = {
            "overlay/uniques.json": {"uniques": [{"name": "Test Charm", "base": "Ruby Charm",
                "raw": "Test Charm\nRuby Charm\nUsed when you take Fire damage from a Hit\n" + text}]},
            "base/passive_tree.json": [{"skill": 42, "name": "Charm Master", "stats": [text]}],
        }
        audit = {"entries": {text: self.entry(["unique:Test Charm", "passive:42"])}}
        builds = [self.build("a", {"text": text, "status": "unparsed"}),
                  self.build("b", {"text": text, "status": "applied"})]
        with patch.object(self.gaps, "candidates", return_value=([], [])):
            card = self.gaps.research_card(self.data, audit, builds, "#% increased Effect of Charms")
        output = self.gaps.render_card(card)
        self.assertIn("Used when you take Fire damage from a Hit", output)
        self.assertIn("Charm Master", output)
        self.assertIn("https://poe2db.tw/us/Test_Charm", output)
        self.assertIn("https://poe2db.tw/us/Charm_Master", output)
        self.assertIn("applied", output)
        self.assertEqual(card["affected_builds"], 1)
        self.assertEqual(len(card["builds"]), 2)

    def test_usage_cache_is_shared_and_refreshable(self):
        triage = self.gaps.triage
        folder = self.root / "worked/build"
        folder.mkdir(parents=True)
        (folder / "code.txt").write_text("build code", encoding="utf-8")
        report = {"usage": {"lines": [{"text": "Line", "status": "unparsed"}]}}
        with patch.object(triage, "REPO", self.root), \
                patch.object(triage, "engine_report", return_value=report) as engine:
            triage.cache_usage(folder, report, "binary-and-data")
            self.assertEqual(triage.build_usage(Path("binary"), folder, "binary-and-data")["usage"], report["usage"])
            engine.assert_not_called()
            triage.build_usage(Path("binary"), folder, "binary-and-data", refresh=True)
            self.assertEqual(engine.call_count, 1)
            triage.build_usage(Path("binary"), folder, "new-binary-or-data")
            self.assertEqual(engine.call_count, 2)
            (folder / "code.txt").write_text("changed build code", encoding="utf-8")
            triage.build_usage(Path("binary"), folder, "new-binary-or-data")
            self.assertEqual(engine.call_count, 3)


class SkillSweepTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location("skill_sweep", REPO / "devs/scripts/skill-sweep.py")
        cls.sweep = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.sweep)
        root = REPO / "data"
        cls.data = root / (root / "CURRENT").read_text().strip()
        cls.bases = cls.sweep.read_json(cls.data / "base/base_items.json")
        # Fixed oracle identities make XML tests independent of the vendor checkout.
        cls.vendor_gems = {
            "FireballPlayer": [{"gameId": "Metadata/Items/Gem/SkillGemFireball",
                                "variantId": "Fireball"}],
            "IceStrikePlayer": [{"gameId": "Metadata/Items/Gems/SkillGemIceStrike",
                                 "variantId": "IceStrike", "weaponRequirements": "Quarterstaff"}],
            "WolfArcticHowlPlayer": [{"gameId": "Metadata/Items/Gem/SkillGemWolfArcticHowl",
                                      "variantId": "WolfArcticHowl", "weaponRequirements": "Talisman"}],
        }
        cls.cases, cls.skipped, cls.inventory = cls.sweep.skill_cases(
            cls.data, cls.vendor_gems, {*cls.vendor_gems, "UnloadAmmoPlayer"})

    def case(self, effect_id):
        return next(c for c in self.cases if c["skill_id"] == effect_id)

    def test_fireball_uses_real_export_identity_and_explicit_main_selection(self):
        xml, equipment = self.sweep.synthetic_xml(self.case("FireballPlayer"), self.bases)
        root = ET.fromstring(xml)
        self.assertEqual(root.tag, "PathOfBuilding2")
        self.assertEqual(root.find("Build").get("level"), "90")
        self.assertEqual(root.find("Build").get("mainSocketGroup"), "1")
        self.assertEqual(root.find("Tree/Spec").get("classId"), "1")
        skill = root.find("Skills/SkillSet/Skill")
        self.assertEqual(skill.get("mainActiveSkill"), "1")
        self.assertEqual(skill.get("mainActiveSkillCalcs"), "1")
        gem = skill.find("Gem")
        self.assertEqual(gem.get("gemId"), "Metadata/Items/Gem/SkillGemFireball")
        self.assertEqual(gem.get("variantId"), "Fireball")
        self.assertEqual(gem.get("skillId"), "FireballPlayer")
        self.assertEqual((gem.get("level"), gem.get("quality")), ("20", "0"))
        self.assertEqual(equipment, [])
        self.assertEqual(root.findall("Items/Item"), [])
        encoded = self.sweep.encode_code(xml)
        self.assertEqual(zlib.decompress(base64.urlsafe_b64decode(encoded)).decode(), xml)

    def test_attack_uses_a_normal_data_base_and_matching_equipped_slot(self):
        xml, equipment = self.sweep.synthetic_xml(self.case("IceStrikePlayer"), self.bases)
        root = ET.fromstring(xml)
        self.assertEqual(equipment[0]["item_class"], "Warstaff")
        item = root.find("Items/Item")
        self.assertIn("Rarity: NORMAL\n" + equipment[0]["name"], item.text)
        self.assertIn("Physical Damage:", item.text)
        self.assertIn("Implicits: 0", item.text)
        slot = root.find("Items/ItemSet/Slot")
        self.assertEqual(slot.get("name"), "Weapon 1")
        self.assertEqual(slot.get("itemId"), item.get("id"))

    def test_non_attack_shapeshift_skill_gets_its_required_weapon(self):
        case = self.case("WolfArcticHowlPlayer")
        self.assertNotIn("Attack", case["skill_types"])
        xml, equipment = self.sweep.synthetic_xml(case, self.bases)
        self.assertEqual(equipment[0]["item_class"], "Talisman")
        self.assertEqual(ET.fromstring(xml).find("Items/ItemSet/Slot").get("name"), "Weapon 1")

    def test_effect_order_filters_supports_after_ordering_and_keeps_unknowns(self):
        link = {"granted_effect_id": "Primary",
                "additional_granted_effect_ids": ["Support", "Secondary", "Unknown", ""],
                "granted_effect_display_order": [1, 2, 2, 90]}
        effects = {"Primary": {}, "Secondary": {}, "Support": {"is_support": True}}
        self.assertEqual(self.sweep.active_effects(link, effects),
                         [(1, "Secondary"), (2, "Primary"), (3, "Unknown")])
        link.pop("granted_effect_display_order")
        self.assertEqual(self.sweep.active_effects(link, effects),
                         [(1, "Primary"), (2, "Secondary"), (3, "Unknown")])

    def test_crossbow_shots_keep_ammo_gem_identity_and_select_firing_ordinal(self):
        ammo = [c for c in self.cases if c["is_primary"] and "CrossbowAmmoSkill" in c["skill_types"]]
        self.assertEqual(len(ammo), 16)
        for primary in ammo:
            with self.subTest(skill=primary["skill_id"]):
                shot = next(c for c in self.cases if c["primary_skill_id"] == primary["skill_id"]
                            and not c["is_primary"])
                self.assertEqual((shot["active_skill_index"], primary["active_skill_index"]), (1, 2))
                self.assertIn("CrossbowSkill", shot["skill_types"])
                xml, equipment = self.sweep.synthetic_xml(shot, self.bases)
                skill = ET.fromstring(xml).find("Skills/SkillSet/Skill")
                self.assertEqual(skill.get("mainActiveSkill"), "1")
                self.assertEqual(skill.get("mainActiveSkillCalcs"), "1")
                self.assertEqual(skill.find("Gem").get("skillId"), primary["skill_id"])
                self.assertEqual([b["item_class"] for b in equipment], ["Crossbow"])

    def test_banner_and_mirage_active_rows_retain_their_primary_rows(self):
        pairs = [("WarBannerReservationPlayer", "WarBannerPlayer"),
                 ("DefianceBannerReservationPlayer", "DefianceBannerPlayer"),
                 ("DreadBannerReservationPlayer", "DreadBannerPlayer"),
                 ("BlackPowderBlitzReservationPlayer", "BlackPowderBlitzPlayer"),
                 ("MetaMirageArcherPlayer", "MirageArcherSpawnPlayer"),
                 ("MetaMirageDeadeyePlayer", "MirageDeadeyeSpawnPlayer")]
        for primary_id, active_id in pairs:
            with self.subTest(primary=primary_id):
                primary, active = self.case(primary_id), self.case(active_id)
                self.assertTrue(primary["is_primary"])
                self.assertFalse(active["is_primary"])
                self.assertEqual(active["primary_skill_id"], primary_id)
                self.assertEqual((active["active_skill_index"], primary["active_skill_index"]), (1, 2))
                xml, _ = self.sweep.synthetic_xml(active, self.bases)
                self.assertEqual(ET.fromstring(xml).find("Skills/SkillSet/Skill/Gem").get("skillId"), primary_id)

    def test_missing_originating_gem_is_not_counted_as_vendor_supported(self):
        case = next(c for c in self.cases if c["case_id"] == "SummonMercenaryCompanionPlayer/UnloadAmmoPlayer@2")
        self.assertTrue(case["vendor_has_effect"])
        self.assertFalse(case["vendor_has_gem"])
        row = {**case, "classification": "pob2_other_skill", "pobr": None, "pob2": None}
        summary = self.sweep.summarize([row])
        self.assertEqual(summary["measured"], 1)
        self.assertEqual(summary["vendor_supported"], 0)
        # A primary can still be selected directly by skillId without gemId.
        self.assertEqual(self.sweep.summarize([{**row, "is_primary": True}])["vendor_supported"], 1)

    def test_unknown_vendor_gem_uses_effect_id_without_invalid_gem_id(self):
        xml, _ = self.sweep.synthetic_xml(self.case("Melee1HAxePlayer"), self.bases)
        gem = ET.fromstring(xml).find("Skills/SkillSet/Skill/Gem")
        self.assertEqual(gem.get("skillId"), "Melee1HAxePlayer")
        self.assertIsNone(gem.get("gemId"))

    def test_meta_gems_are_included_and_effect_ids_are_deduplicated(self):
        all_cases = self.cases + self.skipped
        self.assertEqual(len({c["case_id"] for c in all_cases}), len(all_cases))
        self.assertEqual(sum(c["is_primary"] for c in all_cases), self.inventory["primary_active_effects"])
        self.assertEqual(sum(not c["is_primary"] for c in all_cases), self.inventory["additional_active_effects"])
        self.assertEqual(self.case("MetaCastOnCritPlayer")["gem"]["gem_type"], 2)
        self.assertIn("meta", self.case("MetaCastOnCritPlayer")["families"])
        self.assertTrue(all(not c["gem"]["is_support"] for c in all_cases))
        self.assertTrue(any(c["skill_id"] == "DoLiterallyNothingPlayer" for c in self.skipped))

    def test_every_case_generates_a_valid_build_without_oracle(self):
        for case in self.cases:
            with self.subTest(skill=case["skill_id"]):
                xml, equipment = self.sweep.synthetic_xml(case, self.bases)
                root = ET.fromstring(xml)
                self.assertEqual(len(root.findall("Skills/SkillSet/Skill/Gem")), 1)
                self.assertEqual(len(root.findall("Items/Item")), len(equipment))

    def result(self, dps, skill="FireballPlayer"):
        return {"main_skill": skill, "metrics": dict.fromkeys(self.sweep.METRICS, dps), "minion": None}

    def test_classification_checks_identity_before_zero_or_agreement(self):
        classify = lambda a, b: self.sweep.classify("FireballPlayer", a, b)[0]
        self.assertEqual(classify(self.result(100), self.result(100)), "agree")
        self.assertEqual(classify(self.result(105), self.result(100)), "agree")
        self.assertEqual(classify(self.result(106), self.result(100)), "differs")
        self.assertEqual(classify(self.result(0), self.result(100)), "pobr_zero")
        self.assertEqual(classify(self.result(100), self.result(0)), "pob2_zero")
        self.assertEqual(classify(self.result(0), self.result(0)), "both_zero")
        self.assertEqual(classify(self.result(0), self.result(0, "MeleeUnarmedPlayer")), "pob2_other_skill")
        self.assertEqual(classify(self.result(100, "other"), self.result(100)), "error")

    def test_average_hit_basis_and_hit_rate_do_not_create_false_differences(self):
        left, right = self.result(100), self.result(100)
        left["metrics"]["AverageHit"] = 200
        right["metrics"]["Speed"] = 10
        right["comparison_rate"] = 100
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right)[0], "agree")

    def test_crossbow_firing_and_reload_metrics_are_compared(self):
        for metric in ("FiringRate", "ReloadTime"):
            left, right = self.result(100), self.result(100)
            left["metrics"][metric] = 106
            classification, differences, _ = self.sweep.classify("FireballPlayer", left, right)
            self.assertEqual(classification, "differs")
            self.assertEqual(differences, [metric])

    def test_minion_damage_is_not_hidden_as_both_zero(self):
        left, right = self.result(0), self.result(0)
        right["minion"] = {"TotalDPS": 100, "skill_id": "minion_attack"}
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right)[0], "pobr_zero")

    def test_selected_stat_evidence_uses_only_the_selected_set_and_level(self):
        selected = self.sweep.selected_stat_set([
            {"set_id": "first", "implicit_stats": ["implicit"],
             "constant_stats": [{"stat": "constant", "value": 1}],
             "levels": [{"gem_level": 20, "stats": [{"stat": "level", "value": 2}]},
                        {"gem_level": 21, "stats": [{"stat": "future", "value": 3}]}]},
            {"set_id": "sibling", "implicit_stats": [self.sweep.AVERAGE_DAMAGE_STAT]}])
        self.assertEqual(selected, {"id": "first", "stats": {"implicit": 1, "constant": 1, "level": 2}})
        # Conductive Runes' placement and explosion must not share flags/models.
        case = self.case("ConductiveRunesPlayer")
        self.assertEqual(case["selected_stat_set"]["id"], "ConductiveRunesPlayer")
        self.assertTrue(case["selected_stat_set"]["stats"]["base_deal_no_damage"])

    def test_average_display_is_known_only_when_other_metrics_agree(self):
        case = {"selected_stat_set": {"stats": {self.sweep.AVERAGE_DAMAGE_STAT: 1}}}
        left, right = self.result(100), self.result(100)
        right["metrics"]["CombinedDPS"] = 50
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right)[0], "differs")
        cls, differences, reason = self.sweep.classify("FireballPlayer", left, right, case)
        self.assertEqual((cls, differences, reason),
                         ("known", ["CombinedDPS"], "known: average-damage display"))
        for metric in ("TotalDPS", "Speed", "CritChance", "FiringRate", "ReloadTime"):
            with self.subTest(metric=metric):
                right["metrics"][metric] = 120
                self.assertEqual(self.sweep.classify("FireballPlayer", left, right, case)[0], "differs")
                right["metrics"][metric] = 100
        # A displayed average can be nonzero even when both DPS/rate values are zero.
        left, right = self.result(0), self.result(0)
        right["metrics"]["CombinedDPS"] = 50
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right, case)[0], "known")

    def test_average_display_examples_really_have_the_game_stat(self):
        for skill in ("GrimPillarsPlayer", "IncineratePlayer", "LightningWarpPlayer", "LivingBombPlayer",
                      "VolcanoPlayer", "VileDisruptionPlayer", "ParryPlayer", "RakePlayer",
                      "HandOfChayulaPlayer", "StampedePlayer", "SkyfallPlayer", "AmazonTriggerElementalSurgePlayer",
                      "ChaosSpearTriggerChaosSurgePlayer", "GeminiSurgePlayer", "ShatteringPalmExplosionPlayer",
                      "StaggeringPalmProjectilePlayer", "LeapSlamPlayer", "ThunderousLeapPlayer", "SunderPlayer",
                      "WhirlingAssaultPlayer", "EscapeShotPlayer", "BearRampagePlayer", "ShieldBlockPlayer",
                      "TheStarsAnswerCometPlayer"):
            with self.subTest(skill=skill):
                self.assertTrue(self.case(skill)["selected_stat_set"]["stats"][self.sweep.AVERAGE_DAMAGE_STAT])

    def test_no_damage_stat_requires_zero_engine_damage_and_matching_identity(self):
        for skill in ("ConductiveRunesPlayer", "RollingSlamPlayer", "SpearfieldPlayer", "TempestBellPlayer"):
            with self.subTest(skill=skill):
                case = self.case(skill)
                left, right = self.result(0, skill), self.result(100, skill)
                cls, _, reason = self.sweep.classify(skill, left, right, case)
                self.assertEqual((cls, reason), ("known", "known: no-damage stat set"))
                left["metrics"]["TotalDPS"] = 10
                self.assertEqual(self.sweep.classify(skill, left, right, case)[0], "differs")
                right["main_skill"] = "other"
                self.assertEqual(self.sweep.classify(skill, left, right, case)[0], "pob2_other_skill")

    def test_documented_models_are_keyed_by_effect_and_scoped_stat_set(self):
        for skill, (reason, selected_set) in self.sweep.KNOWN_MODELS.items():
            with self.subTest(skill=skill):
                case = {"case_id": "any-parent/any-case@9", "selected_stat_set": {"id": selected_set, "stats": {}}}
                left, right = self.result(100, skill), self.result(200, skill)
                self.assertEqual(self.sweep.classify(skill, left, right, case),
                                 ("known", [k for k in self.sweep.METRICS if k != "AverageHit"], "known: " + reason))
                self.assertEqual(self.sweep.classify(skill, left, left, case)[0], "agree")
                if selected_set:
                    case["selected_stat_set"]["id"] = "different-form"
                    self.assertEqual(self.sweep.classify(skill, left, right, case)[0], "differs")

    def test_rounding_recomputes_bounds_and_must_explain_the_damage_ratio(self):
        # Guide game numbers: Wooden Club 6-10 * Earthquake 1.25 = 7.5-12.5;
        # integer bounds 8-13, 75% physical reduction, 5% crit and 2x crit damage.
        left, right = self.result(0), self.result(0)
        hit = 0.775345425
        left["metrics"].update(TotalDPS=10 * .25 * hit * (1 + .05 * hit),
                               CombinedDPS=10 * .25 * hit * (1 + .05 * hit),
                               Speed=1, CritChance=5 * hit)
        right["metrics"].update(TotalDPS=10.5 * .25 * .78 * 1.039,
                                CombinedDPS=10.5 * .25 * .78 * 1.039, Speed=1, CritChance=3.9)
        left["rounding_inputs"] = {"damage": [{"damage_type": "Physical", "min": 7.5, "max": 12.5,
                                               "avg": 10, "crit_avg": 20}],
                                   "mitigation": [{"damage_type": "Physical", "reduction": 75, "multiplier": .25}],
                                   "damage_effect": 1, "hit_chance": hit, "crit_multiplier": 2}
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right)[0], "known")
        self.assertIn("known: rounding", self.sweep.classify("FireballPlayer", left, right)[2])
        case = {"selected_stat_set": {"stats": {self.sweep.AVERAGE_DAMAGE_STAT: 1}}}
        right["metrics"]["CombinedDPS"] = 30
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right)[0], "differs")
        self.assertIn("average-damage display", self.sweep.classify("FireballPlayer", left, right, case)[2])
        right["metrics"]["TotalDPS"] *= 1.02
        # Within 5% after rounding is insufficient evidence; the ratio must match.
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right, case)[0], "differs")
        right["metrics"]["TotalDPS"] /= 1.02
        left["rounding_inputs"]["mitigation"][0]["reduction"] = 70
        self.assertEqual(self.sweep.classify("FireballPlayer", left, right, case)[0], "differs")

    def test_known_rows_remain_visible_but_do_not_rank_as_unexplained(self):
        case = self.case("GrimPillarsPlayer")
        left, right = self.result(100, case["skill_id"]), self.result(100, case["skill_id"])
        right["metrics"]["CombinedDPS"] = 50
        cls, differences, reason = self.sweep.classify(case["skill_id"], left, right, case)
        row = {**case, "classification": cls, "differing_metrics": differences,
               "reasons": [reason], "pobr": left, "pob2": right}
        summary = self.sweep.summarize([row])
        self.assertEqual(summary["counts"]["known"], 1)
        self.assertTrue(all(f["different"] == 0 and not f["examples"] for f in summary["families"]))
        text = self.sweep.text_report({"summary": summary, "data_version": "test", "vendor_commit": "test",
                                      "inventory": self.inventory, "skipped": [], "skills": [row],
                                      "elapsed_seconds": 0, "limitations": []})
        for expected in ("known=1", "known: average-damage display", "pobr [GrimPillarsPlayer]",
                         "pob2 [GrimPillarsPlayer]"):
            self.assertIn(expected, text)

    def test_cache_reuses_oracle_and_engine_failure_keeps_oracle_result(self):
        case = self.case("FireballPlayer")
        oracle = {"mainOutput": {"TotalDPS": 100}, "skillInfo": {"grantedEffectId": "FireballPlayer"}}
        engine = {"output": {"dps": 100, "action_rate": 1, "total_hit_avg": 100, "crit_chance": 0, "firing_rate": 0, "reload_time": 0},
                  "main_skill": {"skill_id": "FireballPlayer", "combined_dps": 100}}
        with tempfile.TemporaryDirectory() as temp, patch.object(self.sweep, "CACHE", Path(temp)):
            with patch.object(self.sweep, "run", side_effect=[json.dumps(engine), json.dumps(oracle)]) as run:
                first = self.sweep.measure(case, self.bases, Path("pobr"), "commit", b"wrapper", False)
                self.assertEqual(run.call_count, 2)
                self.assertFalse(first["oracle_cached"])
            with patch.object(self.sweep, "run", return_value="{}") as run:
                second = self.sweep.measure(case, self.bases, Path("pobr"), "commit", b"wrapper", False)
                self.assertEqual(run.call_count, 1)
                self.assertEqual(second["classification"], "error")
                self.assertTrue(second["oracle_cached"])
                self.assertEqual(second["pob2"]["main_skill"], "FireballPlayer")
            with patch.object(self.sweep, "run", side_effect=[json.dumps(engine), json.dumps(oracle)]) as run:
                refreshed = self.sweep.measure(case, self.bases, Path("pobr"), "commit", b"wrapper", True)
                self.assertEqual(run.call_count, 2)
                self.assertFalse(refreshed["oracle_cached"])

    def test_cache_key_changes_with_xml_vendor_or_wrapper(self):
        key = self.sweep.cache_key("xml", "commit", b"wrapper")
        self.assertEqual(key, self.sweep.cache_key("xml", "commit", b"wrapper"))
        for xml, commit, wrapper in [("changed", "commit", b"wrapper"),
                                     ("xml", "changed", b"wrapper"), ("xml", "commit", b"changed")]:
            self.assertNotEqual(key, self.sweep.cache_key(xml, commit, wrapper))

    def test_focused_run_preserves_full_report_and_honors_explicit_destinations(self):
        effect = "FireballPlayer"
        result = self.result(100, effect)
        row = {"skill_id": effect, "classification": "agree", "families": [],
               "vendor_has_effect": True, "pobr": result, "pob2": result, "reasons": []}
        commit = self.sweep.read_json(self.data / "overlay/skill_overrides.json")["_meta"]["vendor_commit"]
        with tempfile.TemporaryDirectory() as temp, contextlib.ExitStack() as stack:
            root = Path(temp)
            cache = root / "cache"
            cache.mkdir()
            vendor = root / "vendor"
            (vendor / "src").mkdir(parents=True)
            (vendor / "src/HeadlessWrapper.lua").touch()
            (root / "target/debug").mkdir(parents=True)
            (root / "target/debug/pobr").touch()
            for suffix in ("json", "txt"):
                (cache / f"report.{suffix}").write_text("previous full inventory")
            for name, value in (("REPO", root), ("CACHE", cache), ("VENDOR", vendor)):
                stack.enter_context(patch.object(self.sweep, name, value))
            stack.enter_context(patch.dict(os.environ, {"POBR_DATA_ROOT": str(self.data.parent),
                                                      "POBR_DATA_VERSION": self.data.name}))
            stack.enter_context(patch.object(self.sweep, "run", return_value=commit))
            stack.enter_context(patch.object(self.sweep, "vendor_metadata", return_value=({}, set())))
            stack.enter_context(patch.object(self.sweep, "skill_cases", return_value=(
                [{"skill_id": effect}], [], {"primary_active_effects": 1, "additional_active_effects": 0})))
            stack.enter_context(patch.object(self.sweep, "measure", return_value=row))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            stack.enter_context(contextlib.redirect_stderr(io.StringIO()))
            self.assertEqual(self.sweep.main(["--no-build", "--skill", effect]), 0)
            self.assertEqual(self.sweep.read_json(cache / "report-selected.json")["selection"], [effect])
            self.assertTrue((cache / "report-selected.txt").is_file())
            for suffix in ("json", "txt"):
                self.assertEqual((cache / f"report.{suffix}").read_text(), "previous full inventory")
            custom_json, custom_text = root / "chosen.json", root / "chosen.txt"
            self.assertEqual(self.sweep.main(["--no-build", "--skill", effect,
                "--json", str(custom_json), "--text", str(custom_text)]), 0)
            self.assertEqual(self.sweep.read_json(custom_json)["selection"], [effect])
            self.assertTrue(custom_text.is_file())
            self.assertEqual(self.sweep.main(["--no-build"]), 0)
            self.assertEqual(self.sweep.read_json(cache / "report.json")["selection"], "all")
            self.assertIn("Skill sweep:", (cache / "report.txt").read_text())

    def test_oracle_attack_average_falls_back_to_average_damage(self):
        result = self.sweep.oracle_result({"mainOutput": {"AverageDamage": 42}, "skillInfo": {}})
        self.assertEqual(result["metrics"]["AverageHit"], 42)
        self.assertEqual(result["average_hit_field"], "AverageDamage")
        with self.assertRaises(ValueError):
            self.sweep.oracle_result({"mainOutput": {"TotalDPS": "Infinity"}})


class ConfigCoverageTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            "config_coverage", REPO / "devs/scripts/config-coverage.py")
        self.coverage = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.coverage)

    def test_all_effects_list_variants_conditions_and_handlers(self):
        def option(var, names=(), **extra):
            return {"var": var, "section": "Combat", "effects": [
                {"name": name, "target": "player"} for name in names], **extra}
        options = [option("read", ["Damage"]), option("mixed", ["Damage", "Missing"]),
                   option("variants", option_effects={"one": [{"name": "Damage"}],
                                                      "two": [{"name": "Missing"}]}),
                   option("condition", ["Condition:Frozen"]),
                   option("multiplier", ["Multiplier:Stacks"]),
                   option("referenced", ["Condition:Waiting"]),
                   option("derived", ["Condition:Combat"]),
                   option("implied", imply_conditions=["Frozen"]),
                   option("missing_implied", imply_conditions=["Unasked"]),
                   option("handler", handler_id="config:ok"),
                   option("unregistered", handler_id="config:missing"),
                   option("scalar"), option("lost"),
                   option("unworked", ["Missing"], visibility={"if_skill": ["Fireball"]}),
                   option("worked", ["Missing"], visibility={"if_skill": ["Arc"]})]
        builds = [{"usage": {"reads": {"Damage": []}, "condition_reads": ["Frozen"],
                             "multiplier_reads": ["Stacks"], "referenced_variables": ["Waiting"],
                             "skills": [{"skill_id": "ArcPlayer"}]}}]
        report = self.coverage.classify(options, builds,
            {"engine": {"scalar": {"consumer.rs"}}, "data": {}}, {"config:ok"},
            pass_conditions={"Combat"}, skill_names={"ArcPlayer": "Arc"})
        rows = {r["var"]: r for r in report["options"]}
        self.assertEqual(report["counts"], {"read": 5, "handler": 1, "implied": 1,
                                            "unread": 6, "no_effect": 2})
        self.assertEqual(report["sections"]["Combat"], report["counts"])
        self.assertEqual(rows["variants"]["missing"][0]["name"], "Missing")
        self.assertEqual(rows["unworked"]["unworked_skills"], ["Fireball"])
        self.assertEqual(rows["worked"]["unworked_skills"], [])
        self.assertEqual(rows["scalar"]["no_effect_kind"], "scalar_consumer")
        self.assertEqual(rows["lost"]["no_effect_kind"], "lost_mechanic")
        self.assertEqual(report["unregistered_handlers"], [
            {"var": "unregistered", "handler_id": "config:missing"}])
        self.assertEqual(rows["mixed"]["missing"][0]["absent_from"], ["engine", "data"])
        self.assertEqual(rows["missing_implied"]["class"], "unread")
        self.assertIn("No reader anywhere", self.coverage.render(report))

    def test_nested_effects_and_enemy_condition_namespace(self):
        options = [{"var": "nested", "section": "Minions", "effects": [{
            "target": "player", "name": "MinionModifier", "list_value": {
                "kind": "nested_mod", "mod": {"name": "Damage"}}}]},
            {"var": "enemy", "section": "Enemy", "effects": [{
                "target": "enemy", "name": "Condition:Frozen"}]},
            {"var": "direct", "section": "Player", "effects": [{
                "name": "Condition:Direct"}]}]
        report = self.coverage.classify(options, [{"usage": {
            "reads": {"Damage": [], "Condition:Direct": []}, "condition_configs": {"EnemyFrozen": "enemy"}}}],
            {"engine": {}, "data": {}}, set())
        self.assertEqual(report["counts"]["read"], 3)
        self.assertEqual(self.coverage.effect_names(options[0]), [("minion", "Damage")])

    def test_registry_only_counts_registrations_not_comments_or_stubs(self):
        text = '''
        // registry.register("config:comment", stub);
        const STUB_HANDLER_IDS: &[&str] = &["config:unregistered"];
        registry.register("config:direct", handler);
        for (id, flag) in [("config:table", "Flag")] {
            registry.register(id, handler);
        }
        #[cfg(test)] mod tests { registry.register("config:test", handler); }
        '''
        self.assertEqual(self.coverage.registrations(text), {"config:direct", "config:table"})

    def test_source_search_excludes_tests_comments_and_the_catalog(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "crates/core/src/calc.rs"
            source.parent.mkdir(parents=True)
            source.write_text('let x = "Read"; // "Comment"\n#[cfg(test)] mod tests { "Test" }')
            directory = root / "data/test"
            directory.mkdir(parents=True)
            (directory / "config_options.json").write_text('{"name": "OnlyWritten"}')
            (directory / "rules.json").write_text('{"name": "Producer"}')
            index, *_ = self.coverage.source_index(root, directory)
            self.assertIn("Read", index["engine"])
            self.assertNotIn("Comment", index["engine"])
            self.assertNotIn("Test", index["engine"])
            self.assertNotIn("OnlyWritten", index["data"])
            self.assertIn("Producer", index["data"])

    def test_failed_ledger_aborts_instead_of_reporting_a_partial_union(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "overlay").mkdir()
            (directory / "overlay/config_options.json").write_text('{"options": []}')
            with patch.object(self.coverage.gaps.lookup, "data_dir", return_value=directory), \
                 patch.object(self.coverage.gaps, "collect_usage", return_value=([], [{"error": "failed"}])), \
                 contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(self.coverage.main(["--binary", "unused"]), 1)


class OutputCoverageTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            "output_coverage", REPO / "devs/scripts/output-coverage.py")
        self.coverage = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.coverage)

    def test_catalog_aliases_and_sidebar_exclusions(self):
        catalog = self.coverage.catalog_stats('''
            computed("TotalHitAvg", Cat::HitDamage, Vt::Number, "AverageHit"),
            computed("Life", Cat::Resource, Vt::Number, "Life"),
        ''')
        self.assertEqual(catalog, {"TotalHitAvg": "AverageHit", "Life": "Life"})
        sidebar = self.coverage.sidebar_stats('''
            { stat = "Life" },
            { stat = "Life" },
            { stat = "MainHand", childStat = "Accuracy" },
            { stat = "QuantityMultiplier" },
            { stat = "PvpTotalDPS" },
            { stat = "Spec:LifeInc" },
            { stat = "SpellDodgeChance" },
            -- { stat = "OldStat" },
            local minionDisplayStats = { { stat = "MinionOnly" } }
        ''')
        self.assertEqual(sidebar, {"Life", "MainHand.Accuracy", "QuantityMultiplier"})

    def test_agreement_handles_missing_zero_nonfinite_and_aliases(self):
        builds = [
            {"build": "near", "values": {"Avg": 95}, "oracle": {"calcsOutput": {"AverageHit": 100}}},
            {"build": "far", "values": {"Avg": 80}, "oracle": {"calcsOutput": {"AverageHit": 100}}},
            {"build": "zero", "values": {"Avg": 0}, "oracle": {"calcsOutput": {"AverageHit": 0}}},
            {"build": "zero_ref", "values": {"Avg": 3}, "oracle": {"calcsOutput": {"AverageHit": 0}}},
            {"build": "missing", "values": {}, "oracle": {"calcsOutput": {"AverageHit": 2}}},
            {"build": "nan", "values": {"Avg": 2}, "oracle": {"calcsOutput": {"AverageHit": "nan"}}},
        ]
        row = self.coverage.summarize("Avg", "AverageHit", builds)
        self.assertEqual((row["both"], row["nonzero"], row["within_5pct"], row["within_10pct"]), (4, 3, 2, 2))
        self.assertEqual([pair["build"] for pair in row["worst"]], ["zero_ref", "far"])
        self.assertEqual(self.coverage.oracle_value({"calcsMainHandOutput": {"Accuracy": 123}}, "MainHand.Accuracy"), 123)
        self.assertIsNone(self.coverage.number(True))
        self.assertIsNone(self.coverage.number("inf"))
        json.dumps(row, allow_nan=False)

    def test_holdout_and_nested_holdout_are_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "holdout" / "build"
            path.mkdir(parents=True)
            (path / "code.txt").write_text("example")
            with self.assertRaisesRegex(ValueError, "holdout"):
                self.coverage.build_files([path])
            with self.assertRaisesRegex(ValueError, "holdout"):
                self.coverage.build_files([Path(root)])


if __name__ == "__main__":
    unittest.main()
