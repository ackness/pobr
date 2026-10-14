#!/usr/bin/env python3
"""One build against the pinned PoB2, stat by stat.

    mise run compare <build dir | code file | committed build name fragment> [Stat ...]

Calculates the build once with the engine (`pobr calculate-build --breakdown
all --json --reference-basis`) and once with the PoB2 oracle
(`tools/pob2-oracle/run.sh`), then prints for each stat both totals and their
ratio, the BASE / INC / MORE sums on each side, and the modifier lines only
one side has or has with another value. Lines are matched by where they come
from: a passive node, an item slot, a skill, the character's base or
attributes, the config, a quest reward. What is left over on both sides with
the same type and value is listed once, under both source names.

Without stats it lists the reference rows that differ (the values the build
code carries, compared on their basis) and compares those stats; when every
row agrees, the panel stats where the engine and the oracle disagree.

The oracle's JSON is cached per build code under `.cache/pob2-oracle/`, so a
rerun only recalculates the engine (`--refresh` recalculates both). The
oracle needs the pinned vendor checkout (`mise run vendor`) and LuaJIT.
Standard library only.
"""

import argparse
import base64
from dataclasses import dataclass, field
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Optional
import zlib
from xml.etree import ElementTree

REPO = Path(__file__).resolve().parents[2]
CACHE = REPO / ".cache/pob2-oracle"
ORACLE = REPO / "tools/pob2-oracle"
VENDOR = REPO / "vendor/PathOfBuilding-PoE2"
# The committed builds a name fragment is looked up in, as for `mise run
# diagnose`. The holdout set of `examples/reference-builds` is only measured.
WORKED_CORPORA = (
    "examples/reference-builds/tuned/builds",
    "examples/wasm-benchmark/builds",
    "examples/demo-bd-test/builds",
)
# Relative difference above which two totals disagree (as the CLI's reference check).
TOLERANCE = 0.05

# The modifier names PoB2 sums for a display stat; the engine's own names are
# `CalculationSession::stat_mod_names`. Another name is looked up under
# itself (and its PoB2 output name) on the PoB2 side.
POB2_NAMES = {
    "Life": ("Life",),
    "Mana": ("Mana",),
    "EnergyShield": ("EnergyShield", "Defences"),
    "Spirit": ("Spirit",),
    "Armour": ("Armour", "ArmourAndEvasion", "Defences"),
    "Evasion": ("Evasion", "ArmourAndEvasion", "Defences"),
    "FireResist": ("FireResist", "ElementalResist"),
    "ColdResist": ("ColdResist", "ElementalResist"),
    "LightningResist": ("LightningResist", "ElementalResist"),
    "ChaosResist": ("ChaosResist",),
    "CritChance": ("CritChance",),
    "CritMultiplier": ("CritMultiplier",),
    "Accuracy": ("Accuracy",),
    "HitChance": ("Accuracy",),
    "ActionRate": ("Speed",),
    "IgniteDPS": ("AilmentMagnitude",),
    "BleedDPS": ("AilmentMagnitude",),
    "PoisonDPS": ("AilmentMagnitude",),
    "TotalDotDPS": (),
}
# The names every oracle run tabulates, so that one cached run serves any
# panel stat.
DEFAULT_ORACLE_NAMES = tuple(sorted({name for names in POB2_NAMES.values() for name in names}))
# The oracle's lists of a stat's lines, in the order they are looked in.
POB2_TABLES = ("extraModList", "critModList", "damageModList")
NUMERIC_TYPES = ("BASE", "INC", "MORE", "OVERRIDE")

# The hit's damage: these stats are compared through the average hit's table
# by damage type and PoB2's damage lists (`damageModList`).
DAMAGE_STATS = ("TotalDPS", "TotalHitAvg", "CombinedDPS")
DAMAGE_TYPES = ("Physical", "Lightning", "Cold", "Fire", "Chaos")
ELEMENTS = ("Lightning", "Cold", "Fire")
# The names `damageModList` holds that scale every damage type of the hit.
SHARED_DAMAGE_NAMES = (
    "Damage", "AttackDamage", "SpellDamage", "ProjectileDamage", "AreaDamage", "MeleeDamage",
    "BowDamage", "CrossbowDamage", "GrenadeDamage", "StaffDamage", "MaceDamage", "SwordDamage",
    "AxeDamage", "ClawDamage", "DaggerDamage", "WandDamage", "UnarmedDamage", "SpellSkillDamage",
)
# What hit DPS is the product of, by the engine's display stat id.
DPS_FACTORS = ("TotalHitAvg", "ActionRate", "HitChance", "CritChance", "CritMultiplier")

