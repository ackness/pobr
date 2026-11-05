#!/usr/bin/env python3
"""Report game-text translation coverage of the prepared data, per language.

For every language directory in `data/<version>/i18n/`, compare its tables with the
English game data of the same snapshot: names per table, stat line templates and
native display texts. It is a report, never a gate: missing translations show English to
players, and a table, language or snapshot that cannot be read is printed as
`unreadable: <reason>`; the script still exits 0. CI prints it in the job summary.
"""

import argparse
import json
from pathlib import Path
import sys

REPO = Path(__file__).resolve().parents[2]


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def reason(error):
    """One line saying why something could not be read."""
    text = " ".join(f"{type(error).__name__}: {error}".split())
    return text if len(text) <= 200 else text[:197] + "..."


class Unreadable(Exception):
    pass


def attempt(build):
    """The value `build` returns, or `Unreadable` with the reason it failed."""
    try:
        return build()
    except Exception as error:  # A report: any failure is shown, never raised.
        return Unreadable(reason(error))


def _skills(version_dir):
    effects = {effect["id"]: effect for effect in load(version_dir / "base/granted_effects.json")}
    skills = set()
    for gem in load(version_dir / "base/skill_gems.json"):
        effect = effects.get(gem.get("granted_effect_id"))
        if not gem.get("is_support") and effect and effect.get("active_skill"):
            skills.add(effect["active_skill"])
    return skills


def _uniques(version_dir):
    path = version_dir / "overlay/uniques.json"
    uniques = load(path)["uniques"] if path.exists() else []
    return {unique["name"] for unique in uniques if unique.get("name")}


def _classes(version_dir):
    classes = load(version_dir / "base/passive_tree_meta.json")["classes"]
    return {cls["name"] for cls in classes} | {asc["name"] for cls in classes for asc in cls.get("ascendancies", [])}


def english_references(version_dir):
    """What each table should translate, from the snapshot's English data.

    Each entry is `(label, reference, ids)`, or `Unreadable` when its English data cannot be read.
    """
    def mods():
        named = [mod for mod in load(version_dir / "base/mods.json") if mod.get("name")]
        # The engine reads affix names by English name; a table keyed by mod id is reported as such.
        return ("affix names", {mod["name"] for mod in named}, {mod["id"] for mod in named})
    builders = {
        "base_items.json": lambda: ("base item ids", {base["id"] for base in load(version_dir / "base/base_items.json")}, set()),
        "skills.json": lambda: ("active skill ids of skill gems", _skills(version_dir), set()),
        "mods.json": mods,
        "passive_names.json": lambda: ("passive node names", {node["name"] for node in load(version_dir / "base/passive_tree.json")
                                                               if node.get("name")}, set()),
        "words.json": lambda: ("unique item names", _uniques(version_dir), set()),
        "rare_words.json": lambda: (None, set(), set()),
        "classes.json": lambda: ("class and ascendancy names", _classes(version_dir), set()),
    }
    return {file: attempt(build) for file, build in builders.items()}


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


def unreadable(table, why):
    return {"table": table, "unreadable": why, "examples": []}


def language_report(version_dir, lang, references, templates, examples):
    directory = version_dir / "i18n" / lang
    meta_path = directory / "_meta.json"
    meta = attempt(lambda: load(meta_path) if meta_path.exists() else {})
    fallback = meta.get("fallback", []) if isinstance(meta, dict) else []
    rows = [unreadable("_meta.json", str(meta))] if isinstance(meta, Unreadable) else []
    for file, reference in references.items():
        if isinstance(reference, Unreadable):
            rows.append(unreadable(file, f"English data: {reference}"))
            continue
        result = attempt(lambda: name_row(directory, file, *reference, examples))
        rows.append(unreadable(file, str(result)) if isinstance(result, Unreadable) else result)
    if isinstance(templates, Unreadable):
        rows.append(unreadable("stat_lines.json", f"English data: {templates}"))
    else:
        result = attempt(lambda: lines_row(directory, templates, examples))
        rows.append(unreadable("stat_lines.json", str(result)) if isinstance(result, Unreadable) else result)
    result = attempt(lambda: display_rows(directory, examples))
    rows.extend([unreadable("display_texts.json", str(result))] if isinstance(result, Unreadable) else result)
    return {"lang": lang, "fallback": [*fallback, "en"], "rows": rows}


def name_row(directory, file, label, reference, ids, examples):
    path = directory / file
    if not path.exists():
        return {**row(file, reference, label or "no English reference", set()), "absent": True,
                "examples": sorted(reference)[:examples]}
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
    return row(file, reference, keys or "no English reference", translated,
               len(translated - reference) if reference else 0, missing)


