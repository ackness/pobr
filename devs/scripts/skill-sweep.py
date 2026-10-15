#!/usr/bin/env python3
"""Measure every primary player active effect, without tuning real builds.

    mise run skills:sweep --jobs 4 [--refresh] [--json FILE] [--text FILE]
    mise run skills:sweep --skill FireballPlayer --skill IceStrikePlayer

Uses the active data version (POBR_DATA_ROOT / POBR_DATA_VERSION), builds the
worktree's CLI once, and compares level-90, level-20/quality-0 single-gem builds.
Non-support gems include gem_type 2 (meta gems); duplicate granted effect IDs
are measured once. Additional effects/stat sets are listed but not separate
cases. Pure movement, command-only and empty placeholder skills are skipped.

XML, build codes and oracle JSON live in .cache/skill-sweep; oracle keys include
XML, vendor commit and wrapper contents. --refresh recalculates the oracle;
PoBR always runs again. --no-build reuses target/debug/pobr. Output defaults to
.cache/skill-sweep/report.json and report.txt. Standard library only.

Classes compare DPS, rate and crit within 5% of the oracle; zero classes
use damage only. Average hit is reported but excluded: PoBR exports it before
enemy mitigation, PoB2 after. As in reference_check.rs, HitSpeed replaces Speed
for rate comparisons when present. Main-minion DPS is also compared. both_zero is
not evidence that buffs, triggers or minions work: this fixture supplies no
trigger payload, corpses, charges, targets, supports or other skill inputs.
The report separates missing vendor effects from engine differences and gives
both all-candidate and vendor-supported main-skill coverage, never silently
excluding unavailable effects from the former denominator.
"""

import argparse
import base64
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
import zlib

REPO = Path(__file__).resolve().parents[2]
VENDOR = REPO / "vendor/PathOfBuilding-PoE2"
CACHE = REPO / ".cache/skill-sweep"
CLASSES = ("agree", "differs", "pobr_zero", "pob2_zero", "both_zero", "error", "pob2_other_skill")
METRICS = ("TotalDPS", "CombinedDPS", "Speed", "AverageHit", "CritChance")
DIFFERENT = {"differs", "pobr_zero", "pob2_zero"}
FAMILY_TYPES = {
    "warcries": {"Warcry"}, "totems": {"SummonsTotem", "SummonsAttackTotem"},
    "traps": {"Trap"}, "mines": {"Mine"}, "channelling": {"Channel"},
    "marks": {"Mark"}, "curses": {"AppliesCurse"}, "minions": {"Minion", "CreatesMinion"},
    "companions": {"Companion", "CreatesCompanion"}, "auras": {"Aura"},
    "buffs": {"Buff"}, "heralds": {"Herald"}, "offerings": {"Offering"},
    "triggers": {"Triggered", "Triggers", "InbuiltTrigger", "GeneratesEnergy"},
    "bow": {"Bow"}, "crossbow": {"CrossbowSkill", "CrossbowAmmoSkill", "Grenade"},
    "spear": {"Spear"}, "shapeshift": {"Shapeshift"}, "slams": {"Slam"},
    "strikes": {"MeleeSingleTarget"}, "projectiles": {"Projectile"},
    "damage_over_time": {"DamageOverTime"}, "attacks": {"Attack"}, "spells": {"Spell"},
}


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def vendor_metadata(vendor):
    """Read identities and weapon-requirement text from the generated gem table.

    The base snapshot does not export ActiveSkills' weapon restrictions. The
    generated table's displayed requirement text fills that gap; no calculation
    formulas are borrowed. Game skill types and base items still drive cases.
    """
    effects, hidden = set(), set()
    for path in sorted((vendor / "src/Data/Skills").glob("*.lua")):
        source = path.read_text(encoding="utf-8")
        for match in re.finditer(r'^skills\["([^"\n]+)"\] = \{\n(.*?)(?=^skills\[|\Z)', source, re.M | re.S):
            effects.add(match[1])
            if re.search(r'^\t(?:support|hideFromSideBar) = true,', match[2], re.M):
                hidden.add(match[1])
    source = (vendor / "src/Data/Gems.lua").read_text(encoding="utf-8")
    gems = defaultdict(list)
    for match in re.finditer(r'^\t\["[^"\n]+"\] = \{\n(.*?)^\t\},', source, re.M | re.S):
        fields = dict(re.findall(r'^\t\t(\w+) = "([^"\n]*)",', match[1], re.M))
        if fields.get("grantedEffectId"):
            effect_list = [fields["grantedEffectId"]]
            effect_list += [fields[key] for key in sorted(fields) if key.startswith("additionalGrantedEffectId")]
            order = re.search(r'^\t\tgrantedEffectDisplayOrder = \{ ([\d, ]+) \}', match[1], re.M)
            indices = [int(i) for i in re.findall(r'\d+', order[1])] if order else []
            indices = list(dict.fromkeys([*indices, *range(len(effect_list))]))
            active = [effect_list[i] for i in indices if i < len(effect_list) and effect_list[i] not in hidden]
            primary = fields["grantedEffectId"]
            fields["primary_active_index"] = active.index(primary) + 1 if primary in active else None
            gems[fields["grantedEffectId"]].append(fields)
    if not gems:
        raise ValueError("cannot read the vendor's generated gem metadata")
    return gems, effects


