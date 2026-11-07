"""Contracts for the local E2E runner: tiers, affected mapping, ports, preflight and release receipts. No browser."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("e2e", HERE / "e2e.py")
e2e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e2e)
E2E = e2e.ROOT / "web/e2e"
STEMS = e2e.spec_stems(E2E)


def affected(*files):
    return e2e.select("affected", list(files), STEMS, e2e.spec_inputs(E2E))


def main_grep(selection):
    args = selection.suites[0].args
    return args[args.index("--grep") + 1]


class TierTests(unittest.TestCase):
    def test_fixed_tiers(self):
        self.assertEqual(e2e.select("smoke").suites[0].args, ("--grep", "@smoke"))
        self.assertEqual(e2e.select("default").suites[0].args, ())
        release = e2e.select("release").suites
        self.assertEqual([suite.name for suite in release], ["matrix", "calcs-config", "item-display", "controls"])
        self.assertEqual(release[0].args, ("--grep-invert", "@quarantine"))
        self.assertEqual(dict(release[0].env), {"POBR_E2E_ALL": "1"})
        # Every isolated config exists and keeps its own worker count.
        for suite in release[1:]:
            self.assertTrue((e2e.ROOT / "web" / suite.config).is_file(), suite.config)
            self.assertFalse(suite.main)

    def test_isolated_specs_are_left_out_of_the_main_config_and_map_to_their_suite(self):
        web = e2e.ROOT / "web"
        ignored = set(re.findall(r"'([\w-]+\.spec\.ts)'",
                                 re.search(r"testIgnore: \[([^\]]*)\]", (web / "playwright.config.ts").read_text())[1]))
        isolated = {}
        for name, suite in e2e.ISOLATED.items():
            match = re.search(r"testMatch: (\[[^\]]*\]|'[^']*')", (web / suite.config).read_text())[1]
            for spec_name in re.findall(r"'([\w.-]+)'", match):
                isolated[spec_name] = name
        self.assertEqual(ignored, {name for name in isolated if name.endswith(".spec.ts")})
        for spec_name, suite in isolated.items():
            self.assertEqual(e2e.map_file(f"web/e2e/{spec_name}", STEMS, {}), (f"@{suite}",), spec_name)

    def test_docs_and_scripts_run_nothing_and_say_so(self):
        for files in (["docs/development-workflow.md"], ["devs/scripts/e2e.py", "RELEASING.md"],
                      ["web/src/lib/statDisplay.test.ts", "web/worker-tests/import.test.mjs",
                       "web/scripts/bench-calc.mjs", "web/README.md"], ["crates/pobr-core/tests/aggregation.rs"]):
            with self.subTest(files=files):
                selection = affected(*files)
                self.assertEqual(selection.suites, ())
                self.assertIn("none with a browser or engine effect", selection.reasons[0])

    def test_tree_component_runs_smoke_plus_tree_specs(self):
        selection = affected("web/src/components/tree/TreePanel.tsx")
        self.assertEqual([suite.name for suite in selection.suites], ["affected"])
        grep = main_grep(selection)
        self.assertTrue(grep.startswith("@smoke|"))
        for stem in ("passive-continuity", "tree-keyboard-decisions", "weapon-set-tree", "node-power"):
            self.assertIn(re.escape(f"{stem}.spec.ts"), grep)
        self.assertNotIn("config-controls", grep)
        self.assertIn("TreePanel.tsx", "\n".join(selection.reasons))

    def test_unknown_web_change_runs_the_default_tier(self):
        for name in ("web/src/hooks/useBuildSession.ts", "web/src/locales/en-US.json", "web/e2e/shell.ts",
                     "web/playwright.config.ts", "web/package.json"):
            with self.subTest(name=name):
                self.assertEqual([suite.name for suite in affected(name).suites], ["default"])

    def test_engine_changes_run_smoke_plus_calc_and_import_specs(self):
        for name in ("crates/pobr-core/src/calc/offence.rs", "apps/pobr-wasm/src/lib.rs",
                     "pipeline/data.lock.json", "Cargo.lock"):
            with self.subTest(name=name):
                grep = main_grep(affected(name))
                for stem in ("calcs-breakdown", "import-planning-errors", "data-bump-smoke"):
                    self.assertIn(re.escape(f"{stem}.spec.ts"), grep)
        self.assertEqual(affected("apps/pobr-cli/src/main.rs").suites, ())

    def test_changed_specs_fixtures_and_isolated_configs(self):
        self.assertIn(re.escape("tree-convert.spec.ts"), main_grep(affected("web/e2e/tree-convert.spec.ts")))
        self.assertEqual(affected("web/e2e/removed.spec.ts").suites, ())
        selection = affected("web/e2e/fixtures/controls.tsx")
        self.assertEqual([suite.name for suite in selection.suites], ["affected", "controls"])
        self.assertEqual(main_grep(selection), "@smoke")
        # A component that an isolated fixture mounts also runs that config.
        self.assertEqual([suite.name for suite in affected("web/src/components/calcs/CalcsPanel.tsx").suites],
                         ["affected", "calcs-config"])
        # A repository file a spec reads selects that spec.
        inputs = e2e.spec_inputs(E2E)
        name, readers = next(iter(sorted(inputs.items())))
        grep = main_grep(affected(name))
        self.assertTrue(all(re.escape(f"{stem}.spec.ts") in grep for stem in readers), (name, readers))

    def test_grep_matches_whole_file_names_only(self):
        pattern = re.compile(e2e.grep_for(("tree-convert",), STEMS))
        self.assertTrue(pattern.search("chromium tree-convert.spec.ts converts a tree"))
        self.assertTrue(pattern.search("smoke-ish title @smoke"))
        self.assertFalse(pattern.search("chromium old-tree-convert.spec.ts other"))
        self.assertFalse(pattern.search("chromium passive-jewels.spec.ts title"))

    def test_playwright_arguments_pass_through_with_or_without_separator(self):
        args, extra = e2e.parse(["smoke", "--workers", "2", "--", "--grep", "x"])
        self.assertEqual((args.tier, args.workers, extra), ("smoke", 2, ["--grep", "x"]))
        args, extra = e2e.parse(["default", "tree-convert.spec.ts", "--headed"])
        self.assertEqual((args.tier, args.workers, extra), ("default", e2e.WORKERS, ["tree-convert.spec.ts", "--headed"]))
        self.assertEqual(e2e.parse([])[0].tier, "affected")


class PortTests(unittest.TestCase):
    def test_worktree_ports_are_stable_distinct_and_in_range(self):
        first = e2e.preferred_port("/work/pobr/.claude/worktrees/a")
        self.assertEqual(first, e2e.preferred_port("/work/pobr/.claude/worktrees/a"))
        self.assertNotEqual(first, e2e.preferred_port("/work/pobr/.claude/worktrees/b"))
        for path in ("/a", "/b", "/c/d", e2e.ROOT):
            self.assertIn(e2e.preferred_port(path), e2e.PORTS)
        # The fixed ports of a plain `playwright test` stay outside the runner's range.
        configs = [e2e.ROOT / "web/playwright.config.ts", *(e2e.ROOT / "web" / s.config for s in e2e.ISOLATED.values())]
        fixed = [int(re.search(r"e2eServer\((\d+)\)", path.read_text())[1]) for path in configs]
        self.assertEqual(len(set(fixed)), len(configs))
        self.assertFalse(set(fixed) & set(e2e.PORTS))

    def test_taken_ports_probe_forward_and_wrap(self):
        root = "/work/pobr"
        start = e2e.preferred_port(root)
        self.assertEqual(e2e.free_port(root, lambda port: port != start), start + 1 if start + 1 in e2e.PORTS
                         else e2e.PORTS.start)
        # Past the end of the range the probe wraps to its start.
        self.assertEqual(e2e.free_port(root, lambda port: port == e2e.PORTS.start), e2e.PORTS.start)
        with self.assertRaisesRegex(e2e.Stop, "no free port"):
            e2e.free_port(root, lambda port: False)


class FakeRun:
    """Records commands; answers by the first matching substring of the joined command."""

    def __init__(self, root, answers=None):
        self.root, self.answers, self.calls = root, answers or {}, []

    def __call__(self, command, **kwargs):
        line = " ".join(str(part) for part in command)
        self.calls.append(line)
        if "vite build" in line:
            (self.root / "web/dist").mkdir(parents=True, exist_ok=True)
            (self.root / "web/dist/index.html").write_text(f"build {len(self.calls)}")
        for needle, (code, stdout) in self.answers.items():
            if needle in line:
                return subprocess.CompletedProcess(command, code, stdout, "")
        return subprocess.CompletedProcess(command, 0, "", "")

    def ran(self, needle):
        return any(needle in line for line in self.calls)


class PreflightTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        browsers = self.root / "browsers/chromium-1"
        browsers.mkdir(parents=True)
        files = {
            "pipeline/data.lock.json": json.dumps({"target": {"game_patch": "1.0"}}),
            "data/CURRENT": "1.0\n", "data/1.0/manifest.json": "{}", "data/overlay-common/rule.json": "{}",
            "Cargo.toml": "[workspace.package]\nversion = \"1\"\n",
            "web/pnpm-lock.yaml": "lock", "web/node_modules/.modules.yaml": "modules",
            "web/src/app.ts": "app", "web/public/data/1.0/x.json": "{}",
            "web/src/wasm/pkg/build-receipt.json": "{}", "web/scripts/sync-data.mjs": "sync",
        }
        for name, text in files.items():
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            (self.root / name).write_text(text)
        self.answers = {
            "--dry-run chromium": (0, f"Chrome\n  Install location:    {browsers}\n"),
            "wasm-cache-key.mjs": (0, "key=abc\n"),
            "--git-common-dir": (0, f"{self.root}/.git\n"),
        }

    def inputs(self, **answers):
        run = FakeRun(self.root, {**self.answers, **answers})
        return e2e.Inputs(self.root, env={}, run=run), run

    def prepared(self):
        inputs, run = self.inputs()
        with contextlib.redirect_stdout(io.StringIO()):
            inputs.prepare()
        return run

    def test_first_run_refreshes_everything_and_the_second_nothing(self):
        run = self.prepared()
        for needle in ("build_data --install data --offline", "check_data_ownership", "pnpm sync-data",
                       "build-wasm.mjs", "vite build"):
            self.assertTrue(run.ran(needle), needle)
        self.assertFalse(run.ran("tsc"))
        self.assertFalse(run.ran("pnpm --dir web install"))
        again = self.prepared()
        for needle in ("build_data", "sync-data", "build-wasm.mjs", "vite build"):
            self.assertFalse(again.ran(needle), needle)

    def test_a_source_edit_rebuilds_only_dist(self):
        self.prepared()
        (self.root / "web/src/app.ts").write_text("edited")
        run = self.prepared()
        self.assertTrue(run.ran("vite build"))
        self.assertFalse(run.ran("build-wasm.mjs"))
        self.assertFalse(run.ran("sync-data"))

    def test_a_replaced_dist_or_new_wasm_identity_is_stale(self):
        self.prepared()
        (self.root / "web/dist/index.html").write_text("someone else's build")
        self.assertTrue(self.prepared().ran("vite build"))
        self.answers["wasm-cache-key.mjs"] = (0, "key=changed\n")
        run = self.prepared()
        self.assertTrue(run.ran("build-wasm.mjs"))
        self.assertFalse(run.ran("vite build"))

    def test_a_lock_change_reinstalls_and_resyncs_data(self):
        self.prepared()
        (self.root / "pipeline/data.lock.json").write_text(json.dumps({"target": {"game_patch": "1.0"}, "x": 1}))
        run = self.prepared()
        self.assertTrue(run.ran("build_data --install data --offline"))
        self.assertTrue(run.ran("sync-data"))
        self.assertTrue(run.ran("vite build"))

    def test_missing_node_modules_installs_offline(self):
        (self.root / "web/node_modules/.modules.yaml").unlink()
        self.assertTrue(self.prepared().ran("pnpm --dir web install --offline --frozen-lockfile"))
        # A lockfile newer than the installed tree (e.g. after a pull) installs again.
        marker = self.root / "web/node_modules/.modules.yaml"
        marker.write_text("modules")
        os.utime(marker, (0, 0))
        self.assertTrue(self.prepared().ran("pnpm --dir web install --offline --frozen-lockfile"))
        inputs, _ = self.inputs(**{"pnpm --dir web install": (1, "")})
        with self.assertRaisesRegex(e2e.Stop, "pnpm --dir web install --frozen-lockfile"), \
                contextlib.redirect_stdout(io.StringIO()):
            inputs.prepare()

    def test_problems_without_a_cheap_fix_stop_with_the_command_to_run(self):
        cases = {
            "--dry-run chromium": ((0, "  Install location:    /nonexistent/chromium\n"), "mise run setup:e2e"),
            "build_data": ((1, ""), "mise run data:prepare"),
            "build-wasm.mjs": ((1, ""), "node web/scripts/build-wasm.mjs"),
            "vite build": ((1, ""), "vite build"),
        }
        for needle, (answer, message) in cases.items():
            with self.subTest(needle=needle):
                inputs, _ = self.inputs(**{needle: answer})
                with self.assertRaisesRegex(e2e.Stop, re.escape(message)), contextlib.redirect_stdout(io.StringIO()):
                    inputs.prepare()

    def test_empty_rust_target_in_a_worktree_prints_the_warm_hint(self):
        self.answers["--git-common-dir"] = (0, "/checkouts/main/.git\n")
        inputs, run = self.inputs()
        with contextlib.redirect_stdout(io.StringIO()) as out:
            inputs.prepare()
        self.assertIn("mise run warm /checkouts/main", out.getvalue())
        self.assertFalse(run.ran("warm"))


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.git("init", "-q")
        for name, text in {"pipeline/data.lock.json": "{}",
                           "web/node_modules/@playwright/test/package.json": '{"version": "1.2.3"}'}.items():
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            (self.root / name).write_text(text)
        (self.root / ".gitignore").write_text("web/node_modules/\n.cache/\n")
        self.commit("first")
        self.commit("second")

    def git(self, *args):
        return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *args], cwd=self.root,
                              check=True, capture_output=True, text=True).stdout.strip()

    def commit(self, message):
        (self.root / "file.txt").write_text(message)
        self.git("add", ".")
        self.git("commit", "-qm", message)

    def results(self, failed=0):
        return [e2e.Result(suite.name, int(failed > 0), 3, failed, 0, 0, 1.5) for suite in e2e.RELEASE]

    def write(self, receipt):
        path = e2e.receipt_path(receipt["tree"], self.root)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(receipt))

    def receipt(self, results=None, after=None):
        before = e2e.snapshot(self.root)
        return e2e.release_receipt(before, after or before, results or self.results(), 4, "s", "f", self.root)

    def check(self, rev):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            code = e2e.release_check(rev, self.root)
        return code, out.getvalue()

    def test_receipt_is_keyed_by_the_tree_and_records_the_run(self):
        receipt = self.receipt()
        tree = self.git("rev-parse", "HEAD^{tree}")
        self.assertTrue(receipt["passed"])
        self.assertEqual((receipt["tree"], receipt["commit"]), (tree, self.git("rev-parse", "HEAD")))
        self.assertEqual(e2e.receipt_path(tree, self.root).name, f"release-{tree}.json")
        self.assertEqual((receipt["playwright"], receipt["workers"]), ("1.2.3", 4))
        self.assertEqual(len(receipt["data_lock_sha256"]), 64)
        self.assertEqual([suite["name"] for suite in receipt["suites"]],
                         ["matrix", "calcs-config", "item-display", "controls"])

    def test_check_passes_for_the_tree_and_fails_for_another(self):
        self.write(self.receipt())
        self.assertEqual(self.check("HEAD")[0], 0)
        code, out = self.check("HEAD~1")
        self.assertEqual(code, 1)
        self.assertIn("mise run e2e release", out)
        self.assertEqual(self.check("no-such-rev")[0], 1)

    def test_a_merge_commit_with_the_same_tree_matches(self):
        self.write(self.receipt())
        self.git("commit", "-q", "--allow-empty", "-m", "merge-like commit, same tree")
        self.assertEqual(self.check("HEAD")[0], 0)

    def test_failed_dirty_or_moved_runs_never_pass(self):
        self.assertFalse(self.receipt(self.results(failed=1))["passed"])
        moved = {**e2e.snapshot(self.root), "tree": "0" * 40}
        self.assertFalse(self.receipt(after=moved)["passed"])
        (self.root / "file.txt").write_text("uncommitted")
        dirty = self.receipt()
        self.assertFalse(dirty["clean"])
        self.assertFalse(dirty["passed"])

    def test_check_rejects_incomplete_or_tampered_receipts(self):
        good = self.receipt()
        for broken in ({**good, "suites": good["suites"][:1]}, {**good, "passed": False}, {**good, "clean": False},
                       {**good, "schema": "other"},
                       {**good, "suites": [{**suite, "failed": 1} for suite in good["suites"]]}):
            with self.subTest(broken=broken):
                self.write(broken)
                self.assertEqual(self.check("HEAD")[0], 1)


if __name__ == "__main__":
    unittest.main()