ITEM_KINDS = {"Item", "ItemAffix", "ItemImplicit", "ItemEnchant", "ItemQuality"}
SKILL_KINDS = {"SkillGem", "SupportGem", "SkillLevel", "GemQuality", "Buff"}
# Engine source ids `<prefix>.<granted effect>[.…]`.
EFFECT_PREFIXES = {"gem", "support", "buff", "aura", "curse", "debuff", "spirit", "minion", "warcry"}
# Engine source ids for the main skill's own numbers.
MAIN_SKILL_PREFIXES = {"base", "trigger", "triggered", "cwc"}
SLOT_IDS = {
    "weapon1", "weapon2", "helmet", "bodyarmour", "gloves", "boots", "amulet",
    "ring1", "ring2", "ring3", "belt", "arm1", "arm2", "leg1", "leg2",
}


@dataclass
class Line:
    """One modifier line of one side: its type, the key its source is matched
    by, its value and how that side names it."""

    mod_type: str
    key: str
    value: Optional[float]
    label: str
    active: bool = True


@dataclass
class Group:
    """The active lines of one type from one source, and what they add up to."""

    mod_type: str
    key: str
    value: float
    lines: list = field(default_factory=list)


@dataclass
class Comparison:
    only_pob2: list = field(default_factory=list)
    only_pobr: list = field(default_factory=list)
    # (PoBR group, PoB2 group) with the same source and another value.
    different: list = field(default_factory=list)
    # (PoBR group, PoB2 group) left over on both sides with the same type and value.
    renamed: list = field(default_factory=list)
    # The engine's inactive lines from the source of a group only PoB2 counts.
    inactive: dict = field(default_factory=dict)

    def empty(self) -> bool:
        return not (self.only_pob2 or self.only_pobr or self.different or self.renamed)


# Inputs


def resolve_target(target: str) -> tuple[str, Path]:
    """The build `target` names and the file holding its code."""
    path = Path(target)
    if (path / "code.txt").is_file():
        return path.resolve().name, path / "code.txt"
    if path.is_file():
        return (path.resolve().parent.name if path.name == "code.txt" else path.name), path
    matches = sorted(
        folder
        for corpus in WORKED_CORPORA
        if (REPO / corpus).is_dir()
        for folder in (REPO / corpus).iterdir()
        if target in folder.name and (folder / "code.txt").is_file()
    )
    if len(matches) == 1:
        return matches[0].name, matches[0] / "code.txt"
    if not matches:
        raise SystemExit(f"compare: {target}: no such directory, file or committed build")
    listed = "\n  ".join(match.name for match in matches)
    raise SystemExit(f"compare: {target} matches {len(matches)} committed builds:\n  {listed}")


def decode_code(code: str) -> str:
    """The build XML a PoB build code holds: URL-safe Base64 of zlib data,
    whitespace ignored, padding optional, standard Base64 characters accepted."""
    cleaned = "".join(code.split()).replace("+", "-").replace("/", "_").rstrip("=")
    data = base64.urlsafe_b64decode(cleaned + "=" * (-len(cleaned) % 4))
    try:
        xml = zlib.decompress(data)
    except zlib.error:
        xml = zlib.decompress(data, -zlib.MAX_WBITS)
    return xml.decode("utf-8")


def item_slot_keys(xml: str) -> dict:
    """PoB2 item id → the source key of the slot it is equipped in, in the
    active item set. Jewels and unslotted items are left out: their lines are
    keyed as `jewel`."""
    root = ElementTree.fromstring(xml)
    items = root.find("Items")
    if items is None:
        return {}
    sets = items.findall("ItemSet")
    active = items.get("activeItemSet")
    chosen = next((s for s in sets if s.get("id") == active), sets[0] if sets else items)
    second_set = (chosen.get("useSecondWeaponSet") or items.get("useSecondWeaponSet")) == "true"
    keys = {}
    for slot in chosen.iter("Slot"):
        item_id = slot.get("itemId")
        key = slot_key(slot.get("name") or "", second_set)
        if item_id and item_id != "0" and key:
            keys.setdefault(item_id, key)
    return keys


def slot_key(name: str, second_weapon_set: bool) -> Optional[str]:
    """The source key of a PoB2 slot name (`Body Armour` → `item:bodyarmour`)."""
    if name.startswith("Jewel") or "Socket" in name:
        return None
    compact = "".join(name.lower().split())
    if compact.startswith(("flask", "charm")):
        return "flask:" + compact
    if compact.endswith("swap") and second_weapon_set:
        # The engine's weapon slots hold the swap set while the build uses it.
        return "item:" + compact[: -len("swap")]
    return "item:" + compact


# Source keys