def skip_reason(effect, gem, stat_sets):
    types = set(effect.get("skill_types", []))
    tags = set(gem.get("gem_tags", []))
    if not types and not stat_sets:
        return "placeholder with no skill types or stat sets (no damage or buff)"
    if "CommandsMinions" in types and not types & {"Attack", "Damage", "Buff", "Minion"}:
        return "command-only skill needs its parent minion; no independent damage or buff"
    implicit = {s for row in stat_sets for s in row.get("implicit_stats", [])}
    if ("Travel" in types and "base_deal_no_damage" in implicit
            and not types & {"Buff", "Minion", "AppliesCurse"} and "buff" not in tags):
        return "movement-only skill with base_deal_no_damage and no buff tag"
    return None


def skill_cases(directory, vendor_gems, vendor_effects):
    base = directory / "base"
    gems = read_json(base / "skill_gems.json")
    effects = {row["id"]: row for row in read_json(base / "granted_effects.json")}
    sets = {row["effect_id"]: row["sets"] for row in read_json(base / "granted_effect_stat_sets.json")}
    grouped = defaultdict(list)
    for gem in gems:
        if not gem["is_support"]:
            grouped[gem["granted_effect_id"]].append(gem)
    cases, skipped = [], []
    for effect_id, variants in sorted(grouped.items()):
        gem = min(variants, key=lambda row: row["id"])
        effect = effects[effect_id]
        oracle_gems = vendor_gems.get(effect_id, [])
        oracle_gem = next((row for row in oracle_gems if row.get("gameId") == gem["id"]),
                          oracle_gems[0] if oracle_gems else {})
        types = effect.get("skill_types", [])
        families = sorted(name for name, flags in FAMILY_TYPES.items() if flags.intersection(types))
        if any(row.get("gem_type") == 2 for row in variants):
            families.append("meta")
        case = {"skill_id": effect_id, "name": effect.get("displayed_name") or effect_id,
                "skill_types": types, "families": families or ["utility"], "gem": gem,
                "gem_ids": sorted(row["id"] for row in variants), "oracle_gem": oracle_gem,
                "vendor_has_effect": effect_id in vendor_effects,
                "additional_effects": sorted({i for row in variants
                    for i in row.get("additional_granted_effect_ids", []) if not effects.get(i, {}).get("is_support", False)}),
                "additional_stat_sets": effect.get("additional_stat_set_ids", [])}
        reason = skip_reason(effect, gem, sets.get(effect_id, []))
        if reason:
            skipped.append({**case, "reason": reason})
        else:
            cases.append(case)
    return cases, skipped, {"gem_records": len(gems), "non_support_gems": sum(not g["is_support"] for g in gems),
                            "primary_active_effects": len(grouped)}


