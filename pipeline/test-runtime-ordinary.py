"""Exercise the producer-to-pack boundary with the real subprocess fixture.

This is orchestration evidence, not Rust conversion or gameplay acceptance.
"""
import copy
import importlib.util
from pathlib import Path
import shutil
import unittest

import data_bundle as bundle
import ordinary_inputs as ordinary
import runtime_ordinary
import runtime_pack as pack

spec = importlib.util.spec_from_file_location("ordinary_fixture", Path(__file__).with_name("test-ordinary-inputs.py"))
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)


class RuntimeOrdinaryTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixture_module.Contracts()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root, self.target = self.fixture.root, self.fixture.target
        self.version = self.target["game_patch"]
        self.complete = self.fixture.run_candidate()
        self.source = self.root / "outputs-only"
        names = ["completed.json", "inputs/handoff.json/handoff.json"]
        for domain, result in self.complete["domains"].items():
            names.extend(f"{domain}/{name}" for name in ["producer-receipt.json", *result["outputs"]])
        for name in names:
            self.write(self.source / name, (self.root / "candidate" / name).read_bytes())
        shutil.rmtree(self.root / "candidate")
        self.config = {"requirements": "ordinary.requirements.json", "lock": "ordinary.lock.json",
                       "outputs": {domain: sorted(result["outputs"]) for domain, result in self.complete["domains"].items()}}
        self.write(self.root / self.config["requirements"], ordinary.encoded(self.fixture.request))
        self.write(self.root / self.config["lock"], ordinary.encoded(self.fixture.lock))
        self.files = {f"data/{self.version}/overlay/{domain}.json": b'{"old":true}\n'
                      for domain in self.complete["domains"]}
        self.files[f"data/{self.version}/base/skills.json"] = b'[{"raw":9223372036854775807,"rate":1.00e-09}]\n'
        self.files["data/overlay-common/rules.json"] = b'{"retained":[null,3,2,1]}\n'
        self.files[f"data/{self.version}/i18n/zh-CN/skills.json"] = b'{"old":"retained"}\n'

    def write(self, path, raw):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
        return path

    def overlay(self, source=None):
        return runtime_ordinary.overlay(self.files, self.version, self.target, self.config, self.root,
                                        self.source if source is None else source)

    def update_complete(self, value):
        self.write(self.source / "completed.json", ordinary.encoded(value))

    def test_outputs_only_artifact_replaces_declared_paths_and_keeps_exact_other_bytes(self):
        result, evidence = self.overlay()
        self.assertEqual(set(result), set(self.files))
        self.assertEqual(len(evidence["replacements"]), 2)
        for domain in self.complete["domains"]:
            name = f"overlay/{domain}.json"
            self.assertEqual(result[f"data/{self.version}/{name}"], b'{"unknown":[null,-5]}\n')
            self.assertEqual(evidence["replacements"][name]["previous"], pack.pin(b'{"old":true}\n'))
        self.assertEqual(result[f"data/{self.version}/base/skills.json"], self.files[f"data/{self.version}/base/skills.json"])
        self.assertEqual(result["data/overlay-common/rules.json"], self.files["data/overlay-common/rules.json"])
        self.assertFalse((self.source / "source").exists())
        self.assertFalse((self.source / "inputs/ordinary-input.zip").exists())

    def test_partial_wrong_version_lock_request_and_missing_domains_fail(self):
        for change in ("status", "version", "realm", "lock", "requirements", "domain", "source", "binary", "verifier"):
            changed = copy.deepcopy(self.complete)
            if change == "status": changed["status"] = "verified-inputs-only"
            elif change in ("version", "realm"): changed["target"]["game_patch" if change == "version" else "realm"] = "wrong"
            elif change in ("lock", "requirements"): changed[change]["sha256"] = "0" * 64
            elif change == "domain": changed["domains"].pop("spectres")
            elif change == "source": changed["facts_manifest"]["sha256"] = "0" * 64
            elif change == "verifier": changed["source_verifier"]["library"]["revision"] = "b" * 40
            else: changed["binary"]["bytes"] = True
            self.update_complete(changed)
            with self.subTest(change=change), self.assertRaises(ValueError): self.overlay()
        (self.source / "completed.json").unlink()
        with self.assertRaises(ValueError): self.overlay()

    def test_declaration_cannot_redirect_or_expand_domain_ownership(self):
        for outputs in ({}, {"minions": ["../escape.json"], "spectres": ["overlay/spectres.json"]},
                        {"minions": ["overlay/minions.json", "overlay/minions.json"], "spectres": ["overlay/spectres.json"]}):
            self.config["outputs"] = outputs
            with self.assertRaisesRegex(ValueError, "ownership"): self.overlay()

    def test_coherently_rehashed_receipt_still_checks_source_target_selection_and_outputs(self):
        path = self.source / "minions/producer-receipt.json"
        original = ordinary.load(path)
        for change in ("realm", "source", "selection", "source-lock", "outputs", "adapter", "ready"):
            receipt = copy.deepcopy(original)
            if change == "realm": receipt["target"]["realm"] = "china"
            elif change == "source": receipt["input"]["manifest_sha256"] = "0" * 64
            elif change == "selection": receipt["input"]["selected_payloads"].pop("tables/A.json")
            elif change == "source-lock": receipt["files"]["source-lock.json"]["sha256"] = "0" * 64
            elif change == "outputs": receipt["files"]["overlay/extra.json"] = pack.pin(b"{}")
            elif change == "adapter": receipt["adapter"]["name"] = "another adapter"
            else: receipt["complete_runtime"] = True
            raw = ordinary.encoded(receipt)
            self.write(path, raw)
            completed = copy.deepcopy(self.complete)
            completed["domains"]["minions"]["receipt"] = pack.pin(raw)
            self.update_complete(completed)
            with self.subTest(change=change), self.assertRaises(ValueError): self.overlay()

    def test_modified_missing_and_symlink_outputs_or_handoff_are_rejected(self):
        for name in ("minions/overlay/minions.json", "minions/producer-receipt.json", "inputs/handoff.json/handoff.json"):
            path = self.source / name
            original = path.read_bytes()
            path.write_bytes(original + b" ")
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "bytes/SHA256"): self.overlay()
            path.unlink()
            with self.assertRaises(ValueError): self.overlay()
            alternative = self.write(self.root / "alternative", original)
            path.symlink_to(alternative)
            with self.assertRaisesRegex(ValueError, "Symlink"): self.overlay()
            path.unlink()
            path.write_bytes(original)

    def test_output_must_already_exist_in_frozen_runtime(self):
        self.files.pop(f"data/{self.version}/overlay/minions.json")
        with self.assertRaisesRegex(ValueError, "unique replacement"): self.overlay()

    def import_fixture(self):
        snapshot = {"schema_version": 3, "poe_version": self.version, "languages": ["zh-CN"],
                    "domains": {"base": ["skills"], "overlay": ["minions", "spectres"]},
                    "files": {name.removeprefix(f"data/{self.version}/"): bundle.sha256(raw)
                              for name, raw in self.files.items() if name.startswith(f"data/{self.version}/")}}
        for name, raw in self.files.items(): self.write(self.root / "baseline" / name, raw)
        self.write(self.root / f"baseline/data/{self.version}/manifest.json", pack.raw_json(snapshot))
        self.write(self.root / "baseline/data/CURRENT", (self.version + "\n").encode())
        bundle_receipt = bundle.create(self.root / "baseline", self.root / "sealed")
        recipe = {"schema": "pobr-remaining-domain-integration/v1", "runtime_identity": bundle_receipt["runtime_identity"],
                  "bundle_id": bundle_receipt["bundle_id"], "snapshot_manifest_sha256": bundle.sha256(pack.raw_json(snapshot)),
                  "files": {name: {"sha256": value, "owner": "test retained"} for name, value in snapshot["files"].items()},
                  "shared_files": {"rules.json": {"sha256": bundle.sha256(self.files["data/overlay-common/rules.json"])}},
                  "producer_receipts": {}, "language_receipt_sha256": "a" * 64, "scope": "Test retained inputs"}
        recipe_path = self.write(self.root / "recipe.json", pack.raw_json(recipe))
        lock = pack.export_inputs(self.root / "sealed", recipe_path, self.root / "exported")
        lock.update(publication="published", repository="test/fixture", revision="a" * 40)
        lock_path = self.write(self.root / "core.lock.json", pack.raw_json(lock))
        requirements = {"target": self.target, "translation": {"locale": "zh-CN"},
                        "runtime_pack": {"schema": pack.SCHEMA, "ordinary": self.config}}
        request_path = self.write(self.root / "requirements.json", pack.raw_json(requirements))
        display = pack.raw_json({"schema": "pobr-native-display-texts/v1", "game_patch": self.version,
                                "realm": self.target["realm"], "locale": "zh-CN", "entries": {"blocked": None}})
        self.write(self.root / "imports/runtime/display_texts.json", display)
        self.write(self.root / "imports/runtime/receipt.json", pack.raw_json({"files": {"display_texts.json": pack.pin(display)}}))
        self.write(self.root / "imports/import-receipt.json", pack.raw_json({"requirements_sha256": bundle.sha256(request_path.read_bytes()),
                   "target": self.target, "locale": "zh-CN"}))
        archive = (self.root / "exported/core-inputs.zip").read_bytes()
        return request_path, lock_path, archive

    def test_actual_import_materialization_and_offline_replay_with_ci_shaped_artifacts(self):
        request, lock, archive = self.import_fixture()
        args = (request, lock, self.root / "imports", self.root / "pack", self.root / "cache")
        result = pack.import_pack(*args, fetcher=lambda _: archive, ordinary_path=self.source)
        release, files, payloads = pack.read_release(self.root / "pack/player")
        self.assertEqual(result["release_id"], release["release_id"])
        self.assertEqual(files["overlay/minions.json"], b'{"unknown":[null,-5]}\n')
        self.assertIn(b'9223372036854775807', files["base/skills.json"])
        self.assertIn(b'1.00e-09', files["base/skills.json"])
        self.assertEqual(bundle.decode(files["i18n/zh-CN/display_texts.json"])["entries"], {"blocked": None})
        self.assertFalse(any("receipt" in name or "source-lock" in name for name in files))
        pack.import_pack(*(*args[:3], self.root / "replay", args[4]), offline=True, ordinary_path=self.source,
                         fetcher=lambda _: self.fail("Offline fetch"))
        self.assertEqual((self.root / "replay/runtime-pack.zip").read_bytes(), (self.root / "pack/runtime-pack.zip").read_bytes())
        pack.materialize(self.root / "pack/runtime-pack.zip", self.root / "materialized")
        self.assertEqual((self.root / f"materialized/data/{self.version}/overlay/minions.json").read_bytes(), files["overlay/minions.json"])
        evidence = ordinary.load(self.root / "pack/build-receipt.json")
        self.assertEqual(evidence["ordinary"]["completion"], pack.pin((self.source / "completed.json").read_bytes()))
        self.assertEqual(len(payloads), 4)

    def test_import_cannot_omit_declared_production_or_publish_failed_assembly(self):
        request, lock, archive = self.import_fixture()
        args = (request, lock, self.root / "imports", self.root / "pack", self.root / "cache")
        with self.assertRaisesRegex(ValueError, "requires --ordinary"):
            pack.import_pack(*args, fetcher=lambda _: archive)
        self.assertFalse((self.root / "pack").exists())
        (self.source / "minions/overlay/minions.json").write_bytes(b"{}")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            pack.import_pack(*args, offline=True, ordinary_path=self.source)
        self.assertFalse((self.root / "pack").exists())


if __name__ == "__main__":
    unittest.main()
