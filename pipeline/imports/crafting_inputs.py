"""Project PoBR's special-currency catalog from pinned neutral RePoE facts.

This is a category/identified-regular-stat catalog, not a complete effect packet,
crafting simulator or source admission. Raw values stay in the external package;
source and projection diagnostics stay in an authenticated build-input receipt.
"""

import argparse
from pathlib import Path
import re
import tempfile

from pipeline.runtime.data_bundle import decode
from pipeline.imports.data_imports import GitInputs, acquire, encoded, git_address, pinned, regular, require, sha, unpack, validate_pin, write

ROOT = Path(__file__).resolve().parents[2]
FORMAT = "repoe-native-crafting-facts/v1"
POLICY = "pobr-trade-crafting-catalog/v1"
TABLES = ("BaseItemTypes", "ItemClasses", "Mods", "Stats", "Essences", "EssenceTargetItemCategories", "EssenceMods")
PRODUCERS = {"RePoE/native_crafting_facts.py", "RePoE/native_descriptions.py"}


def validate(spec):
    require(spec.get("publication") in ("local-only", "published")
            and spec.get("repository") == "ackness/repoe", "Crafting facts must belong to RePoE")
    git_address(spec, spec["path"])
    for pin in (spec, spec["manifest"], spec["input_manifest"]):
        validate_pin(pin)
    require(type(spec["expanded_bytes"]) is int and spec["expanded_bytes"] > 0,
            "Invalid crafting expanded size")
    require(set(spec["producer"]) == PRODUCERS and all(
        isinstance(value, str) and re.fullmatch(r"[a-f0-9]{64}", value)
        for value in spec["producer"].values()), "Invalid crafting producer closure")


