"""Synthetic GitHub callbacks and local Git objects; no network or real CI."""

import copy
import base64
import importlib.util
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from pipeline.latest import latest_sync as sync
from pipeline.latest import latest_data as latest
spec = importlib.util.spec_from_file_location("latest_fixture", Path(__file__).with_name("test_latest_data.py"))
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)


def protection():
    return {"required_status_checks": {"strict": True, "contexts": ["PR gate"]},
            "enforce_admins": {"enabled": True}}


class SyncTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.LatestDataTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root, self.source = self.fixture.root, self.fixture.source
        self.git = lambda *args: sync.git(self.source, *args)
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture")
        self.base = self.git("rev-parse", "HEAD").decode().strip()
        self.fixture.next_publication()
        self.candidate = self.root / "frozen"
        latest.propose(self.source, self.candidate, self.fixture.fetch, self.fixture.head)
        self.head = sync.create_commit(self.source, self.candidate, self.base)
        self.pr = {"number": 7, "state": "open", "draft": False, "mergeable": True, "html_url": "synthetic-pr",
                   "head": {"ref": sync.BRANCH, "sha": self.head, "repo": {"full_name": sync.REPOSITORY}},
                   "base": {"ref": sync.DEFAULT, "sha": self.base, "repo": {"full_name": sync.REPOSITORY}}}
        self.runs = {"pr.yml": [self.fixture_run(20)], "ci.yml": [self.fixture_run(21, "workflow_dispatch")]}
        self.settings = protection()
        self.calls = []
        self.env = patch.dict(os.environ, {"DATA_SYNC_AUTH_READY": "true"})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.official = {"version": "0.5.5.5", "scope": "Synthetic fixture"}

    def fixture_run(self, number, event="pull_request"):
        return {"id": number, "run_attempt": 1, "head_sha": self.head, "event": event,
                "head_branch": sync.BRANCH, "head_repository": {"full_name": sync.REPOSITORY},
                "status": "completed", "conclusion": "success"}

    def request(self, method, path, body=None):
        self.calls.append((method, path, body))
        path = path.removeprefix(f"repos/{sync.REPOSITORY}/")
        if path == "git/ref/heads/master": return {"object": {"sha": self.base}}
        if path == "git/matching-refs/heads/" + sync.BRANCH:
            return [{"ref": "refs/heads/" + sync.BRANCH, "object": {"sha": self.head}}]
        if path == "git/commits/" + self.head:
            return {"message": sync.MARKER, "parents": [{"sha": self.base}]}
        if path == f"contents/pipeline/latest/data-composition.json?ref={self.head}":
            return {"content": base64.b64encode((self.candidate / "pipeline/latest/data-composition.json").read_bytes()).decode()}
        if path.startswith("pulls?"): return [self.pr]
        if path == "pulls/7": return copy.deepcopy(self.pr)
        if path == "branches/master/protection": return self.settings
        if path.startswith("compare/"): return {"files": [{"filename": "pipeline/latest/data-composition.json", "status": "modified"}]}
        if path.startswith("actions/workflows/") and "/runs?" in path:
            return {"workflow_runs": copy.deepcopy(self.runs[path.split("/")[2]])}
        if path.startswith("actions/runs/"):
            number = int(path.split("/")[2])
            return {"total_count": 1, "jobs": [{"id": number * 10, "status": "completed", "conclusion": "success",
                                               "name": "PR gate" if number == 20 else "Full gate"}]}
        if path == "pulls/7/merge": return {"merged": True, "sha": "a" * 40}
        raise AssertionError((method, path, body))

    def scheduled(self, request=None):
        real_git, propose, recheck = sync.git, latest.propose, latest.recheck
        def local_git(root, *args, **kwargs):
            if "fetch" in args: return b""
            return real_git(root, *args, **kwargs)
        def synthetic_propose(root, out, fetcher, heads=None, **kwargs):
            return propose(root, out, fetcher, heads or self.fixture.head, **kwargs)
        def synthetic_recheck(candidate, root, fetcher, heads=None, **kwargs):
            return recheck(candidate, root, fetcher, heads or self.fixture.head, **kwargs)
        directory = self.root / ("scheduled-" + str(len(list(self.root.glob("scheduled-*")))))
        directory.mkdir()
        with patch.object(sync, "git", side_effect=local_git), patch.object(sync, "push", side_effect=AssertionError("Unexpected push")), \
             patch.object(latest, "propose", side_effect=synthetic_propose), patch.object(latest, "recheck", side_effect=synthetic_recheck):
            return sync.propose(self.source, directory, self.fixture.fetch, self.official, request or self.request)

    def accept(self, request=None):
        real_git = sync.git
        def local_git(root, *args, **kwargs):
            if "fetch" in args: return b""  # Exact immutable object already created locally.
            return real_git(root, *args, **kwargs)
        directory = self.root / ("accept-" + str(len(list(self.root.glob("accept-*")))))
        directory.mkdir()
        with patch.object(sync, "git", side_effect=local_git), patch.object(latest, "resolve_head", self.fixture.head):
            # Defaults bind at definition time; pass the synthetic head lookup at
            # the public resolver boundary without changing production behavior.
            propose = latest.propose
            def synthetic_propose(root, out, fetcher, heads=None, **kwargs):
                return propose(root, out, fetcher, heads or self.fixture.head, **kwargs)
            with patch.object(latest, "propose", side_effect=synthetic_propose):
                return sync.accept(self.source, directory, self.fixture.fetch, self.official, request or self.request)

    def test_only_both_exact_successful_gates_and_fresh_composition_allow_merge(self):
        result = self.accept()
        self.assertEqual(result["status"], "adopted")
        merges = [value for value in self.calls if value[0] == "PUT"]
        self.assertEqual(merges[0][2], {"sha": self.head, "merge_method": "squash"})
        self.assertEqual(self.git("rev-parse", "HEAD").decode().strip(), self.base)

    def test_missing_failed_cancelled_approval_required_and_wrong_head_never_merge(self):
        original = copy.deepcopy(self.runs)
        for failure in ("missing", "failure", "cancelled", "action_required", "queued", "wrong-head"):
            self.runs = copy.deepcopy(original)
            if failure == "missing": self.runs["ci.yml"] = []
            elif failure == "wrong-head": self.runs["ci.yml"][0]["head_sha"] = "b" * 40
            elif failure == "queued": self.runs["ci.yml"][0]["status"] = "queued"
            else: self.runs["ci.yml"][0]["conclusion"] = failure
            with self.subTest(failure=failure), self.assertRaises(ValueError): self.accept()
        self.assertFalse(any(value[0] == "PUT" for value in self.calls))

    def test_latest_failed_run_cannot_reuse_older_success(self):
        failed = self.fixture_run(22, "workflow_dispatch")
        failed["conclusion"] = "failure"
        self.runs["ci.yml"].append(failed)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.accept()

    def test_strict_protection_including_admins_is_required(self):
        for change in ("strict", "admins", "context"):
            self.settings = protection()
            if change == "strict": self.settings["required_status_checks"]["strict"] = False
            elif change == "admins": self.settings["enforce_admins"]["enabled"] = False
            else: self.settings["required_status_checks"]["contexts"] = ["Full gate"]
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "strict-branch-checks"):
                self.accept()

    def test_moving_base_or_head_and_code_changes_reject(self):
        for change in ("base", "head", "code"):
            reads = 0
            def moving(method, path, body=None):
                nonlocal reads
                value = self.request(method, path, body)
                if path.endswith("pulls/7"):
                    reads += 1
                    if reads > 1: value[change if change in ("base", "head") else "base"]["sha"] = "c" * 40
                if change == "code" and "/compare/" in path:
                    value["files"].append({"filename": "pipeline/latest/latest_sync.py", "status": "modified"})
                return value
            with self.subTest(change=change), self.assertRaises(ValueError): self.accept(moving)
        self.assertFalse(any(value[0] == "PUT" for value in self.calls))

    def test_missing_dedicated_auth_never_merges(self):
        os.environ["DATA_SYNC_AUTH_READY"] = "false"
        with self.assertRaisesRegex(ValueError, "needs-data-sync-token"):
            self.accept()
        self.assertFalse(any(value[0] == "PUT" for value in self.calls))

    def test_git_commit_preserves_default_code_and_uses_exact_parent(self):
        self.assertEqual(self.git("rev-parse", self.head + "^1").decode().strip(), self.base)
        files = self.git("diff-tree", "--no-commit-id", "--name-only", "-r", self.head).decode().splitlines()
        self.assertEqual(set(files), {"pipeline/latest/data-composition.json", "pipeline/imports/data-imports.lock.json"})

    def test_next_schedule_recovers_a_failed_adoption_without_repeating_successful_ci(self):
        failed = False
        def transient(method, path, body=None):
            nonlocal failed
            if method == "GET" and path.endswith("pulls/7") and not failed:
                failed = True
                raise OSError("Synthetic temporary GitHub outage")
            return self.request(method, path, body)
        with self.assertRaisesRegex(OSError, "temporary GitHub outage"):
            self.scheduled(transient)
        self.assertFalse(any(value[0] == "PUT" for value in self.calls))
        self.assertEqual(self.scheduled()["status"], "adopted")
        self.assertEqual(len([value for value in self.calls if value[0] == "PUT"]), 1)
        self.assertFalse(any(value[0] == "POST" and value[1].endswith("/dispatches") for value in self.calls))

    def test_schedule_reconciliation_preserves_waiting_state_for_incomplete_gates(self):
        self.runs["pr.yml"][0]["status"] = "queued"
        result = self.scheduled()
        self.assertEqual(result["status"], "candidate-proposed-awaiting-gates")
        self.assertIn("PR gate", result["reason"])
        self.assertFalse(any(value[0] == "PUT" for value in self.calls))


if __name__ == "__main__":
    unittest.main()
