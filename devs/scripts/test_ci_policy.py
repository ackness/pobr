"""Behavioral tests for check routing, failure propagation and agent handoff."""

import importlib.util
import json
import os
import sys
from pathlib import Path
import subprocess
import tempfile
import unittest


HERE = Path(__file__).resolve().parent


def load(filename):
    spec = importlib.util.spec_from_file_location(filename, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


plan = load("ci-plan.py")
gate = load("ci-gate.py")
ready = load("pr-ready.py")
tool_pins = load("tool-versions.py")
gh_installer = load("install-gh.py")
actionlint_installer = load("install-actionlint.py")


class RoutingTests(unittest.TestCase):
    def test_docs_only_and_known_areas(self):
        self.assertFalse(any(plan.classify([
            "docs/new-guide.md", "web/README.md", "crates/pobr-core/README.md",
            "apps/pobr-wasm/README.md", "examples/reference-builds/README.md",
            "tools/pob2-oracle/README.md", ".claude/skills/workflow/SKILL.md",
            "apps/pobr-wasm/tests/fixtures/pob2-planner.md",
            "web/worker-tests/fixtures/README.md",
        ]).values()))
        self.assertTrue(plan.classify(["crates/pobr-core/src/calc.rs"])["rust"])
        self.assertTrue(plan.classify(["web/src/App.tsx"])["web"])
        self.assertTrue(plan.classify(["pipeline/data_snapshot.py"])["full"])
        self.assertTrue(plan.classify(["web/public/_worker.js"])["scripts"])

    def test_shared_or_unknown_inputs_select_full_without_duplicate_light_checks(self):
        for name in ["Cargo.lock", "crates/new/Cargo.toml", "mise.toml", "web/package.json",
                     ".github/workflows/ci.yml", "devs/scripts/ci-plan.py",
                     "devs/scripts/tool-versions.py", "devs/scripts/install-actionlint.py",
                     "devs/scripts/setup-environment.sh", "new-runtime.conf", "docs/hidden-executable.py"]:
            with self.subTest(name=name):
                self.assertEqual(plan.classify([name, "web/src/App.tsx"]),
                                 {area: area == "full" for area in plan.AREAS})

    def test_data_text_and_removed_source_are_not_documentation(self):
        self.assertTrue(plan.classify(["examples/build/code.txt"])["rust"])
        self.assertTrue(plan.classify(["web/worker-tests/fixtures/build-code.txt"])["web"])
        self.assertTrue(plan.classify(["web/worker-tests/fixtures/build-code.txt"])["scripts"])
        self.assertTrue(plan.classify(["data/test/manifest.json"])["rust"])
        self.assertTrue(plan.classify(["web/e2e/fixtures/item.md"])["web"])
        self.assertTrue(plan.classify(["crates/pobr-core/tests/fixtures/mod.md"])["rust"])
        for name in ("pipeline/fixtures/input.md", "pipeline/fixtures/input.txt",
                     "devs/fixtures/input.md", "devs/fixtures/input.txt"):
            with self.subTest(name=name):
                self.assertTrue(plan.classify([name])["full"])

    def test_pr_is_quick_and_master_integrates_the_same_code_range(self):
        for files, areas in ((["crates/pobr-core/src/lib.rs"], {"rust"}),
                             (["web/src/App.tsx"], {"web"}),
                             (["devs/scripts/lookup.py"], {"scripts"}),
                             (["data/current/base/items.json"], {"rust", "scripts"}),
                             (["Cargo.lock"], {"rust", "web", "scripts"}),
                             ([".github/workflows/pr.yml"], {"rust", "web", "scripts"}),
                             (["unknown-runtime.conf"], {"rust", "web", "scripts"})):
            with self.subTest(files=files):
                self.assertEqual(plan.plan_for_event(files, "pull_request"),
                                 {area: area in areas for area in plan.AREAS})
                self.assertEqual(plan.plan_for_event(files, "push"),
                                 {area: area == "full" for area in plan.AREAS})
        for event in ("pull_request", "push"):
            for name in ("docs/new-guide.md", "web/worker-tests/fixtures/README.md",
                         "apps/pobr-wasm/tests/fixtures/pob2-planner.md"):
                self.assertFalse(any(plan.plan_for_event([name], event).values()))
            self.assertFalse(any(plan.plan_for_event([], event).values()))

    def test_missing_pr_base_fails_and_initial_master_push_selects_full(self):
        with self.assertRaisesRegex(ValueError, "PR comparison base unavailable"):
            plan.plan_for_event(None, "pull_request")
        self.assertEqual(plan.plan_for_event(None, "push"),
                         {area: area == "full" for area in plan.AREAS})

    def test_git_ranges_include_early_commits_deletions_and_both_rename_sides(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
            git("init", "-q")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            (root / "initial.md").write_text("base")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (root / "web").mkdir()
            (root / "web/runtime.js").write_text("runtime")
            git("add", ".")
            git("commit", "-qm", "code")
            (root / "initial.md").write_text("docs")
            git("add", ".")
            git("commit", "-qm", "docs last")
            self.assertTrue(plan.classify(plan.changed_files(base, cwd=root))["web"])
            previous = git("rev-parse", "HEAD")
            git("mv", "web/runtime.js", "runtime.md")
            git("commit", "-qm", "move to docs")
            names = plan.changed_files(previous, cwd=root)
            self.assertIn("web/runtime.js", names)
            self.assertIn("runtime.md", names)
            self.assertTrue(plan.classify(names)["web"])
            # A non-ancestor comparison is valid for a force push too.
            self.assertTrue(plan.changed_files("HEAD", base, cwd=root))
            self.assertIsNone(plan.changed_files("unavailable", cwd=root))
            self.assertIsNone(plan.changed_files("0" * 40, cwd=root))


class GateTests(unittest.TestCase):
    def needs(self, files=(), event="pull_request"):
        outputs = {name: str(value).lower() for name, value in plan.plan_for_event(files, event).items()}
        return {"changes": {"result": "success", "outputs": outputs},
                **{name: {"result": "success" if selected == "true" else "skipped"}
                   for name, selected in outputs.items()}}

    def test_expected_success_and_explicit_skips(self):
        for files in [[], ["web/src/App.tsx"], ["Cargo.lock"], ["pipeline/a.py"]]:
            for event in ("pull_request", "push"):
                with self.subTest(files=files, event=event):
                    gate.check(self.needs(files, event), "pr")
        gate.check({name: {"result": "success"} for name in ("lint", "ci", "web", "scripts")}, "full")

    def test_selected_failure_cancel_skip_or_missing_never_passes(self):
        for name in plan.AREAS:
            for result in ["failure", "cancelled", "skipped", None]:
                needs = self.needs()
                needs["changes"]["outputs"][name] = "true"
                needs[name]["result"] = result
                with self.subTest(name=name, result=result), self.assertRaises(ValueError):
                    gate.check(needs, "pr")

    def test_failed_detector_bad_outputs_and_unselected_failed_job_reject(self):
        for mutate in [lambda n: n["changes"].update(result="failure"),
                       lambda n: n["changes"]["outputs"].pop("full"),
                       lambda n: n["changes"]["outputs"].update(full="maybe"),
                       lambda n: n["web"].update(result="cancelled"),
                       lambda n: n.pop("rust")]:
            needs = self.needs()
            mutate(needs)
            with self.assertRaises(ValueError):
                gate.check(needs, "pr")
        for result in ["skipped", "failure", "cancelled"]:
            needs = {name: {"result": "success"} for name in ("lint", "ci", "web", "scripts")}
            needs["ci"]["result"] = result
            with self.assertRaises(ValueError):
                gate.check(needs, "full")


class ToolVersionTests(unittest.TestCase):
    def test_active_version_check_rejects_a_shadowed_executable(self):
        from types import SimpleNamespace
        values = {key: "1.2.3" for key in tool_pins.COMMANDS}
        tool_pins.check_active(values, run=lambda *a, **kw: SimpleNamespace(stdout="tool 1.2.3", stderr=""))
        with self.assertRaisesRegex(ValueError, "expected 1.2.3, got 9.8.7"):
            tool_pins.check_active(values, run=lambda *a, **kw: SimpleNamespace(stdout="tool 9.8.7", stderr=""))

    def test_exports_authoritative_pins_and_rejects_floating_or_missing_versions(self):
        config = {"tools": {key: "1.2.3" for key in tool_pins.TOOLS.values()}}
        rust = {"toolchain": {"channel": "1.99.0", "components": ["rustfmt", "clippy"],
                              "targets": ["wasm32-unknown-unknown"]}}
        values = tool_pins.versions(config, rust)
        self.assertEqual(values["wasm_pack"], "1.2.3")
        self.assertEqual(values["rust_components"], "rustfmt,clippy")
        self.assertEqual(values["rust_targets"], "wasm32-unknown-unknown")
        for value in ("latest", "26", "26.13", "26.13.0-rc.1"):
            config["tools"]["node"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                tool_pins.versions(config, rust)
        config["tools"].pop("node")
        with self.assertRaises(KeyError):
            tool_pins.versions(config, rust)


class GhInstallerTests(unittest.TestCase):
    def test_official_asset_checksum_and_expected_binary(self):
        import hashlib, io, tarfile
        payload = b"#!/bin/sh\necho fixture\n"
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as bundle:
            info = tarfile.TarInfo("gh_1.2.3_linux_amd64/bin/gh")
            info.size = len(payload)
            bundle.addfile(info, io.BytesIO(payload))
        data = archive.getvalue()
        checksum = f"{hashlib.sha256(data).hexdigest()}  gh_1.2.3_linux_amd64.tar.gz\n".encode()
        requests = []
        def fetch(url):
            requests.append(url)
            return checksum if url.endswith("checksums.txt") else data
        with tempfile.TemporaryDirectory() as root:
            binary = gh_installer.install("1.2.3", "X64", root, fetch) / "gh"
            self.assertEqual(binary.read_bytes(), payload)
            self.assertTrue(binary.stat().st_mode & 0o111)
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                gh_installer.install("1.2.3", "X64", root,
                    lambda url: checksum if url.endswith("checksums.txt") else b"corrupt")
        self.assertTrue(all(url.startswith("https://github.com/cli/cli/releases/download/v1.2.3/") for url in requests))


class ActionlintInstallerTests(unittest.TestCase):
    def release(self, architecture, *, member="actionlint", symlink=False):
        import hashlib, io, tarfile
        payload = b"#!/bin/sh\necho fixture\n"
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as bundle:
            info = tarfile.TarInfo(member)
            if symlink:
                info.type = tarfile.SYMTYPE
                info.linkname = "../outside"
            else:
                info.size = len(payload)
            bundle.addfile(info, None if symlink else io.BytesIO(payload))
        data = archive.getvalue()
        asset = f"actionlint_1.2.3_linux_{architecture}.tar.gz"
        checksum = f"{hashlib.sha256(data).hexdigest()}  {asset}\n".encode()
        return payload, data, checksum

    def test_official_assets_install_only_the_verified_binary_for_both_runners(self):
        for runner, architecture in (("X64", "amd64"), ("ARM64", "arm64")):
            with self.subTest(runner=runner), tempfile.TemporaryDirectory() as root:
                payload, archive, checksum = self.release(architecture)
                requests = []
                def fetch(url):
                    requests.append(url)
                    return checksum if url.endswith("checksums.txt") else archive
                binary = actionlint_installer.install("1.2.3", runner, root, fetch) / "actionlint"
                self.assertEqual(binary.read_bytes(), payload)
                self.assertTrue(binary.stat().st_mode & 0o111)
                base = "https://github.com/rhysd/actionlint/releases/download/v1.2.3/"
                self.assertEqual(requests, [base + "actionlint_1.2.3_checksums.txt",
                                           base + f"actionlint_1.2.3_linux_{architecture}.tar.gz"])

    def test_invalid_releases_do_not_replace_an_existing_binary(self):
        _, archive, checksum = self.release("amd64")
        _, missing, missing_sum = self.release("amd64", member="README.md")
        _, linked, linked_sum = self.release("amd64", symlink=True)
        for data, sums, error in ((b"corrupt", checksum, ValueError),
                                  (archive, b"", ValueError),
                                  (missing, missing_sum, KeyError),
                                  (linked, linked_sum, ValueError)):
            with self.subTest(error=error, data=data[:10]), tempfile.TemporaryDirectory() as root:
                binary = Path(root) / "bin/actionlint"
                binary.parent.mkdir()
                binary.write_bytes(b"existing")
                with self.assertRaises(error):
                    actionlint_installer.install("1.2.3", "X64", root,
                        lambda url: sums if url.endswith("checksums.txt") else data)
                self.assertEqual(binary.read_bytes(), b"existing")


class ReadyTests(unittest.TestCase):
    def test_cli_is_read_only_and_rejects_a_head_change_during_preflight(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gh = root / "gh"
            gh.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["CALLS"], "a") as out: out.write(json.dumps(args) + "\\n")
if args[:2] == ["repo", "view"]:
    print(json.dumps({"nameWithOwner": "fixture/repo"}))
elif args[:2] == ["pr", "checks"]:
    print(json.dumps([{"name": "PR gate", "bucket": "pass"}]))
elif args[:1] == ["api"]:
    count_file = pathlib.Path(os.environ["COUNT"])
    count = int(count_file.read_text()) if count_file.exists() else 0
    count_file.write_text(str(count + 1))
    head = "c" * 40 if os.environ.get("RACE") and count else "a" * 40
    print(json.dumps({"draft": False, "state": "open", "mergeable": True,
                      "body": "Free-form description.", "head": {"sha": head}, "base": {"sha": "b" * 40}}))
else:
    raise SystemExit("Unexpected write operation")
""")
            gh.chmod(0o755)
            env = {"PATH": str(root) + os.pathsep + os.environ["PATH"],
                   "CALLS": str(root / "calls"), "COUNT": str(root / "count")}
            command = [sys.executable, str(HERE / "pr-ready.py"), "123"]
            result = subprocess.run(command, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("a" * 40, result.stdout)
            (root / "count").unlink()
            result = subprocess.run(command, env={**env, "RACE": "1"}, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            calls = [json.loads(line) for line in (root / "calls").read_text().splitlines()]
            self.assertTrue(all(args[0] in {"repo", "pr", "api"} and args[:2] != ["pr", "merge"] for args in calls))

    def test_preflight_rejects_unknown_or_bad_required_checks(self):
        pr = {"draft": False, "head": {"sha": "a" * 40}, "base": {"sha": "b" * 40}}
        pr.update(state="open", mergeable=True)
        checks = [{"name": name, "bucket": "pass"} for name in ready.REQUIRED]
        ready.assess(pr, checks)
        for bucket in ("skip", "pending", "cancel", "fail"):
            with self.subTest(bucket=bucket), self.assertRaises(ValueError):
                ready.assess(pr, [{"name": name, "bucket": bucket} for name in ready.REQUIRED])
        with self.assertRaises(ValueError):
            ready.assess(pr, [])
        pr["mergeable"] = None
        with self.assertRaises(ValueError):
            ready.assess(pr, checks)


if __name__ == "__main__":
    unittest.main()
