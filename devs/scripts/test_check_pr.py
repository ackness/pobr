"""Contracts for offline local planning and failure propagation."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("check_pr", HERE / "check-pr.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


def metadata_result():
    paths = {
        "pobr-core": "crates/pobr-core",
        "pobr-build": "crates/pobr-build",
        "pobr-cli": "apps/pobr-cli",
        "pobr-wasm": "apps/pobr-wasm",
        "pobr-data": "crates/pobr-data",
    }
    dependencies = {
        "pobr-build": [("pobr-core", "dev")],
        "pobr-cli": [("pobr-build", None)],
        "pobr-wasm": [("pobr-build", "build")],
    }
    packages = [{
        "id": name, "name": name,
        "manifest_path": str(check.ROOT / path / "Cargo.toml"),
        "dependencies": [
            {"name": dependency, "rename": "alias", "kind": kind,
             "path": str(check.ROOT / paths[dependency])}
            for dependency, kind in dependencies.get(name, [])
        ],
    } for name, path in paths.items()]
    return subprocess.CompletedProcess([], 0, json.dumps({
        "packages": packages, "workspace_members": list(paths),
    }), "")


class LocalPlannerTests(unittest.TestCase):
    def setUp(self):
        argv = patch.object(sys, "argv", ["check-pr.py"])
        argv.start()
        self.addCleanup(argv.stop)

    def test_local_entry_combines_branch_index_worktree_and_new_files(self):
        outputs = [b"base\n", b"crates/pobr-core/src/lib.rs\0removed.rs\0",
                   b"web/src/staged.ts\0", b"web/src/staged.ts\0docs/note.md\0",
                   b"devs/scripts/new file.py\0"]
        calls = []

        def git(command, **kwargs):
            calls.append(command)
            return subprocess.CompletedProcess(command, 0, outputs[len(calls) - 1], b"")

        with patch.object(check.planner.subprocess, "run", side_effect=git):
            files = check.planner.local_changed_files(HERE)
        self.assertEqual(files, sorted([
            "crates/pobr-core/src/lib.rs", "removed.rs", "web/src/staged.ts",
            "docs/note.md", "devs/scripts/new file.py",
        ]))
        self.assertEqual(calls[0], ["git", "merge-base", "origin/master", "HEAD"])
        self.assertIn("base", calls[1])
        self.assertIn("HEAD", calls[1])
        self.assertIn("--cached", calls[2])
        self.assertEqual(calls[-1], ["git", "ls-files", "--others", "--exclude-standard", "-z"])
        self.assertTrue(check.planner.classify(files)["full"])  # Unknown removed input is conservative.

    def test_missing_master_fails_instead_of_passing_an_empty_plan(self):
        failed = subprocess.CompletedProcess([], 1, b"", b"unknown revision origin/master")
        with patch.object(check.planner.subprocess, "run", return_value=failed):
            with self.assertRaisesRegex(ValueError, "git fetch origin master"):
                check.planner.local_changed_files()

    def test_docs_only_needs_no_tools_or_manifests(self):
        with patch.object(check.planner, "local_changed_files", return_value=["docs/note.md"]), \
                patch.object(check, "run_steps") as run, \
                patch.object(check.subprocess, "run") as tools, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(check.main(), 0)
        run.assert_not_called()
        tools.assert_not_called()

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_touched_crate_features_and_reverse_dependents(self, metadata):
        files = ["crates/pobr-core/src/lib.rs", "crates/pobr-build/README.md"]
        packages = check.touched_packages(files)
        steps = check.steps_for(check.planner.classify(files), packages)
        self.assertEqual(packages,
                         ["pobr-build", "pobr-cli", "pobr-core", "pobr-wasm"])
        metadata.assert_called_with(
            ["cargo", "metadata", "--offline", "--no-deps", "--format-version", "1"],
            cwd=check.ROOT, capture_output=True, text=True,
        )
        clippy = next(command for label, _, command in steps if label == "Rust clippy")
        self.assertIn("pobr-core/benchmarks", clippy[clippy.index("--features") + 1])
        self.assertIn("pobr-build/benchmarks", clippy[clippy.index("--features") + 1])
        self.assertIn("pobr-wasm/diagnostics", clippy[clippy.index("--features") + 1])
        self.assertEqual(clippy[-3:], ["--", "-D", "warnings"])
        self.assertIn("--all-targets", clippy)
        for _, _, command in steps[1:]:
            self.assertIn("--offline", command)
            self.assertIn("--locked", command)
        for label, _, command in steps:
            if label in {"Rust clippy", "Rust tests"}:
                self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == "-p"],
                                 ["pobr-build", "pobr-cli", "pobr-core", "pobr-wasm"])

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_nested_non_member_maps_to_enclosing_crate(self, _metadata):
        self.assertEqual(check.touched_packages(["apps/pobr-cli/nested/helper/src/lib.rs"]),
                         ["pobr-cli"])

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_deleted_file_in_member_selects_its_dependents(self, _metadata):
        self.assertEqual(check.touched_packages(["crates/pobr-build/src/deleted.rs"]),
                         ["pobr-build", "pobr-cli", "pobr-wasm"])

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_deleted_crate_and_shared_inputs_fall_back_to_workspace(self, _metadata):
        for path in ("crates/deleted/src/lib.rs", "data/CURRENT", "examples/build.xml"):
            with self.subTest(path=path):
                self.assertEqual(check.touched_packages([path]),
                                 ["pobr-build", "pobr-cli", "pobr-core", "pobr-data", "pobr-wasm"])

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_full_runs_every_area_and_workspace_tests_including_doctests(self, _metadata):
        files = ["pipeline/data.lock.json"]
        plan = check.planner.classify(files)
        packages = check.touched_packages(files, full=plan["full"])
        steps = check.steps_for(plan, packages)
        commands = [command for _, _, command in steps]
        tests = next(command for label, _, command in steps if label == "Rust tests")
        self.assertNotIn("--all-targets", tests)  # Preserve doctests.
        for name in packages:
            self.assertIn(name, tests)
        for task in ("i18n:check", "typecheck", "test:worker"):
            self.assertIn(["pnpm", task], commands)
        self.assertIn(["bash", "devs/scripts/check-workflows.sh"], commands)

    @patch.object(check.subprocess, "run", return_value=metadata_result())
    def test_plan_only_prints_packages_without_executing_checks(self, _metadata):
        with patch.object(check.planner, "local_changed_files", return_value=["crates/pobr-core/src/lib.rs"]), \
                patch.object(sys, "argv", ["check-pr.py", "--plan-only"]), \
                patch.object(check, "run_steps") as run, contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(check.main(), 0)
        run.assert_not_called()
        self.assertIn("Local plan (1 changed files): rust", out.getvalue())
        self.assertIn("Rust packages: pobr-build, pobr-cli, pobr-core, pobr-wasm", out.getvalue())

    def test_metadata_failure_fails_planning_without_running_checks(self):
        failed = subprocess.CompletedProcess([], 1, "", "invalid workspace member")
        with patch.object(check.planner, "local_changed_files", return_value=["crates/pobr-core/src/lib.rs"]), \
                patch.object(check.subprocess, "run", return_value=failed), \
                patch.object(check, "run_steps") as run, contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(check.main(), 1)
        run.assert_not_called()
        self.assertIn("cargo metadata failed: invalid workspace member", out.getvalue())

    def test_fmt_fails_fast_but_other_failures_are_summarized(self):
        steps = [(label, HERE, ["fake"]) for label in ("Rust fmt", "Rust check", "Rust tests")]
        for codes, count in (([1], 1), ([0, 1, 0], 3)):
            with self.subTest(codes=codes), \
                    patch.object(check.subprocess, "run", side_effect=[
                        subprocess.CompletedProcess([], code) for code in codes]) as run, \
                    contextlib.redirect_stdout(io.StringIO()) as out:
                self.assertEqual(check.run_steps(steps), 1)
            self.assertEqual(run.call_count, count)
            self.assertIn("FAIL", out.getvalue())
            self.assertIn("s)", out.getvalue())

    def test_hook_propagates_failure_and_can_be_explicitly_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            mise = Path(directory) / "mise"
            mise.write_text('#!/bin/sh\n[ "$*" = "run check:pr" ] || exit 99\n'
                            '[ -z "$(cat)" ] || exit 98\nexit 7\n')
            mise.chmod(0o755)
            env = {**os.environ, "PATH": directory + os.pathsep + os.environ["PATH"],
                   "POBR_SKIP_PREPUSH": "0"}
            command = ["bash", str(HERE.parent / "hooks/pre-push")]
            refs = ("refs/heads/test " + "a" * 40 + " refs/heads/test " + "b" * 40 + "\n").encode()
            deleted = ("(delete) " + "0" * 40 + " refs/heads/old " + "b" * 40 + "\n").encode()
            failed = subprocess.run(command, cwd=HERE, env=env, input=deleted + refs, capture_output=True)
            self.assertEqual(failed.returncode, 7, failed.stderr)
            delete_only = subprocess.run(command, env=env, input=deleted + deleted, capture_output=True)
            self.assertEqual(delete_only.returncode, 0, delete_only.stderr)
            self.assertIn(b"no pushed commits", delete_only.stdout)
            skipped = subprocess.run(command, env={**env, "POBR_SKIP_PREPUSH": "1"},
                                     input=refs, capture_output=True)
            self.assertEqual(skipped.returncode, 0, skipped.stderr)
            self.assertIn(b"POBR_SKIP_PREPUSH=1", skipped.stdout)


if __name__ == "__main__":
    unittest.main()
