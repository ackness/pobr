"""Offline handoff evidence tests; no build, network or owner code execution."""

import contextlib
import copy
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import patch_status as status


PATCH = "0.5.5.8"
TARGET = {"game": "poe2", "game_patch": PATCH, "realm": "international"}
SHAS = {"pobr": "a" * 40, "repoe": "b" * 40, "locale": "c" * 40}


def encoded(value):
    return status.imports.encoded(value)


def pin(raw):
    return {"bytes": len(raw), "sha256": status.imports.sha(raw)}


def successful_run(number, sha, rules, branch="master"):
    return {"databaseId": number, "attempt": 1, "headSha": sha, "headBranch": branch,
            "event": "push", "status": "completed", "conclusion": "success",
            "url": f"https://github.com/fixture/actions/runs/{number}", "jobs": [
                {"name": name, "status": "completed", "conclusion": "success", "steps": [
                    {"name": step, "status": "completed", "conclusion": "success"} for step in steps]}
                for name, steps in rules.items()]}


class FakeSources:
    def __init__(self):
        self.tags = {owner: {f"refs/heads/{status.OWNERS[owner][1]}": sha} for owner, sha in SHAS.items()}
        self.tags["pobr"].update({"refs/tags/contract/1.1": "d" * 40, "refs/tags/v0.0.52": SHAS["pobr"]})
        self.releases = {}
        self.bytes = {}
        self.workflow_runs = {}
        self.calls = []
        self.fail = None
        self.patch = PATCH
        self.site_manifest = {"version": PATCH, "files": ["base/stats.json", "manifest.json"],
                              "digests": {"base/stats.json": "e" * 64, "manifest.json": "f" * 64}}
        for index, (owner, workflow) in enumerate(status.CI_RULES, 1):
            run = successful_run(index, SHAS[owner], status.CI_RULES[(owner, workflow)])
            if workflow == "release.yml":
                run["headBranch"] = "v0.0.52"
            self.workflow_runs[(owner, workflow)] = run
        deploy = {"name": "deploy stable v0 to Cloudflare Pages", "status": "completed", "conclusion": "success",
                  "completedAt": "2026-11-15T12:44:23Z", "steps": [
                      {"name": "Run cloudflare/wrangler-action@v4.1.3", "status": "completed", "conclusion": "success"}]}
        self.workflow_runs[("pobr", "release.yml")]["jobs"].append(deploy)

    def check(self, *call):
        self.calls.append(call)
        if self.fail:
            raise status.Unavailable(self.fail)

    def refs(self, owner):
        self.check("refs", owner)
        return self.tags[owner]

    def release(self, owner, tag):
        self.check("release", owner, tag)
        if (owner, tag) not in self.releases:
            raise status.Unavailable("Release unavailable (not proof of absence)")
        return self.releases[(owner, tag)]

    def metadata(self, owner, tag, name):
        self.check("metadata", owner, tag, name)
        return self.bytes[(owner, tag, name)]

    def runs(self, owner, workflow, sha):
        self.check("runs", owner, workflow, sha)
        run = self.workflow_runs.get((owner, workflow))
        return [run] if run else []

    def run(self, owner, number):
        self.check("run", owner, number)
        return next(run for (repo, _), run in self.workflow_runs.items() if repo == owner and run["databaseId"] == number)

    def observe(self, path):
        self.check("observe", str(path))
        return {"version": self.patch, "cdn": f"https://patch-poe2.poecdn.com/{self.patch}/"}

    def site(self, base):
        self.check("site", base)
        return self.site_manifest

    def asset_release(self, owner, tag, assets):
        """assets maps a filename to raw bytes. Only metadata/checksums are read."""
        self.tags[owner][f"refs/tags/{tag}"] = SHAS[owner]
        sums = "".join(f"{pin(raw)['sha256']}  {name}\n" for name, raw in sorted(assets.items())).encode()
        all_assets = {**assets, "SHA256SUMS": sums}
        self.releases[(owner, tag)] = {
            "tagName": tag, "isDraft": False, "isPrerelease": False,
            "publishedAt": "2026-11-15T10:00:00Z", "url": f"https://github.com/fixture/releases/{tag}",
            "assets": [{"name": name, "size": len(raw), "digest": "sha256:" + pin(raw)["sha256"], "state": "uploaded"}
                       for name, raw in all_assets.items()]}
        for name, raw in all_assets.items():
            self.bytes[(owner, tag, name)] = raw


class PatchStatusTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="patch-status ")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.paths = {owner: root / owner for owner in status.OWNERS}
        for path in self.paths.values():
            path.mkdir()
        self.sources = FakeSources()
        input_zip = b"input fixture"
        self.sources.asset_release("repoe", f"inputs/{PATCH}-1", {f"repoe-input-ordinary-{PATCH}.zip": input_zip})
        input_spec = {"repository": "ackness/repoe", "tag": f"inputs/{PATCH}-1",
                      "asset": f"repoe-input-ordinary-{PATCH}.zip", **pin(input_zip)}
        self.specs = {}
        self.receipts = {}
        self.producer_files = {}
        for owner, prefix, schema in (("repoe", "game", "repoe-release-receipt/v2"),
                                      ("locale", "locale", "poe2-locale-release-receipt/v1")):
            archive = f"{owner} fixture".encode()
            receipt = {"schema": schema, "target": TARGET,
                       "producer": {"repository": status.OWNERS[owner][0], "commit": SHAS[owner], "dirty": False},
                       "package": pin(archive)}
            if owner == "repoe":
                receipt["producer"]["contract"] = {"version": 1, "tag": "contract/1.1", "commit": "d" * 40,
                                                    "repository": "ackness/pobr"}
                receipt["inputs"] = {"packages": {"ordinary": input_spec}}
            config_name = f"release/{PATCH}/inputs.lock.json" if owner == "repoe" else f"release/{PATCH}-1.json"
            config_raw = encoded({"target": TARGET, "expect": {"package": pin(archive)}, "packages": {"ordinary": input_spec}})
            receipt["producer"]["config"] = {config_name: pin(config_raw)}
            self.producer_files[(owner, config_name)] = config_raw
            raw = encoded(receipt)
            name = f"pobr-{prefix}-{PATCH}.zip"
            tag = f"{prefix}/{PATCH}-1"
            self.sources.asset_release(owner, tag, {name: archive, "receipt.json": raw})
            self.specs["game" if owner == "repoe" else "language"] = {
                "repository": status.OWNERS[owner][0], "tag": tag, "asset": name, **pin(archive),
                "receipt": pin(raw), "producer_commit": SHAS[owner], "contract_version": 1 if owner == "repoe" else None,
                "manifest": pin(b"{}"), "expanded_bytes": 123}
            self.receipts[owner] = receipt
            self.write(owner, config_name, config_raw)
        self.write("repoe", "tools/pobr-data-adapter/Cargo.toml",
                   '[dependencies]\npobr-data = { git = "https://github.com/ackness/pobr", tag = "contract/1.1" }\n')
        self.write("pobr", "crates/pobr-data/src/lib.rs", "pub const CONTRACT_VERSION: u32 = 1;\n")
        hashes = {"base/stats.json": "e" * 64}
        identity = status.data_bundle.sha256(status.data_bundle.encoded(["pobr-data-pack-v1", 3, PATCH, hashes]))
        self.lock = {"schema": "pobr-data-lock/v2", "target": TARGET, "locale": "zh-CN", "packages": self.specs,
                     "shared_files": {}, "snapshot_manifest": pin(b"{}"), "runtime_identity": identity,
                     "release_id": "1" * 64}
        self.write("pobr", "pipeline/data.lock.json", encoded(self.lock))
        self.sources.asset_release("pobr", "v0.0.52", {
            "pobr-wasm-v0.0.52-web.tar.gz": b"wasm", "wasm-size.json": b"{}", "wasm-size.md": b"report"})
        self.local = patch.object(status, "local_checkout", return_value=status.record(
            "observed", "fixture", head=SHAS["pobr"], branch="master", dirty=False))
        self.git = patch.object(status, "git_read", side_effect=self.read_git)
        self.local_mock = self.local.start()
        self.git_mock = self.git.start()
        self.addCleanup(self.local.stop)
        self.addCleanup(self.git.stop)

    def write(self, owner, name, content):
        path = self.paths[owner] / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content if isinstance(content, bytes) else content.encode())

    def read_git(self, path, revision, name):
        key = (path.name, name)
        return self.producer_files[key] if key in self.producer_files else (path / name).read_bytes()

    def report(self, patch=PATCH):
        return status.make_report(self.paths, patch, self.sources)

    def stage(self, name, report=None):
        return next(item for item in (report or self.report())["stages"] if item["id"] == name)

    def test_completed_metadata_chain_does_not_claim_full_acceptance(self):
        report = self.report()
        self.assertIsNone(report["first_incomplete"])
        self.assertEqual(self.stage("sites", report)["checks"][0]["app_version"], None)
        self.assertTrue(report["unverified"])
        self.assertEqual(self.stage("release", report)["tag"], "v0.0.52")
        self.assertEqual(self.stage("deployment", report)["commit"], SHAS["pobr"])
        self.assertEqual({call[-1] for call in self.sources.calls if call[0] == "metadata"}, {"SHA256SUMS", "receipt.json"})
        with contextlib.redirect_stdout(io.StringIO()) as output:
            status.render(report)
        self.assertIn("none in the implemented metadata checks", output.getvalue())
        self.assertIn("Not verified:", output.getvalue())

    def test_input_release_must_have_every_declared_archive(self):
        release = self.sources.releases[("repoe", f"inputs/{PATCH}-1")]
        release["assets"] = [item for item in release["assets"] if item["name"] == "SHA256SUMS"]
        self.assertEqual(self.report()["first_incomplete"], "inputs")
        self.assertEqual(self.stage("inputs")["state"], "blocked")

    def test_same_patch_new_owner_release_requires_adoption(self):
        old = ("repoe", f"game/{PATCH}-1")
        new = ("repoe", f"game/{PATCH}-2")
        self.sources.releases[new] = copy.deepcopy(self.sources.releases[old])
        self.sources.releases[new]["tagName"] = new[1]
        self.sources.tags["repoe"][f"refs/tags/{new[1]}"] = SHAS["repoe"]
        for key, value in list(self.sources.bytes.items()):
            if key[:2] == old:
                self.sources.bytes[(*new, key[-1])] = value
        report = self.report()
        self.assertEqual(self.stage("game", report)["tag"], new[1])
        self.assertEqual(self.stage("adoption", report)["state"], "pending")
        self.assertEqual(report["first_incomplete"], "adoption")
        self.assertIn(new[1], self.stage("adoption", report)["next"])

    def test_draft_or_missing_assets_never_ready(self):
        release = self.sources.releases[("locale", f"locale/{PATCH}-1")]
        release["isDraft"] = True
        self.assertEqual(self.stage("locale")["state"], "pending")
        release["isDraft"] = False
        release["assets"] = [item for item in release["assets"] if item["name"] != "receipt.json"]
        self.assertEqual(self.stage("locale")["state"], "blocked")

    def test_unknown_metadata_is_not_reported_as_absent(self):
        self.sources.fail = "network unavailable"
        report = self.report()
        for name in ("inputs", "game", "locale", "adoption", "release", "sites"):
            self.assertEqual(self.stage(name, report)["state"], "unknown", name)
        self.assertIsNotNone(report["first_incomplete"])

    def test_unknown_target_uses_no_unrelated_release(self):
        report = self.report("9.9.9")
        self.assertEqual(report["first_incomplete"], "inputs")
        self.assertEqual(self.stage("game", report)["state"], "pending")
        self.assertIsNone(self.stage("game", report).get("tag"))
        self.assertFalse(self.stage("official", report)["matches_target"])

    def test_wrong_head_green_run_is_unknown(self):
        run = self.sources.workflow_runs[("locale", "verify.yml")]
        run["headSha"] = "f" * 40
        evidence, _ = status.ci_evidence(self.sources, "locale", "verify.yml", SHAS["locale"])
        self.assertEqual(evidence["state"], "unknown")
        self.assertIn("exact-SHA", evidence["detail"])

    def test_skipped_required_step_is_not_green(self):
        run = self.sources.workflow_runs[("repoe", "release-build.yml")]
        run["jobs"][0]["steps"][0]["conclusion"] = "skipped"
        self.assertEqual(self.stage("ci")["state"], "blocked")

    def test_new_attempt_is_used_even_after_an_older_success(self):
        run = self.sources.workflow_runs[("repoe", "verify.yml")]
        run["attempt"] = 2
        run["conclusion"] = "failure"
        evidence, _ = status.ci_evidence(self.sources, "repoe", "verify.yml", SHAS["repoe"])
        self.assertEqual(evidence["state"], "blocked")
        self.assertEqual(evidence["attempt"], 2)

    def test_release_success_without_actual_deployment_step_is_pending(self):
        deploy = self.sources.workflow_runs[("pobr", "release.yml")]["jobs"][-1]
        deploy["steps"][0]["conclusion"] = "skipped"
        self.assertEqual(self.stage("deployment")["state"], "pending")

    def test_site_cannot_prove_package_identity_from_patch_number_only(self):
        self.sources.site_manifest["digests"]["base/stats.json"] = "0" * 64
        self.assertEqual(self.stage("sites")["state"], "blocked")
        self.sources.site_manifest = {"version": PATCH}
        self.assertEqual(self.stage("sites")["state"], "unknown")

    def test_receipt_with_wrong_producer_or_untrusted_bytes_is_blocked(self):
        owner, tag = "repoe", f"game/{PATCH}-1"
        raw = self.sources.bytes[(owner, tag, "receipt.json")]
        self.sources.bytes[(owner, tag, "receipt.json")] = raw + b"\n"
        self.assertEqual(self.stage("game")["state"], "blocked")

    def test_local_declared_pin_mismatch_does_not_invalidate_publication(self):
        path = self.paths["locale"] / f"release/{PATCH}-1.json"
        document = json.loads(path.read_text())
        document["expect"]["package"]["sha256"] = "0" * 64
        path.write_bytes(encoded(document))
        evidence = self.stage("locale")
        self.assertEqual(evidence["state"], "ready")
        self.assertEqual(evidence["local_declaration"]["state"], "blocked")
        self.assertEqual(evidence["producer_declaration"]["state"], "ready")

    def test_no_asset_digest_is_unknown(self):
        release = self.sources.releases[("repoe", f"game/{PATCH}-1")]
        release["assets"][0]["digest"] = None
        self.assertEqual(self.stage("game")["state"], "unknown")

    def test_missing_remote_git_object_is_unknown_without_fetch(self):
        self.git_mock.side_effect = status.Unavailable("object absent locally; fetch explicitly")
        self.assertEqual(self.stage("adoption")["state"], "unknown")
        self.assertEqual(self.stage("release")["state"], "unknown")

    def test_dirty_local_checkout_cannot_replace_remote_master(self):
        self.local_mock.return_value["dirty"] = True
        changed = copy.deepcopy(self.lock)
        changed["packages"]["game"]["tag"] = f"game/{PATCH}-2"
        self.write("pobr", "pipeline/data.lock.json", encoded(changed))
        self.git_mock.side_effect = lambda *args: encoded(self.lock).decode()
        report = self.report()
        self.assertEqual(self.stage("adoption", report)["state"], "ready")
        self.assertTrue(self.stage("adoption", report)["local_lock"]["differs_from_master"])
        self.assertTrue(report["repositories"]["pobr"]["local"]["dirty"])

    def test_offline_sources_perform_no_network_or_owner_observation(self):
        with patch.object(status, "command_text", side_effect=AssertionError("must not run")), \
             patch.object(status, "urlopen", side_effect=AssertionError("must not fetch")):
            sources = status.Sources(offline=True)
            for method, args in ((sources.refs, ("repoe",)), (sources.release, ("repoe", "game/test")),
                                 (sources.runs, ("repoe", "verify.yml", SHAS["repoe"])),
                                 (sources.observe, (self.paths["repoe"],)), (sources.site, (status.SITES[0],))):
                with self.assertRaises(status.Unavailable):
                    method(*args)

    def test_transport_only_calls_read_only_gh_commands_with_explicit_repo(self):
        with patch.object(status, "command_text", return_value="{}") as command:
            sources = status.Sources()
            sources.release("repoe", f"game/{PATCH}-1")
            sources.runs("locale", "verify.yml", SHAS["locale"])
            sources.run("pobr", 123)
        for call in command.call_args_list:
            args = call.args[0]
            self.assertEqual(args[0], "gh")
            self.assertIn(args[1:3], (["release", "view"], ["run", "list"], ["run", "view"]))
            self.assertEqual(args[-2], "-R")
            self.assertIn(args[-1], [item[0] for item in status.OWNERS.values()])

    def test_local_unpublished_candidate_does_not_hide_published_pair(self):
        self.write("repoe", f"release/{PATCH}-2/inputs.lock.json",
                   encoded({"target": TARGET, "expect": {"package": pin(b"future candidate")}}))
        report = self.report()
        self.assertEqual(self.stage("game", report)["tag"], f"game/{PATCH}-1")
        self.assertEqual(self.stage("adoption", report)["state"], "ready")
        self.assertEqual(self.stage("game", report)["local_candidates"][-1]["tag"], f"game/{PATCH}-2")
        self.assertFalse(self.stage("game", report)["local_candidates"][-1]["remote_tag_exists"])

    def test_unrelated_deployed_app_does_not_complete_future_target(self):
        report = self.report("9.9.9")
        self.assertEqual(self.stage("deployment", report)["state"], "pending")
        self.assertEqual(self.stage("sites", report)["state"], "pending")
        self.assertEqual(self.stage("deployment", report)["observed_deployment_state"], "ready")

    def test_parent_workflow_full_gate_counts_but_pr_gate_does_not(self):
        self.sources.workflow_runs.pop(("pobr", "ci.yml"))
        proof = status.master_ci_evidence(self.sources, SHAS["pobr"])
        self.assertEqual(proof["state"], "ready")
        self.assertEqual(proof["workflow"], "pr.yml")
        self.sources.workflow_runs.pop(("pobr", "release.yml"))
        self.sources.workflow_runs[("pobr", "pr.yml")]["jobs"] = [
            {"name": "PR gate", "status": "completed", "conclusion": "success", "steps": []}]
        self.assertEqual(status.master_ci_evidence(self.sources, SHAS["pobr"])["state"], "unknown")

    def test_remote_refs_changed_during_collection_require_retry(self):
        original = self.sources.refs
        calls = {}

        def refs(owner):
            values = copy.deepcopy(original(owner))
            calls[owner] = calls.get(owner, 0) + 1
            if calls[owner] > 1 and owner == "repoe":
                values["refs/heads/master"] = "9" * 40
            return values

        self.sources.refs = refs
        report = self.report()
        self.assertEqual(report["first_incomplete"], "consistency")
        self.assertEqual(report["consistency"]["state"], "unknown")
        self.assertIn("rerun", self.stage("consistency", report)["checks"][1]["detail"])

    def test_remote_tag_without_release_does_not_hide_previous_publication(self):
        self.sources.tags["locale"][f"refs/tags/locale/{PATCH}-2"] = "9" * 40
        report = self.report()
        evidence = self.stage("locale", report)
        self.assertEqual(evidence["tag"], f"locale/{PATCH}-1")
        self.assertEqual(evidence["state"], "ready")
        self.assertEqual(evidence["newer_tags"][0]["state"], "unknown")
        self.assertEqual(self.stage("adoption", report)["state"], "ready")
        self.assertTrue(any(item["kind"] == "newer_tag" for item in report["attention"]))

    def test_wrong_target_newer_release_does_not_hide_valid_publication(self):
        receipt = copy.deepcopy(self.receipts["repoe"])
        receipt["target"] = {**TARGET, "game_patch": "9.9.9"}
        tag = f"game/{PATCH}-2"
        self.sources.asset_release("repoe", tag, {f"pobr-game-{PATCH}.zip": b"repoe fixture", "receipt.json": encoded(receipt)})
        report = self.report()
        evidence = self.stage("game", report)
        self.assertEqual(evidence["tag"], f"game/{PATCH}-1")
        self.assertEqual(evidence["state"], "ready")
        self.assertEqual(evidence["newer_tags"][0]["state"], "blocked")
        self.assertEqual(evidence["newer_tags"][0]["tag"], tag)
        self.assertTrue(any(item["tag"] == tag for item in report["attention"]))

    def test_only_master_lock_unavailable_remains_unknown_downstream(self):
        master_sha = "9" * 40
        self.sources.tags["pobr"]["refs/heads/master"] = master_sha
        original = self.read_git

        def read(path, revision, name):
            if path == self.paths["pobr"] and revision == master_sha:
                raise status.Unavailable("master object is not present locally")
            return original(path, revision, name)

        self.git_mock.side_effect = read
        report = self.report()
        for name in ("adoption", "release", "deployment", "sites"):
            self.assertEqual(self.stage(name, report)["state"], "unknown", name)

    def test_http_protocol_failure_preserves_other_observations(self):
        with patch.object(status, "urlopen", side_effect=status.http.client.IncompleteRead(b"partial")):
            evidence = status.site_evidence(status.Sources(), status.SITES[0], self.lock)
        self.assertEqual(evidence["state"], "unknown")
        self.assertIn("Site request failed", evidence["detail"])

    def test_published_contract_is_independent_of_current_adapter_upgrade(self):
        self.sources.tags["pobr"]["refs/tags/contract/1.2"] = "e" * 40
        self.write("repoe", "tools/pobr-data-adapter/Cargo.toml",
                   '[dependencies]\npobr-data = { git = "https://github.com/ackness/pobr", tag = "contract/1.2" }\n')
        evidence = self.stage("contract")
        self.assertEqual(evidence["tag"], "contract/1.1")
        self.assertEqual(evidence["local_adapter"]["tag"], "contract/1.2")
        self.assertEqual(evidence["state"], "ready")

    def test_malformed_local_declaration_is_visible_not_authoritative(self):
        self.write("locale", f"release/{PATCH}-1.json", "{not JSON")
        evidence = self.stage("locale")
        self.assertEqual(evidence["state"], "ready")
        self.assertEqual(evidence["local_declaration"]["state"], "blocked")
        self.assertEqual(evidence["producer_declaration"]["state"], "ready")

    def test_refs_are_peeled_and_malformed_output_rejected(self):
        parsed = status.parse_refs(f"{'1' * 40}\trefs/tags/game/{PATCH}-1\n{'2' * 40}\trefs/tags/game/{PATCH}-1^{{}}\n")
        self.assertEqual(parsed[f"refs/tags/game/{PATCH}-1"], "2" * 40)
        for raw in ("garbage", f"{'2' * 40}\trefs/tags/test^{{}}\n"):
            with self.assertRaises(status.Unavailable):
                status.parse_refs(raw)

    def test_command_errors_or_timeout_are_bounded_unavailable_evidence(self):
        with patch.object(status.subprocess, "run", side_effect=subprocess.TimeoutExpired(["git"], 25)):
            with self.assertRaises(status.Unavailable):
                status.command_text(["git", "status"])


if __name__ == "__main__":
    unittest.main()
