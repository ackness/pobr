#!/usr/bin/env python3
"""Report game-text translation coverage of the prepared data, per language.

For every language directory in `data/<version>/i18n/`, compare its tables with the
English game data of the same snapshot: names per table, stat line templates and
native display texts. Missing translations never fail the report (players see English
there); only unreadable data or a broken script does. CI prints it in the job summary.
"""

import argparse
import json
from pathlib import Path
import sys

REPO = Path(__file__).resolve().parents[2]


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def english_references(version_dir):
    """What each table should translate, from the snapshot's English data."""
    bases = load(version_dir / "base/base_items.json")
    gems = load(version_dir / "base/skill_gems.json")
    effects = {effect["id"]: effect for effect in load(version_dir / "base/granted_effects.json")}
    mods = load(version_dir / "base/mods.json")
    tree = load(version_dir / "base/passive_tree.json")
    meta = load(version_dir / "base/passive_tree_meta.json")
    uniques_path = version_dir / "overlay/uniques.json"
    uniques = load(uniques_path)["uniques"] if uniques_path.exists() else []
    skills = set()
    for gem in gems:
        effect = effects.get(gem.get("granted_effect_id"))
        if not gem.get("is_support") and effect and effect.get("active_skill"):
            skills.add(effect["active_skill"])
    return {
        "base_items.json": ("base item ids", {base["id"] for base in bases}, set()),
        "skills.json": ("active skill ids of skill gems", skills, set()),
        # The engine reads affix names by English name; a table keyed by mod id is reported as such.
        "mods.json": ("affix names", {mod["name"] for mod in mods if mod.get("name")},
                      {mod["id"] for mod in mods if mod.get("name")}),
        "passive_names.json": ("passive node names", {node["name"] for node in tree if node.get("name")}, set()),
        "words.json": ("unique item names", {unique["name"] for unique in uniques if unique.get("name")}, set()),
        "rare_words.json": (None, set(), set()),
        "classes.json": ("class and ascendancy names", {cls["name"] for cls in meta["classes"]}
                         | {asc["name"] for cls in meta["classes"] for asc in cls.get("ascendancies", [])}, set()),
    }


def stat_templates(version_dir):
    """Every English stat description template of the snapshot (all scopes)."""
    templates = set()
    def walk(value):
        if isinstance(value, dict):
            for key, inner in value.items():
                if key == "template" and isinstance(inner, str):
                    if inner.strip():
                        templates.add(inner)
                else:
                    walk(inner)
        elif isinstance(value, list):
            for inner in value:
                walk(inner)
    walk(load(version_dir / "overlay/stat_descriptions.json").get("scopes", {}))
    return templates


def unused(text):
    """Game rows marked do-not-translate (`[DNT-UNUSED] ...`) never reach players."""
    return isinstance(text, str) and text.startswith("[DNT")


def row(table, reference, keys, translated, extra_keys=0, examples=()):
    covered = len(reference & translated) if reference else len(translated)
    return {
        "table": table, "reference": len(reference) if reference else None,
        "translated": covered, "missing": len(reference - translated) if reference else None,
        "extra": extra_keys, "keys": keys, "examples": sorted(examples),
    }