def pob2_source_key(source: str, item_slots: dict) -> str:
    """The key a PoB2 modifier source is matched by (`Tree:123` → `tree:123`,
    `Item:23:Name, Base` → the slot of item 23, `jewel` for any jewel,
    `Skill:Id` → `skill:Id`). The engine numbers its jewels by their place in
    the build, so the jewels meet as one source."""
    source = source or ""
    head, _, rest = source.partition(":")
    if head == "Tree":
        return "tree:" + rest
    if head == "Item":
        item_id, _, name = rest.partition(":")
        return item_slots.get(item_id) or "jewel"
    if head == "Skill":
        return "skill:" + rest
    if head == "Quest":
        return "quest"
    if source == "Base":
        return "base"
    if source in ("Strength", "Dexterity", "Intelligence"):
        return "attributes"
    if source == "Config":
        return "config"
    if source == "Custom":
        return "custom"
    return "other:" + source


def pobr_source_key(line: dict, main_skill: str) -> str:
    """The key an engine breakdown line is matched by, from its origin
    (`PassiveNode node.123` → `tree:123`, `ItemAffix item.gloves.explicit` →
    `item:gloves`, `Jewel jewel.2` → `jewel`). A line without an origin (a
    custom line's) is keyed by its text."""
    kind = line.get("origin_kind")
    ident = line.get("origin_id") or ""
    if not kind:
        return "text:" + (line.get("source_text") or "?")
    head, _, rest = ident.partition(".")
    if kind in ("PassiveNode", "AscendancyNode"):
        return "tree:" + (rest if head == "node" else ident)
    if kind in ITEM_KINDS:
        slot = line.get("slot")
        if not slot:
            if head == "item":
                slot = rest.partition(".")[0]
            elif head in SLOT_IDS:
                slot = head
            else:
                slot = ident
        return "item:" + slot
    if kind == "Flask":
        return "flask:" + rest
    if kind == "Jewel":
        return "jewel"
    if kind in SKILL_KINDS:
        return "skill:" + skill_effect(ident, main_skill)
    if kind == "CharacterBase":
        return "attributes" if ident == "base.attr_derived" else "base"
    if kind == "GameConstant":
        return "base"
    if kind == "Config" and ident == "config.custom":
        return "custom"
    if kind in ("Config", "ConfigOption", "EnemyConfig"):
        return "config"
    if kind == "CampaignReward":
        return "quest"
    if kind == "GrantedKeystone":
        return "keystone:" + rest
    return f"{kind}:{ident}"


def skill_effect(ident: str, main_skill: str) -> str:
    """The granted effect an engine skill source id belongs to."""
    parts = ident.split(".")
    if parts[0] in EFFECT_PREFIXES and len(parts) > 1:
        effect = parts[1]
    elif parts[0] == "skill":
        # `skill.<effect>.<name>`, or `skill.<stat>` for the main skill's own stats.
        effect = parts[1] if len(parts) > 2 else main_skill
    elif parts[0] in MAIN_SKILL_PREFIXES:
        effect = main_skill
    else:
        # A support's stats: `<effect>.<stat>`.
        effect = parts[0]
    return effect.rsplit("/", 1)[-1]


# Lines


def number(value) -> Optional[float]:
    if value is None or isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return float(value)  # the oracle writes "NaN" and "Infinity" as strings
        except ValueError:
            return None
    return None


def as_dict(value) -> dict:
    return value if isinstance(value, dict) else {}


def reference_value(oracle: dict, pob_key: str) -> Optional[float]:
    """PoB2's value for a reference row: `Minion:<stat>` is the main skill's
    minion's, any other key the character's."""
    if pob_key.startswith("Minion:"):
        return number(as_dict(oracle.get("minionOutput")).get(pob_key[len("Minion:"):]))
    return number(as_dict(oracle.get("mainOutput")).get(pob_key))


def as_list(value) -> list:
    # The oracle writes an empty Lua table as `{}`.
    return value if isinstance(value, list) else []


def pobr_lines(mods, main_skill: str) -> list:
    """The engine's breakdown lines (`BreakdownMod`), inactive ones included."""
    lines = []
    for mod in mods or []:
        text = mod.get("source_text") or ""
        origin = " ".join(part for part in (mod.get("origin_kind"), mod.get("origin_id")) if part)
        label = f"{text}  [{origin}]" if text and origin else text or origin or "?"
        lines.append(Line(
            mod.get("mod_type") or "?",
            pobr_source_key(mod, main_skill),
            number(mod.get("value")),
            label,
            bool(mod.get("active", True)),
        ))
    return lines


