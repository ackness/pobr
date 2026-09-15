import runpy
import unittest
from pathlib import Path

extract = runpy.run_path(str(Path(__file__).with_name("extract-trade-crafting.py")))["extract"]


class TradeCraftingTest(unittest.TestCase):
    def test_stat_ids_rolls_and_dynamic_zero_are_preserved(self):
        tables = {
            "BaseItemTypes": [{"Id": "Metadata/CurrencyCorruptedEssenceDelirium"}],
            "ItemClasses": [{"Id": "Body Armour"}],
            "Mods": [{"Id": "EssenceGrantedPassive", "Level": 1, "Stat1": 0, "Stat1Value": [0, 0],
                      "Stat2": 1, "Stat2Value": [30, 50]}],
            "Stats": [{"Id": "passive_hash"}, {"Id": "chance"}],
            "Essences": [{"BaseItemType": 0}], "EssenceTargetItemCategories": [{"ItemClasses": [0]}],
            "EssenceMods": [{"Essence": 0, "TargetItemCategory": 0, "Mod": 0, "OutcomeMods": []}],
        }
        self.assertEqual(extract(tables)[0]["stats"], [
            {"id": "passive_hash", "min": 0, "max": 0}, {"id": "chance", "min": 30, "max": 50}])

    def test_categories_random_outcomes_and_source_boundaries(self):
        tables = {
            "BaseItemTypes": [{"Id": f"Metadata/{name}"} for name in (
                "CurrencyVerisiumAlloy11", "CurrencyPerfectEssenceAttribute", "CurrencyNormalEssence")],
            "ItemClasses": [{"Id": name} for name in ("Wand", "Focus", "Amulet")],
            "Mods": [{"Id": name, "Level": 25} for name in ("AlloyHybrid", "DisplayOnly", "Strength", "Dexterity")],
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
            {"source": "alloy", "id": "AlloyHybrid", "item_types": ["Focus", "Wand"], "level": 25, "stats": []},
            {"source": "essence", "id": "Dexterity", "item_types": ["Amulet"], "level": 25, "stats": []},
            {"source": "essence", "id": "Strength", "item_types": ["Amulet"], "level": 25, "stats": []},
        ])


if __name__ == "__main__":
    unittest.main()