def language_report(version_dir, lang, references, templates, examples):
    directory = version_dir / "i18n" / lang
    meta_path = directory / "_meta.json"
    meta = load(meta_path) if meta_path.exists() else {}
    fallback = meta.get("fallback", []) if isinstance(meta, dict) else []
    rows = []
    for file, (label, reference, ids) in references.items():
        path = directory / file
        if not path.exists():
            rows.append({**row(file, reference, label or "no English reference", set()), "absent": True,
                         "examples": sorted(reference)[:examples]})
            continue
        table = load(path)
        if file == "classes.json":
            translated = {key for part in ("classes", "ascendancies")
                          for key, value in table.get(part, {}).items() if value}
        else:
            translated = {key for key, value in table.items() if isinstance(value, str) and value.strip()}
        keys = label
        if ids and len(translated & ids) > len(translated & reference):
            # Keyed by mod id: count it against the ids, and say the engine cannot use it by name.
            reference, keys = ids, "mod ids (the engine looks affixes up by English name)"
        missing = sorted(reference - translated)[:examples] if reference else []
        rows.append(row(file, reference, keys or "no English reference", translated,
                        len(translated - reference) if reference else 0, missing))
    lines_path = directory / "stat_lines.json"
    if lines_path.exists():
        english = {entry["en"] for entry in load(lines_path) if isinstance(entry, dict) and entry.get("en")}
        rows.append(row("stat_lines.json", templates, "stat description templates (exact text)", english,
                        len(english - templates), sorted(templates - english)[:examples]))
    else:
        rows.append({**row("stat_lines.json", templates, "stat description templates (exact text)", set()),
                     "absent": True, "examples": sorted(templates)[:examples]})
    display_path = directory / "display_texts.json"
    if display_path.exists():
        display = load(display_path)
        entries = {key: value for key, value in display.get("entries", {}).items() if not unused(key)}
        rows.append(row("display_texts.json entries", set(entries), "English display texts",
                        {key for key, value in entries.items() if value},
                        examples=sorted(key for key, value in entries.items() if not value)[:examples]))
        for scope, values in sorted(display.get("scopes", {}).items()):
            values = {key: pair for key, pair in values.items() if not unused(pair[0])}
            rows.append(row(f"display_texts.json {scope}", set(values), "ids",
                            {key for key, pair in values.items() if pair[1]},
                            examples=sorted(pair[0] or key for key, pair in values.items() if not pair[1])[:examples]))
    else:
        rows.append({**row("display_texts.json", set(), "native display texts", set()), "absent": True})
    return {"lang": lang, "fallback": [*fallback, "en"], "rows": rows}


def report(data_root, version=None, examples=5):
    version = version or (data_root / "CURRENT").read_text(encoding="utf-8").strip()
    version_dir = data_root / version
    i18n = version_dir / "i18n"
    if not i18n.is_dir():
        raise SystemExit(f"No prepared language data at {i18n}; run `mise run data:prepare` first")
    references = english_references(version_dir)
    templates = stat_templates(version_dir)
    languages = sorted(path.name for path in i18n.iterdir() if path.is_dir())
    return {"version": version,
            "languages": [language_report(version_dir, lang, references, templates, examples) for lang in languages]}


def percent(part, whole):
    return "—" if not whole else f"{100 * part / whole:.1f}%"


def markdown(result):
    out = [f"### Game-text translation coverage ({result['version']})", "",
           "Report only: a missing translation shows English to players and never fails CI. "
           "`Extra` counts table keys outside the compared English set (other game text or "
           "entries for content this snapshot no longer has). `[DNT-UNUSED]` rows are not counted.", ""]
    for language in result["languages"]:
        chain = " → ".join(language["fallback"])
        out += [f"#### {language['lang']} (falls back to {chain})", "",
                "| Table | Compared with | English | Translated | Coverage | Missing | Extra |",
                "| --- | --- | ---: | ---: | ---: | ---: | ---: |"]
        details = []
        for item in language["rows"]:
            if item.get("absent"):
                out.append(f"| {item['table']} | {item['keys']} | {item['reference'] or '—'} | absent | "
                           f"{percent(0, item['reference'])} | {item['reference'] or '—'} | — |")
            else:
                reference = item["reference"]
                out.append(f"| {item['table']} | {item['keys']} | {reference if reference is not None else '—'} | "
                           f"{item['translated']} | {percent(item['translated'], reference)} | "
                           f"{item['missing'] if item['missing'] is not None else '—'} | {item['extra']} |")
            if item["examples"]:
                shown = ", ".join("`" + text.replace("\n", " ").replace("`", "'")[:80] + "`" for text in item["examples"])
                details.append(f"- {item['table']}: {shown}")
        if details:
            out += ["", "<details><summary>Examples of missing entries</summary>", "", *details, "", "</details>"]
        out.append("")
    return "\n".join(out)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, default=REPO / "data", help="data root holding CURRENT (default: repo data/)")
    parser.add_argument("--version", help="data version (default: <data>/CURRENT)")
    parser.add_argument("--examples", type=int, default=5, help="missing entries listed per table (default 5)")
    parser.add_argument("--json", type=Path, help="also write the report as JSON to this path")
    args = parser.parse_args(argv)
    result = report(args.data, args.version, max(args.examples, 0))
    if args.json:
        args.json.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    sys.stdout.write(markdown(result) + "\n")


if __name__ == "__main__":
    main()
