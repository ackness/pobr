"""Transport regressions using the real assembler, archive and legacy materializer."""

import copy
import json
from pathlib import Path
import tempfile
import subprocess
import unittest

from pipeline.runtime import data_bundle as bundle
from pipeline.runtime import runtime_pack as pack
class RuntimePackTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr runtime pack ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.version = "9.1"
        self.target = {"game": "poe2", "game_patch": self.version, "realm": "international"}
        self.write("source/data/CURRENT", b"9.1\n")
        contents = {
            "base/skills.json": b'[{"id":"raw","stats":[3,2,1],"i64":9223372036854775807,"value":0.10000000149011612,"text":"a\\r\\nb"}]',
            "base/passive_trees/0_1.json": b'[{"skill":42,"stats":["old build"]}]',
            "overlay/trade_catalog.json": b'{"_meta":{"source":"retained compatibility"},"gems":[{"id":"support"}],"bases":[]}',
            "generated/special_vendor.json": b'{"_meta":{"build":"only"},"entries":[{"id":"rule","batch":"S0","verified":false,"game_source":"stat_id","examples":["audit"],"vendor_pattern":"audit","source_note":"audit","mods":[]}]}',
            "i18n/zh-CN/skills.json": '{"raw":"技能"}'.encode(),
            "i18n/zh-CN/_meta.json": b'{"build":"only"}',
            "i18n/zh-TW/skills.json": '{"raw":"技能TW"}'.encode(),
        }
        for name, raw in contents.items():
            self.write(f"source/data/9.1/{name}", raw)
        snapshot = {"schema_version": 3, "poe_version": self.version, "languages": ["zh-CN", "zh-TW"],
                    "domains": {"base": ["skills"], "overlay": ["trade_catalog"], "generated": ["special_vendor"]},
                    "files": {name: bundle.sha256(raw) for name, raw in contents.items()}}
        self.write("source/data/9.1/manifest.json", pack.raw_json(snapshot))
        self.write("source/data/overlay-common/special_mods.json", b'{"_meta":{"audit":true},"entries":[]}')
        self.source = self.root / "sealed"
        receipt = bundle.create(self.root / "source", self.source)
        recipe = {"schema": "pobr-remaining-domain-integration/v1", "runtime_identity": receipt["runtime_identity"],
                  "bundle_id": receipt["bundle_id"], "snapshot_manifest_sha256": bundle.sha256(pack.raw_json(snapshot)),
                  "files": {name: {"sha256": bundle.sha256(raw), "owner": "retained-test-input", "path": "/unavailable/original"}
                            for name, raw in contents.items()},
                  "shared_files": {"special_mods.json": {"sha256": bundle.sha256(b'{"_meta":{"audit":true},"entries":[]}')}},
                  "producer_receipts": {}, "language_receipt_sha256": "a" * 64, "scope": "Frozen test compatibility"}
        self.recipe = self.write("recipe.json", pack.raw_json(recipe))
        self.display = self.write("display.json", pack.raw_json({"schema": "pobr-native-display-texts/v1",
            "game_patch": self.version, "realm": "international", "locale": "zh-CN", "entries": {"blocked": None},
            "scopes": {"BaseItemTypes.Name": {"id": ["raw", None]}}}))
        self.output = self.root / "candidate"

    def write(self, name, raw):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
        return path

    def create(self, output=None):
        return pack.create(self.source, self.recipe, self.display, output or self.output, self.target, "zh-CN")

    def test_real_assembly_is_deterministic_and_archive_uses_same_block_bytes(self):
        first = self.create()
        other = self.root / "second"
        self.assertEqual(first, self.create(other))
        self.assertEqual((self.output / "runtime-pack.zip").read_bytes(), (other / "runtime-pack.zip").read_bytes())
        release, files, payloads = pack.read_release(self.output / "player", first["release_id"])
        self.assertEqual(pack.read_release(self.output / "runtime-pack.zip")[2], payloads)
        self.assertEqual(len(payloads), 4)
        self.assertIn(b'9223372036854775807', files["base/skills.json"])
        self.assertIn(b'0.10000000149011612', files["base/skills.json"])
        self.assertEqual(json.loads(files["base/skills.json"])[0]["stats"], [3, 2, 1])
        self.assertEqual(json.loads(files["base/skills.json"])[0]["text"], "a\r\nb")
        self.assertEqual(json.loads(files["i18n/zh-CN/display_texts.json"])["entries"], {"blocked": None})
        self.assertIn("base/passive_trees/0_1.json", files)
        self.assertEqual(json.loads(files["overlay/trade_catalog.json"])["gems"], [{"id": "support"}])
        self.assertNotIn("i18n/zh-TW/skills.json", files)
        self.assertNotIn("i18n/zh-CN/_meta.json", files)
        rule = json.loads(files["generated/special_vendor.json"])
        self.assertNotIn("_meta", rule)
        self.assertNotIn("examples", rule["entries"][0])
        self.assertEqual(rule["entries"][0]["batch"], "S0")
        self.assertEqual(rule["entries"][0]["game_source"], "stat_id")
        self.assertNotEqual(release["runtime_identity"], bundle.verify(self.source)["runtime_identity"])

    def test_release_builds_directly_from_prepared_owner_snapshot(self):
        display = self.display.read_bytes()
        source = self.root / "source/data/9.1"
        self.write("source/data/9.1/i18n/zh-CN/display_texts.json", display)
        snapshot = json.loads((source / "manifest.json").read_bytes())
        snapshot["files"]["i18n/zh-CN/display_texts.json"] = bundle.sha256(display)
        self.write("source/data/9.1/manifest.json", pack.raw_json(snapshot))
        requirements = self.write("requirements.json", pack.raw_json(
            {"target": self.target, "translation": {"locale": "zh-CN"}}))
        first = pack.build_release(requirements, self.root / "source/data", self.root / "built")
        again = pack.build_release(requirements, self.root / "source/data", self.root / "built-again",
                                   expected_id=first["release_id"])
        self.assertEqual(first, again)
        release, _, payloads = pack.read_release(self.root / "built/player", first["release_id"])
        self.assertEqual(release["runtime_identity"], first["runtime_identity"])
        self.assertEqual((self.root / "built/runtime-pack.zip").read_bytes(), pack.archive_bytes(payloads))
        with self.assertRaisesRegex(ValueError, "Unexpected release identity"):
            pack.build_release(requirements, self.root / "source/data", self.root / "wrong", expected_id="0" * 64)
        self.assertFalse((self.root / "wrong").exists())
        self.write("source/data/9.1/i18n/zh-CN/display_texts.json", display + b" ")
        with self.assertRaisesRegex(ValueError, "Snapshot hash mismatch"):
            pack.build_release(requirements, self.root / "source/data", self.root / "tampered")
        self.assertFalse((self.root / "tampered").exists())

    def test_archive_materializes_a_complete_existing_bundle_with_matching_identity(self):
        release = self.create()
        destination = self.root / "materialized"
        receipt = pack.materialize(self.output / "runtime-pack.zip", destination, release["release_id"])
        self.assertEqual(bundle.verify(destination), receipt)
        self.assertEqual(receipt["runtime_identity"], release["runtime_identity"])
        self.assertEqual((destination / "data/CURRENT").read_bytes(), b"9.1\n")
        self.assertTrue((destination / "data/overlay-common/special_mods.json").is_file())
        with self.assertRaisesRegex(ValueError, "Fresh output"):
            pack.materialize(self.output / "player", destination)

    def test_corruption_wrong_external_pin_and_unlisted_files_fail_closed(self):
        self.create()
        source = self.output / "player"
        with self.assertRaisesRegex(ValueError, "Unexpected release"):
            pack.read_release(source, "0" * 64)
        raw = (source / "blocks/core.json").read_bytes()
        (source / "blocks/core.json").write_bytes(raw + b" ")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            pack.materialize(source, self.root / "bad")
        self.assertFalse((self.root / "bad").exists())
        (source / "blocks/core.json").write_bytes(raw)
        (source / "build-evidence.bin").write_bytes(b"extra")
        with self.assertRaisesRegex(ValueError, "Unlisted"):
            pack.read_release(source)

    def test_mixed_version_locale_shared_rules_and_file_inventory_are_rejected(self):
        self.create()
        release, _, payloads = pack.read_release(self.output / "player")
        for mutate in ("version", "locale", "shared", "inventory", "snapshot"):
            with self.subTest(mutate=mutate):
                changed = dict(payloads)
                spec = copy.deepcopy(release["blocks"][-1 if mutate in ("version", "locale", "snapshot") else 1])
                block = bundle.decode(changed[spec["path"]])
                if mutate == "version":
                    block["target"]["game_patch"] = "9.2"
                elif mutate == "locale":
                    block["files"]["i18n/zh-TW/skills.json"] = "{}"
                elif mutate == "shared":
                    block["files"]["overlay-common/special_mods.json"] = '{"entries":[1]}'
                elif mutate == "snapshot":
                    block["files"]["manifest.json"] = '{"schema_version":3,"poe_version":"9.2"}'
                else:
                    block["files"]["../escape.json"] = "{}"
                changed[spec["path"]] = pack.raw_json(block)
                # Rehash the transport envelope so these assertions exercise
                # target/member/snapshot validation, not just corrupt bytes.
                manifest = copy.deepcopy(release)
                actual_spec = next(b for b in manifest["blocks"] if b["path"] == spec["path"])
                actual_spec.update(pack.pin(changed[spec["path"]]))
                if mutate == "snapshot":
                    manifest["digests"]["manifest.json"] = bundle.sha256(block["files"]["manifest.json"].encode())
                manifest = pack.release_document(manifest["target"], manifest["locale"], manifest["digests"], manifest["blocks"])
                changed["manifest.json"] = pack.raw_json(manifest)
                with self.assertRaises(ValueError):
                    pack.inspect_release(changed)

    def test_native_runtime_provenance_and_source_number_tokens_survive_projection(self):
        raw = b'[{"native_stats":{"stats":[{"value":-9223372036854775808,"provenance":{"references":[1,2]}}]},"a":1.00e-09}]'
        projected = pack.project("base/passive_tree.json", raw)
        self.assertIn(b'1.00e-09', projected)
        self.assertEqual(json.loads(raw), json.loads(projected))
        for invalid in (b'{"a":1,"a":2}', b'{"a":NaN}'):
            with self.assertRaises(ValueError):
                pack.project("base/skills.json", invalid)

    def test_full_alignment_ignores_only_existing_projection_audit_fields(self):
        prefix = "data/0.5.5.5/"
        name = prefix + "base/passive_tree.json"
        sidecar = prefix + "i18n/zh-CN/display_texts.json"
        original = {name: b'{"_meta":{"audit":1},"nodes":[{"value":9223372036854775807,"rate":1.00e-09}]}',
                    sidecar: b'{"text":null}'}
        candidate = {**original, name: b'{"nodes":[{"rate":1.00e-09,"value":9223372036854775807}]}'}
        pack.require_full_alignment(original, candidate, "0.5.5.5", "zh-CN", original[sidecar])
        for changed in (b'{"nodes":[{"rate":1.00e-09,"value":9223372036854775806}]}',
                        b'{"nodes":[{"rate":0.000000001,"value":9223372036854775807}]}',
                        b'{"nodes":[{"rate":1.00e-09,"value":9223372036854775807,"provenance":null}]}'):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "consumer-data-divergence"):
                pack.require_full_alignment(original, {**candidate, name: changed}, "0.5.5.5", "zh-CN", original[sidecar])
        with self.assertRaisesRegex(ValueError, "display sidecar"):
            pack.require_full_alignment(original, candidate, "0.5.5.5", "zh-CN", b'{"text":"new"}')

    def test_source_recipe_display_identity_and_symlinks_reject_without_output(self):
        with self.assertRaisesRegex(ValueError, "separate from input"):
            self.create(self.source / "new-parent/nested-output")
        self.assertFalse((self.source / "new-parent").exists())
        recipe = bundle.decode(self.recipe.read_bytes())
        recipe["runtime_identity"] = "0" * 64
        self.recipe.write_bytes(pack.raw_json(recipe))
        with self.assertRaisesRegex(ValueError, "recipe identity"):
            self.create()
        self.assertFalse(self.output.exists())
        alias = self.root / "alias"
        alias.symlink_to(self.display)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            pack.regular(alias)

    def test_portable_core_import_is_exact_and_pending_publication_has_no_fallback(self):
        exported = self.root / "handoff"
        lock = pack.export_inputs(self.source, self.recipe, exported)
        requirements = {"target": self.target, "translation": {"locale": "zh-CN"}, "runtime_pack": {"schema": pack.SCHEMA}}
        requirements_path = self.write("requirements.json", pack.raw_json(requirements))
        lock_path = exported / "runtime-inputs.lock.json"
        args = (requirements_path, lock_path, self.root / "imports", self.root / "imported", self.root / "downloads")
        with self.assertRaisesRegex(ValueError, "unpublished"):
            pack.import_pack(*args)
        pending = pack.import_pack(*args, allow_unpublished=True)
        self.assertEqual(pending["status"], "pending-core-publication")
        self.assertFalse((self.root / "imported").exists())
        lock.update(publication="published", repository="example/core-inputs", revision="a" * 40)
        lock_path.write_bytes(pack.raw_json(lock))
        self.write("imports/runtime/display_texts.json", self.display.read_bytes())
        self.write("imports/runtime/receipt.json", pack.raw_json({"files": {"display_texts.json": pack.pin(self.display.read_bytes())}}))
        self.write("imports/import-receipt.json", pack.raw_json({"requirements_sha256": bundle.sha256(requirements_path.read_bytes()),
                   "target": self.target, "locale": "zh-CN"}))
        calls = []

        def fetch(address):
            calls.append(address)
            return (exported / "core-inputs.zip").read_bytes()

        result = pack.import_pack(*args, fetcher=fetch)
        self.assertEqual(result["status"], "assembled")
        self.assertEqual(len(calls), 1)
        self.create()
        self.assertEqual((self.root / "imported/runtime-pack.zip").read_bytes(), (self.output / "runtime-pack.zip").read_bytes())
        replay_args = (*args[:3], self.root / "offline", args[4])
        pack.import_pack(*replay_args, offline=True, fetcher=lambda _: self.fail("network during replay"))
        cached = args[4] / lock["sha256"]
        cached.write_bytes(b"broken")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            pack.import_pack(*(*args[:3], self.root / "corrupt", args[4]), offline=True)

    def test_local_bare_git_handoff_contains_the_exact_portable_artifact(self):
        output = self.root / "git-artifact"
        lock = pack.export_inputs(self.source, self.recipe, output)
        handoff = pack.git_handoff(output)
        raw = subprocess.check_output(["git", "--git-dir", str(output / "handoff.git"), "cat-file", "blob",
                                       f"{handoff['revision']}:{handoff['path']}"])
        self.assertEqual(pack.pin(raw), {k: lock[k] for k in ("bytes", "sha256")})
        self.assertEqual(raw, (output / "core-inputs.zip").read_bytes())
        self.assertFalse((output / "handoff.git/.git").exists())
        self.assertIsNone(bundle.decode((output / "runtime-inputs.lock.json").read_bytes())["revision"])


if __name__ == "__main__":
    unittest.main()