def project(facts, *, source_manifest_sha256, facts_manifest_sha256, target):
    """Apply the existing PoBR UI selection policy without inferring probabilities."""
    require(facts.get("format") == FORMAT, "Unknown crafting facts format")
    counts = facts["summary"]["tables"]
    require(set(counts) == set(TABLES) and all(type(n) is int and n >= 0 for n in counts.values()),
            "Invalid crafting table inventory")

    def ref(value, table, *, nullable=True):
        if value is None:
            require(nullable, f"Selected {table} reference is null")
            return None
        require(isinstance(value, dict) and value.get("table") == table
                and type(value.get("row")) is int and 0 <= value["row"] < counts[table],
                f"Invalid {table} reference")
        if table not in ("Essences", "EssenceMods"):
            require(isinstance(value.get("id"), str) and bool(value["id"]), f"Missing {table} string ID")
        return value

    def ordered(values, table):
        require(isinstance(values, list), f"Expected ordered {table} references")
        for index, value in enumerate(values):
            require(type(value.get("source_slot")) is int and value["source_slot"] == index,
                    "Changed reference array order")
            ref(value["target"], table)
        return [value["target"] for value in values]

    mods, ids = facts["mods"], set()
    require(len(mods) == counts["Mods"], "Incomplete Mods collection")
    for index, mod in enumerate(mods):
        identity = ref(mod["mod"], "Mods", nullable=False)
        require(identity["row"] == index and identity["id"] not in ids, "Duplicate/changed Mods identity")
        ids.add(identity["id"])
        require(type(mod["level_raw"]) is int and type(mod["has_hardmode_values_raw"]) is bool,
                "Invalid raw mod level/hardmode flag")
        slots = mod["slots"]
        require(len(slots) == 8 and all(type(s["slot"]) is int and s["slot"] == i
                                      for i, s in enumerate(slots, 1)), "Eight ordered stat slots required")
        for slot in slots:
            ref(slot["stat"], "Stats")
            require(all(slot[key] is None or type(slot[key]) is int for key in
                        ("min_raw", "max_raw", "min_hardmode_raw", "max_hardmode_raw")),
                    "Invalid raw stat endpoints")
    essences, categories = facts["essences"], facts["categories"]
    require(len(essences) == counts["Essences"] and len(categories) == counts["EssenceTargetItemCategories"],
            "Incomplete crafting join owners")
    for index, row in enumerate(essences):
        require(ref(row["essence"], "Essences", nullable=False)["row"] == index, "Changed essence identity")
        ref(row["currency"], "BaseItemTypes")
    for index, row in enumerate(categories):
        require(ref(row["category"], "EssenceTargetItemCategories", nullable=False)["row"] == index,
                "Changed category identity")
        ordered(row["item_classes"], "ItemClasses")

    selected, diagnostics, result, provenance = [], [], {}, {}
    rows = facts["essence_mods"]
    require(len(rows) == counts["EssenceMods"], "Incomplete EssenceMods collection")
    for index, row in enumerate(rows):
        require(ref(row["source"], "EssenceMods", nullable=False)["row"] == index, "Changed recipe row identity")
        essence, category = ref(row["essence"], "Essences"), ref(row["target_category"], "EssenceTargetItemCategories")
        currency = ref(row["currency"], "BaseItemTypes")
        require(currency == (None if essence is None else essences[essence["row"]]["currency"]),
                "Recipe currency differs from essence owner")
        require(row["item_classes"] == (None if category is None else categories[category["row"]]["item_classes"]),
                "Recipe classes differ from category owner")
        direct, display = ref(row["mod"], "Mods"), ref(row["display_mod"], "Mods")
        outcomes = ordered(row["outcome_mods"], "Mods")
        for value in (direct, display, *outcomes):
            require(value is None or value == mods[value["row"]]["mod"], "Changed joined Mods identity")
        weights = row["outcome_mod_weights_raw"]
        require(isinstance(weights, list) and all(w is None or type(w) is int for w in weights),
                "Invalid raw outcome weights")
        currency_name = "" if currency is None else currency["id"].rsplit("/", 1)[-1]
        source = "alloy" if currency_name.startswith("CurrencyVerisiumAlloy") else "essence"
        if source != "alloy" and not currency_name.startswith(("CurrencyPerfectEssence", "CurrencyCorruptedEssence")):
            if currency is None:
                diagnostics.append({"code": "unidentified_currency_not_selected", "source": row["source"]})
            continue
        require(category is not None, "Selected category owner is null")
        classes = ordered(row["item_classes"], "ItemClasses")
        require(all(value is not None for value in classes), "Selected item class is null")
        # This is PoBR's existing catalog policy, not a RePoE fallback or proof
        # of crafting eligibility. DisplayMod is never substituted for a Mod.
        choices = outcomes if outcomes else ([direct] if direct is not None else [])
        require(choices and all(value is not None for value in choices), "Selected recipe has no identified outcome")
        selected.append({"source": row["source"], "currency": currency, "category": category,
                         "relationship": "OutcomeMods" if outcomes else "Mod",
                         "mods": choices, "weights_raw": weights})
        if len(outcomes) != len(weights):
            diagnostics.append({"code": "selected_outcome_weights_unresolved", "source": row["source"],
                                "outcomes": row["outcome_mods"], "weights_raw": weights})
        for choice in choices:
            key = (source, choice["id"])
            result.setdefault(key, {"row": mods[choice["row"]], "types": set()})["types"].update(
                value["id"] for value in classes)
            provenance.setdefault(key, []).append(row["source"])

    recipes = []
    for (source, identity), entry in sorted(result.items()):
        mod, stats = entry["row"], []
        for slot in mod["slots"]:
            if slot["stat"] is None:
                if any(slot[key] not in (None, 0) for key in
                       ("min_raw", "max_raw", "min_hardmode_raw", "max_hardmode_raw")):
                    diagnostics.append({"code": "unidentified_stat_interval", "mod": mod["mod"],
                                        "sources": provenance[(source, identity)], **slot})
                continue
            low, high = slot["min_raw"], slot["max_raw"]
            require(type(low) is int and type(high) is int and low <= high,
                    "Selected identified stat lacks a regular interval")
            stats.append({"id": slot["stat"]["id"], "min": low, "max": high})
        if mod["has_hardmode_values_raw"]:
            diagnostics.append({"code": "hardmode_channel_not_projected", "mod": mod["mod"]})
        recipes.append({"source": source, "id": identity, "item_types": sorted(entry["types"]),
                        "level": mod["level_raw"], "stats": stats})
    document = {"_meta": {"schema": POLICY, "source": FORMAT,
                           "source_manifest_sha256": source_manifest_sha256,
                           "input_manifest_sha256": facts_manifest_sha256,
                           "declared_target": target,
                           "scope": "Selected special-currency categories and identified regular-stat ranges; not complete effects, eligibility or probabilities.",
                           "client_identity_independently_observed": False}, "mods": recipes}
    return document, {"selected": selected, "diagnostics": diagnostics,
                      "source_diagnostics": facts["diagnostics"],
                      "summary": {"selected_rows": len(selected), "catalog_mods": len(recipes)}}