def pob2_lines(oracle: dict, tables, names, item_slots: dict) -> list:
    """The lines PoB2 counts for `names`, each (type, name) from the first of
    `tables` that lists it. PoB2 lists only the lines it counts."""
    lines = []
    for mod_type in NUMERIC_TYPES:
        for name in names:
            records = []
            for table in tables:
                records = as_list(as_dict(as_dict(oracle.get(table)).get(mod_type)).get(name))
                if records:
                    break
            for record in records:
                source = record.get("source") or ""
                tags = ", ".join(str(tag) for tag in as_list(record.get("tags")))
                label = f"{source}  [{name}{'; ' + tags if tags else ''}]"
                lines.append(Line(mod_type, pob2_source_key(source, item_slots),
                                  number(record.get("evalValue")), label))
    return lines


def combine(mod_type: str, values) -> float:
    """What lines of one type add up to: MORE multiplies (as a percentage),
    OVERRIDE takes the largest, the others add."""
    values = list(values)
    if mod_type == "MORE":
        product = 1.0
        for value in values:
            product *= 1 + value / 100
        return (product - 1) * 100
    if mod_type == "OVERRIDE":
        return max(values)
    return sum(values)


def close(a: float, b: float) -> bool:
    return abs(a - b) <= max(0.01, 0.001 * max(abs(a), abs(b)))


def groups(lines) -> dict:
    """The active numeric lines by (type, source key)."""
    out = {}
    for line in lines:
        if line.active and line.value is not None and line.mod_type in NUMERIC_TYPES:
            out.setdefault((line.mod_type, line.key), []).append(line)
    return {key: Group(key[0], key[1], combine(key[0], (line.value for line in found)), found)
            for key, found in out.items()}


def compare(pobr: list, pob2: list) -> Comparison:
    """The lines of one stat on both sides, matched by type and source."""
    ours, theirs = groups(pobr), groups(pob2)
    result = Comparison()
    for key in sorted(set(ours) | set(theirs)):
        mine, other = ours.get(key), theirs.get(key)
        if mine and other:
            if not close(mine.value, other.value):
                result.different.append((mine, other))
        elif other:
            result.only_pob2.append(other)
        else:
            result.only_pobr.append(mine)
    # A source both sides name differently (a custom line) is left
    # over on both sides with the same type and value.
    for other in list(result.only_pob2):
        mine = next((group for group in result.only_pobr
                     if group.mod_type == other.mod_type and close(group.value, other.value)), None)
        if mine is not None:
            result.only_pobr.remove(mine)
            result.only_pob2.remove(other)
            result.renamed.append((mine, other))
    wanted = {(group.mod_type, group.key) for group in result.only_pob2}
    for line in pobr:
        if not line.active and (line.mod_type, line.key) in wanted:
            result.inactive.setdefault((line.mod_type, line.key), []).append(line)
    return result


def sums(lines) -> dict:
    """The BASE and INC totals and the MORE multiplier of the active lines."""
    totals = {"BASE": 0.0, "INC": 0.0, "MORE": 1.0}
    for line in lines:
        if not line.active or line.value is None:
            continue
        if line.mod_type in ("BASE", "INC"):
            totals[line.mod_type] += line.value
        elif line.mod_type == "MORE":
            totals["MORE"] *= 1 + line.value / 100
    return totals


# Stats


def resolve_stats(asked, entries: dict) -> list:
    """The engine's stat ids for the names asked for; a PoB2 output name
    (`Speed`, `AverageHit`) is accepted for the stat it names."""
    stats = []
    for name in asked:
        # The engine aggregates any other name as a modifier name: its entry has no value.
        if (entries.get(name) or {}).get("value") is None:
            name = next((stat for stat, entry in entries.items()
                         if entry.get("pob_key") == name and entry.get("value") is not None), name)
        if name not in stats:
            stats.append(name)
    return stats


def pob2_names(stat: str, entry: Optional[dict]) -> tuple:
    if stat in POB2_NAMES:
        return POB2_NAMES[stat]
    pob_key = (entry or {}).get("pob_key")
    return tuple(dict.fromkeys(name for name in (stat, pob_key) if name))


def oracle_names(stats, entries: dict) -> list:
    """The names the oracle tabulates for these stats (`ORACLE_EXTRA_STATS`)."""
    names = set(DEFAULT_ORACLE_NAMES)
    for stat in stats:
        if stat not in DAMAGE_STATS:
            names.update(pob2_names(stat, entries.get(stat)))
    return sorted(names)


def agrees(a: float, b: float) -> bool:
    scale = max(abs(a), abs(b))
    return a == b or scale == 0 or abs(a - b) <= TOLERANCE * scale


def disagreeing_stats(entries: dict, main_output: dict) -> list:
    """The stats whose engine and oracle totals disagree."""
    found = []
    for stat, entry in entries.items():
        ours = number(entry.get("value"))
        theirs = number(main_output.get(entry.get("pob_key") or ""))
        if ours is not None and theirs is not None and not agrees(ours, theirs):
            found.append(stat)
    return found


