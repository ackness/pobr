#!/usr/bin/env python3
"""Export special currency category overrides from same-version GGG tables."""
import argparse
import json
from pathlib import Path


def extract(tables):
    bases, classes, mods = (tables[name] for name in ("BaseItemTypes", "ItemClasses", "Mods"))
    essences, targets = (tables[name] for name in ("Essences", "EssenceTargetItemCategories"))
    result = {}
    for row in tables["EssenceMods"]:
        currency = bases[essences[row["Essence"]]["BaseItemType"]]["Id"].rsplit("/", 1)[-1]
        source = "alloy" if currency.startswith("CurrencyVerisiumAlloy") else "essence"
        if source != "alloy" and not currency.startswith(("CurrencyPerfectEssence", "CurrencyCorruptedEssence")):
            continue
        item_types = [classes[index]["Id"] for index in targets[row["TargetItemCategory"]]["ItemClasses"]]
        # OutcomeMods names the actual alternatives for randomized crafts; Mod can
        # be a display-only placeholder such as "% Dexterity, Intelligence or Strength".
        outcomes = row["OutcomeMods"] or ([row["Mod"]] if row["Mod"] is not None else [])
        for index in outcomes:
            key = (source, mods[index]["Id"])
            result.setdefault(key, {"row": mods[index], "types": set()})["types"].update(item_types)
    recipes = []
    for (source, mod), entry in sorted(result.items()):
        row = entry["row"]
        stats = [{"id": tables["Stats"][row[f"Stat{i}"]]["Id"],
                  "min": row[f"Stat{i}Value"][0], "max": row[f"Stat{i}Value"][1]}
                 for i in range(1, 5) if row.get(f"Stat{i}") is not None]
        recipes.append({"source": source, "id": mod, "item_types": sorted(entry["types"]),
                        "level": row["Level"], "stats": stats})
    return recipes


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tables", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    names = ("BaseItemTypes", "ItemClasses", "Mods", "Stats", "Essences", "EssenceTargetItemCategories", "EssenceMods")
    tables = {name: json.loads((args.tables / f"{name}.json").read_text(encoding="utf-8")) for name in names}
    recipes = extract(tables)
    result = {"_meta": {"source": "GGG EssenceMods/Essences/EssenceTargetItemCategories x BaseItemTypes/ItemClasses/Mods/Stats",
        "regen_command": "python3 pipeline/extract-trade-crafting.py pipeline/tables/English <out>"}, "mods": recipes}
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"trade crafting: {len(recipes)} special currency modifiers")