def equipment_classes(case):
    types = set(case["skill_types"])
    requirement = case["oracle_gem"].get("weaponRequirements", "")
    # Explicit equipment requirements also apply to non-attacks, such as
    # Arctic Howl's Talisman. Ordinary spells without a requirement stay unarmed.
    if "RequiresBuckler" in types:
        return ["Buckler"]
    if "RequiresShield" in types or "Shield" in requirement:
        return ["Shield"]
    if ("NonWeaponAttack" in types or requirement == "Unarmed"
            or ("Attack" not in types and not requirement)):
        return []
    if requirement:
        first = requirement.split(", ")[0]
        weapon = {"Quarterstaff": "Warstaff"}.get(first, first)
        if first.startswith("Any "):
            weapon = "One Hand Mace"
    elif types & {"CrossbowSkill", "CrossbowAmmoSkill", "Grenade"}:
        weapon = "Crossbow"
    elif "Bow" in types:
        weapon = "Bow"
    elif "Spear" in types:
        weapon = "Spear"
    elif "Shapeshift" in types:
        weapon = "Talisman"
    else:
        # Unreleased default attacks still have a weapon identity in their game ID.
        effect_id = case["skill_id"]
        weapon = next((name for token, name in (
            ("2HAxe", "Two Hand Axe"), ("2HSword", "Two Hand Sword"), ("2HMace", "Two Hand Mace"),
            ("Axe", "One Hand Axe"), ("Sword", "One Hand Sword"), ("Claw", "Claw"),
            ("Dagger", "Dagger"), ("Flail", "Flail"), ("Quarterstaff", "Warstaff"),
            ("Wand", "Wand"), ("Sceptre", "Sceptre"), ("Unarmed", "")) if token in effect_id), "One Hand Mace")
    return [weapon] * (2 if "DualWieldOnly" in types else 1) if weapon else []


def choose_base(bases, item_class):
    candidates = [b for b in bases if b["item_class"] == item_class and b.get("drop_level", 0) <= 90
                  and "Demigod" not in b["id"]]
    if item_class == "Shield":
        candidates = [b for b in candidates if b.get("armour", {}).get("armour", 0) > 0]
    if not candidates:
        raise ValueError(f"no normal base item for {item_class}")
    return min(candidates, key=lambda b: (b.get("drop_level", 0), b["id"]))


def item_text(base):
    lines = ["Rarity: NORMAL", base["name"], "Item Level: 90", "Quality: 0"]
    if weapon := base.get("weapon"):
        lines += [f'Physical Damage: {weapon["physical_min"]}-{weapon["physical_max"]}',
                  f'Critical Hit Chance: {weapon["crit_chance"] / 100:g}%',
                  f'Attacks per Second: {1000 / weapon["speed_ms"]:.2f}']
    for stat, label in (("armour", "Armour"), ("evasion", "Evasion Rating"), ("energy_shield", "Energy Shield")):
        if amount := base.get("armour", {}).get(stat):
            lines.append(f"{label}: {amount}")
    # The fixture deliberately has no implicit modifiers or granted item skills.
    return "\n" + "\n".join([*lines, "Implicits: 0"]) + "\n"


