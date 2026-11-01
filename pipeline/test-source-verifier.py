"""Synthetic code fixtures test the pin/import boundary, not RePoE semantics."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from data_imports import sha
import source_verifier as v


class SourceVerifierTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.library = self.root / "library"
        self.sources = {
            "RePoE/__init__.py": b'"""Synthetic package; no game or acquisition data."""\n',
            "RePoE/source_io.py": b'def value(): return "pinned helper"\n',
            "RePoE/source_rebinding.py": (
                b'from . import source_io\n'
                b'def verify(archive, handoff, output=None):\n'
                b'    return (source_io.value(), archive, handoff, output)\n'),
        }
        self.spec = {"schema": "pobr-source-verifier/v1", "repository": "ackness/repoe",
                     "revision": "a" * 40, "files": {name: self.pin(raw) for name, raw in self.sources.items()}}
        for name, raw in self.sources.items():
            path = self.library / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)

    @staticmethod
    def pin(raw):
        return {"bytes": len(raw), "sha256": sha(raw)}

    def namespaces(self):
        return {name for name in sys.modules if name.startswith("_pobr_source_verifier_")}

    def test_private_pinned_package_calls_helper_and_is_cleaned_up(self):
        before = self.namespaces()
        with v.open_verifier(self.library, self.spec) as (verify, receipt):
            self.assertEqual(verify("zip", {"test": True}, "out"),
                             ("pinned helper", "zip", {"test": True}, "out"))
            self.assertEqual(receipt["library"], self.spec)
            self.assertEqual(len(self.namespaces() - before), 3)
        self.assertEqual(self.namespaces(), before)
        self.assertFalse(list(self.library.rglob("*.pyc")))

    def test_source_identity_and_closure_are_explicit(self):
        changes = ("repo", "revision", "missing", "extra", "bool", "size", "hash", "field")
        for change in changes:
            spec = copy.deepcopy(self.spec)
            file = spec["files"][v.FILES[0]]
            if change == "repo": spec["repository"] = "ackness/pobr"
            elif change == "revision": spec["revision"] = "main"
            elif change == "missing": spec["files"].pop(v.FILES[0])
            elif change == "extra": spec["files"]["RePoE/client_sources.py"] = file
            elif change == "bool": file["bytes"] = True
            elif change == "size": file["bytes"] = v.MAX_CODE_BYTES + 1
            elif change == "hash": file["sha256"] = "wrong"
            else: file["optional"] = True
            with self.subTest(change=change), self.assertRaises(ValueError): v.validate(spec)

    def test_every_module_is_pinned_before_initializer_can_execute(self):
        marker = self.root / "executed"
        raw = f'from pathlib import Path\nPath({str(marker)!r}).write_text("bad")\n'.encode()
        (self.library / v.FILES[0]).write_bytes(raw)
        self.spec["files"][v.FILES[0]] = self.pin(raw)
        (self.library / v.FILES[1]).write_bytes(b'raise AssertionError("unapproved")\n')
        with self.assertRaises(ValueError):
            with v.open_verifier(self.library, self.spec): self.fail("unapproved code imported")
        self.assertFalse(marker.exists())

    def test_symlinks_and_extra_files_cannot_extend_the_closure(self):
        helper = self.library / v.FILES[1]
        helper.unlink()
        actual = self.root / "actual.py"
        actual.write_bytes(self.sources[v.FILES[1]])
        helper.symlink_to(actual)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            with v.open_verifier(self.library, self.spec): pass
        helper.unlink()
        helper.write_bytes(self.sources[v.FILES[1]])
        (self.library / "RePoE/extra.py").write_text("raise AssertionError()")
        with self.assertRaisesRegex(ValueError, "Unexpected"):
            with v.open_verifier(self.library, self.spec): pass

    def test_modified_code_after_use_rejects_completion(self):
        before = self.namespaces()
        with self.assertRaises(ValueError):
            with v.open_verifier(self.library, self.spec) as (verify, _):
                self.assertEqual(verify(None, None)[0], "pinned helper")
                (self.library / v.FILES[1]).write_bytes(b"# corrupt\n")
        self.assertEqual(self.namespaces(), before)

    def test_execution_uses_verified_snapshot_not_a_second_filesystem_load(self):
        original = v._payloads
        calls = 0
        results = []
        def mutate_after_snapshot(root, spec):
            nonlocal calls
            captured = original(root, spec)
            calls += 1
            if calls == 1:
                (self.library / v.FILES[1]).write_bytes(b'raise AssertionError("mutable code executed")\n')
            return captured
        with patch.object(v, "_payloads", side_effect=mutate_after_snapshot), self.assertRaises(ValueError):
            with v.open_verifier(self.library, self.spec) as (verify, _): results.append(verify(None, None)[0])
        self.assertEqual(results, ["pinned helper"])

    def test_initialization_failure_cleans_namespace_and_has_no_import_fallback(self):
        for raw in (b'from . import client_sources\n', b'verify = None\n'):
            with self.subTest(raw=raw):
                before = self.namespaces()
                (self.library / v.FILES[-1]).write_bytes(raw)
                self.spec["files"][v.FILES[-1]] = self.pin(raw)
                with self.assertRaisesRegex(ValueError, "initialization"):
                    with v.open_verifier(self.library, self.spec): pass
                self.assertEqual(self.namespaces(), before)

    def test_pinned_git_acquisition_and_offline_cache_reuse(self):
        cache = self.root / "downloads"
        calls = []
        def fetch(address):
            calls.append(address)
            return self.sources[address[2]]
        first = v.materialize(self.spec, self.root / "first", cache, False, fetch)
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(a[:2] == ("https://github.com/ackness/repoe.git", "a" * 40) for a in calls))
        with patch.object(v, "acquire", wraps=v.acquire):
            second = v.materialize(self.spec, self.root / "second", cache, True,
                                   lambda _: self.fail("offline must not fetch"))
        for root in (first, second):
            with v.open_verifier(root, self.spec) as (verify, _):
                self.assertEqual(verify(None, None)[0], "pinned helper")
        (cache / self.spec["files"][v.FILES[1]]["sha256"]).write_bytes(b"corrupt")
        with self.assertRaises(ValueError):
            v.materialize(self.spec, self.root / "corrupt", cache, True, fetch)
        self.assertEqual(len(calls), 3)


if __name__ == "__main__":
    unittest.main()