# Running both sides


def run_engine(code_file: Path, stats) -> dict:
    breakdown = ",".join(["all", *stats])
    command = ["cargo", "run", "-q", "-p", "pobr-cli", "--", "calculate-build",
               "--file", str(code_file), "--breakdown", breakdown, "--json", "--reference-basis"]
    print(f"compare: calculating with the engine ({' '.join(command[:6])} ...)", file=sys.stderr)
    # Cargo's own messages (a build lock, compile errors) stay on stderr.
    result = subprocess.run(command, cwd=REPO, stdout=subprocess.PIPE, text=True)
    if result.returncode != 0:
        raise SystemExit(f"compare: the engine failed ({result.returncode})")
    return json.loads(result.stdout)


def vendor_commit() -> str:
    if (VENDOR / ".git").exists():
        result = subprocess.run(["git", "-C", str(VENDOR), "rev-parse", "HEAD"],
                                capture_output=True, text=True)
        if result.returncode == 0:
            return result.stdout.strip()
    pinned = REPO / "vendor/.pob2-version.txt"
    return pinned.read_text(encoding="utf-8").strip() if pinned.is_file() else "unknown"


def oracle_cache_path(code: str, names, commit: str) -> Path:
    """`.cache/pob2-oracle/<code hash>-<inputs hash>.json`: the inputs are the
    tabulated names, the vendor commit and the oracle script."""
    code_hash = hashlib.sha256("".join(code.split()).encode("utf-8")).hexdigest()[:16]
    inputs = hashlib.sha256()
    inputs.update(",".join(sorted(names)).encode("utf-8"))
    inputs.update(commit.encode("utf-8"))
    script = ORACLE / "oracle.lua"
    if script.is_file():
        inputs.update(script.read_bytes())
    return CACHE / f"{code_hash}-{inputs.hexdigest()[:10]}.json"


def run_oracle(xml: str, out: Path, names) -> None:
    if not (VENDOR / "src/HeadlessWrapper.lua").is_file():
        raise SystemExit("compare: the PoB2 oracle needs the pinned vendor checkout: "
                         "run `mise run vendor` (and `mise run deps` for LuaJIT)")
    out.parent.mkdir(parents=True, exist_ok=True)
    xml_file = out.with_suffix(".xml")
    xml_file.write_text(xml, encoding="utf-8")
    handle, partial_name = tempfile.mkstemp(dir=out.parent, suffix=".partial")
    os.close(handle)
    partial = Path(partial_name)
    print("compare: calculating with the PoB2 oracle (tools/pob2-oracle/run.sh) ...", file=sys.stderr)
    env = {**os.environ, "ORACLE_EXTRA_STATS": ",".join(names)}
    try:
        result = subprocess.run(["bash", str(ORACLE / "run.sh"), str(xml_file), str(partial)],
                                cwd=REPO, env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=900)
        if result.returncode != 0 or partial.stat().st_size == 0:
            tail = "\n".join((result.stderr or result.stdout).strip().splitlines()[-15:])
            raise SystemExit(f"compare: the PoB2 oracle failed ({result.returncode}):\n{tail}")
        partial.replace(out)
    except subprocess.TimeoutExpired:
        raise SystemExit("compare: the PoB2 oracle did not finish in 15 minutes") from None
    finally:
        partial.unlink(missing_ok=True)


# Report


def num(value) -> str:
    if value is None:
        return "—"
    if math.isnan(value) or math.isinf(value):
        return str(value)
    text = f"{value:,.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("", "-0") else text


def signed(value) -> str:
    text = num(value)
    return text if text.startswith("-") or text in ("0", "—") else "+" + text


def amount(mod_type: str, value) -> str:
    if mod_type == "INC":
        return signed(value) + "%"
    if mod_type == "MORE":
        return signed(value) + "% more"
    if mod_type == "OVERRIDE":
        return "= " + num(value)
    return signed(value)


def ratio(a, b) -> str:
    if a is None or b is None or b == 0 or math.isnan(a) or math.isnan(b):
        return "—"
    return f"{a / b:.3f}"


def multiplier(value) -> str:
    return "×" + f"{value:.4f}".rstrip("0").rstrip(".")


def total(mod_type: str, value: float) -> str:
    if mod_type == "MORE":
        return multiplier(value)
    return num(value) + ("%" if mod_type == "INC" else "")


