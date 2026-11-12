"""Data acceptance contracts with fake commands; no Cargo, WASM or browser."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import accept_data


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.corpus = "examples/demo/builds"
        for name in ("b", "a"):
            code = self.root / self.corpus / name / "code.txt"
            code.parent.mkdir(parents=True)
            code.write_text("code", encoding="utf-8")
        snapshot = self.root / "examples/output-snapshot.json"
        # A new build absent from the recorded snapshot still has to compute.
        snapshot.write_text(json.dumps({"builds": {f"{self.corpus}/a": {}}}), encoding="utf-8")
        self.snapshot = snapshot
        self.calls = []
        self.failures = {}
        for method in ("node_modules", "data", "wasm"):
            patcher = patch.object(accept_data.e2e.Inputs, method)
            self.addCleanup(patcher.stop)
            setattr(self, f"{method}_patcher", patcher)
            setattr(self, method, patcher.start())

    def run_command(self, command, **kwargs):
        self.calls.append((command, kwargs))
        for needle, failure in self.failures.items():
            if needle in " ".join(command):
                if isinstance(failure, Exception):
                    raise failure
                return subprocess.CompletedProcess(command, failure)
        return subprocess.CompletedProcess(command, 0)

    def accept(self, *args):
        with contextlib.redirect_stdout(io.StringIO()) as output:
            code = accept_data.main(list(args), self.root, self.run_command)
        return code, output.getvalue()

    def test_all_steps_run_in_order_over_the_same_live_l2_corpus(self):
        code, output = self.accept()
        self.assertEqual(code, 0)
        self.assertIn("Data acceptance: PASS\n", output)
        self.assertNotIn("INCOMPLETE", output)
        commands = [command for command, _ in self.calls]
        self.assertEqual(commands[:2], [
            ["mise", "run", "test", "build", "parity", "multi_version::"],
            ["mise", "run", "build"],
        ])
        codes = [str(self.root / self.corpus / name / "code.txt") for name in ("a", "b")]
        self.assertEqual([command[-1] for command in commands if "calculate-build" in command], codes)
        self.assertEqual(commands[-2], ["mise", "run", "audit:display", "--strict",
                                      *(str(Path(code).relative_to(self.root)) for code in codes)])
        self.assertEqual(commands[-1], ["mise", "run", "e2e", "smoke", "--grep", "data-bump-smoke.spec.ts"])
        for _, kwargs in self.calls:
            self.assertEqual(kwargs["cwd"], self.root)
            self.assertEqual(kwargs["env"]["CARGO_TARGET_DIR"], str(self.root / "target"))
        for prepare in (self.node_modules, self.data, self.wasm):
            prepare.assert_called_once()

    def test_repeated_skips_omit_commands_and_report_incomplete(self):
        code, output = self.accept("--skip", "multi-version", "--skip", "display")
        self.assertEqual(code, 0)
        self.assertNotIn("multi_version::", str(self.calls))
        self.assertNotIn("audit:display", str(self.calls))
        self.node_modules.assert_not_called()
        self.assertIn("| multi-version | SKIP |", output)
        self.assertIn("| display | SKIP |", output)
        self.assertIn("INCOMPLETE", output)
        self.assertNotIn("Data acceptance: PASS", output)

    def test_each_failure_propagates_and_later_steps_still_run(self):
        for command, step in (("multi_version::", "multi-version"), ("run build", "corpus"),
                              ("calculate-build", "corpus"), ("audit:display", "display"),
                              ("run e2e", "e2e")):
            with self.subTest(command=command):
                self.calls.clear()
                self.failures = {command: 7}
                code, output = self.accept()
                self.assertEqual(code, 1)
                self.assertIn(f"| {step} | FAIL |", output)
                self.assertIn("Data acceptance: FAIL", output)
                self.assertNotIn("Data acceptance: PASS", output)
                self.assertEqual(self.calls[-1][0][:4], ["mise", "run", "e2e", "smoke"])

    def test_missing_command_or_preparation_failure_gets_a_summary(self):
        self.failures = {"multi_version::": FileNotFoundError("missing mise")}
        self.data.side_effect = accept_data.e2e.Stop("prepare data")
        code, output = self.accept()
        self.assertEqual(code, 1)
        for message in ("missing mise", "prepare data", "| display | FAIL |", "| e2e | PASS |"):
            self.assertIn(message, output)

    def test_missing_offline_archives_explain_how_to_prepare_data(self):
        self.data_patcher.stop()
        self.failures = {"pipeline.runtime.build_data": 1}
        with patch.object(accept_data.e2e.Inputs, "data_key", return_value=None):
            code, output = self.accept("--skip", "multi-version", "--skip", "corpus", "--skip", "e2e")
        self.assertEqual(code, 1)
        self.assertIn("run `mise run data:prepare`", output)
        self.assertIn("| display | FAIL |", output)
        self.assertIn("Data acceptance: FAIL", output)
        self.assertEqual(self.calls[0][0][1:], [
            "-m", "pipeline.runtime.build_data", "--install", "data", "--offline",
        ])

    def test_empty_or_missing_corpus_fails_both_corpus_steps(self):
        for contents in ({"builds": {}}, {"builds": {"examples/missing/builds/a": {}}}):
            with self.subTest(contents=contents):
                self.snapshot.write_text(json.dumps(contents), encoding="utf-8")
                code, output = self.accept()
                self.assertEqual(code, 1)
                self.assertIn("| corpus | FAIL |", output)
                self.assertIn("| display | FAIL |", output)
                self.assertIn("| e2e | PASS |", output)

    def test_holdout_corpus_is_rejected_before_calculation_or_display(self):
        holdout = "examples/reference-builds/holdout/builds/a"
        code = self.root / holdout / "code.txt"
        code.parent.mkdir(parents=True)
        code.write_text("code", encoding="utf-8")
        self.snapshot.write_text(json.dumps({"builds": {holdout: {}}}), encoding="utf-8")
        result, output = self.accept()
        self.assertEqual(result, 1)
        self.assertIn("holdout set is only measured", output)
        self.assertIn("| corpus | FAIL |", output)
        self.assertIn("| display | FAIL |", output)
        self.assertFalse(any("calculate-build" in command or "audit:display" in command
                             for command, _ in self.calls))

    def test_skip_does_not_hide_other_failures(self):
        self.failures = {"calculate-build": 1}
        code, output = self.accept("--skip", "e2e")
        self.assertEqual(code, 1)
        self.assertIn("| e2e | SKIP |", output)
        self.assertIn("Data acceptance: FAIL", output)

    def test_unknown_skip_is_rejected_before_running_commands(self):
        for args in (("--skip", "typo"), ("--skip",), ("--unknown",)):
            with self.subTest(args=args):
                with self.assertRaises(SystemExit) as error, contextlib.redirect_stderr(io.StringIO()):
                    self.accept(*args)
                self.assertEqual(error.exception.code, 2)
                self.assertEqual(self.calls, [])

    def test_cli_from_another_checkout_is_not_used(self):
        with patch.dict(os.environ, {"POBR_BIN": "/other/pobr", "CARGO_TARGET_DIR": "/other/target"}):
            self.assertEqual(self.accept()[0], 0)
        calculation = next(command for command, _ in self.calls if "calculate-build" in command)
        self.assertEqual(calculation[0], str(self.root / "target/debug/pobr"))


if __name__ == "__main__":
    unittest.main()
