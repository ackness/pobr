#!/usr/bin/env python3
"""Every worked build against its reference and the pinned PoB2, in one run.

    mise run triage [set dir | build dir ...] [--jobs N] [--refresh] [--top N] [--json FILE]

Runs the engine (`pobr calculate-build --breakdown all --json
--reference-basis --usage`) and the PoB2 oracle once for each build of the
sets, the benchmark and tuned sets by default, several builds at a time. Each
reference row that differs falls in one class by what the pinned PoB2 says
today:

  engine     PoB2 agrees with the reference and the engine does not.
  both       the engine, PoB2 and the reference all differ.
  reference  the engine agrees with PoB2 and the reference does not: the value
             comes from another version of the exporting tool.
  known      a difference the repository explains
             (devs/scripts/known-differences.json).

The report then gathers what explains the engine and both rows across builds:
the modifier lines one side counts and the other does not or counts with
another value (by source and type), the engine's lines that do not apply
where PoB2 counts their source (a condition the engine does not set), the
factors of the damage and the enemy's mitigation that differ, PoB2's pools
behind a largest hit or EHP that differs, and the lines no rule parses, by how
many builds carry them. From each build's ledger of lines (`--usage`) it
gathers the inputs the calculation drops whether or not a row differs: lines a
rule recognises and nothing models, modifiers stored and never read (first
those no triaged build reads), conditions nothing sets, gem stats the engine
rejects, and lines parsed yet storing nothing. Each build also lists the
skills the two sides calculate at another level or quality, which change a
buff, aura or curse without any of the above. Fix what several builds share
first, then rerun.

The holdout set is only measured (`mise run accuracy`), never triaged. The
oracle's results share `mise run compare`'s cache, so a build triaged once
compares at once. Standard library only.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]
_SPEC = importlib.util.spec_from_file_location("compare_build", REPO / "devs/scripts/compare-build.py")
compare = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(compare)

DEFAULT_SETS = ("examples/wasm-benchmark/builds", "examples/reference-builds/tuned/builds")
KNOWN_DIFFERENCES = REPO / "devs/scripts/known-differences.json"
# The bases whose granted skill reserves no Spirit (game table ItemInherentSkills).
RESERVATION_FREE = REPO / "data/overlay-common/item_granted_skills.json"
# The engine's Spirit reservation of each skill (origin `spirit.<skill id>`).
SKILL_RESERVATION = "SkillSpiritReservationBase"
SPIRIT_REASON = ("the skill an Absent, Lament or Portent Amulet grants reserves no Spirit "
                 "(docs/maintainer-guide.md)")
ACTIONABLE = ("engine", "both")
CLASSES = ("engine", "both", "reference", "known", "no oracle")
# PoB2's figures behind the largest hits and EHP, listed when one of those differs.
POB2_POOLS = (
    ("life", "LifeRecoverable"), ("energy shield", "EnergyShieldRecoveryCap"),
    ("mana", "ManaUnreserved"), ("ward", "Ward"), ("life loss prevented %", "preventedLifeLoss"),
    ("MoM pool", "sharedMoMHitPool"), ("physical hit pool", "PhysicalTotalHitPool"),
    ("physical taken", "PhysicalTakenHitMult"), ("chaos taken", "ChaosTakenHitMult"),
    ("block %", "EffectiveBlockChance"), ("evade %", "EvadeChance"),
)
PoBR_POOLS = ("Life", "EnergyShield", "Mana", "Ward")
POOL_STAT = re.compile(r"(MaxHit|EHP)$")
# The enemy's mitigation PoB2 records per damage type.
ENEMY_FIELDS = ("resistBase", "pen", "takenInc", "takenMore")


def short(name: str) -> str:
    """A build's folder name without the fetch date and the code hash."""
    name = re.sub(r"^ninja-\d{4}-\d{2}-\d{2}-", "", name)
    return re.sub(r"-[0-9a-f]{12}$", "", name)


def template(text: str) -> str:
    """A line without where it is and with its numbers as `#`, so that rolls
    of one line meet."""
    text = re.sub(r"^(Tree:\d+|Config): ", "", text)
    return re.sub(r"[+-]?\d+(?:\.\d+)?", "#", text)


def classify(pobr, code, now) -> str:
    if pobr is None or code is None or now is None:
        return "no oracle"
    if compare.agrees(pobr, now):
        return "reference"
    if compare.agrees(now, code):
        return "engine"
    return "both"