def sum_rows(pobr: list, pob2: list, types=("BASE", "INC", "MORE")) -> list:
    ours, theirs = sums(pobr), sums(pob2)
    rows = ["   sums    " + "".join(f"{mod_type:>14}" for mod_type in types)]
    for side, totals in (("PoBR", ours), ("PoB2", theirs)):
        rows.append(f"   {side}    " + "".join(f"{total(t, totals[t]):>14}" for t in types))
    return rows


def group_rows(group: Group, indent: str) -> list:
    head = f"{indent}{group.mod_type:<9}{amount(group.mod_type, group.value):>16}  {group.key}"
    if len(group.lines) == 1:
        return [f"{head}  {group.lines[0].label}"]
    rows = [f"{head}  ({len(group.lines)} lines)"]
    rows += [f"{indent}{'':<9}{amount(line.mod_type, line.value):>16}    {line.label}"
             for line in group.lines]
    return rows


def comparison_rows(comparison: Comparison, indent: str = "   ") -> list:
    if comparison.empty():
        return [f"{indent}every line matches"]
    rows = []
    if comparison.only_pob2:
        rows.append(f"{indent}only in PoB2 ({len(comparison.only_pob2)})")
        for group in comparison.only_pob2:
            rows += group_rows(group, indent + "  ")
            for line in comparison.inactive.get((group.mod_type, group.key), []):
                rows.append(f"{indent}      PoBR has it, inactive: "
                            f"{amount(line.mod_type, line.value)}  {line.label}")
            if group.key == "base":
                rows.append(f"{indent}      (PoBR's starting values are not modifier lines: "
                            "`pobr calculate-build --breakdown` shows them in the steps)")
    if comparison.only_pobr:
        rows.append(f"{indent}only in PoBR ({len(comparison.only_pobr)})")
        for group in comparison.only_pobr:
            rows += group_rows(group, indent + "  ")
    if comparison.different:
        rows.append(f"{indent}different values ({len(comparison.different)})")
        for mine, other in comparison.different:
            rows.append(f"{indent}  {mine.mod_type:<9}{mine.key}: PoBR "
                        f"{amount(mine.mod_type, mine.value)} · PoB2 {amount(other.mod_type, other.value)}")
            rows += [f"{indent}      PoBR {amount(line.mod_type, line.value):>14}  {line.label}"
                     for line in mine.lines]
            rows += [f"{indent}      PoB2 {amount(line.mod_type, line.value):>14}  {line.label}"
                     for line in other.lines]
    if comparison.renamed:
        rows.append(f"{indent}same value, different sources ({len(comparison.renamed)})")
        for mine, other in comparison.renamed:
            rows.append(f"{indent}  {mine.mod_type:<9}{amount(mine.mod_type, mine.value):>16}  "
                        f"PoBR {mine.key} · PoB2 {other.key}")
    return rows


def stat_header(stat: str, entry: dict, main_output: dict) -> str:
    pob_key = entry.get("pob_key") or stat
    ours = number(entry.get("value"))
    theirs = number(main_output.get(pob_key))
    text = (f"\n== {stat}: PoBR {num(ours)} · PoB2 {num(theirs)} ({pob_key}) · "
            f"PoBR/PoB2 {ratio(ours, theirs)}")
    reference = number(entry.get("reference"))
    if reference is not None:
        text += f" · code reference {num(reference)}"
    return text


def stat_rows(stat: str, entry: dict, oracle: dict, item_slots: dict, main_skill: str) -> list:
    names = pob2_names(stat, entry)
    if not names:
        return ["   (no PoB2 modifier list for this stat; totals only)"]
    breakdown = entry.get("breakdown") or {}
    ours = pobr_lines(breakdown.get("mods"), main_skill)
    theirs = pob2_lines(oracle, POB2_TABLES, names, item_slots)
    rows = [f"   PoB2 names: {', '.join(names)}"]
    if not ours and not theirs:
        return rows + ["   no modifier lines on either side"]
    return rows + sum_rows(ours, theirs) + comparison_rows(compare(ours, theirs))


def damage_table(entries: dict) -> tuple:
    """The engine's average-hit table, and the hand it is for when there are two."""
    breakdown = (entries.get("TotalHitAvg") or {}).get("breakdown") or {}
    if breakdown.get("damage"):
        return breakdown["damage"], None
    for hand in breakdown.get("hands") or []:
        if hand.get("damage"):
            return hand["damage"], hand.get("hand")
    return None, None


def factor_rows(entries: dict, main_output: dict) -> list:
    rows = [f"   {'factor':<18}{'PoBR':>14}{'PoB2':>14}  PoB2 name"]
    for stat in DPS_FACTORS:
        entry = entries.get(stat) or {}
        key = entry.get("pob_key") or stat
        rows.append(f"   {stat:<18}{num(number(entry.get('value'))):>14}"
                    f"{num(number(main_output.get(key))):>14}  {key}")
    return rows


