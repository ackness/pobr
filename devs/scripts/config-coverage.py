#!/usr/bin/env python3
"""Audit config consumers using the same worked-build ledgers as gaps/triage.

Reader coverage is not proof that an option changes a numeric result. Static
references are research leads, never promoted to observed modifier reads.
"""
import argparse
from collections import defaultdict
import importlib.util
import json
from pathlib import Path
import re
import sys

import config_relevance

REPO = Path(__file__).resolve().parents[2]
_SPEC = importlib.util.spec_from_file_location("gaps", REPO / "devs/scripts/gaps.py")
gaps = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(gaps)
CLASSES = ("read", "handler", "implied", "unread", "no_effect", "not_in_game")


def production(text):
    """Exclude Rust test modules and comments from static evidence."""
    text = text.split("#[cfg(test)]")[0]
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)


def strings(text):
    return re.findall(r'"([^"\n]*)"', text)


def constant(text, name):
    match = re.search(r'\b' + name + r'\b[^=]*=\s*&?\[(.*?)\];', text, re.S)
    return set(strings(match[1])) if match else set()


def registrations(text):
    """The registry's literal registrations and its literal for-loop tables."""
    text = production(text)
    result = set(re.findall(r'\.register\(\s*"([^"]+)"', text))
    for match in re.finditer(r'for\s+.*?\s+in\s+\[(.*?)\]\s*\{(.*?\.register\(\s*\w+\s*,)', text, re.S):
        result.update(s for s in strings(match[1]) if s.startswith("config:"))
    return result


def config_tag_reads(value):
    """Catalog effects produce names, but their tags consume variables.

    For example, ShockVal reads ShockedConfig and the Corrosion Armour effect
    reads CorrosionStack. Do not count the surrounding producer or visibility.
    """
    if isinstance(value, dict):
        for tag in value.get("tags") or []:
            if tag.get("type") in ("condition", "actor_condition", "multiplier"):
                if isinstance(tag.get("var"), str):
                    yield tag["var"]
        for item in value.values():
            yield from config_tag_reads(item)
    elif isinstance(value, list):
        for item in value:
            yield from config_tag_reads(item)


def source_index(root, directory):
    """Exact string-token references, with producer data kept separate.

    A textual occurrence is not necessarily a consumer. Conversely, absence is
    a search lead: computed names may escape literal searches (runtime reads do
    not). Exclude this catalog itself, historical snapshots and test fixtures.
    """
    index = {"engine": defaultdict(set), "data": defaultdict(set)}
    handlers, stubs, gated, pass_conditions = set(), set(), set(), set()
    for crate in (root / "crates").glob("*/src"):
        for path in sorted(crate.rglob("*.rs")):
            if path.stem == "tests" or "tests" in path.parts:
                continue
            text = production(path.read_text())
            relative = str(path.relative_to(root))
            for name in strings(text):
                index["engine"][name].add(relative)
            # Computed modifier names are search leads too: ailment_apply uses
            # format!("{}Val", spec.name) with literal Chill/Shock specifications.
            # This never promotes a static reference to an observed read.
            literals = strings(text)
            for template in re.findall(r'format!\(\s*"([^"\n]*\{\}[^"\n]*)"', text):
                if template.count("{}") == 1:
                    for name in literals:
                        if re.fullmatch(r"[A-Za-z]+", name):
                            index["engine"][template.replace("{}", name)].add(relative)
            if path.name == "handlers.rs" or "handlers" in path.parts:
                handlers.update(registrations(text))
                stubs.update(constant(text, "STUB_HANDLER_IDS"))
                gated.update(constant(text, "CTX_GATED_HANDLER_IDS"))
            pass_conditions.update(constant(text, "CONDITIONS_SET_BY_A_PASS"))
    for base in (directory, directory.parent / "overlay-common"):
        for path in sorted(base.rglob("*.json")):
            if path.name.endswith("config_options.json"):
                for name in config_tag_reads(json.loads(path.read_text())):
                    index["data"][name].add(str(path.relative_to(root)))
                continue
            for name in set(strings(path.read_text())):
                index["data"][name].add(str(path.relative_to(root)))
    return index, handlers, stubs, gated, pass_conditions


def read_union(builds):
    ledgers = [build.get("usage") or {} for build in builds]
    reads = gaps.triage.reader_union(ledgers)
    conditions, multipliers, skills = set(), set(), set()
    for ledger in ledgers:
        conditions.update(ledger.get("condition_reads") or [])
        multipliers.update(ledger.get("multiplier_reads") or [])
        # The engine's existing referenced_variables deliberately combines tags.
        conditions.update(ledger.get("referenced_variables") or [])
        multipliers.update(ledger.get("referenced_variables") or [])
        conditions.update(ledger.get("condition_configs") or {})
        conditions.update(ledger.get("condition_states") or [])
        conditions.update(row["condition"] for row in ledger.get("conditions") or [])
        multipliers.update(row["multiplier"] for row in ledger.get("multipliers") or [])
        skills.update(row["skill_id"] for row in ledger.get("skills") or [])
        # Backward-compatible evidence from older ledgers. Only variable modifiers
        # consumed outside ModDB count; an inactive numeric mod is not a read.
        for line in ledger.get("lines") or []:
            for mod in line.get("modifiers") or []:
                if mod.get("status") not in ("applied", "outside"):
                    continue
                name = mod.get("name", "")
                if name.startswith("Condition:"):
                    conditions.add(name.removeprefix("Condition:"))
                elif name.startswith("Multiplier:"):
                    multipliers.add(name.removeprefix("Multiplier:"))
    return reads, conditions, multipliers, skills