def load_known(path: Path = KNOWN_DIFFERENCES) -> list:
    if not path.is_file():
        return []
    return json.loads(path.read_text(encoding="utf-8")).get("differences") or []


def skill_key(name: str) -> str:
    """A skill's id (`SummonSkeletalReaversPlayer`), gem or PoB2 name
    (`Skeletal Reaver Minion`) as one key. PoB2 names a Spectre and a tamed
    beast by their monster (`Spectre: Quill Crab`, `Companion: Quadrilla`)."""
    lowered = name.lower()
    if lowered.startswith("spectre:"):
        return "spectre"
    if lowered.startswith("companion:"):
        return "beast"
    key = re.sub(r"[^a-z0-9]", "", lowered)
    key = re.sub(r"player$", "", key)
    key = re.sub(r"^(meta|summon|companion)", "", key)
    key = re.sub(r"minion$", "", key)
    return key[:-1] if key.endswith("s") else key


_GEM_NAMES = {}


def gem_names() -> dict:
    """The gem of each granted effect, by the gem's metadata name
    (`SkillGemCastOnCritical` → `CastOnCritical`)."""
    if not _GEM_NAMES:
        version = (REPO / "data/CURRENT").read_text(encoding="utf-8").strip()
        try:
            gems = json.loads((REPO / f"data/{version}/base/skill_gems.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            gems = []
        for gem in gems if isinstance(gems, list) else []:
            name = (gem.get("id") or "").rpartition("/")[2]
            name = re.sub(r"^SkillGem", "", name)
            for effect in [gem.get("granted_effect_id"), *(gem.get("additional_granted_effect_ids") or [])]:
                if effect and name:
                    _GEM_NAMES.setdefault(effect, name)
    return _GEM_NAMES


def reservation_free_skills(xml: str) -> set:
    """The skills the build's amulets of a reservation-free base grant."""
    try:
        bases = json.loads(RESERVATION_FREE.read_text(encoding="utf-8")).get("reservation_free_bases") or []
    except (OSError, ValueError):
        return set()
    found = set()
    for item in re.findall(r"<Item\b[^>]*>(.*?)</Item>", xml, re.S):
        if any(base in item for base in bases):
            found.update(skill_key(name) for name in
                         re.findall(r"Grants Skill: Level \d+ ([^\n<{]+)", item))
    return found


def spirit_leads(entries: dict, oracle: dict) -> list:
    """The Spirit total and each skill's Spirit reservation, where the two
    sides differ."""
    theirs, names = {}, {}
    for record in compare.as_list(oracle.get("spiritReservedBreakdown")):
        key = skill_key(record.get("skillName") or "?")
        theirs[key] = theirs.get(key, 0.0) + (compare.number(record.get("total")) or 0.0)
        names[key] = record.get("skillName") or key
    ours = {}
    for mod in ((entries.get(SKILL_RESERVATION) or {}).get("breakdown") or {}).get("mods") or []:
        skill = (mod.get("origin_id") or "").partition("spirit.")[2] or mod.get("origin_id") or "?"
        # The id's key, or its gem's when PoB2 names the skill as the gem.
        key = skill_key(skill)
        gem = gem_names().get(skill)
        if key not in theirs and gem and skill_key(gem) in theirs:
            key = skill_key(gem)
        ours[key] = ours.get(key, 0.0) + (compare.number(mod.get("value")) or 0.0)
        names.setdefault(key, skill)
    found = []
    total = compare.number((entries.get("Spirit") or {}).get("value"))
    pob2_total = compare.number(compare.as_dict(oracle.get("mainOutput")).get("Spirit"))
    if total is not None and pob2_total is not None and not compare.close(total, pob2_total):
        found.append(lead("Spirit", "total", "", "Spirit", pobr=total, pob2=pob2_total))
    for key in sorted(set(ours) | set(theirs)):
        mine, other = ours.get(key), theirs.get(key)
        if compare.close(mine or 0.0, other or 0.0):
            continue
        kind = "different" if mine is not None and other is not None else \
            ("only PoB2" if other is not None else "only PoBR")
        found.append(lead("Spirit reserved", kind, "", names[key], pobr=mine, pob2=other))
    return found


def known_reason(known: list, build: str, stat: str):
    for entry in known:
        if entry.get("build") and entry["build"] in build and stat in (entry.get("stats") or []):
            return entry.get("reason") or "known"
    return None


# What explains a row


def lead(scope: str, kind: str, mod_type: str, key: str, pobr=None, pob2=None,
         lines=(), inactive=()) -> dict:
    return {"scope": scope, "kind": kind, "type": mod_type, "key": key, "pobr": pobr,
            "pob2": pob2, "lines": list(lines)[:3], "inactive": list(inactive)[:3]}


def comparison_leads(comparison, scope: str) -> list:
    found = []
    for group in comparison.only_pob2:
        inactive = comparison.inactive.get((group.mod_type, group.key)) or []
        found.append(lead(scope, "inactive in PoBR" if inactive else "only PoB2", group.mod_type,
                          group.key, pob2=group.value, lines=[line.label for line in group.lines],
                          inactive=[line.label for line in inactive]))
    for group in comparison.only_pobr:
        found.append(lead(scope, "only PoBR", group.mod_type, group.key, pobr=group.value,
                          lines=[line.label for line in group.lines]))
    for mine, other in comparison.different:
        found.append(lead(scope, "different", mine.mod_type, mine.key, pobr=mine.value,
                          pob2=other.value,
                          lines=[line.label for line in mine.lines[:2] + other.lines[:2]]))
    return found


def enemy_leads(entries: dict, oracle: dict) -> list:
    """The enemy's resistance, penetration and damage taken per type, where the
    engine's DPS breakdown and PoB2 differ."""
    found = []
    theirs = compare.as_dict(oracle.get("enemyMitigation"))
    breakdown = (entries.get("TotalDPS") or {}).get("breakdown") or {}
    for row in compare.as_list(breakdown.get("enemy")):
        damage_type = row.get("damage_type")
        other = compare.as_dict(theirs.get(damage_type))
        if not other:
            continue
        ours = {
            "resistBase": compare.number(row.get("resist")),
            "pen": compare.number(row.get("penetration")),
            "takenInc": compare.number(row.get("taken_inc")),
            "takenMore": compare.number(row.get("taken_more")),
        }
        for field in ENEMY_FIELDS:
            mine, pob2 = ours.get(field), compare.number(other.get(field))
            if mine is not None and pob2 is not None and not compare.close(mine, pob2):
                found.append(lead("enemy", "enemy", field, damage_type, pobr=mine, pob2=pob2))
    return found


def damage_leads(entries: dict, oracle: dict, item_slots: dict, main_skill: str) -> list:
    found = []
    main_output = compare.as_dict(oracle.get("mainOutput"))
    # PoB2's average hit is after the enemy's mitigation; the engine's is before it.
    for factor in compare.DPS_FACTORS[1:]:
        entry = entries.get(factor) or {}
        ours = compare.number(entry.get("value"))
        theirs = compare.number(main_output.get(entry.get("pob_key") or factor))
        if ours is not None and theirs is not None and not compare.agrees(ours, theirs):
            found.append(lead("damage", "factor", "", factor, pobr=ours, pob2=theirs))
    found += enemy_leads(entries, oracle)
    table, _hand = compare.damage_table(entries)
    if table is None:
        return found
    shared_ours = compare.pobr_lines(table.get("shared_mods"), main_skill)
    shared_theirs = compare.pob2_lines(oracle, ("damageModList",), compare.SHARED_DAMAGE_NAMES,
                                       item_slots)
    found += comparison_leads(compare.compare(shared_ours, shared_theirs), "damage")
    for row in table.get("rows") or []:
        damage_type = row.get("damage_type") or "?"
        names = [damage_type + "Damage"] + (["ElementalDamage"] if damage_type in compare.ELEMENTS else [])
        ours = compare.pobr_lines(row.get("mods"), main_skill)
        theirs = compare.pob2_lines(oracle, ("damageModList",), names, item_slots)
        found += comparison_leads(compare.compare(ours, theirs), f"{damage_type} damage")
    return found


def stat_leads(stat: str, entries: dict, oracle: dict, item_slots: dict, main_skill: str) -> list:
    if stat == "SpiritUnreserved":
        return spirit_leads(entries, oracle)
    if stat in compare.DAMAGE_STATS:
        return damage_leads(entries, oracle, item_slots, main_skill)
    entry = entries.get(stat) or {}
    names = compare.pob2_names(stat, entry)
    if not names:
        return []
    ours = compare.pobr_lines((entry.get("breakdown") or {}).get("mods"), main_skill)
    theirs = compare.pob2_lines(oracle, compare.POB2_TABLES, names, item_slots)
    return comparison_leads(compare.compare(ours, theirs), stat)


def skill_levels(usage, oracle: dict) -> list:
    """The skills both sides calculate at another level or quality: a gem
    level changes what a buff, aura or curse gives without dropping an
    input, so no other lead names it. By skill id; a skill in several groups
    compares as the set of its levels, as the exporting tool may list a
    skill once that the build holds in several groups."""
    def by_skill(entries, key):
        found = {}
        for entry in entries:
            skill = entry.get(key)
            if skill:
                found.setdefault(skill, []).append((entry.get("level"), entry.get("quality")))
        return {skill: sorted(set(levels), key=str) for skill, levels in found.items()}
    ours = by_skill(compare.as_dict(usage).get("skills") or [], "skill_id")
    theirs = by_skill(compare.as_list(oracle.get("skillLevels")), "skillId")
    return [{"skill": skill, "pobr": ours[skill], "pob2": theirs[skill]}
            for skill in sorted(set(ours) & set(theirs)) if ours[skill] != theirs[skill]]


def levels_text(found: dict) -> str:
    side = lambda levels: ", ".join(f"{level}/q{quality}" for level, quality in levels)
    return f"{found['skill']} PoBR {side(found['pobr'])} · PoB2 {side(found['pob2'])}"


def pools(entries: dict, oracle: dict) -> str:
    main_output = compare.as_dict(oracle.get("mainOutput"))
    theirs = ", ".join(f"{label} {compare.num(compare.number(main_output.get(key)))}"
                       for label, key in POB2_POOLS if compare.number(main_output.get(key)))
    ours = ", ".join(f"{stat} {compare.num(compare.number(entries[stat].get('value')))}"
                     for stat in PoBR_POOLS
                     if compare.number((entries.get(stat) or {}).get("value")))
    return f"PoB2 {theirs or '—'}; PoBR {ours or '—'}"


def line_source(line: dict) -> str:
    """Where a line of the ledger is: its source id and holder."""
    source = line.get("source_id") or line.get("source_kind") or "?"
    return f"{source} ({line['holder']})" if line.get("holder") else source


def ledger_findings(usage) -> dict:
    """The inputs a build's ledger of lines (`calculate-build --usage`) says the
    calculation drops, and the modifier names it read."""
    found = {"unread": [], "unmodelled": [], "no_modifier": [], "conditions": [],
             "gem_stats": [], "reads": []}
    if not usage:
        return found
    for line in usage.get("lines") or []:
        status, text, where = line.get("status"), line.get("text") or "", line_source(line)
        if status in ("unmodelled", "no_modifier"):
            found[status].append({"text": text, "source": where, "reason": line.get("reason")})
        if status == "no_effect":  # such as a config option nothing in the build asks for
            continue
        config_option = (line.get("source_id") or "").startswith("config.")
        for modifier in line.get("modifiers") or []:
            name = modifier.get("name") or ""
            if config_option and name.startswith(("Condition:", "Multiplier:")):
                continue  # what a config option sets, with no effect on this build
            if modifier.get("status") == "unread":
                found["unread"].append({"name": modifier.get("name"), "text": text,
                                        "source": where})
    for condition in usage.get("conditions") or []:
        if condition.get("config_option") is None:
            modifiers = condition.get("modifiers") or [{}]
            found["conditions"].append({"condition": condition.get("condition"),
                                        "example": modifiers[0].get("source", "")})
    for stat in usage.get("gem_stats") or []:
        if stat.get("status") == "unmapped":
            found["gem_stats"].append({"owner": stat.get("owner"), "stat": stat.get("stat"),
                                       "reason": stat.get("reason")})
    found["reads"] = sorted((usage.get("reads") or {}).keys())
    return found


def analyse(name: str, corpus: str, report: dict, oracle: dict, xml: str, known: list) -> dict:
    """One build's differing rows, their classes and what explains them."""
    entries = {entry.get("stat"): entry for entry in report.get("breakdowns") or []}
    main_output = compare.as_dict(oracle.get("mainOutput"))
    reference = report.get("reference") or {}
    info = compare.as_dict(oracle.get("skillInfo"))
    skill = (report.get("main_skill") or {}).get("skill_id")
    main_skill = skill or info.get("grantedEffectId") or "main"
    item_slots = compare.item_slot_keys(xml)
    rows = []
    for row in reference.get("differing") or []:
        stat = row.get("stat")
        pobr, code = compare.number(row.get("pobr")), compare.number(row.get("reference"))
        now = compare.reference_value(oracle, row.get("pob_key") or "")
        reason = known_reason(known, name, stat)
        rows.append({"stat": stat, "pobr": pobr, "code": code, "pob2": now,
                     "class": "known" if reason else classify(pobr, code, now), "reason": reason})
    leads, seen, damage_done, pool_line = [], set(), False, None
    free = reservation_free_skills(xml)
    for row in rows:
        if row["class"] not in ACTIONABLE:
            continue
        stat = row["stat"]
        if stat in compare.DAMAGE_STATS:
            if damage_done:
                continue
            damage_done = True
        if POOL_STAT.search(stat or "") and pool_line is None:
            pool_line = pools(entries, oracle)
        found_leads = stat_leads(stat, entries, oracle, item_slots, main_skill)
        # PoB2 also reserves the skill a reservation-free amulet grants
        # (once more when the build has that gem too).
        if stat == "SpiritUnreserved" and found_leads and all(
                skill_key(found["key"]) in free and (found["pob2"] or 0) > (found["pobr"] or 0)
                for found in found_leads):
            row["class"], row["reason"] = "known", SPIRIT_REASON
            continue
        for found in found_leads:
            identity = (found["kind"], found["type"], found["key"],
                        round(found["pobr"] or 0, 2), round(found["pob2"] or 0, 2))
            if identity in seen:
                continue
            seen.add(identity)
            found["stat"] = stat
            leads.append(found)
    return {
        "build": name, "short": short(name), "set": corpus, "compared": reference.get("compared"),
        "rows": rows, "leads": leads, "pools": pool_line,
        "unsupported": report.get("unsupported_modifiers") or [],
        "ledger": ledger_findings(report.get("usage")),
        "missing_items": report.get("missing_items") or [],
        "skill_levels": skill_levels(report.get("usage"), oracle),
        "main_skill": skill, "pob2_main_skill": info.get("grantedEffectId"),
    }


# Running


def engine_binary() -> Path:
    """Builds the CLI once; every build then runs the binary directly."""
    result = subprocess.run(["cargo", "build", "-q", "-p", "pobr-cli"], cwd=REPO)
    if result.returncode != 0:
        raise SystemExit(f"triage: building the CLI failed ({result.returncode})")
    target = Path(os.environ.get("CARGO_TARGET_DIR") or "target")
    return (target if target.is_absolute() else REPO / target) / "debug" / "pobr"


def build_dirs(paths) -> list:
    """(set, build folder) for every build the paths name: a set of builds or one build."""
    found = []
    for raw in paths:
        path = Path(raw)
        path = path if path.is_absolute() else REPO / path
        if "holdout" in path.parts:
            raise SystemExit("triage: the holdout set is only measured (`mise run accuracy`); "
                             "triage the benchmark and tuned sets")
        if (path / "code.txt").is_file():
            found.append((str(path.parent.relative_to(REPO)), path))
        elif path.is_dir():
            found += [(raw, folder) for folder in sorted(path.iterdir())
                      if (folder / "code.txt").is_file()]
        else:
            raise SystemExit(f"triage: {raw}: no such set or build directory")
    return found


def triage_build(binary: Path, corpus: str, folder: Path, commit: str, refresh: bool,
                 known: list) -> dict:
    code_file = folder / "code.txt"
    code = code_file.read_text(encoding="utf-8")
    xml = compare.decode_code(code)
    command = [str(binary), "calculate-build", "--file", str(code_file), "--breakdown",
               f"all,{SKILL_RESERVATION}", "--json", "--reference-basis", "--usage"]
    result = subprocess.run(command, cwd=REPO, capture_output=True, text=True)
    if result.returncode != 0:
        tail = "\n".join(result.stderr.strip().splitlines()[-5:])
        raise RuntimeError(f"the engine failed ({result.returncode}): {tail}")
    report = json.loads(result.stdout)
    entries = {entry.get("stat"): entry for entry in report.get("breakdowns") or []}
    differing = [row.get("stat") for row in (report.get("reference") or {}).get("differing") or []]
    # The names `mise run compare` asks for by default, so both share one cached run.
    names = compare.oracle_names(differing, entries)
    cached = compare.oracle_cache_path(code, names, commit)
    if refresh or not cached.is_file():
        compare.run_oracle(xml, cached, names)
    oracle = json.loads(cached.read_text(encoding="utf-8"))
    return analyse(folder.name, corpus, report, oracle, xml, known)


def run_all(found: list, jobs: int, refresh: bool) -> tuple:
    binary = engine_binary()
    commit = compare.vendor_commit()
    known = load_known()
    results, failures = [], []
    print(f"triage: {len(found)} builds, {jobs} at a time", file=sys.stderr)
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = {pool.submit(triage_build, binary, corpus, folder, commit, refresh, known):
                   folder.name for corpus, folder in found}
        for done, future in enumerate(as_completed(futures), 1):
            name = futures[future]
            try:
                results.append(future.result())
            except (Exception, SystemExit) as error:  # one build's failure is reported, not fatal
                failures.append((name, str(error)))
            print(f"triage: {done}/{len(found)} {short(name)}", file=sys.stderr)
    results.sort(key=lambda result: (result["set"], result["build"]))
    return results, failures, commit


# Gathering across builds


def gather_leads(results: list) -> list:
    """The leads of the engine and both rows by kind, type and source, most builds first."""
    gathered = {}
    for result in results:
        for found in result["leads"]:
            key = (found["kind"], found["type"], found["key"])
            entry = gathered.setdefault(key, {"builds": {}, "example": found})
            entry["builds"].setdefault(result["short"], set()).add(found["stat"])
    return sorted(gathered.items(), key=lambda item: (
        -len(item[1]["builds"]), -sum(len(stats) for stats in item[1]["builds"].values()), item[0]))


def gather_unsupported(results: list) -> list:
    """The lines no rule parses by template; first those on builds with
    engine or both rows."""
    gathered = {}
    for result in results:
        actionable = any(row["class"] in ACTIONABLE for row in result["rows"])
        for text in result["unsupported"]:
            entry = gathered.setdefault(template(text), {"example": text, "builds": set(),
                                                         "actionable": set()})
            entry["builds"].add(result["short"])
            if actionable:
                entry["actionable"].add(result["short"])
    return sorted(gathered.items(), key=lambda item: (
        -len(item[1]["actionable"]), -len(item[1]["builds"]), item[0]))


def gather_lines(results: list, status: str) -> list:
    """The ledger's lines of `status` (`unmodelled`, `no_modifier`) by
    template; first those on builds with engine or both rows."""
    gathered = {}
    for result in results:
        actionable = any(row["class"] in ACTIONABLE for row in result["rows"])
        for line in result["ledger"][status]:
            entry = gathered.setdefault(template(line["text"]), {
                "example": line, "builds": set(), "actionable": set()})
            entry["builds"].add(result["short"])
            if actionable:
                entry["actionable"].add(result["short"])
    return sorted(gathered.items(), key=lambda item: (
        -len(item[1]["actionable"]), -len(item[1]["builds"]), item[0]))


def gather_unread(results: list) -> list:
    """Modifier names stored and never read, by name. First the names no
    triaged build reads, which have no reader at all; a name another build
    reads is one this build's skills have no use for."""
    read = set()
    for result in results:
        read.update(result["ledger"]["reads"])
    gathered = {}
    for result in results:
        for item in result["ledger"]["unread"]:
            entry = gathered.setdefault(item["name"], {
                "example": item, "builds": set(), "read_elsewhere": item["name"] in read})
            entry["builds"].add(result["short"])
    return sorted(gathered.items(), key=lambda item: (
        item[1]["read_elsewhere"], -len(item[1]["builds"]), item[0]))


def gather_conditions(results: list) -> list:
    """Conditions modifiers wait for that nothing sets, by condition."""
    gathered = {}
    for result in results:
        for item in result["ledger"]["conditions"]:
            entry = gathered.setdefault(item["condition"], {"example": item["example"],
                                                            "builds": set()})
            entry["builds"].add(result["short"])
    return sorted(gathered.items(), key=lambda item: (-len(item[1]["builds"]), item[0]))


def gather_gem_stats(results: list) -> list:
    """Gem stats with a stat map entry the engine rejects, by owner and stat."""
    gathered = {}
    for result in results:
        for item in result["ledger"]["gem_stats"]:
            entry = gathered.setdefault((item["owner"], item["stat"]), {
                "reason": item["reason"], "builds": set()})
            entry["builds"].add(result["short"])
    return sorted(gathered.items(), key=lambda item: (-len(item[1]["builds"]), item[0]))


def builds_text(builds: set) -> str:
    return ", ".join(sorted(builds)[:4]) + (" …" if len(builds) > 4 else "")


def gather_stats(results: list) -> list:
    gathered = {}
    for result in results:
        for row in result["rows"]:
            counts = gathered.setdefault(row["stat"], {})
            counts[row["class"]] = counts.get(row["class"], 0) + 1
    return sorted(gathered.items(), key=lambda item: (-sum(item[1].values()), item[0]))


# Report


def value(found: dict, side: str) -> str:
    amount = found.get(side)
    return "—" if amount is None else compare.num(amount)


def lead_text(found: dict) -> str:
    if found["kind"] in ("factor", "total"):
        return (f"{found['kind']} {found['key']}: PoBR {value(found, 'pobr')} · "
                f"PoB2 {value(found, 'pob2')}")
    if found["kind"] == "enemy":
        return (f"enemy {found['key']} {found['type']}: PoBR {value(found, 'pobr')} · "
                f"PoB2 {value(found, 'pob2')}")
    return (f"{found['kind']} {found['type']} {found['key']} ({found['scope']}): "
            f"PoBR {value(found, 'pobr')} · PoB2 {value(found, 'pob2')}")


def render(results: list, failures: list, commit: str, top: int) -> str:
    out = []
    sets = {}
    for result in results:
        sets[result["set"]] = sets.get(result["set"], 0) + 1
    classes = {name: 0 for name in CLASSES}
    compared = 0
    for result in results:
        compared += result["compared"] or 0
        for row in result["rows"]:
            classes[row["class"]] += 1
    differing = sum(classes.values())
    out.append(f"== triage: {len(results)} builds ("
               + ", ".join(f"{corpus} {count}" for corpus, count in sets.items())
               + f") · PoB2 vendor {commit[:10]}")
    out.append(f"rows that differ from the build codes' references: {differing} of {compared}: "
               + " · ".join(f"{name} {count}" for name, count in classes.items() if count))
    for name, error in failures:
        out.append(f"!! {short(name)}: {error}")

    out.append("\n== by build: engine and both rows (`mise run compare <build>` lists every line)")
    for result in results:
        counts = {}
        for row in result["rows"]:
            counts[row["class"]] = counts.get(row["class"], 0) + 1
        summary = ", ".join(f"{name} {counts[name]}" for name in CLASSES if counts.get(name))
        out.append(f"{result['short']}  ({len(result['rows'])} of {result['compared']} differ"
                   + (f": {summary}" if summary else "") + ")")
        if result["main_skill"] and result["pob2_main_skill"] and \
                result["main_skill"] != result["pob2_main_skill"]:
            out.append(f"   !! main skill: PoBR {result['main_skill']} · PoB2 {result['pob2_main_skill']}")
        for row in result["rows"]:
            if row["class"] in ACTIONABLE:
                out.append(f"   {row['stat']:<26} PoBR {compare.num(row['pobr']):>12} · code "
                           f"{compare.num(row['code']):>12} · PoB2 {compare.num(row['pob2']):>12}  "
                           f"{row['class']}")
        if result["pools"]:
            out.append(f"   pools: {result['pools']}")
        for found in result["skill_levels"][:4]:
            out.append(f"   level: {levels_text(found)}")
        if len(result["skill_levels"]) > 4:
            out.append(f"   level: … {len(result['skill_levels']) - 4} more")
        for found in result["leads"][:6]:
            out.append(f"   - {lead_text(found)}")
            for label in found["inactive"][:1]:
                out.append(f"       not applied in PoBR: {label}")
        if len(result["leads"]) > 6:
            out.append(f"   - … {len(result['leads']) - 6} more")

    out.append("\n== by stat: builds whose row differs, by class")
    for stat, counts in gather_stats(results):
        out.append(f"   {stat:<26} " + ", ".join(f"{name} {counts[name]}" for name in CLASSES
                                                 if counts.get(name)))

    out.append(f"\n== shared leads (engine and both rows), most builds first; top {top}")
    for (kind, mod_type, key), entry in gather_leads(results)[:top]:
        builds = "; ".join(f"{name}: {', '.join(sorted(stats))}"
                           for name, stats in sorted(entry["builds"].items()))
        out.append(f"   [{len(entry['builds'])}] {lead_text(entry['example'])}")
        out.append(f"       {builds}")
        for label in entry["example"]["lines"][:2]:
            out.append(f"       e.g. {label}")

    out.append(f"\n== lines no rule parses, by template (* = on a build with engine or both rows); "
               f"top {top}")
    for _template, entry in gather_unsupported(results)[:top]:
        mark = "*" if entry["actionable"] else " "
        names = ", ".join(sorted(entry["builds"])[:4]) + (" …" if len(entry["builds"]) > 4 else "")
        out.append(f" {mark} [{len(entry['builds'])}] {entry['example'][:150]}")
        out.append(f"       {names}")

    for status, title in (
            ("unmodelled", "lines a rule recognises and nothing models"),
            ("no_modifier", "lines parsed yet storing nothing (the ledger does not know why)")):
        gathered = gather_lines(results, status)
        if not gathered:
            continue
        out.append(f"\n== {title}, by template (* = on a build with engine or both rows); "
                   f"top {top}")
        for _template, entry in gathered[:top]:
            mark = "*" if entry["actionable"] else " "
            example = entry["example"]
            out.append(f" {mark} [{len(entry['builds'])}] {example['text'][:150]}")
            out.append(f"       {example['source']} · {example['reason']}")
            out.append(f"       {builds_text(entry['builds'])}")

    out.append(f"\n== modifiers stored and never read, by name: first those no triaged build "
               f"reads; top {top}")
    for name, entry in gather_unread(results)[:top]:
        reader = "read in another build" if entry["read_elsewhere"] else "no build reads it"
        example = entry["example"]
        out.append(f"   [{len(entry['builds'])}] {name} ({reader})")
        out.append(f"       e.g. {example['source']} | {example['text'][:120]}")

    out.append(f"\n== conditions modifiers wait for that nothing sets; top {top}")
    for condition, entry in gather_conditions(results)[:top]:
        out.append(f"   [{len(entry['builds'])}] {condition}")
        out.append(f"       e.g. {entry['example'][:150]}")

    out.append(f"\n== gem stats the engine rejects; top {top}")
    for (owner, stat), entry in gather_gem_stats(results)[:top]:
        out.append(f"   [{len(entry['builds'])}] {owner} · {stat}: {entry['reason']}")
    return "\n".join(out) + "\n"


def to_json(value):
    if isinstance(value, set):
        return sorted(value)
    raise TypeError(type(value).__name__)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="mise run triage",
        description="Every worked build against its reference and the pinned PoB2 at once: "
                    "which differing rows are the engine's, and what explains them across builds.")
    parser.add_argument("paths", nargs="*", default=list(DEFAULT_SETS),
                        help="build sets or build directories (default: the benchmark and tuned sets)")
    parser.add_argument("--jobs", type=int, default=min(8, os.cpu_count() or 4),
                        help="builds calculated at a time")
    parser.add_argument("--refresh", action="store_true",
                        help="recalculate the PoB2 side even when it is cached")
    parser.add_argument("--top", type=int, default=30, help="entries listed per gathered section")
    parser.add_argument("--json", type=Path, help="also write everything found to this file")
    args = parser.parse_args(argv)

    found = build_dirs(args.paths)
    if not found:
        raise SystemExit("triage: no builds found")
    results, failures, commit = run_all(found, max(1, args.jobs), args.refresh)
    if args.json:
        payload = {"results": results, "failures": failures,
                   "leads": [{"kind": key[0], "type": key[1], "key": key[2], **entry}
                             for key, entry in gather_leads(results)],
                   "unsupported": [{"template": key, **entry}
                                   for key, entry in gather_unsupported(results)],
                   "unmodelled": [{"template": key, **entry}
                                  for key, entry in gather_lines(results, "unmodelled")],
                   "no_modifier": [{"template": key, **entry}
                                   for key, entry in gather_lines(results, "no_modifier")],
                   "unread": [{"name": key, **entry} for key, entry in gather_unread(results)],
                   "conditions": [{"condition": key, **entry}
                                  for key, entry in gather_conditions(results)],
                   "gem_stats": [{"owner": key[0], "stat": key[1], **entry}
                                 for key, entry in gather_gem_stats(results)]}
        args.json.write_text(json.dumps(payload, indent=1, default=to_json), encoding="utf-8")
    sys.stdout.write(render(results, failures, commit, args.top))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