def damage_rows(entries: dict, oracle: dict, item_slots: dict, main_skill: str) -> list:
    table, hand = damage_table(entries)
    if table is None:
        return ["   PoBR has no average-hit table for this build's main skill"]
    rows = []
    if hand:
        rows.append(f"   (PoBR's {hand.replace('_', ' ')} pass; PoB2's lists are for the skill)")
    shared_ours = pobr_lines(table.get("shared_mods"), main_skill)
    shared_theirs = pob2_lines(oracle, ("damageModList",), SHARED_DAMAGE_NAMES, item_slots)
    rows.append("   -- every damage type (Damage, AttackDamage, SpellDamage, ProjectileDamage, ... "
                "as PoB2 lists them)")
    rows += sum_rows(shared_ours, shared_theirs, ("INC", "MORE"))
    rows += comparison_rows(compare(shared_ours, shared_theirs))
    by_type = {row.get("damage_type"): row for row in table.get("rows") or []}
    components = as_dict(oracle.get("components"))
    main_output = as_dict(oracle.get("mainOutput"))
    for damage_type in DAMAGE_TYPES:
        row = by_type.get(damage_type) or {}
        component = as_dict(components.get(damage_type))
        ours_avg = number(row.get("avg"))
        theirs_avg = number(component.get("HitAverage"))
        if theirs_avg is None:
            theirs_avg = number(main_output.get(damage_type + "HitAverage"))
        if not (ours_avg or theirs_avg):
            continue
        rows.append(
            f"   -- {damage_type}: PoBR {num(number(row.get('min')))}–{num(number(row.get('max')))}"
            f" (average {num(ours_avg)}; base {num(number(row.get('base_min')))}–"
            f"{num(number(row.get('base_max')))}) · PoB2 {num(number(component.get('Min')))}–"
            f"{num(number(component.get('Max')))} (average {num(theirs_avg)})")
        names = [damage_type + "Damage"] + (["ElementalDamage"] if damage_type in ELEMENTS else [])
        ours = pobr_lines(row.get("mods"), main_skill)
        theirs = pob2_lines(oracle, ("damageModList",), names, item_slots)
        theirs_sums = sums(shared_theirs + theirs)
        ours_more = number(row.get("more"))
        rows.append(f"   with the shared lines: INC PoBR {total('INC', number(row.get('increased')) or 0)}"
                    f" · PoB2 {total('INC', theirs_sums['INC'])}; MORE PoBR "
                    f"{multiplier(ours_more) if ours_more is not None else '—'}"
                    f" · PoB2 {multiplier(theirs_sums['MORE'])}")
        rows += comparison_rows(compare(ours, theirs))
    return rows