def effect_names(option):
    effects = list(option.get("effects") or [])
    effects += [effect for group in (option.get("option_effects") or {}).values() for effect in group]
    result = set()
    for effect in effects:
        target, name = effect.get("target", "player"), effect["name"]
        payload = effect.get("list_value") or {}
        if payload.get("kind") == "nested_mod":
            target = {"MinionModifier": "minion", "EnemyModifier": "enemy",
                      "PlayerModifier": "player"}.get(name, target)
            name = payload["mod"]["name"]
        elif payload.get("kind") == "key_value":
            name = "SkillData:" + payload["key"]
        result.add((target, name))
    return sorted(result)


def classify(options, builds, index, handlers, stubs=(), gated=(), pass_conditions=(), skill_names=None,
             relevance=None):
    reads, conditions, multipliers, skills = read_union(builds)
    conditions.update(pass_conditions)
    skills.update((skill_names or {}).get(skill, skill) for skill in list(skills))
    rows = []
    for option in options:
        required = effect_names(option)
        implied = option.get("imply_conditions") or []
        handler = option.get("handler_id")
        missing, evidence = [], []
        for target, name in sorted(set(required + [("player", "Condition:" + c) for c in implied])):
            variable = name.partition(":")[2]
            candidates = {name}
            if name.startswith("Condition:"):
                variables = {variable, "Enemy" + variable} if target == "enemy" else {variable}
                read = name in reads or bool(variables & conditions)
                candidates.update(variables)
            elif name.startswith("Multiplier:"):
                read = name in reads or variable in multipliers
                candidates.add(variable)
            else:
                read = name in reads
                if name.startswith("SkillData:"):
                    candidates.add(variable)
                    candidates.add(option["var"])
            if read:
                continue
            refs = {kind: sorted(set().union(*(names.get(n, set()) for n in candidates)))
                    for kind, names in index.items()}
            absent = [kind for kind, paths in refs.items() if not paths]
            item = {"target": target, "name": name, "references": refs, "absent_from": absent}
            missing.append(item)
        if handler:
            category = "handler" if handler in handlers else "unread"
            evidence.append("registered handler; registration does not prove an effect" if handler in handlers
                            else "unregistered handler")
        elif not required and not implied:
            category = "no_effect"
        elif missing:
            category = "unread"
        else:
            category = "read" if required else "implied"
        unworked = option.get("visibility", {}).get("if_skill") or []
        unworked = sorted(unworked) if unworked and not (set(unworked) & skills) else []
        # Keep both facets: a skill absent from worked builds may also lack a reader.
        no_reader = sorted({item["name"] for item in missing
                            if not any(item["references"].values())})
        row = {"var": option["var"], "section": option["section"], "class": category,
               "label": option.get("label"), "missing": missing,
               "unworked_skills": unworked, "no_reader_anywhere": no_reader,
               "unread_kind": ("unworked_skill" if unworked else "no_reader_anywhere" if no_reader
                               else "unobserved_reader") if category == "unread" else None}
        if handler:
            row.update(handler_id=handler, registered=handler in handlers,
                       stub=handler in stubs, context_gated=handler in gated)
        if category == "no_effect":
            # Empty effects do not mean no calculation: scalar consumers are explicit
            # strings outside the catalog. Preserve these as a third, honest subtype.
            refs = sorted(index["engine"].get(option["var"], set()))
            row.update(no_effect_kind="scalar_consumer" if refs else "lost_mechanic",
                       references=refs, explanation=option.get("tooltip") or option.get("label"))
        if option.get("input_type") == "text":
            evidence.append("interpreter text lane bypasses handler dispatch; custom lines are parsed separately")
        if evidence:
            row["notes"] = evidence
        if relevance is not None:
            row["game_evidence"] = relevance[option["var"]]
            if row["game_evidence"]["class"] == "not_in_game":
                row["reader_class"] = category
                row["class"] = "not_in_game"
                row["unread_kind"] = None
        rows.append(row)
    counts = lambda items: {kind: sum(row["class"] == kind for row in items) for kind in CLASSES}
    relevant = [row for row in rows if row["class"] != "not_in_game"]
    covered = sum(row["class"] in ("read", "handler") for row in relevant)
    return {"counts": counts(rows),
            "relevant_coverage": {"covered": covered, "total": len(relevant),
                                  "percent": 100 * covered / len(relevant) if relevant else 0},
            "unread_counts": {kind: sum(r["unread_kind"] == kind for r in rows)
                              for kind in ("unworked_skill", "no_reader_anywhere", "unobserved_reader")},
            "no_effect_counts": {kind: sum(r.get("no_effect_kind") == kind for r in rows)
                                 for kind in ("display_only", "scalar_consumer", "lost_mechanic")},
            "sections": {section: counts([r for r in rows if r["section"] == section])
                         for section in sorted({r["section"] for r in rows})},
            "unread": [r["var"] for r in rows if r["class"] == "unread"],
            "no_reader_anywhere": [{"var": r["var"], "names": r["no_reader_anywhere"]}
                                   for r in relevant if r["no_reader_anywhere"]],
            "unregistered_handlers": [{"var": r["var"], "handler_id": r["handler_id"]}
                                      for r in rows if r.get("registered") is False],
            "options": rows}


