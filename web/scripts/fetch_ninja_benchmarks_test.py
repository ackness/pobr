"""Offline regression coverage; run with python3 -m unittest discover -s web/scripts -p '*_test.py'."""
import base64
import importlib.util
import tempfile
import unittest
import zlib
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("fetch_ninja_benchmarks", Path(__file__).with_name("fetch-ninja-benchmarks.py"))
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)

XML = b'''<PathOfBuilding2>
<Build className="Warrior" ascendClassName="Titan" level="100" mainSocketGroup="1" targetVersion="0_1"/>
<Tree><Spec treeVersion="0_5"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
<Gem nameSpec="Resonating Shield" skillId="ResonatingShield" level="20" enabled="true"/>
</Skill></SkillSet></Skills></PathOfBuilding2>'''
CODE = base64.urlsafe_b64encode(zlib.compress(XML)).decode().rstrip("=")
CANDIDATE = {"account": "public-1234", "character": "Example", "base_class": "Warrior",
             "index_ascendancy": "Titan", "index_url": "https://poe.ninja/index"}
MANIFEST = {"league": {"name": "Forbidden Rites", "url": "forbiddenrites"},
            "snapshot": {"version": "pinned", "snapshotName": "forbidden-rites"}}
DATA = {"account": "public-1234", "name": "Example", "league": "Forbidden Rites",
        "passiveTreeName": "PassiveTree-0.5", "pathOfBuildingExport": CODE}


def export(data=None):
    return collector.export_fixture(data or DATA, CANDIDATE, MANIFEST,
                                    "2026-10-11T12:00:00+00:00", "https://poe.ninja/api")


def number(value):
    out = bytearray()
    while value > 127:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return bytes(out)


def field(tag, data):
    return number((tag << 3) | 2) + number(len(data)) + data


class ExportTests(unittest.TestCase):
    def test_truthful_xml_metadata_and_hashes(self):
        code, meta = export()
        self.assertEqual(code, CODE)
        self.assertEqual(meta["character"]["main_skill"], "Resonating Shield")
        self.assertEqual(meta["source"]["game_version"], "0.5")
        self.assertEqual(meta["pob"]["target_version"], "0_1")
        self.assertEqual(meta["pob"]["code_sha256"], collector.sha256(CODE.encode()))
        self.assertEqual(meta["pob"]["code_chars"], len(CODE))
        self.assertEqual(meta["pob"]["decoded_bytes"], len(XML))
        self.assertTrue(meta["name"].startswith("ninja-2026-10-11-warrior-titan-resonating-shield-"))

    def test_missing_or_invalid_export_is_error(self):
        for bad in (None, "", "%%%", "not-a-zlib-pob-code"):
            with self.subTest(export=bad), self.assertRaises(ValueError):
                export({**DATA, "pathOfBuildingExport": bad})

    def test_wrong_xml_root_rejected(self):
        code = base64.urlsafe_b64encode(zlib.compress(b"<PathOfBuilding/>" )).decode()
        with self.assertRaisesRegex(ValueError, "PathOfBuilding2"):
            export({**DATA, "pathOfBuildingExport": code})

    def test_identity_season_and_game_version_must_match(self):
        for change in ({"account": "other"}, {"name": "other"},
                       {"league": "Runes of Aldur"}, {"passiveTreeName": "PassiveTree-0.4"}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                export({**DATA, **change})

    def test_fixture_collision_and_hash_corruption_are_errors(self):
        code, meta = export()
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            collector.save_fixture(output, code, meta)
            collector.save_fixture(output, code, meta)
            with self.assertRaisesRegex(ValueError, "overwrite"):
                collector.save_fixture(output, code + "A", meta)
            import json
            path = output / meta["name"] / "meta.json"
            damaged = json.loads(path.read_text())
            damaged["pob"]["code_sha256"] = "0" * 64
            path.write_text(json.dumps(damaged))
            with self.assertRaisesRegex(ValueError, "corrupt"):
                collector.save_fixture(output, code, meta)


class IndexTests(unittest.TestCase):
    def test_candidates_have_deterministic_nonperformance_order(self):
        names = field(1, b"name") + field(7, b"Z") + field(7, b"A")
        accounts = field(1, b"account") + field(7, b"b") + field(7, b"a")
        binary = field(1, field(12, names) + field(12, accounts))
        self.assertEqual(collector.index_candidates(binary), [
            {"account": "a", "character": "A"}, {"account": "b", "character": "Z"}])

    def test_missing_mismatched_or_truncated_schema_is_error(self):
        missing = field(1, field(12, field(1, b"name") + field(7, b"A")))
        mismatched = field(1, field(12, field(1, b"name") + field(7, b"A") + field(7, b"B")) +
                           field(12, field(1, b"account") + field(7, b"one")))
        for bad in (b"", b"\x0a\x7fX", missing, mismatched):
            with self.subTest(binary=bad), self.assertRaises(ValueError):
                collector.index_candidates(bad)


if __name__ == "__main__":
    unittest.main()