def render(name: str, report: dict, oracle: dict, xml: str, stats, basis: str,
           commit: str = "") -> str:
    """The report. `basis` says where `stats` came from: `asked`, `reference`
    (the reference rows that differ) or `oracle` (the panel stats where the
    engine and the oracle disagree)."""
    entries = {entry.get("stat"): entry for entry in report.get("breakdowns") or []}
    main_output = as_dict(oracle.get("mainOutput"))
    item_slots = item_slot_keys(xml)
    skill = report.get("main_skill") or {}
    info = as_dict(oracle.get("skillInfo"))
    main_skill = skill.get("skill_id") or info.get("grantedEffectId") or "main"
    build = report.get("build") or {}
    pob2_skill = f"{info.get('grantedEffectId') or 'none'}"
    if info.get("activeGemName"):
        pob2_skill += f" ({info['activeGemName']})"
    if info.get("activeGemLevel") is not None:
        pob2_skill += f" level {info['activeGemLevel']}"
    if info.get("activeGemQuality") is not None:
        pob2_skill += f" quality {info['activeGemQuality']}"
    rows = [
        f"######## {name}",
        f"PoBR  data {report.get('data_version')} · {build.get('class_name')} / "
        f"{build.get('ascendancy_name')} {build.get('level')} · main skill "
        f"{skill.get('skill_id') or 'none'} · on the reference's basis",
        f"PoB2  vendor {commit[:10] or 'unknown'} · main skill {pob2_skill}",
    ]
    if skill.get("skill_id") and info.get("grantedEffectId") and skill["skill_id"] != info["grantedEffectId"]:
        rows.append("!! The main skills differ: the damage lines below compare different skills.")
    losses = []
    if report.get("missing_items"):
        losses.append(f"items not read: {len(report['missing_items'])}")
    if report.get("unsupported_modifiers"):
        losses.append(f"lines no rule parses: {len(report['unsupported_modifiers'])}")
    if losses:
        rows.append(f"PoBR  {', '.join(losses)} (`mise run diagnose` lists them)")

    reference = report.get("reference") or {}
    if basis == "reference":
        differing = reference.get("differing") or []
        rows.append(f"\nReference rows that differ (on the reference's basis, tolerance "
                    f"{TOLERANCE * 100:.0f}%): {len(differing)} of {reference.get('compared')}")
        rows.append(f"   {'stat':<26}{'PoBR':>14}{'code':>14}{'PoB2 now':>14}{'PoBR/code':>11}")
        for row in differing:
            pobr, code = number(row.get("pobr")), number(row.get("reference"))
            now = reference_value(oracle, row.get("pob_key") or "")
            rows.append(f"   {str(row.get('stat') or ''):<26}{num(pobr):>14}{num(code):>14}"
                        f"{num(now):>14}{ratio(pobr, code):>11}")
    elif basis == "oracle":
        why = ("every reference row agrees" if report.get("reference")
               else "the build code carries no reference values")
        if stats:
            rows.append(f"\n{why[0].upper() + why[1:]}; comparing the panel stats where PoBR and "
                        f"PoB2 disagree by more than {TOLERANCE * 100:.0f}%: {', '.join(stats)}")
        else:
            rows.append(f"\n{why[0].upper() + why[1:]}, and every panel stat agrees with PoB2 within "
                        f"{TOLERANCE * 100:.0f}%; name stats to compare their lines.")

    damage_done = None
    for stat in stats:
        entry = entries.get(stat)
        if entry is None:
            rows.append(f"\n== {stat}: not in the engine's output")
            continue
        rows.append(stat_header(stat, entry, main_output))
        if stat in DAMAGE_STATS:
            if stat != "TotalHitAvg":
                rows += factor_rows(entries, main_output)
            if damage_done is None:
                rows += damage_rows(entries, oracle, item_slots, main_skill)
                damage_done = stat
            else:
                rows.append(f"   (the damage lines are under {damage_done} above)")
        else:
            rows += stat_rows(stat, entry, oracle, item_slots, main_skill)
    return "\n".join(row for row in rows if row is not None) + "\n"


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="mise run compare",
        description="One build against the pinned PoB2: totals, sums and the modifier lines "
                    "only one side has, stat by stat.")
    parser.add_argument("target", help="build directory holding code.txt, code file, "
                                       "or a fragment of a committed build's name")
    parser.add_argument("stats", nargs="*",
                        help="display stats (TotalDPS, Life, ActionRate; PoB2 names such as Speed "
                             "work too) or modifier names; default: the reference rows that differ")
    parser.add_argument("--refresh", action="store_true",
                        help="recalculate the PoB2 side even when it is cached")
    parser.add_argument("--engine-json", type=Path,
                        help="the engine's `calculate-build --breakdown all --json "
                             "--reference-basis` output, instead of running it")
    parser.add_argument("--oracle-json", type=Path,
                        help="the oracle's output, instead of running it")
    args = parser.parse_args(argv)

    name, code_file = resolve_target(args.target)
    code = code_file.read_text(encoding="utf-8")
    xml = decode_code(code)
    asked = [stat.strip() for arg in args.stats for stat in arg.split(",") if stat.strip()]
    extra = [stat for stat in asked if stat != "all"]

    report = load_json(args.engine_json) if args.engine_json else run_engine(code_file, extra)
    entries = {entry.get("stat"): entry for entry in report.get("breakdowns") or []}
    differing = [row.get("stat") for row in (report.get("reference") or {}).get("differing") or []]
    if asked:
        basis = "asked"
        stats = resolve_stats([stat for name in asked
                               for stat in (list(entries) if name == "all" else [name])], entries)
    else:
        basis = "reference" if differing else "oracle"
        stats = differing
    missing = [stat for stat in stats if stat not in entries]
    if missing and not args.engine_json:
        report = run_engine(code_file, [*extra, *missing])
        entries = {entry.get("stat"): entry for entry in report.get("breakdowns") or []}

    commit = vendor_commit()
    if args.oracle_json:
        oracle = load_json(args.oracle_json)
    else:
        names = oracle_names(stats, entries)
        cached = oracle_cache_path(code, names, commit)
        if args.refresh or not cached.is_file():
            run_oracle(xml, cached, names)
        else:
            print(f"compare: PoB2 side from {cached.relative_to(REPO)}", file=sys.stderr)
        oracle = load_json(cached)

    if basis == "oracle":
        stats = disagreeing_stats(entries, as_dict(oracle.get("mainOutput")))
    sys.stdout.write(render(name, report, oracle, xml, stats, basis, commit))
    return 0


if __name__ == "__main__":
    sys.exit(main())