def render(report):
    coverage = report["relevant_coverage"]
    lines = ["Config reader coverage (not numeric-effect validation)",
             f"Worked builds: {report.get('build_count', 0)}; options: {len(report['options'])}",
             f"PoE2-relevant read+handler: {coverage['covered']}/{coverage['total']} ({coverage['percent']:.1f}%)",
             "Relevance rule: " + config_relevance.RULE,
             "Section | " + " | ".join(CLASSES)]
    for section, counts in [("TOTAL", report["counts"]), *report["sections"].items()]:
        lines.append(section + " | " + " | ".join(str(counts[c]) for c in CLASSES))
    lines.append("Unread split: " + json.dumps(report["unread_counts"], sort_keys=True))
    lines.append("No-effect split: " + json.dumps(report["no_effect_counts"], sort_keys=True))
    for heading, rows in [
        ("Unread options", [r for r in report["options"] if r["class"] == "unread"]),
        ("No reader anywhere (absent in engine and data; computed names need review)",
         [r for r in report["options"] if r["class"] != "not_in_game" and r["no_reader_anywhere"]]),
        ("Not in the game snapshot", [r for r in report["options"] if r["class"] == "not_in_game"]),
        ("Unregistered handlers", [r for r in report["options"] if r.get("registered") is False]),
        ("No declarative effect", [r for r in report["options"] if r["class"] == "no_effect"]),
        ("Registered stubs / context gates", [r for r in report["options"] if r.get("stub") or r.get("context_gated")]),
    ]:
        lines.append(f"\n{heading} ({len(rows)}):")
        for row in rows:
            detail = ", ".join(item["name"] for item in row["missing"])
            if row.get("registered") is False:
                detail += " " + row["handler_id"]
            if row["unworked_skills"]:
                detail += "; no worked skill: " + ", ".join(row["unworked_skills"])
            if row.get("no_effect_kind"):
                detail = row["no_effect_kind"] + ": " + (row.get("explanation") or "")
            if row.get("stub") or row.get("context_gated"):
                detail += "; " + ("stub" if row.get("stub") else "context-gated")
            if row["class"] == "not_in_game":
                detail = row["game_evidence"]["reason"] + ": " + ", ".join(row["game_evidence"]["queries"])
            lines.append(f"  {row['var']}: {detail}")
            for item in row["missing"]:
                if item["name"] in row["no_reader_anywhere"]:
                    lines.append("    " + item["name"] + ": absent from " + ", ".join(item["absent_from"]))
            for note in row.get("notes", []):
                lines.append("    " + note)
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", type=Path, help="save all options and evidence as JSON (use - for stdout)")
    parser.add_argument("--binary", type=Path, help="reuse an already built CLI")
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args(argv)
    directory = gaps.lookup.data_dir()
    catalog = json.loads((directory / "overlay/config_options.json").read_text())
    binary = args.binary.resolve() if args.binary else gaps.triage.engine_binary()
    builds, failures = gaps.collect_usage(binary, args.refresh)
    if failures:
        print(json.dumps(failures, indent=2), file=sys.stderr)
        return 1  # A partial union would make unsupported coverage claims.
    index, handlers, stubs, gated, conditions = source_index(REPO, directory)
    names = {row["id"]: row.get("displayed_name", row["id"])
             for row in json.loads((directory / "base/granted_effects.json").read_text())}
    options = config_relevance.catalog_options(directory)
    report = classify(options, builds, index, handlers, stubs, gated, conditions, names,
                      config_relevance.relevance(directory, options))
    report.update(build_count=len(builds), sets=list(gaps.triage.DEFAULT_SETS),
                  data_version=directory.name, generator=catalog.get("_meta", {}).get("generator"),
                  search_scope={"engine": "crates/*/src, without tests/comments",
                                "data": "active snapshot + overlay-common; config catalogs contribute only tag consumers"})
    if args.json:
        payload = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
        if str(args.json) == "-":
            print(payload, end="")
        else:
            args.json.parent.mkdir(parents=True, exist_ok=True)
            args.json.write_text(payload)
    if str(args.json) != "-":
        print(render(report))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
