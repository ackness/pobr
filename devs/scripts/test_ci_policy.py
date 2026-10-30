"""Behavioral tests for check routing, failure propagation and agent handoff."""

import importlib.util
import json
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
handoff = load("pr-handoff.py")
ready = load("pr-ready.py")


class RoutingTests(unittest.TestCase):
    def test_docs_only_and_known_areas(self):
        self.assertFalse(any(plan.classify(["docs/new-guide.md", "web/README.md"]).values()))
        self.assertTrue(plan.classify(["crates/pobr-core/src/calc.rs"])["rust"])
        self.assertTrue(plan.classify(["web/src/App.tsx"])["web"])
        self.assertTrue(plan.classify(["pipeline/data_snapshot.py"])["full"])
        self.assertTrue(plan.classify(["web/public/_worker.js"])["scripts"])

    def test_shared_or_unknown_inputs_select_full_without_duplicate_light_checks(self):
        for name in ["Cargo.lock", "crates/new/Cargo.toml", "mise.toml", "web/package.json",
                     ".github/workflows/ci.yml", "devs/scripts/ci-plan.py", "new-runtime.conf", "docs/hidden-executable.py"]:
            with self.subTest(name=name):
                self.assertEqual(plan.classify([name, "web/src/App.tsx"]),
                                 {area: area == "full" for area in plan.AREAS})

    def test_data_text_and_removed_source_are_not_documentation(self):
        self.assertTrue(plan.classify(["examples/build/code.txt"])["rust"])
        self.assertTrue(plan.classify(["web/worker-tests/fixtures/build-code.txt"])["web"])
        self.assertTrue(plan.classify(["web/worker-tests/fixtures/build-code.txt"])["scripts"])
        self.assertTrue(plan.classify(["data/test/manifest.json"])["rust"])

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
    def needs(self, files=()):
        outputs = {name: str(value).lower() for name, value in plan.classify(files).items()}
        return {"changes": {"result": "success", "outputs": outputs},
                **{name: {"result": "success" if selected == "true" else "skipped"}
                   for name, selected in outputs.items()}}

    def test_expected_success_and_explicit_skips(self):
        for files in [[], ["web/src/App.tsx"], ["Cargo.lock"], ["pipeline/a.py"]]:
            gate.check(self.needs(files), "pr")
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


class HandoffTests(unittest.TestCase):
    def pr(self, **changes):
        audit = {"head": "a" * 40, "base": "b" * 40, "reviewer": "independent-agent/session",
                 "verdict": "pass", "findings": []}
        audit.update(changes)
        body = "\n".join(f"## {section}\nConcrete evidence.\n" for section in handoff.SECTIONS)
        body += "\n<!-- pobr-audit\n" + json.dumps(audit) + "\n-->"
        return {"draft": False, "body": body, "head": {"sha": "a" * 40}, "base": {"sha": "b" * 40}}

    def test_complete_fresh_handoff_and_draft(self):
        handoff.validate(self.pr())
        handoff.validate({"draft": True})

    def test_stale_audit_blockers_bad_record_and_unfilled_template_reject(self):
        for changes in [{"head": "c" * 40}, {"base": "c" * 40}, {"verdict": "fail"},
                        {"reviewer": ""}, {"findings": [{"severity": "blocking", "status": "open", "summary": "wrong result"}]}]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                handoff.validate(self.pr(**changes))
        for body in ["", "## Problem\n<!-- fill me -->", self.pr()["body"] + self.pr()["body"]]:
            pr = self.pr()
            pr["body"] = body
            with self.assertRaises(ValueError):
                handoff.validate(pr)

    def test_old_draft_or_body_event_cannot_replace_current_audit_state(self):
        event = self.pr()
        event["draft"] = True
        current = self.pr(verdict="pending")
        with self.assertRaises(ValueError):
            handoff.validate_current(event, current)
        current = self.pr()
        current["head"]["sha"] = "c" * 40
        with self.assertRaises(ValueError):
            handoff.validate_current(event, current)

    def test_resolved_findings_and_open_suggestions_are_allowed(self):
        for severity, status in [("blocking", "resolved"), ("suggestion", "open")]:
            handoff.validate(self.pr(findings=[{"severity": severity, "status": status, "summary": "review finding"}]))


class ReadyTests(unittest.TestCase):
    def test_preflight_rejects_unknown_or_bad_required_checks(self):
        pr = HandoffTests().pr()
        pr.update(state="open", mergeable=True)
        checks = [{"name": name, "bucket": "pass"} for name in ready.REQUIRED]
        ready.assess(pr, checks)
        for bucket in ("skip", "pending", "cancel", "fail"):
            with self.subTest(bucket=bucket), self.assertRaises(ValueError):
                ready.assess(pr, [{"name": name, "bucket": bucket} for name in ready.REQUIRED])
        with self.assertRaises(ValueError):
            ready.assess(pr, checks[:1])
        pr["mergeable"] = None
        with self.assertRaises(ValueError):
            ready.assess(pr, checks)


if __name__ == "__main__":
    unittest.main()