def materialize(spec, target, cache, workspace, offline=False, fetcher=None, local_archive=None):
    """Fetch/authenticate the whole bounded package, project only its neutral DTO."""
    validate(spec)
    workspace = Path(workspace)
    fetcher = fetcher or GitInputs(workspace / "git")
    if local_archive is not None:
        archive = pinned(regular(local_archive), spec)
    else:
        require(spec["publication"] == "published", "Unpublished crafting facts: explicit local archive required")
        archive = acquire(spec, spec["path"], spec, cache, offline, fetcher)
    part = workspace / "facts"
    manifest = unpack(archive, spec, part)
    source = manifest["source_identity"]
    require(manifest.get("format") == FORMAT and manifest.get("status") == "partial-candidate"
            and manifest.get("production_ready") is False
            and all(source.get(key) == value for key, value in target.items()), "Crafting target/format mismatch")
    require(manifest["input_package"]["manifest"] == spec["input_manifest"]
            and manifest["producer"] == spec["producer"], "Crafting producer/input identity mismatch")
    expected_files = {"crafting.json", "input-manifest.json", *(f"{name}.json" for name in TABLES)}
    require(set(manifest["files"]) == expected_files, "Unexpected crafting package inventory")
    require(manifest["files"]["input-manifest.json"] == spec["input_manifest"], "Crafting input manifest pin differs")
    pinned(regular(part / "input-manifest.json"), spec["input_manifest"])
    policy_raw = regular(__file__)
    document, decisions = project(decode(pinned(regular(part / "crafting.json"), manifest["files"]["crafting.json"])),
        source_manifest_sha256=spec["manifest"]["sha256"], facts_manifest_sha256=spec["input_manifest"]["sha256"], target=target)
    raw = encoded(document)
    require(regular(__file__) == policy_raw, "Crafting projection changed during prepare")
    receipt = {"schema": "pobr-crafting-input-receipt/v1", "target": target, "package": spec,
               "policy": {"id": POLICY, "path": "pipeline/imports/crafting_inputs.py", "sha256": sha(policy_raw)},
               "source_identity": source, "observation": manifest["observation"],
               "source_scope": manifest["scope"], "source_summary": manifest["summary"],
               "output": {"bytes": len(raw), "sha256": sha(raw)}, **decisions,
               "scope": "Catalog projection only; not source admission, complete effects or crafting simulation."}
    return raw, receipt


def record(cache, lock_raw, receipt):
    """Keep build diagnostics outside the strict runtime bundle/player inventory."""
    name = Path(cache) / "build-input-receipts" / (sha(lock_raw) + "-" + receipt["policy"]["sha256"] + ".json")
    raw = encoded({"build_lock_sha256": sha(lock_raw), **receipt})
    if name.exists() or name.is_symlink():
        require(regular(name) == raw, "Existing crafting receipt differs")
    else:
        write(name, raw)
    return name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-lock", type=Path, default=ROOT / "pipeline/data.lock.json")
    parser.add_argument("--patch", required=True)
    parser.add_argument("--check-lock", action="store_true")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/data-import-downloads")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--crafting", type=Path, help="Explicit pinned ZIP for a local-only crafting handoff")
    args = parser.parse_args()
    lock_raw = regular(args.build_lock)
    lock = decode(lock_raw)
    from pipeline.runtime.build_data import CRAFTING_SCHEMAS
    require(lock.get("schema") in CRAFTING_SCHEMAS and lock["target"]["game_patch"] == args.patch,
            "Crafting build lock does not match requested patch")
    validate(lock["crafting"])
    if args.check_lock:
        return
    require(args.out is not None, "--out is required")
    from pipeline.runtime.build_data import safe_directory
    output, cache = safe_directory(args.out), safe_directory(args.cache)
    require(not output.is_relative_to(ROOT / "data"), "Crafting output must be a separate candidate")
    output.parent.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".crafting-", dir=output.parent) as temporary:
        stage = Path(temporary)
        raw, receipt = materialize(lock["crafting"], lock["target"], cache, stage, args.offline, local_archive=args.crafting)
        require(regular(args.build_lock) == lock_raw, "Build data lock changed during projection")
        record(cache, lock_raw, receipt)
        write(stage / "output.json", raw)
        (stage / "output.json").replace(output)


if __name__ == "__main__":
    main()