def synthetic_xml(case, bases):
    root = ET.Element("PathOfBuilding2")
    ET.SubElement(root, "Build", {"level": "90", "className": "Witch", "ascendClassName": "None",
        "mainSocketGroup": "1", "targetVersion": "0_1", "viewMode": "TREE", "characterLevelAutoMode": "false"})
    tree = ET.SubElement(root, "Tree", {"activeSpec": "1"})
    ET.SubElement(tree, "Spec", {"treeVersion": "0_5", "classId": "1", "classInternalId": "1",
                               "ascendClassId": "0", "nodes": ""})
    skills = ET.SubElement(root, "Skills", {"activeSkillSet": "1", "defaultGemLevel": "normalMaximum"})
    skill_set = ET.SubElement(skills, "SkillSet", {"id": "1"})
    active_index = str(case["oracle_gem"].get("primary_active_index") or 1)
    skill = ET.SubElement(skill_set, "Skill", {"enabled": "true", "mainActiveSkill": active_index,
                                             "mainActiveSkillCalcs": active_index})
    gem = case["gem"]
    attrs = {"nameSpec": case["name"], "skillId": case["skill_id"], "level": "20", "quality": "0",
             "enabled": "true", "enableGlobal1": "true", "enableGlobal2": "true", "count": "1", "corruptLevel": "0"}
    oracle_gem = case["oracle_gem"]
    if oracle_gem:
        attrs.update(gemId=oracle_gem["gameId"], variantId=oracle_gem["variantId"])
    else:
        # Old exports can identify an effect directly. An unknown gemId prevents
        # PoB2 from even considering skillId, so do not fabricate that identity.
        attrs["variantId"] = gem["gem_effect_id"]
    ET.SubElement(skill, "Gem", attrs)
    items = ET.SubElement(root, "Items", {"activeItemSet": "1", "useSecondWeaponSet": "false"})
    equipment = [choose_base(bases, cls) for cls in equipment_classes(case)]
    for index, base in enumerate(equipment, 1):
        ET.SubElement(items, "Item", {"id": str(index)}).text = item_text(base)
    item_set = ET.SubElement(items, "ItemSet", {"id": "1", "useSecondWeaponSet": "false"})
    for index, base in enumerate(equipment, 1):
        slot = "Weapon 2" if base["item_class"] in {"Shield", "Buckler"} else f"Weapon {index}"
        ET.SubElement(item_set, "Slot", {"name": slot, "itemId": str(index)})
    ET.SubElement(root, "Config")
    ET.indent(root, space="\t")
    return ET.tostring(root, encoding="unicode") + "\n", equipment


def encode_code(xml):
    return base64.urlsafe_b64encode(zlib.compress(xml.encode("utf-8"))).decode("ascii")


def cache_key(xml, commit, wrapper):
    return hashlib.sha256(xml.encode("utf-8") + b"\0" + commit.encode() + b"\0" + wrapper).hexdigest()


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as out:
        temporary = Path(out.name)
        json.dump(value, out, indent=2, allow_nan=False)
        out.write("\n")
    temporary.replace(path)


def run(command, timeout=120):
    result = subprocess.run(command, cwd=REPO, capture_output=True, text=True, timeout=timeout,
                            stdin=subprocess.DEVNULL)
    if result.returncode:
        detail = (result.stderr or result.stdout).strip()[-2000:]
        raise ValueError(f"{Path(command[0]).name} exited {result.returncode}: {detail}")
    return result.stdout


def number(value):
    if value is None:
        return 0.0  # The oracle omits inapplicable output fields (e.g. buff DPS).
    if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value):
        raise ValueError(f"non-finite or non-numeric metric: {value!r}")
    return float(value)


def engine_result(report, expected):
    output = report["output"]
    main = report.get("main_skill") or {}
    metrics = dict(zip(METRICS, map(number, (output["dps"], main.get("combined_dps"),
        output["action_rate"], output["total_hit_avg"], output["crit_chance"] * 100))))
    minion = next((m for m in output.get("minions", []) if m["summoned_by"] == expected), None)
    return {"main_skill": main.get("skill_id"), "metrics": metrics,
            "minion": {"skill_id": minion["skill_id"], "TotalDPS": number(minion["dps"])} if minion else None}


