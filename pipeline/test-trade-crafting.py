import runpy
import unittest
from pathlib import Path

extract = runpy.run_path(str(Path(__file__).with_name("extract-trade-crafting.py")))["extract"]


class TradeCraftingTest(unittest.TestCase):
    def test_categories_random_outcomes_and_source_boundaries(self):
        tables = {
            "BaseItemTypes": [{"Id": f"Metadata/{name}"} for name in (
                "CurrencyVerisiumAlloy11", "CurrencyPerfectEssenceAttribute", "CurrencyNormalEssence")],
            "ItemClasses": [{"Id": name} for name in ("Wand", "Focus", "Amulet")],
            "Mods": [{"Id": name} for name in ("AlloyHybrid", "DisplayOnly", "Strength", "Dexterity")],
            "Essences": [{"BaseItemType": index} for index in range(3)],
            "EssenceTargetItemCategories": [{"ItemClasses": indices} for indices in ([0], [0, 1], [2])],
            "EssenceMods": [
                {"Essence": 0, "TargetItemCategory": 0, "Mod": 0, "OutcomeMods": []},
                {"Essence": 0, "TargetItemCategory": 1, "Mod": 0, "OutcomeMods": []},
                {"Essence": 1, "TargetItemCategory": 2, "Mod": 1, "OutcomeMods": [2, 3]},
                {"Essence": 2, "TargetItemCategory": 2, "Mod": 1, "OutcomeMods": []},
            ],
        }
        self.assertEqual(extract(tables), [
            {"source": "alloy", "id": "AlloyHybrid", "item_types": ["Focus", "Wand"]},
            {"source": "essence", "id": "Dexterity", "item_types": ["Amulet"]},
            {"source": "essence", "id": "Strength", "item_types": ["Amulet"]},
        ])


if __name__ == "__main__":
    unittest.main()
