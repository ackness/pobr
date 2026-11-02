import copy
import hashlib
import io
import json
from pathlib import Path
import re
import tempfile
import unittest
import zipfile

from pipeline.imports import data_imports as imports
def pin(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


class DataImportsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.original_root = imports.ROOT
        imports.ROOT = self.root
        self.addCleanup(setattr, imports, "ROOT", self.original_root)
        self.addCleanup(self.temporary.cleanup)
        self.remote = {}
        self.calls = []
        self.requirements = {
            "schema": "pobr-data-requirements/v1", "purpose": "candidate-runtime-imports",
            "target": {"game": "poe2", "game_patch": "9.1", "realm": "international"},
            "repoe": {"usage": "text-applicability-only", "tables": {
                "BaseItemTypes": {"key": "Id", "display_fields": ["Name"]}}},
            "translation": {"locale": "zh-CN", "scopes": ["BaseItemTypes.Name"],
                            "literals": "all-catalog-conflict-participants"},
        }
        self.lock = {"schema": "pobr-data-import-lock/v1", "target": self.requirements["target"],
                     "locale": "zh-CN", "packages": {}}
        self.source = {"format": "repoe-localized-tables/v2", "production_ready": False}
        self.source_files = {"tables.json": imports.encoded({"BaseItemTypes": {
            "physical_fields": ["Id", "Name"], "rows": []}})}
        self.catalog_files = {"catalog.json": imports.encoded({"records": [
            {"table": "BaseItemTypes", "field": "Name"}]})}
        library = b'export const fixture = true;\n'
        self.lock["library"] = {"repository": "example/translation", "revision": "b" * 40,
                                "files": {"src/native-catalog.mjs": pin(library)}}
        self.remote[imports.git_address(self.lock["library"], "src/native-catalog.mjs")] = library
        self.catalog = {"schema": "poe2-native-catalog-manifest/v2", "production_ready": False,
                        **self.lock["target"], "locale": "zh-CN",
                        "producer": {"src/native-catalog.mjs": pin(library)["sha256"]}}
        self.refresh()

    def package(self, role, manifest, files):
        manifest = {**manifest, "files": {name: pin(raw) for name, raw in files.items()}}
        members = {**files, "manifest.json": imports.encoded(manifest)}
        handle = io.BytesIO()
        with zipfile.ZipFile(handle, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, raw in members.items():
                archive.writestr(name, raw)
        raw = handle.getvalue()
        spec = {"repository": f"example/{role}", "revision": ("b" if role == "translation" else "a") * 40,
                "path": "exports/package.zip", **pin(raw), "manifest": pin(members["manifest.json"]),
                "expanded_bytes": sum(map(len, members.values()))}
        self.lock["packages"][role] = spec
        self.remote[imports.git_address(spec, spec["path"])] = raw

    def refresh(self):
        self.package("repoe", self.source, self.source_files)
        self.catalog_files["request.json"] = imports.encoded({"localized": {
            "sha256": self.lock["packages"]["repoe"]["manifest"]["sha256"]}})
        self.package("translation", self.catalog, self.catalog_files)
        self.requirements_path = self.root / "requirements.json"
        self.lock_path = self.root / "lock.json"
        self.requirements_path.write_bytes(imports.encoded(self.requirements))
        self.lock_path.write_bytes(imports.encoded(self.lock))

    def fetch(self, url):
        self.calls.append(url)
        return self.remote[url]

    def prepare(self, name="actual", offline=False):
        return imports.prepare(self.requirements_path, self.lock_path, self.root / ".cache" / name,
                               self.root / ".cache/downloads", offline=offline, fetcher=self.fetch)

    def enable_supplement(self, sources=None):
        """Pin an explicit reviewed-only supplement package and its own module revision."""
        self.source["format"] = "repoe-localized-tables/v3"
        self.catalog["schema"] = "poe2-native-catalog-manifest/v3"
        self.catalog_files["verification.json"] = b'{"fixture":true}\n'
        self.refresh()
        workbench = b'export const createReviewedSupplementLookup = () => () => {};\n'
        self.lock["workbench"] = {"repository": self.lock["packages"]["translation"]["repository"],
                                  "revision": "d" * 40, "file": "src/translation-workbench.mjs",
                                  "bytes": len(workbench), "sha256": hashlib.sha256(workbench).hexdigest()}
        self.remote[imports.git_address(self.lock["workbench"], "src/translation-workbench.mjs")] = workbench
        self.requirements["translation"]["supplement"] = {"mode": "reviewed-only"}
        self.supplement_runtime = {
            "schema": "poe2-reviewed-native-supplements/v1", "status": "candidate", "production_ready": False,
            "sources": sources or [{"game": "poe2", "game_patch": self.lock["target"]["game_patch"],
                                    "realm": self.lock["target"]["realm"], "locale": self.lock["locale"],
                                    "catalog_sha256": pin(self.catalog_files["catalog.json"])["sha256"],
                                    "verification": pin(self.catalog_files["verification.json"])}],
            "records": [],
        }
        files = {"supplements.json": imports.encoded(self.supplement_runtime),
                 "request.json": imports.encoded({"schema": "poe2-translation-workbench-request/v1"})}
        manifest = {"schema": "poe2-reviewed-native-supplements-manifest/v1", "status": "candidate",
                    "production_ready": False,
                    "producer": {**{name.split("/")[-1]: spec["sha256"]
                                    for name, spec in self.lock["library"]["files"].items()},
                                 "translation-workbench.mjs": self.lock["workbench"]["sha256"]}}
        self.package("supplement", manifest, files)
        self.requirements_path.write_bytes(imports.encoded(self.requirements))
        self.lock_path.write_bytes(imports.encoded(self.lock))

    def test_declared_projection_and_offline_replay_keep_source_separate(self):
        receipt = self.prepare()
        self.assertEqual(len(self.calls), 3)
        self.calls.clear()
        replay = self.prepare("replay", offline=True)
        self.assertEqual(receipt, replay)
        self.assertEqual(self.calls, [])
        request = json.loads((self.root / ".cache/actual/display-request.json").read_text())
        self.assertEqual(request["projection"]["scopes"], ["BaseItemTypes.Name"])
        self.assertEqual(receipt["adoption"], "not-adopted")
        self.assertFalse((self.root / ".cache/actual/runtime").exists())

    def test_corrupt_cache_never_falls_back_to_network(self):
        self.prepare()
        cached = self.root / ".cache/downloads" / self.lock["packages"]["repoe"]["sha256"]
        cached.write_bytes(b"corrupt")
        self.calls.clear()
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            self.prepare("second")
        self.assertEqual(self.calls, [])
        self.assertFalse((self.root / ".cache/second").exists())

    def test_missing_field_and_changed_target_fail_before_completion(self):
        self.source_files["tables.json"] = imports.encoded({"BaseItemTypes": {"physical_fields": ["Id"]}})
        self.refresh()
        with self.assertRaisesRegex(ValueError, "source fields missing"):
            self.prepare()
        self.assertFalse((self.root / ".cache/actual").exists())
        self.lock["target"] = {**self.lock["target"], "realm": "china"}
        self.lock_path.write_bytes(imports.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "Target identity mismatch"):
            self.prepare()

    def test_another_source_manifest_or_library_cannot_be_substituted(self):
        self.catalog_files["request.json"] = imports.encoded({"localized": {"sha256": "0" * 64}})
        self.package("translation", self.catalog, self.catalog_files)
        self.lock_path.write_bytes(imports.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "another RePoE"):
            self.prepare()
        self.refresh()
        self.lock["library"]["revision"] = "c" * 40
        self.lock_path.write_bytes(imports.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "closure mismatch"):
            self.prepare()

    def test_moving_refs_unknown_scopes_and_filtered_ambiguities_reject(self):
        with self.assertRaisesRegex(ValueError, "immutable"):
            imports.git_address({"repository": "example/source", "revision": "latest"}, "tables.json")
        for edit in [{"scopes": ["Missing.Name"]}, {"literals": "translated-only"}]:
            request = copy.deepcopy(self.requirements)
            request["translation"].update(edit)
            with self.assertRaises(ValueError):
                imports.validate(request, self.lock)
        with self.assertRaisesRegex(ValueError, "Invalid input pin"):
            imports.acquire(self.lock["packages"]["repoe"], "package.zip",
                            {"bytes": 1, "sha256": "../../outside"}, self.root / ".cache", False, self.fetch)
        self.assertEqual(self.calls, [])

    def test_archive_paths_and_unlisted_members_reject(self):
        for member in ["../outside", "extra.json"]:
            raw = io.BytesIO()
            with zipfile.ZipFile(raw, "w") as archive:
                archive.writestr("manifest.json", '{"files":{}}')
                archive.writestr(member, "bad")
            spec = {"manifest": pin(b'{"files":{}}'), "expanded_bytes": 15}
            with self.assertRaises(ValueError):
                imports.unpack(raw.getvalue(), spec, self.root / "rejected")
        self.assertFalse((self.root / "outside").exists())

    def test_existing_output_and_offline_cache_miss_reject(self):
        with self.assertRaisesRegex(ValueError, "Offline cache miss"):
            self.prepare(offline=True)
        self.prepare()
        with self.assertRaisesRegex(ValueError, "Fresh"):
            self.prepare()

    def test_output_traversal_and_source_catalog_version_mismatch_reject(self):
        with self.assertRaisesRegex(ValueError, "repository cache"):
            self.prepare("../outside")
        self.source["format"] = "repoe-localized-tables/v3"
        self.refresh()
        with self.assertRaisesRegex(ValueError, "source/catalog format"):
            self.prepare()
        self.catalog["schema"] = "poe2-native-catalog-manifest/v3"
        self.catalog_files["verification.json"] = b'{}\n'
        self.refresh()
        self.prepare("v3")
        self.assertEqual((self.root / ".cache/v3/translation/verification.json").read_bytes(), b'{}\n')


    def test_reviewed_supplement_package_and_module_stay_separately_pinned(self):
        self.enable_supplement()
        receipt = self.prepare()
        request = json.loads((self.root / ".cache/actual/display-request.json").read_text())
        self.assertEqual(request["supplement"], {
            "manifest": str(self.root / ".cache/actual/supplement/manifest.json"),
            **self.lock["packages"]["supplement"]["manifest"],
            "module": {"file": "src/translation-workbench.mjs", "bytes": self.lock["workbench"]["bytes"],
                       "sha256": self.lock["workbench"]["sha256"], "revision": "d" * 40}})
        self.assertEqual((self.root / ".cache/actual/supplement/supplements.json").read_bytes(),
                         imports.encoded(self.supplement_runtime))
        self.assertTrue((self.root / ".cache/actual/library/src/translation-workbench.mjs").is_file())
        self.assertNotIn("src/translation-workbench.mjs", request["library"]["files"])
        self.assertEqual(receipt["packages"]["supplement"], self.lock["packages"]["supplement"])
        self.assertEqual(receipt["workbench"], self.lock["workbench"])
        self.assertFalse((self.root / ".cache/actual/runtime").exists())
        self.assertEqual(receipt, self.prepare("replay", offline=True))

    def test_supplement_must_be_declared_and_bound_to_the_current_catalog(self):
        self.enable_supplement()
        other = {"game": "poe2", "game_patch": "9.1", "realm": "international", "locale": "zh-CN",
                 "catalog_sha256": "0" * 64, "verification": None}
        self.enable_supplement(sources=[other])
        with self.assertRaisesRegex(ValueError, "bound to another catalog"):
            self.prepare()
        self.enable_supplement(sources=[{**other, "realm": "china",
                                        "catalog_sha256": pin(self.catalog_files["catalog.json"])["sha256"]}])
        with self.assertRaisesRegex(ValueError, "bound to another catalog"):
            self.prepare()
        self.enable_supplement()
        self.lock["packages"].pop("supplement")
        self.lock_path.write_bytes(imports.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "Missing library package"):
            self.prepare("missing")
        self.enable_supplement()
        self.lock["packages"].pop("supplement")
        del self.requirements["translation"]["supplement"]
        self.requirements_path.write_bytes(imports.encoded(self.requirements))
        self.lock_path.write_bytes(imports.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "Unused reviewed supplement pin"):
            self.prepare("unused")
        self.assertFalse((self.root / ".cache/missing").exists())
        self.assertFalse((self.root / ".cache/unused").exists())

    def test_supplement_module_revision_repository_and_bytes_are_exact(self):
        self.enable_supplement()
        edits = [({"revision": "latest"}, "Invalid workbench module pin"),
                 ({"file": "src/native-catalog.mjs"}, "Invalid workbench module pin"),
                 ({"repository": "example/other"}, "Workbench module repository mismatch"),
                 ({"sha256": "0" * 64}, "Input bytes/SHA256 mismatch")]
        for index, (edit, message) in enumerate(edits):
            self.enable_supplement()
            self.lock["workbench"].update(edit)
            self.lock_path.write_bytes(imports.encoded(self.lock))
            # A different blob at the pinned address must never satisfy another pin.
            if re.fullmatch(r"[a-f0-9]{40}", self.lock["workbench"]["revision"]):
                self.remote[imports.git_address(self.lock["workbench"], "src/translation-workbench.mjs")] = \
                    b"export const other = true;\n"
            with self.assertRaisesRegex(ValueError, message):
                self.prepare(f"reject-{index}")
            self.assertFalse((self.root / f".cache/reject-{index}").exists())
        self.enable_supplement()
        unsupported = copy.deepcopy(self.requirements)
        unsupported["translation"]["supplement"] = {"mode": "all"}
        with self.assertRaisesRegex(ValueError, "Unsupported reviewed supplement mode"):
            imports.validate(unsupported, self.lock)

    def test_unsupported_catalog_rejects_supplement_before_import_completion(self):
        self.enable_supplement()
        self.source["format"] = "repoe-localized-tables/v2"
        self.catalog["schema"] = "poe2-native-catalog-manifest/v2"
        self.refresh()
        with self.assertRaisesRegex(ValueError, "Reviewed supplements require a v3 catalog"):
            self.prepare("old-with-supplement")
        self.assertFalse((self.root / ".cache/old-with-supplement").exists())

if __name__ == "__main__":
    unittest.main()
