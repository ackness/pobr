#!/usr/bin/env python3
"""Advance quality receipts for compatible official data updates, without code edits."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


TABLES = ("GrantedEffectQualityStats.json", "GrantedEffects.json", "Stats.json")


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fingerprints(raw):
    return {name: digest(raw / name) for name in TABLES}


def write_new(path, document):
    # Existing reviewed decisions must never be overwritten by automation.
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(document, ensure_ascii=False, indent=2) + "\n")


def indexed_ids(path):
    result = {}
    ids = set()
    for row in read(path):
        key, value = row["_index"], row["Id"]
        if type(key) is not int or key < 0 or key in result or not value or value in ids:
            raise ValueError(f"invalid or duplicate index/ID in {path.name}")
        result[key] = value
        ids.add(value)
    return result


def advance(raw, export, previous_receipt, previous_quality, patch):
    source = read(export)
    inputs = fingerprints(raw)
    if (source.get("schema") != "official-table-export/v1"
            or source.get("patch") != patch or source.get("inputs") != inputs):
        raise ValueError("export receipt does not match target patch and table bytes; export again")
    previous = read(previous_receipt)
    quality = read(previous_quality)
    meta = quality["_meta"]
    if (previous["schema"] != "gem-quality-source/v1"
            or meta["source_receipt_sha256"] != digest(previous_receipt)
            or meta["poe_version"] != previous["poe_version"]
            or meta["compatibility"] != previous["compatibility"]):
        raise ValueError("previous quality snapshot and receipt do not match")
    compatibility = previous["compatibility"]
    if compatibility["scope_policy"] != "preserve_effect_wide_quality":
        raise ValueError("unsupported previous scope policy")
    enabled = set(compatibility["effects"])
    excluded = set(compatibility["excluded_effects"])
    if not enabled or enabled & excluded or len(enabled) != len(compatibility["effects"]):
        raise ValueError("invalid previous compatibility decisions")
    old_effects = {row["effect_id"]: row["stats"] for row in quality["effects"]}
    if set(old_effects) != enabled or len(old_effects) != len(quality["effects"]):
        raise ValueError("previous quality payload does not match its enabled effects")
    effects = indexed_ids(raw / "GrantedEffects.json")
    stats = indexed_ids(raw / "Stats.json")
    seen = set()
    changed = []
    for row in read(raw / "GrantedEffectQualityStats.json"):
        effect = effects[row["GrantedEffect"]]
        if effect in seen or effect not in enabled | excluded:
            raise ValueError(f"quality effect needs a compatibility decision: {effect}")
        seen.add(effect)
        values = []
        for keys, numbers, alt in [("Stats", "StatsValuesPermille", False),
                                   ("AltStats", "AltStatValuesPermille", True)]:
            if len(row[keys]) != len(row[numbers]):
                raise ValueError(f"stat/value length mismatch: {effect}")
            for key, number in zip(row[keys], row[numbers]):
                if type(number) is not int or not -(2**31) <= number < 2**31:
                    raise ValueError(f"invalid quality value: {effect}")
                values.append({"stat": stats[key], "per_quality_rate": number / 1000, "alt": alt})
        scopes = {"main": row["ApplyToStatSets"], "alt": row["AltApplyToStatSets"]}
        for indexes in scopes.values():
            if not isinstance(indexes, list) or any(type(i) is not int or i < 0 for i in indexes):
                raise ValueError(f"invalid stat-set scope: {effect}")
        old_scope = meta["unapplied_stat_set_scopes"].get(effect, {"main": [], "alt": []})
        if scopes != old_scope:
            raise ValueError(f"stat-set scope changed; calculation review required: {effect}")
        if effect in enabled:
            shape = lambda rows: [(stat["stat"], stat.get("alt", False)) for stat in rows]
            if not values or shape(values) != shape(old_effects[effect]):
                raise ValueError(f"quality stat identity/order changed; calculation review required: {effect}")
            if values != [{**stat, "alt": stat.get("alt", False)} for stat in old_effects[effect]]:
                changed.append(effect)
    if seen != enabled | excluded:
        raise ValueError(f"quality effects disappeared: {sorted((enabled | excluded) - seen)}")
    return {
        "schema": "gem-quality-source/v1", "poe_version": patch,
        "provenance": {
            "kind": "compatible-official-table-update",
            "export": source,
            "previous_version": previous["poe_version"],
            "previous_receipt_sha256": digest(previous_receipt),
            "previous_quality_sha256": digest(previous_quality),
            "validation": "Stable effect/stat IDs, order, main/alternate and stat-set scopes unchanged; numerical rates may change. Source attests successful exporter execution, not an independent CDN byte comparison.",
            "changed_effect_values": sorted(changed),
        },
        "inputs": inputs, "compatibility": compatibility,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    record = sub.add_parser("record-export")
    record.add_argument("--config", type=Path, required=True)
    record.add_argument("--raw", type=Path, required=True)
    record.add_argument("--out", type=Path, required=True)
    update = sub.add_parser("advance")
    for name in ["raw", "export", "previous-receipt", "previous-quality", "out"]:
        update.add_argument(f"--{name}", type=Path, required=True)
    update.add_argument("--patch", required=True)
    args = parser.parse_args()
    if args.command == "record-export":
        patch = read(args.config)["patch"]
        if not re.fullmatch(r"\d+(?:\.\d+)+", patch):
            raise ValueError("invalid export patch")
        document = {"schema": "official-table-export/v1", "patch": patch,
                    "exporter": "pathofexile-dat@15", "config_sha256": digest(args.config),
                    "inputs": fingerprints(args.raw)}
        # This record is written only after a successful export. Atomic replacement
        # prevents an interrupted write from masquerading as a valid source receipt.
        temp = args.out.with_suffix(".tmp")
        args.out.parent.mkdir(parents=True, exist_ok=True)
        temp.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
        temp.replace(args.out)
    else:
        write_new(args.out, advance(args.raw, args.export, args.previous_receipt,
                                    args.previous_quality, args.patch))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"quality receipt: {error}", file=sys.stderr)
        sys.exit(1)
