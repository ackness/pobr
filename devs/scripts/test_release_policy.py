"""Release-policy unit and CLI regressions using local fixtures, never the network."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("release_policy", SCRIPTS / "release-policy.py")
policy = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = policy
SPEC.loader.exec_module(policy)


class SemVerTests(unittest.TestCase):
    def test_precedence(self):
        versions = [
            "0.1.0-alpha", "0.1.0-alpha.1", "0.1.0-alpha.beta",
            "0.1.0-beta", "0.1.0-beta.2", "0.1.0-beta.11",
            "0.1.0-rc.2", "0.1.0-rc.10", "0.1.0", "0.2.0", "0.10.0", "1.0.0",
        ]
        for before, after in zip(versions, versions[1:]):
            with self.subTest(before=before, after=after):
                self.assertLess(policy.Version.parse(before).precedence(),
                                policy.Version.parse(after).precedence())

    def test_build_metadata_does_not_affect_precedence(self):
        for version in ("0.1.0", "0.1.0-rc.2"):
            self.assertEqual(policy.Version.parse(version).precedence(),
                             policy.Version.parse(version + "+001.build-2").precedence())

    def test_malformed_versions(self):
        for version in (
            "", "v0.1.0", "0.1", "00.1.0", "0.01.0", "0.1.00", "-1.0.0",
            "0.1.0-01", "0.1.0-rc.01", "0.1.0-", "0.1.0+", "0.1.0-a..b",
            "0.1.0+foo..bar", "0.1.0_rc1", "0.1.0\n", " 0.1.0", "０.1.0", 1,
        ):
            with self.subTest(version=version), self.assertRaises(policy.PolicyError):
                policy.Version.parse(version)

    def test_production_tags(self):
        for tag, production in (
            ("v0.10.23", True), ("v0.1.0-rc.2", False),
            ("v0.1.0+build", False), ("v1.0.0", False),
        ):
            with self.subTest(tag=tag):
                self.assertEqual(policy.tag_version(tag).is_production(), production)
        for tag in ("0.1.0", "V0.1.0", "v00.1.0", "v0.1", "vv0.1.0"):
            with self.subTest(tag=tag), self.assertRaises(policy.PolicyError):
                policy.tag_version(tag)


class ReleasePolicyTests(unittest.TestCase):
    inherited = ("fixture-root", "pobr-cli", "pobr-wasm", "new-release-tool")

    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="pobr release policy ")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.real_git = shutil.which("git")
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith("GIT_")}
        self.env.update({
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0", "REAL_GIT": self.real_git,
            "REMOTE_OUTPUT": "", "REMOTE_EXIT": "0",
            "REMOTE_CALLS": str(self.root / "remote-calls.jsonl"),
            "GITHUB_OUTPUT": str(self.root / "github-output"),
        })
        for name in ("release-policy.py", "release-check.sh"):
            dest = self.root / "devs/scripts" / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(SCRIPTS / name, dest)
        for directory, name, inherited in (
            ("apps/cli", "pobr-cli", True), ("apps/wasm", "pobr-wasm", True),
            ("tools/new", "new-release-tool", True),
            ("apps/excluded", "excluded", True), ("crates/library", "library", False),
        ):
            version = "version.workspace = true" if inherited else 'version = "8.0.0"'
            self.write(f"{directory}/Cargo.toml", f'[package]\nname = "{name}"\n{version}\n')
        self.write("bin/git", '''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
if args[0] == "ls-remote":
    with open(os.environ["REMOTE_CALLS"], "a") as output:
        output.write(json.dumps(args) + "\\n")
    if os.environ["REMOTE_EXIT"] != "0":
        print("mock remote lookup failed", file=sys.stderr)
        sys.exit(int(os.environ["REMOTE_EXIT"]))
    print(os.environ["REMOTE_OUTPUT"], end="")
    sys.exit(0)
if args[0] not in ("show", "rev-parse"):
    sys.exit("unexpected git operation: " + repr(args))
os.execv(os.environ["REAL_GIT"], [os.environ["REAL_GIT"], *args])
''')
        (self.root / "bin/git").chmod(0o755)
        self.env["PATH"] = str(self.root / "bin") + os.pathsep + self.env["PATH"]
        self.git("init", "-q")
        self.git("config", "user.name", "Release Policy Tests")
        self.git("config", "user.email", "tests@example.invalid")
        self.base = self.set_version("0.0.9")

    def write(self, path, text):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def git(self, *args):
        return subprocess.run([self.real_git, *args], cwd=self.root, env=self.env,
                              capture_output=True, text=True, check=True).stdout.strip()

    def set_version(self, version):
        self.write("Cargo.toml", f'''[workspace]
members = ["apps/*", "tools/*", "crates/library"]
exclude = ["apps/excluded"]
[workspace.package]
version = {json.dumps(version)} # Read TOML, not line-oriented text.
[package]
name = "fixture-root"
version.workspace = true
''')
        self.write_lock(version)
        self.git("add", "Cargo.toml", "Cargo.lock", "apps", "tools", "crates")
        self.git("commit", "-qm", "fixture version " + version)
        return self.git("rev-parse", "HEAD")

    def write_lock(self, version, overrides=None, omitted=()):
        versions = {name: version for name in self.inherited}
        versions.update(overrides or {})
        versions["library"] = "8.0.0"
        self.write("Cargo.lock", "version = 4\n" + "".join(
            f'[[package]]\nname = "{name}"\nversion = "{value}"\n'
            for name, value in versions.items() if name not in omitted
        ))

    def run_policy(self, *args, wrapper=False, reset_output=True):
        if reset_output:
            (self.root / "github-output").unlink(missing_ok=True)
        script = "release-check.sh" if wrapper else "release-policy.py"
        command = "bash" if wrapper else sys.executable
        return subprocess.run(
            [command, str(self.root / "devs/scripts" / script), *args],
            cwd=self.root / "apps/cli", env=self.env,
            capture_output=True, text=True, timeout=10,
        )

    def assert_output(self, result, output):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, output + "\n")
        self.assertEqual((self.root / "github-output").read_text(), output + "\n")

    def assert_error(self, result, message):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(message, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse((self.root / "github-output").exists())

    def remote(self, tag, sha, annotated=False):
        tag_sha = "a" * 40 if annotated else sha
        self.env["REMOTE_OUTPUT"] = f"{tag_sha}\trefs/tags/{tag}\n"
        if annotated:
            self.env["REMOTE_OUTPUT"] += f"{sha}\trefs/tags/{tag}^{{}}\n"

    def test_unchanged_skips_lock_and_remote(self):
        (self.root / "Cargo.lock").unlink()
        self.env["REMOTE_EXIT"] = "128"
        self.assert_output(self.run_policy("bump", self.base), "release=false")
        self.assertFalse((self.root / "remote-calls.jsonl").exists())

    def test_bump_missing_remote_tag(self):
        self.set_version("0.0.10")
        self.assert_output(self.run_policy("bump", self.base), "release=true")
        self.assertEqual(json.loads((self.root / "remote-calls.jsonl").read_text()),
                         ["ls-remote", "--tags", "origin", "refs/tags/v0.0.10", "refs/tags/v0.0.10^{}"])

    def test_bump_remote_errors_are_fatal_including_retry(self):
        self.set_version("0.0.10")
        for code in ("2", "128"):
            self.env["REMOTE_EXIT"] = code
            for flags in ((), ("--allow-existing-tag",)):
                with self.subTest(code=code, flags=flags):
                    self.assert_error(self.run_policy("bump", self.base, *flags), "ls-remote failed")

    def test_bump_rejects_downgrade(self):
        self.set_version("0.0.8")
        self.assert_error(self.run_policy("bump", self.base), "must increase")

    def test_bump_prerelease_progression(self):
        self.base = self.set_version("0.1.0-rc.2")
        self.set_version("0.1.0-rc.10")
        self.assert_output(self.run_policy("bump", self.base), "release=true")
        self.base = self.git("rev-parse", "HEAD")
        self.set_version("0.1.0")
        self.assert_output(self.run_policy("bump", self.base), "release=true")
        self.base = self.git("rev-parse", "HEAD")
        self.set_version("0.1.0-rc.11")
        self.assert_error(self.run_policy("bump", self.base), "must increase")

    def test_bump_metadata_only_changes_are_not_increases(self):
        self.base = self.set_version("0.1.0+build.1")
        self.set_version("0.1.0+build.2")
        self.assert_error(self.run_policy("bump", self.base), "must increase")
        self.set_version("0.1.1+build.001")
        self.assert_output(self.run_policy("bump", self.base), "release=true")

    def test_bump_rejects_malformed_current_and_base(self):
        self.set_version("0.1.0-rc.01")
        self.assert_error(self.run_policy("bump", self.base), "leading zero")
        self.base = self.git("rev-parse", "HEAD")
        self.set_version("0.1.0")
        self.assert_error(self.run_policy("bump", self.base), "leading zero")

    def test_every_inherited_package_is_required_at_new_version(self):
        head = self.set_version("0.0.10")
        self.git("tag", "v0.0.10")
        for name in self.inherited:
            for missing in (False, True):
                with self.subTest(name=name, missing=missing):
                    self.write_lock("0.0.10", overrides={name: "0.0.9"},
                                    omitted=(name,) if missing else ())
                    self.assert_error(self.run_policy("bump", self.base), name)
                    self.assert_error(self.run_policy("tag", "v0.0.10", head), name)

    def test_external_package_cannot_satisfy_local_lock_entry(self):
        self.set_version("0.0.10")
        self.write_lock("0.0.10", omitted=("pobr-wasm",))
        with (self.root / "Cargo.lock").open("a") as lock:
            lock.write('[[package]]\nname = "pobr-wasm"\nversion = "0.0.10"\n'
                       'source = "registry+https://example.invalid/index"\n')
        self.assert_error(self.run_policy("bump", self.base), "pobr-wasm")

    def test_existing_tags_require_explicit_same_head_retry(self):
        head = self.set_version("0.0.10")
        for annotated in (False, True):
            with self.subTest(annotated=annotated):
                self.remote("v0.0.10", head, annotated)
                self.assert_error(self.run_policy("bump", self.base), "already exists")
                self.assert_output(self.run_policy("bump", self.base, "--allow-existing-tag"),
                                   "release=true")
                self.remote("v0.0.10", self.base, annotated)
                self.assert_error(self.run_policy("bump", self.base, "--allow-existing-tag"),
                                  "already exists")

    def test_tag_peels_lightweight_and_annotated_tags(self):
        head = self.set_version("0.0.10")
        for annotated in (False, True):
            with self.subTest(annotated=annotated):
                self.git("tag", "-f", *(["-a", "-m", "release"] if annotated else []), "v0.0.10")
                self.assert_output(self.run_policy("tag", "v0.0.10", head), "production=true")
                self.assert_error(self.run_policy("tag", "v0.0.10", self.base), "expected commit")
        self.assertFalse((self.root / "remote-calls.jsonl").exists())

    def test_tag_valid_nonproduction_versions(self):
        for version in ("0.1.0-rc.2", "0.1.0+build.001", "1.0.0"):
            with self.subTest(version=version):
                head = self.set_version(version)
                self.git("tag", "v" + version)
                self.assert_output(self.run_policy("tag", "v" + version, head), "production=false")

    def test_tag_requires_exact_workspace_version(self):
        head = self.set_version("0.1.0+build.1")
        self.git("tag", "v0.1.0+build.2")
        self.assert_error(self.run_policy("tag", "v0.1.0+build.2", head), "does not match")

    def test_tag_missing_or_moved_is_fatal(self):
        head = self.set_version("0.0.10")
        self.assert_error(self.run_policy("tag", "v0.0.10", head), "rev-parse failed")
        self.git("tag", "v0.0.10", self.base)
        self.assert_error(self.run_policy("tag", "v0.0.10", head), "expected commit")

    def test_tag_checkout_must_match_expected_sha(self):
        head = self.set_version("0.0.10")
        self.git("tag", "v0.0.10")
        self.git("commit", "--allow-empty", "-qm", "later checkout")
        self.assert_error(self.run_policy("tag", "v0.0.10", head), "checkout HEAD")

    def test_publication_rechecks_remote_identity_for_stable_and_prerelease(self):
        for tag in ("v0.0.10", "v0.0.11-rc.1"):
            head = self.set_version(tag[1:])
            for annotated in (False, True):
                with self.subTest(tag=tag, annotated=annotated):
                    self.remote(tag, head, annotated)
                    self.assert_output(self.run_policy("publication", tag, head), "verified=true")
                    self.remote(tag, self.base, annotated)
                    self.assert_error(self.run_policy("publication", tag, head), "missing or moved")
        self.env["REMOTE_EXIT"] = "128"
        self.assert_error(self.run_policy("publication", tag, head), "ls-remote failed")

    def test_deployment_selects_highest_canonical_stable_zero_major_tag(self):
        head = self.set_version("0.0.10")
        self.remote("v0.0.10", head, annotated=True)
        self.env["REMOTE_OUTPUT"] += "".join(
            f"{self.base}\trefs/tags/{tag}\n" for tag in
            ("v0.0.9", "v0.0.11-rc.10", "v0.0.11+build", "v1.0.0", "v0.00.11", "v9")
        )
        self.assert_output(self.run_policy("deployment", "v0.0.10", head), "deploy=true")
        self.assert_output(self.run_policy("deployment", "v0.0.10", head), "deploy=true")
        self.assert_output(self.run_policy("deployment", "v0.0.9", self.base), "deploy=false")
        self.env["REMOTE_OUTPUT"] += f"{head}\trefs/tags/v0.0.11\n"
        self.assert_output(self.run_policy("deployment", "v0.0.10", head), "deploy=false")
        self.assert_output(self.run_policy("deployment", "v0.0.11", head), "deploy=true")
        calls = (self.root / "remote-calls.jsonl").read_text().splitlines()
        self.assertTrue(all(json.loads(call) == ["ls-remote", "--tags", "origin", "refs/tags/v*"]
                            for call in calls))

    def test_deployment_nonproduction_is_false_even_without_stable_tags(self):
        for tag in ("v0.1.0-rc.2", "v0.1.0+build", "v1.0.0"):
            with self.subTest(tag=tag):
                self.remote(tag, self.base)
                self.assert_output(self.run_policy("deployment", tag, self.base), "deploy=false")

    def test_deployment_remote_missing_moved_or_error_is_fatal(self):
        head = self.set_version("0.0.10")
        self.assert_error(self.run_policy("deployment", "v0.0.10", head), "missing or moved")
        for annotated in (False, True):
            self.remote("v0.0.10", self.base, annotated)
            self.assert_error(self.run_policy("deployment", "v0.0.10", head), "missing or moved")
        self.remote("v0.0.10", head)
        self.env["REMOTE_EXIT"] = "128"
        self.assert_error(self.run_policy("deployment", "v0.0.10", head), "ls-remote failed")

    def test_malformed_remote_output_is_fatal(self):
        for output in ("not a ref\n", f"{self.base}\trefs/tags/v0.0.9^{{}}\n",
                       f"{self.base}\trefs/tags/v0.0.9\n{'a' * 40}\trefs/tags/v0.0.9\n"):
            with self.subTest(output=output):
                self.env["REMOTE_OUTPUT"] = output
                result = self.run_policy("deployment", "v0.0.9", self.base)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertEqual(result.stdout, "")

    def test_sha_arguments_require_full_object_ids(self):
        for command in ("tag", "publication", "deployment"):
            self.assert_error(self.run_policy(command, "v0.0.9", "HEAD"), "full commit SHA")

    def test_invalid_base_is_fatal(self):
        self.assert_error(self.run_policy("bump", "--", "--help"), "rev-parse failed")

    def test_legacy_wrapper_detects_without_validation_or_network(self):
        self.env["REMOTE_EXIT"] = "128"
        self.assert_output(self.run_policy("--detect-only", self.base, wrapper=True), "release=false")
        self.set_version("0.0.8")
        (self.root / "Cargo.lock").unlink()
        self.assert_output(self.run_policy("--detect-only", self.base, wrapper=True), "release=true")
        self.assertFalse((self.root / "remote-calls.jsonl").exists())

    def test_legacy_wrapper_validates_bumps(self):
        self.set_version("0.0.10")
        self.assert_output(self.run_policy(self.base, wrapper=True), "release=true")
        self.env["REMOTE_EXIT"] = "128"
        self.assert_error(self.run_policy(self.base, wrapper=True), "ls-remote failed")

    def test_github_output_appends(self):
        self.write("github-output", "earlier=value\n")
        result = self.run_policy("bump", self.base, reset_output=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "release=false\n")
        self.assertEqual((self.root / "github-output").read_text(), "earlier=value\nrelease=false\n")


if __name__ == "__main__":
    unittest.main()