def oracle_result(report):
    output = report["mainOutput"]
    metrics = {key: number(output.get(key)) for key in METRICS}
    # Attack averages live under the hand outputs; AverageDamage is the
    # combined value including misses when no top-level AverageHit exists.
    metrics["AverageHit"] = number(output.get("AverageHit", output.get("AverageDamage")))
    minion = report.get("minionOutput")
    return {"main_skill": report.get("skillInfo", {}).get("grantedEffectId"), "metrics": metrics,
            "comparison_rate": number(output.get("HitSpeed", output.get("Speed"))),
            "average_hit_field": "AverageHit" if "AverageHit" in output else "AverageDamage",
            "minion": {"skill_id": (report.get("minionModList") or {}).get("skill"),
                       "TotalDPS": number(minion.get("TotalDPS"))} if minion else None}


def classify(expected, pobr, pob2):
    if pob2["main_skill"] != expected:
        return "pob2_other_skill", [], f'PoB2 selected {pob2["main_skill"]!r}, expected {expected}'
    if pobr["main_skill"] != expected:
        return "error", [], f'PoBR selected {pobr["main_skill"]!r}, expected {expected}'
    left, right = dict(pobr["metrics"]), dict(pob2["metrics"])
    # These are the same exclusions/bases as pobr-build's reference_check.rs.
    del left["AverageHit"], right["AverageHit"]
    right["Speed"] = pob2.get("comparison_rate", right["Speed"])
    if pobr["minion"] or pob2["minion"]:
        left["MinionTotalDPS"] = (pobr["minion"] or {}).get("TotalDPS", 0)
        right["MinionTotalDPS"] = (pob2["minion"] or {}).get("TotalDPS", 0)
    differences = [key for key in left if abs(left[key] - right[key]) > max(1e-9, abs(right[key]) * 0.05)]
    damage_keys = ("TotalDPS", "CombinedDPS", "MinionTotalDPS")
    zero_left = all(left.get(k, 0) == 0 for k in damage_keys)
    zero_right = all(right.get(k, 0) == 0 for k in damage_keys)
    if zero_left and zero_right:
        return "both_zero", differences, "no damage in this isolated fixture; buff/trigger behavior is not validated"
    if zero_left:
        return "pobr_zero", differences, "PoBR has no player or main-minion damage"
    if zero_right:
        return "pob2_zero", differences, "PoB2 has no player or main-minion damage"
    return ("differs" if differences else "agree"), differences, None