def lines_row(directory, templates, examples):
    path = directory / "stat_lines.json"
    if not path.exists():
        return {**row("stat_lines.json", templates, "stat description templates (exact text)", set()),
                "absent": True, "examples": sorted(templates)[:examples]}
    english = {entry["en"] for entry in load(path) if isinstance(entry, dict) and entry.get("en")}
    return row("stat_lines.json", templates, "stat description templates (exact text)", english,
               len(english - templates), sorted(templates - english)[:examples])


def display_rows(directory, examples):
    path = directory / "display_texts.json"
    if not path.exists():
        return [{**row("display_texts.json", set(), "native display texts", set()), "absent": True}]
    display = load(path)
    entries = {key: value for key, value in display.get("entries", {}).items() if not unused(key)}
    rows = [row("display_texts.json entries", set(entries), "English display texts",
                {key for key, value in entries.items() if value},
                examples=sorted(key for key, value in entries.items() if not value)[:examples])]
    for scope, values in sorted(display.get("scopes", {}).items()):
        values = {key: pair for key, pair in values.items() if not unused(pair[0])}
        rows.append(row(f"display_texts.json {scope}", set(values), "ids",
                        {key for key, pair in values.items() if pair[1]},
                        examples=sorted(pair[0] or key for key, pair in values.items() if not pair[1])[:examples]))
    return rows


def report(data_root, version=None, examples=5):
    """The coverage of every language; `unreadable` set when the snapshot itself cannot be read."""
    try:
        version = version or (data_root / "CURRENT").read_text(encoding="utf-8").strip()
        i18n = data_root / version / "i18n"
        if not i18n.is_dir():
            raise Unreadable(f"no prepared language data at {i18n}; run `mise run data:prepare` first")
        languages = sorted(path.name for path in i18n.iterdir() if path.is_dir())
    except Exception as error:
        return {"version": version or "unknown", "unreadable": str(error) if isinstance(error, Unreadable) else reason(error),
                "languages": []}
    version_dir = data_root / version
    references = english_references(version_dir)
    templates = attempt(lambda: stat_templates(version_dir))
    reports = []
    for lang in languages:
        result = attempt(lambda: language_report(version_dir, lang, references, templates, examples))
        reports.append({"lang": lang, "fallback": ["en"], "rows": [unreadable("(language)", str(result))]}
                       if isinstance(result, Unreadable) else result)
    return {"version": version, "languages": reports}


def percent(part, whole):
    return "—" if not whole else f"{100 * part / whole:.1f}%"


def markdown(result):
    out = [f"### Game-text translation coverage ({result['version']})", "",
           "Report only: a missing translation shows English to players and never fails CI. "
           "`Extra` counts table keys outside the compared English set (other game text or "
           "entries for content this snapshot no longer has). `[DNT-UNUSED]` rows are not counted.", ""]
    if result.get("unreadable"):
        out += [f"unreadable: {result['unreadable']}", ""]
    for language in result["languages"]:
        chain = " → ".join(language["fallback"])
        out += [f"#### {language['lang']} (falls back to {chain})", "",
                "| Table | Compared with | English | Translated | Coverage | Missing | Extra |",
                "| --- | --- | ---: | ---: | ---: | ---: | ---: |"]
        details = []
        for item in language["rows"]:
            if item.get("unreadable"):
                out.append(f"| {item['table']} | unreadable: {item['unreadable'].replace('|', '/')} | — | — | — | — | — |")
            elif item.get("absent"):
                out.append(f"| {item['table']} | {item['keys']} | {item['reference'] or '—'} | absent | "
                           f"{percent(0, item['reference'])} | {item['reference'] or '—'} | — |")
            else:
                reference = item["reference"]
                out.append(f"| {item['table']} | {item['keys']} | {reference if reference is not None else '—'} | "
                           f"{item['translated']} | {percent(item['translated'], reference)} | "
                           f"{item['missing'] if item['missing'] is not None else '—'} | {item['extra']} |")
            if item["examples"]:
                shown = ", ".join("`" + text.replace("\r", " ").replace("\n", " ").replace("`", "'")[:80] + "`" for text in item["examples"])
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
    sys.stdout.write(markdown(result) + "\n")
    if args.json:
        try:
            args.json.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        except OSError as error:
            sys.stdout.write(f"unreadable: could not write {args.json}: {reason(error)}\n")


if __name__ == "__main__":
    main()
