"""Transport regressions using the real assembler, archive and legacy materializer."""

import copy
import json
from pathlib import Path
import tempfile
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
        self.data = self.root / "data"
        self.write("data/CURRENT", b"9.1\n")
        contents = {
            "base/skills.json": b'[{"id":"raw","stats":[3,2,1],"i64":9223372036854775807,"value":0.10000000149011612,"text":"a\\r\\nb"}]',
            "base/passive_trees/0_1.json": b'[{"skill":42,"stats":["old build"]}]',
            "overlay/trade_catalog.json": b'{"_meta":{"source":"retained compatibility"},"gems":[{"id":"support"}],"bases":[]}',
            "generated/special_vendor.json": b'{"_meta":{"build":"only"},"entries":[{"id":"rule","batch":"S0","verified":false,"game_source":"stat_id","examples":["audit"],"vendor_pattern":"audit","source_note":"audit","mods":[]}]}',
            "i18n/zh-CN/skills.json": '{"raw":"技能"}'.encode(),
            "i18n/zh-CN/_meta.json": b'{"build":"only"}',
            "i18n/zh-CN/display_texts.json": pack.raw_json({"schema": "pobr-native-display-texts/v1",
                "game_patch": self.version, "realm": "international", "locale": "zh-CN",
                "entries": {"blocked": None},
                "scopes": {"BaseItemTypes.Name": {"id": ["raw", None]}}}),
            "i18n/zh-TW/skills.json": '{"raw":"技能TW"}'.encode(),
        }
        for name, raw in contents.items():
            self.write(f"data/9.1/{name}", raw)
        snapshot = {"schema_version": 3, "poe_version": self.version, "languages": ["zh-CN", "zh-TW"],
                    "domains": {"base": ["skills"], "overlay": ["trade_catalog"], "generated": ["special_vendor"]},
                    "files": {name: bundle.sha256(raw) for name, raw in contents.items()}}
        self.write("data/9.1/manifest.json", pack.raw_json(snapshot))
        self.write("data/overlay-common/special_mods.json", b'{"_meta":{"audit":true},"entries":[]}')
        _, runtime_identity = bundle.data_inputs(self.data, self.version)
        self.lock = self.write("data.lock.json", pack.raw_json(
            {"schema": "pobr-data-lock/v1", "target": self.target, "locale": "zh-CN",
             "runtime_identity": runtime_identity}))
        self.output = self.root / "candidate"

    def write(self, name, raw):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
        return path

    def create(self, output=None):
        return pack.build_release(self.lock, self.data, output or self.output)

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
        self.assertNotEqual(release["runtime_identity"], bundle.data_inputs(self.data, self.version)[1])

    def test_release_identity_lock_and_snapshot_pins_fail_closed(self):
        first = self.create()
        lock = bundle.decode(self.lock.read_bytes())
        lock["release_id"] = first["release_id"]
        self.lock.write_bytes(pack.raw_json(lock))
        self.assertEqual(first, self.create(self.root / "built-again"))
        release, _, payloads = pack.read_release(self.root / "built-again/player", first["release_id"])
        self.assertEqual(release["runtime_identity"], first["runtime_identity"])
        self.assertEqual((self.root / "built-again/runtime-pack.zip").read_bytes(),
                         pack.archive_bytes(payloads))
        with self.assertRaisesRegex(ValueError, "Unexpected release identity"):
            pack.build_release(self.lock, self.data, self.root / "wrong", expected_id="0" * 64)
        self.assertFalse((self.root / "wrong").exists())
        self.lock.write_bytes(pack.raw_json({**lock, "runtime_identity": "0" * 64}))
        with self.assertRaisesRegex(ValueError, "differs from the data lock"):
            self.create(self.root / "other")
        self.assertFalse((self.root / "other").exists())
        self.lock.write_bytes(pack.raw_json(lock))
        display = self.data / "9.1/i18n/zh-CN/display_texts.json"
        original = display.read_bytes()
        try:
            display.write_bytes(original + b" ")
            with self.assertRaisesRegex(ValueError, "Snapshot hash mismatch"):
                self.create(self.root / "tampered")
        finally:
            display.write_bytes(original)
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

    def test_existing_output_and_symlinked_paths_reject(self):
        self.create()
        with self.assertRaisesRegex(ValueError, "Fresh output"):
            self.create()
        alias = self.root / "alias"
        alias.symlink_to(self.lock)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            pack.regular(alias)


if __name__ == "__main__":
    unittest.main()