def measure(case, bases, binary, commit, wrapper, refresh):
    row = {k: v for k, v in case.items() if k not in {"gem", "oracle_gem"}}
    row.update(pobr=None, pob2=None, classification="error", differing_metrics=[], reasons=[])
    try:
        xml, equipment = synthetic_xml(case, bases)
        key = cache_key(xml, commit, wrapper)
        folder = CACHE / key
        folder.mkdir(parents=True, exist_ok=True)
        xml_path, code_path, oracle_path = (folder / name for name in ("build.xml", "code.txt", "oracle.json"))
        xml_path.write_text(xml, encoding="utf-8")
        code_path.write_text(encode_code(xml) + "\n", encoding="utf-8")
        row.update(xml=str(xml_path), equipment=[{"id": b["id"], "name": b["name"], "class": b["item_class"]} for b in equipment],
                   weapon_requirement=case["oracle_gem"].get("weaponRequirements"), oracle_cached=False)
        # Run each side independently: a failure must not hide the other's result.
        try:
            report = json.loads(run([str(binary), "calculate-build", "--file", str(code_path),
                "--reference-basis", "--json", "--breakdown", "TotalDPS"]))
            row["pobr"] = engine_result(report, case["skill_id"])
        except (ValueError, OSError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
            row["reasons"].append(f"PoBR: {error}")
        try:
            if oracle_path.exists() and not refresh:
                try:
                    oracle = read_json(oracle_path)
                    oracle_result(oracle)
                    row["oracle_cached"] = True
                except (ValueError, KeyError, TypeError):
                    oracle = None
            else:
                oracle = None
            if oracle is None:
                oracle = json.loads(run(["bash", str(REPO / "tools/pob2-oracle/run.sh"), str(xml_path)]))
                oracle_result(oracle)
                atomic_json(oracle_path, oracle)
            row["pob2"] = oracle_result(oracle)
        except (ValueError, OSError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
            row["reasons"].append(f"PoB2: {error}")
        if row["pobr"] and row["pob2"]:
            cls, differences, reason = classify(case["skill_id"], row["pobr"], row["pob2"])
            row.update(classification=cls, differing_metrics=differences)
            if reason:
                row["reasons"].append(reason)
        if not case["vendor_has_effect"]:
            row["reasons"].append("granted effect is absent from the pinned vendor's skill catalog")
    except (ValueError, OSError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
        row["reasons"].append(str(error))
    return row


def summarize(rows):
    counts = {name: sum(r["classification"] == name for r in rows) for name in CLASSES}
    families = defaultdict(list)
    for row in rows:
        for family in row["families"]:
            families[family].append(row)
    groups = []
    for name, members in families.items():
        differing = [r for r in members if r["classification"] in DIFFERENT]
        groups.append({"family": name, "total": len(members), "different": len(differing),
                       "classes": dict(Counter(r["classification"] for r in members)),
                       "examples": [r["skill_id"] for r in differing[:5]]})
    matched = sum(bool(r["pob2"] and r["pob2"]["main_skill"] == r["skill_id"]) for r in rows)
    available = [r for r in rows if r["vendor_has_effect"]]
    matched_available = sum(bool(r["pob2"] and r["pob2"]["main_skill"] == r["skill_id"]) for r in available)
    return {"counts": counts, "measured": len(rows), "oracle_main_skill_matches": matched,
            "oracle_main_skill_coverage": matched / len(rows) if rows else 0,
            "vendor_supported": len(available), "vendor_supported_matches": matched_available,
            "vendor_supported_coverage": matched_available / len(available) if available else 0,
            "families": sorted(groups, key=lambda g: (-g["different"], g["family"]))}


def text_report(report):
    summary = report["summary"]
    lines = [f'Skill sweep: {report["data_version"]}, vendor {report["vendor_commit"]}',
             f'{report["inventory"]["primary_active_effects"]} primary active effects; '
             f'{summary["measured"]} measured; {len(report["skipped"])} skipped; {report["elapsed_seconds"]:.1f}s',
             "Classes: " + ", ".join(f"{k}={v}" for k, v in summary["counts"].items()),
             f'PoB2 intended main skill: {summary["oracle_main_skill_matches"]}/{summary["measured"]} '
             f'({summary["oracle_main_skill_coverage"]:.1%}); vendor-supported: '
             f'{summary["vendor_supported_matches"]}/{summary["vendor_supported"]} ({summary["vendor_supported_coverage"]:.1%})',
             "Families overlap; ranked by differing nonzero/one-sided-zero results:"]
    for family in summary["families"]:
        lines.append(f'  {family["family"]}: {family["different"]}/{family["total"]} differ; '
                     + ", ".join(family["examples"]))
    lines += ["", "Per-skill metrics: TotalDPS / CombinedDPS / Speed / AverageHit / CritChance (%)"]
    for row in report["skills"]:
        lines.append(f'{row["skill_id"]}: {row["classification"]}')
        for side in ("pobr", "pob2"):
            result = row[side]
            if result:
                values = " / ".join(f'{result["metrics"][key]:.6g}' for key in METRICS)
                lines.append(f'  {side} [{result["main_skill"]}]: {values}')
                if result["minion"]:
                    lines.append(f'    minion {result["minion"]["skill_id"]}: TotalDPS={result["minion"]["TotalDPS"]:.6g}')
        lines.extend("  " + reason for reason in row["reasons"])
    lines += ["", "Skipped:"] + [f'  {r["skill_id"]}: {r["reason"]}' for r in report["skipped"]]
    lines += ["", "Limits: " + " ".join(report["limitations"])]
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--skill", action="append", default=[], help="exact granted effect ID; repeat to select several")
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--json", type=Path, default=CACHE / "report.json")
    parser.add_argument("--text", type=Path, default=CACHE / "report.txt")
    args = parser.parse_args(argv)
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    root = Path(os.environ.get("POBR_DATA_ROOT") or REPO / "data").resolve()
    version = os.environ.get("POBR_DATA_VERSION") or (root / "CURRENT").read_text().strip()
    directory = root / version
    if not (VENDOR / "src/HeadlessWrapper.lua").is_file():
        parser.error("missing pinned PoB2 checkout; run mise run vendor")
    commit = run(["git", "-C", str(VENDOR), "rev-parse", "HEAD"]).strip()
    pinned = read_json(directory / "overlay/skill_overrides.json")["_meta"]["vendor_commit"]
    if commit != pinned:
        parser.error(f"vendor commit {commit} differs from the data's pin {pinned}")
    vendor_gems, vendor_effects = vendor_metadata(VENDOR)
    cases, skipped, inventory = skill_cases(directory, vendor_gems, vendor_effects)
    if args.skill:
        unknown = set(args.skill) - {r["skill_id"] for r in cases + skipped}
        if unknown:
            parser.error("unknown active effect IDs: " + ", ".join(sorted(unknown)))
        cases = [c for c in cases if c["skill_id"] in args.skill]
        skipped = [c for c in skipped if c["skill_id"] in args.skill]
    binary = REPO / "target/debug/pobr"
    if not args.no_build:
        env = {**os.environ, "CARGO_BUILD_JOBS": "2", "RUST_TEST_THREADS": "2", "CARGO_TARGET_DIR": str(REPO / "target")}
        subprocess.run(["cargo", "build", "-q", "-p", "pobr-cli"], cwd=REPO, env=env, check=True)
    if not binary.is_file():
        parser.error("missing target/debug/pobr; omit --no-build")
    wrapper = b"".join(p.read_bytes() for p in sorted((REPO / "tools/pob2-oracle").rglob("*")) if p.is_file())
    bases = read_json(directory / "base/base_items.json")
    started = time.monotonic()
    rows = []
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(measure, case, bases, binary, commit, wrapper, args.refresh) for case in cases]
        for future in as_completed(futures):
            rows.append(future.result())
            if len(rows) % 25 == 0 or len(rows) == len(cases):
                print(f"skill-sweep: {len(rows)}/{len(cases)} ({time.monotonic() - started:.1f}s)", file=sys.stderr)
    rows.sort(key=lambda r: r["skill_id"])
    report = {"schema": "skill-sweep/v1", "data_version": version, "vendor_commit": commit,
              "selection": args.skill or "all", "inventory": inventory, "skills": rows, "skipped": skipped,
              "elapsed_seconds": time.monotonic() - started, "summary": summarize(rows),
              "limitations": ["Single primary effect/stat set per gem; secondary effects are only inventoried.",
                "Average hit is reported on each engine's native basis and excluded from classification (PoBR before mitigation, PoB2 after). Rate comparisons use PoB2 HitSpeed when present.",
                "No trigger payloads, corpses, charges, supports, target selections or combat configuration.",
                "both_zero does not validate buffs or non-damage utility; minion DPS is one main minion, not a full army.",
                "Weapon requirements use generated gem display text where game skill types lack restrictions.",
                "Normal bases have zero quality and no implicit modifiers or item-granted skills."]}
    atomic_json(args.json, report)
    rendered = text_report(report)
    args.text.parent.mkdir(parents=True, exist_ok=True)
    args.text.write_text(rendered, encoding="utf-8")
    print(rendered.split("\nPer-skill metrics:")[0])
    print(f"Full JSON: {args.json.resolve()}\nFull text: {args.text.resolve()}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"skill-sweep: {error}", file=sys.stderr)
        sys.exit(1)
