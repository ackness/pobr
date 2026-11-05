"""The translation coverage report counts against the English data and never fails on gaps."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("translation_coverage", HERE / "translation-coverage.py")
coverage = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(coverage)


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def snapshot(root):
    """A two-language synthetic snapshot: xx is partial, yy keys affixes by mod id and has little else."""
    version = root / "9.9"
    (root / "CURRENT").write_text("9.9\n")
    write(version / "base/base_items.json", [{"id": "Base/A", "name": "Alpha"}, {"id": "Base/B", "name": "Beta"}])
    write(version / "base/skill_gems.json", [
        {"id": "Gem/Fire", "is_support": False, "granted_effect_id": "Fire"},
        {"id": "Gem/Boost", "is_support": True, "granted_effect_id": "Boost"},
    ])
    write(version / "base/granted_effects.json", [
        {"id": "Fire", "active_skill": "fireball"}, {"id": "Boost", "active_skill": "boost"},
    ])
    write(version / "base/mods.json", [{"id": "ModLife", "name": "Hale"}, {"id": "ModMana", "name": "Wise"},
                                       {"id": "ModHidden"}])
    write(version / "base/passive_tree.json", [{"skill": 1, "name": "Might"}, {"skill": 2, "name": "Grace"}])
    write(version / "base/passive_tree_meta.json", {"classes": [{"name": "Witch", "ascendancies": [{"name": "Lich"}]}]})
    write(version / "overlay/uniques.json", {"uniques": [{"name": "Doomgrasp"}]})
    write(version / "overlay/stat_descriptions.json", {"scopes": {"stat_descriptions": {
        "single": {}, "descriptors": {"life": {"branches": [{"template": "+{0} to maximum Life"}]},
                                      "mana": {"branches": [{"template": "+{0} to maximum Mana"}, {"template": ""}]}}}}})
    xx = version / "i18n/xx"
    write(xx / "_meta.json", {"fallback": ["zz"]})
    write(xx / "base_items.json", {"Base/A": "Alfa", "Base/B": "", "Base/Old": "Viejo"})
    write(xx / "skills.json", {"fireball": "Bola"})
    write(xx / "mods.json", {"Hale": "Sano"})
    write(xx / "passive_names.json", {"Might": "Poder", "Grace": "Gracia"})
    write(xx / "words.json", {"Doomgrasp": "Garra", "Other": "Otro"})
    write(xx / "rare_words.json", {"Blood": "Sangre"})
    write(xx / "classes.json", {"classes": {"Witch": "Bruja"}, "ascendancies": {}})
    write(xx / "stat_lines.json", [{"src": "+{0} a Vida", "en": "+{0} to maximum Life"},
                                   {"src": "extra", "en": "Some other template"}])
    write(xx / "display_texts.json", {"entries": {"Alpha": "Alfa", "Beta": None, "[DNT-UNUSED] Old": None},
                                      "scopes": {"Characters.Name": {"Witch": ["Witch", "Bruja"], "Ghost": ["[DNT-UNUSED] Ghost", None],
                                                                     "Monk": ["Monk", None]}}})
    write(version / "i18n/yy/mods.json", {"ModLife": "Sano"})
    return root


class CoverageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = snapshot(Path(self.temp.name))

    def tearDown(self):
        self.temp.cleanup()

    def rows(self, lang):
        result = coverage.report(self.root, examples=5)
        self.assertEqual(result["version"], "9.9")
        language = next(item for item in result["languages"] if item["lang"] == lang)
        return language, {item["table"]: item for item in language["rows"]}

    def test_names_templates_and_display_texts_count_against_english(self):
        language, rows = self.rows("xx")
        self.assertEqual(language["fallback"], ["zz", "en"])
        base = rows["base_items.json"]
        # An empty string is not a translation; a key the English data lacks is extra.
        self.assertEqual((base["reference"], base["translated"], base["missing"], base["extra"]), (2, 1, 1, 1))
        self.assertEqual(base["examples"], ["Base/B"])
        # Only active skills of skill gems are compared; supports are not.
        self.assertEqual((rows["skills.json"]["reference"], rows["skills.json"]["translated"]), (1, 1))
        self.assertEqual((rows["mods.json"]["reference"], rows["mods.json"]["missing"]), (2, 1))
        self.assertEqual(rows["passive_names.json"]["missing"], 0)
        self.assertEqual((rows["words.json"]["reference"], rows["words.json"]["extra"]), (1, 1))
        self.assertIsNone(rows["rare_words.json"]["reference"])
        self.assertEqual((rows["classes.json"]["reference"], rows["classes.json"]["missing"]), (2, 1))
        lines = rows["stat_lines.json"]
        # Empty templates are not counted.
        self.assertEqual((lines["reference"], lines["translated"], lines["extra"]), (2, 1, 1))
        self.assertEqual(lines["examples"], ["+{0} to maximum Mana"])
        # Do-not-translate rows are left out of both the entries and the scopes.
        entries = rows["display_texts.json entries"]
        self.assertEqual((entries["reference"], entries["translated"], entries["examples"]), (2, 1, ["Beta"]))
        scope = rows["display_texts.json Characters.Name"]
        self.assertEqual((scope["reference"], scope["translated"], scope["examples"]), (2, 1, ["Monk"]))

    def test_a_table_keyed_by_mod_id_and_absent_tables_are_reported_not_failed(self):
        language, rows = self.rows("yy")
        self.assertEqual(language["fallback"], ["en"])
        mods = rows["mods.json"]
        self.assertIn("mod ids", mods["keys"])
        # Mods without an affix name are not expected to have one.
        self.assertEqual((mods["reference"], mods["translated"]), (2, 1))
        for table in ("base_items.json", "skills.json", "stat_lines.json", "display_texts.json"):
            self.assertTrue(rows[table].get("absent"), table)
        text = coverage.markdown(coverage.report(self.root))
        self.assertIn("#### yy (falls back to en)", text)
        self.assertIn("| stat_lines.json | stat description templates (exact text) | 2 | absent | 0.0% | 2 | — |", text)

    def test_cli_succeeds_on_gaps_and_fails_only_on_unreadable_data(self):
        script = HERE / "translation-coverage.py"
        out = Path(self.temp.name) / "report.json"
        result = subprocess.run([sys.executable, str(script), "--data", str(self.root), "--json", str(out)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Game-text translation coverage (9.9)", result.stdout)
        self.assertEqual([item["lang"] for item in json.loads(out.read_text())["languages"]], ["xx", "yy"])
        (self.root / "9.9/base/mods.json").write_text("{not json")
        broken = subprocess.run([sys.executable, str(script), "--data", str(self.root)], capture_output=True, text=True)
        self.assertNotEqual(broken.returncode, 0)
        missing = subprocess.run([sys.executable, str(script), "--data", str(self.root), "--version", "0.0"],
                                 capture_output=True, text=True)
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("data:prepare", missing.stderr)


if __name__ == "__main__":
    unittest.main()
