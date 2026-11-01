"""Synthetic, offline contracts for draft rebinding and portable source inputs."""
import copy
import io
from pathlib import Path
import shutil
import stat
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

import source_rebinding as s


class Fixture:
    def __init__(self, root):
        self.root = root
        self.source = root / "source"
        self.source.mkdir()
        self.target = {"game": "poe2", "realm": "international", "game_patch": "0.5.5.5"}
        self.base = "https://patch-poe2.poecdn.com/0.5.5.5/"
        names = ["Metadata/PassiveSkillGraph.psg", *(f"Data/Balance/{t}.datc64" for t in ("A", "B", "C"))]
        self.resources = {n: {**s.pin(n.encode()), "bundle_url": self.base + "Bundles2/Tiny.bundle.bin",
                              "uncompressed_offset": i * 100, "uncompressed_length": len(n)}
                          for i, n in enumerate(names)}
        groups = {g: [n] for g, n in zip(("base", "a0", "a1", "bc"), names)}
        self.evidence = {"groups.json": groups, "acceptance.json": {"scope": "replacement evidence only"}}
        for group, paths in groups.items():
            self.evidence[group + ".json"] = {**self.target, "resources": {n: self.resources[n] for n in paths}}
        selection = {**self.target, "resources": {
            n: {k: r[k] for k in ("bundle_url", "uncompressed_offset", "uncompressed_length")}
            for n, r in self.resources.items() if n in groups["a1"]}}
        self.evidence["selection.json"] = selection
        online = {**self.evidence["a1.json"], "status": s.STATUS, "production_ready": False,
                  "mode": "online", "selection": {"sha256": s.client.sha256(s.client.encoded(selection))},
                  "network": {"requests": 1, "body_bytes": 9}}
        self.evidence["a1.json"] = online
        self.evidence["offline.json"] = {**online, "mode": "offline", "network": {"requests": 0, "body_bytes": 0}}
        old = {"url": self.base + "Bundles2/Tiny.bundle.bin", "final_url": self.base + "Bundles2/Tiny.bundle.bin",
               "start": 0, "length": 3, "total": 500, "etag": '"fixed"', "sha256": "3" * 64,
               "provenance": {"kind": "verified-cache-copy", "source_root": "/removed/original-cache"}}
        self.evidence["bc.json"]["ranges"] = [old]
        self.evidence["ranges.json"] = {
            "source_patch": "0.5.5.5", "offline": {"requests": 0, "body_bytes": 0},
            "original_bc_receipt_sha256": s.client.sha256(s.client.encoded(self.evidence["bc.json"])),
            "records": [{"historical_record": old, "replacement_record": {**old, "provenance": {"kind": "http-range"}}}]}
        old_a1 = [{"kind": "a1-acquisition-receipt", "sha256": "1" * 64},
                  {"kind": "a1-offline-proof", "sha256": "2" * 64}]
        active = [{"kind": "base-receipt", "sha256": s.client.sha256(s.client.encoded(self.evidence["base.json"]))}, *old_a1]
        ancestor = {"kind": "official-client-resource-candidate", **self.target, "base_url": self.base,
                    "decoder": {"revision": "fixed"}, "scope": "Eight tables (stale)",
                    "acquisition": {"status": s.STATUS, "evidence": active},
                    "resources": {n: r for n, r in self.resources.items() if n != names[-1]},
                    "unknown_source_field": {"raw": [None, 42]}}
        selected = copy.deepcopy(ancestor)
        selected["resources"][names[-1]] = {k: self.resources[names[-1]][k] for k in ("bytes", "sha256")}
        selected["acquisition"]["evidence"].append({
            "kind": "bc-receipt", "sha256": s.client.sha256(s.client.encoded(self.evidence["bc.json"]))})
        self.lock = {"schema_version": 1, "sources": {"ancestor": ancestor, "selected": selected},
                     "unknown_root": ["preserved"]}
        lock_raw = s.client.encoded(self.lock)
        payload = {"source-lock.json": lock_raw, "graph.json": s.client.encoded({"unknown_graph": None})}
        for table in ("A", "B", "C"):
            payload[f"tables/{table}.json"] = s.client.encoded({
                "table": table, "raw_resource": f"Data/Balance/{table}.datc64", "rows": [],
                "validation": {"rows": 0}, "unknown_physical_column": [None, -1]})
        self.manifest = {"format": "repoe-native-facts/v4", "status": "partial-candidate",
                         "production_ready": False, "unknown_manifest": {"original": True},
                         "source_identity": {k: selected[k] for k in ("kind", "game", "realm", "game_patch", "acquisition")},
                         "files": {n: s.pin(raw) for n, raw in payload.items()}}
        self.manifest["source_identity"].update({"source": "selected", "source_lock_sha256": s.client.sha256(lock_raw),
            "selected_source_sha256": s.client.sha256(s.canonical(selected)),
            "raw_resources": {n: {k: r[k] for k in ("bytes", "sha256")} for n, r in self.resources.items()}})
        manifest_raw = s.client.encoded(self.manifest)
        for n, raw in {**payload, "manifest.json": manifest_raw}.items():
            s.client.write_new(self.source / n, raw)
        self.recipe = {
            "schema": s.RECIPE, "original": {"manifest": s.pin(manifest_raw), "lock": s.pin(lock_raw)},
            "sources": {n: {"resource_count": count, "dat_table_count": count - 1,
                             "supplements": [{"kind": "replacement-acceptance-only", "evidence": "acceptance.json"}]}
                        for n, count in (("ancestor", 3), ("selected", 4))},
            "groups": {"evidence": "groups.json", "counts": {g: 1 for g in groups},
                       "receipts": {g: g + ".json" for g in groups}},
            "replacement": {"group": "a1", "online": "a1.json", "offline": "offline.json",
                            "selection": "selection.json", "ranges": "ranges.json", "original_bc": "bc.json", "range_count": 1},
            "retired": [{"original": old_a1[0], "replacement": "a1.json", "kind": "replacement-online"},
                        {"original": old_a1[1], "replacement": "offline.json", "kind": "replacement-offline"}],
            "excluded_domains": ["Compatibility, supplemental sources and runtime outputs are separate."],
        }
        self.recipe_path = root / "recipe.json"
        self.refresh()

    def refresh(self):
        descriptors = {}
        for n, doc in self.evidence.items():
            path = self.root / "evidence" / n
            path.parent.mkdir(exist_ok=True)
            path.write_bytes(s.client.encoded(doc))
            descriptors[n] = {"path": str(path), **s.pin(path.read_bytes())}
        self.recipe["evidence"] = descriptors
        self.recipe_path.write_bytes(s.client.encoded(self.recipe))
        self.recipe_sha = s.client.sha256(self.recipe_path.read_bytes())

    def create(self, name="out"):
        return s.create(self.source, self.recipe_path, self.recipe_sha, self.root / name)


class RebindingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.f = Fixture(Path(self.temp.name).resolve())

    def test_replay_preserves_payload_unknowns_and_historical_identity(self):
        before = s.inventory(self.f.source)
        with patch("urllib.request.build_opener", side_effect=AssertionError("network forbidden")):
            a, b = self.f.create("a"), self.f.create("b")
        self.assertEqual(a, b)
        self.assertEqual((self.f.root / "a/ordinary-input.zip").read_bytes(),
                         (self.f.root / "b/ordinary-input.zip").read_bytes())
        self.assertEqual(before, s.inventory(self.f.source))
        root = self.f.root / "a/input/facts"
        m = s.client.decode((root / "manifest.json").read_bytes())
        lock = s.client.decode((root / "source-lock.json").read_bytes())
        self.assertEqual(m["unknown_manifest"], self.f.manifest["unknown_manifest"])
        self.assertEqual(lock["unknown_root"], self.f.lock["unknown_root"])
        for n, old in self.f.lock["sources"].items():
            new = lock["sources"][n]
            self.assertEqual(new["unknown_source_field"], old["unknown_source_field"])
            self.assertEqual(new["rebinding"]["original_acquisition"], old["acquisition"])
            self.assertEqual(new["rebinding"]["original_scope"], old["scope"])
            self.assertEqual(new["rebinding"]["resource_scope"]["resource_count"], len(old["resources"]))
        self.assertIn("bundle_url", lock["sources"]["selected"]["resources"]["Data/Balance/C.datc64"])
        self.assertNotEqual(m["source_identity"]["source_lock_sha256"], self.f.recipe["original"]["lock"]["sha256"])
        for n in self.f.manifest["files"]:
            if n != "source-lock.json":
                self.assertEqual((root / n).read_bytes(), (self.f.source / n).read_bytes())
        shutil.rmtree(self.f.source)
        shutil.rmtree(self.f.root / "evidence")
        s.verify(self.f.root / "a/ordinary-input.zip", a, self.f.root / "extracted")
        self.assertEqual(s.inventory(self.f.root / "extracted"), s.inventory(self.f.root / "a/input"))

    def test_wrong_counts_and_missing_receipt_are_rejected(self):
        for field in ("resource_count", "dat_table_count"):
            with self.subTest(field=field):
                self.f.recipe["sources"]["selected"][field] += 1
                self.f.refresh()
                with self.assertRaisesRegex(ValueError, "scope/count"):
                    self.f.create()
                self.f.recipe["sources"]["selected"][field] -= 1
        self.f.recipe["retired"].pop()
        self.f.refresh()
        with self.assertRaisesRegex(ValueError, "Missing receipt pin"):
            self.f.create()

    def test_changed_or_missing_replacement_receipt_pin_is_rejected(self):
        p = Path(self.f.recipe["evidence"]["a1.json"]["path"])
        p.write_bytes(p.read_bytes() + b" ")
        with self.assertRaisesRegex(ValueError, "Byte count"):
            self.f.create()
        p.unlink()
        with self.assertRaises(FileNotFoundError):
            self.f.create()

    def test_retired_pins_and_mixed_original_source_are_rejected(self):
        self.f.recipe["retired"][0]["original"]["sha256"] = "8" * 64
        self.f.refresh()
        with self.assertRaisesRegex(ValueError, "not an original"):
            self.f.create()
        self.f.recipe["retired"][0]["original"]["sha256"] = "1" * 64
        self.f.manifest["source_identity"]["realm"] = "another-realm"
        raw = s.client.encoded(self.f.manifest)
        (self.f.source / "manifest.json").write_bytes(raw)
        self.f.recipe["original"]["manifest"] = s.pin(raw)
        self.f.refresh()
        with self.assertRaisesRegex(ValueError, "Mixed source identity"):
            self.f.create()

    def test_bad_acquisition_pin_location_or_identity_is_rejected(self):
        for mutation in ("realm", "raw", "location", "offline", "range", "group"):
            with self.subTest(mutation=mutation):
                saved = copy.deepcopy(self.f.evidence)
                if mutation == "realm":
                    self.f.evidence["a1.json"]["realm"] = "another-realm"
                elif mutation == "raw":
                    self.f.evidence["a1.json"]["resources"]["Data/Balance/B.datc64"]["sha256"] = "9" * 64
                elif mutation == "location":
                    self.f.evidence["selection.json"]["resources"]["Data/Balance/B.datc64"]["uncompressed_offset"] += 1
                elif mutation == "offline":
                    self.f.evidence["offline.json"]["network"]["requests"] = 1
                elif mutation == "range":
                    self.f.evidence["ranges.json"]["records"][0]["replacement_record"]["etag"] = '"different"'
                else:
                    self.f.evidence["groups.json"]["a1"].append("Data/Balance/C.datc64")
                self.f.refresh()
                with self.assertRaises(ValueError):
                    self.f.create()
                self.f.evidence = saved

    def test_altered_payload_and_symlink_inputs_are_rejected(self):
        table = self.f.source / "tables/A.json"
        original = table.read_bytes()
        table.write_bytes(original + b" ")
        with self.assertRaisesRegex(ValueError, "Inventory mismatch"):
            self.f.create()
        table.unlink()
        other = self.f.root / "other.json"
        other.write_bytes(original)
        table.symlink_to(other)
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            self.f.create()

    def test_archive_paths_duplicates_symlinks_and_payload_corruption_are_rejected(self):
        h = self.f.create()
        original = self.f.root / "out/ordinary-input.zip"
        for name in ("../outside", "/absolute", "facts\\escape", "Facts/manifest.json", "rebinding.json",
                     "facts", "facts/link", "facts/tables/A.json"):
            with self.subTest(name=name):
                buffer = io.BytesIO()
                with warnings.catch_warnings(), zipfile.ZipFile(original) as src, zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as z:
                    warnings.filterwarnings("ignore", message="Duplicate name:", category=UserWarning)
                    for member in src.infolist():
                        raw = src.read(member)
                        if member.filename == name:
                            raw += b"corruption"
                        z.writestr(member, raw)
                    if name != "facts/tables/A.json":
                        info = zipfile.ZipInfo(name)
                        info.create_system = 3
                        info.compress_type = zipfile.ZIP_DEFLATED
                        info.external_attr = (stat.S_IFLNK if name.endswith("link") else stat.S_IFREG) << 16
                        z.writestr(info, b"x")
                archive = self.f.root / "bad.zip"
                archive.write_bytes(buffer.getvalue())
                changed = {**h, "archive": {"path": archive.name, **s.pin(buffer.getvalue())}}
                with self.assertRaises(ValueError):
                    s.verify(archive, changed, self.f.root / "must-not-exist")
                self.assertFalse((self.f.root / "must-not-exist").exists())

    def test_wrong_archive_pin_and_expansion_counts_are_rejected(self):
        h = self.f.create()
        archive = self.f.root / "out/ordinary-input.zip"
        for changed in ({**h, "expanded_bytes": h["expanded_bytes"] + 1},
                        {**h, "status": "reviewed-source-input"},
                        {**h, "archive": {**h["archive"], "sha256": "0" * 64}}):
            with self.assertRaises(ValueError):
                s.verify(archive, changed)

    def test_no_arbitrary_json_size_cap_and_existing_output_is_preserved(self):
        self.f.evidence["acceptance.json"]["retained_context"] = "x" * (3 * 1024 * 1024)
        self.f.refresh()
        self.f.create()
        before = s.inventory(self.f.root / "out")
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.f.create()
        self.assertEqual(before, s.inventory(self.f.root / "out"))


if __name__ == "__main__":
    unittest.main()
