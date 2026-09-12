"""Check script failure propagation and working-tree preservation without Cargo."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="pobr script tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        scripts = ["devs/scripts/regen-check.sh", ".claude/skills/run-pobr/driver.sh"]
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
if os.environ.get("FAIL_COMMAND") == args[0]:
    sys.exit(17)
if os.environ.get("FAIL_TEST") and args[0] == "test":
    sys.exit(19)
if "pobr-data-adapter" in args:
    dest = pathlib.Path(args[args.index("--out") + 1]) / "test/generated"
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "special_derived.json").write_text("{}", encoding="utf-8")
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
        (self.root / "tmp").mkdir()
        self.env = {
            **os.environ,
            "PATH": f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}",
            "CALLS": str(self.root / "calls.jsonl"),
            "SOURCE_DATA": str(self.root / "data/test"),
            "TMPDIR": str(self.root / "tmp"),
            "POBR_PATCH": "test",
        }
        for key in ["HAS_NEXTEST", "FAIL_COMMAND", "FAIL_TEST", "FAIL_REGEN"]:
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
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "smoke")
        self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
        calls = (self.root / "calls.jsonl").read_text().splitlines()
        self.assertEqual(len(calls), 1)
        self.assertEqual(json.loads(calls[0])[0], "test")

    def test_targeted_arguments_are_forwarded_without_splitting(self):
        args = ["-p", "pobr-build", "--test", "codec", "build_code::", "--", "--exact"]
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "test", *args)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads((self.root / "calls.jsonl").read_text()), ["test", *args])

    def test_empty_target_does_not_run_workspace(self):
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "test")
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.root / "calls.jsonl").exists())

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
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "lint", *args)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [
            ["fmt", "--all", "--check"], ["clippy", *args, "--", "-D", "warnings"],
        ])

    def test_lint_stops_on_format_failure(self):
        self.env["FAIL_COMMAND"] = "fmt"
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "lint", "-p", "pobr-build")
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
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "full")
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
        result = self.run_script(".claude/skills/run-pobr/driver.sh", "full")
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
                result = self.run_script(".claude/skills/run-pobr/driver.sh", "full")
                self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
                calls = self.calls()
                self.assertEqual(calls[-1][0], command)
                self.assertEqual(sum(call[0] == command and "--version" not in call
                                     for call in calls), 1)


if __name__ == "__main__":
    unittest.main()
