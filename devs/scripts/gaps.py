#!/usr/bin/env python3
"""Rank game-text gaps, or research one line, without consulting holdout or PoB2.

    mise run gaps [--json FILE] [--limit N] [--kind unique|passive|mods|...] [--all]
    mise run gap "<template or text>"

Uses lookup's data access and descriptions, triage's templates and usage cache,
and compare-build's build-code conventions. Standard library only.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import importlib.util
import json
from pathlib import Path
import re
import sys
from urllib.parse import quote

REPO = Path(__file__).resolve().parents[2]
_SPEC = importlib.util.spec_from_file_location("triage", REPO / "devs/scripts/triage.py")
triage = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(triage)
lookup = triage.lookup
SOURCE_KINDS = ("unique", "passive", "mods", "compound", "jewel_node", "bases")
FATES = {"unparsed", "unmodelled", "no_modifier"}
BUCKETS = ("dropped", "config-gated", "state-gated", "unused", "handled")
CLASSES = ("pobr_gap", "upstream_gap", "recognized_empty", "no_effect")
STOPWORDS = set("a an the to of in on for from with and or you your have has is are "
                "increased reduced more less maximum minimum per when while gain grants "
                "all as at by can be not this that skills".split())


def template(text):
    """Join actual rolls, ranged item text, placeholders and display markup."""
    text = re.sub(r"\{\d*(?::[^}]+)?\}", "1", text)
    text = re.sub(r"\{[^}]*\}", "", text)
    text = re.sub(r"\([+-]?\d+(?:\.\d+)?-[+-]?\d+(?:\.\d+)?\)", "1", text)
    # An input template can already contain #, including a leading +#.
    text = re.sub(r"[+-]?#", "1", text)
    return " ".join(triage.template(lookup.plain(text)).split())


def key(text):
    return template(text).casefold()


def classify(status, vendor_status, ledger=False):
    if status in ("recognized_empty", "no_effect"):
        return status
    if status == "parsed" and not ledger:
        return None
    if status is None:
        return "ledger_gap"
    return "pobr_gap" if vendor_status == "parsed" else "upstream_gap"


def collect_usage(binary, refresh=False, jobs=3):
    """Only the two worked sets, through triage's collector; no oracle required."""
    found = triage.build_dirs(triage.DEFAULT_SETS)
    token = triage.usage_token(binary)
    results, failures = [], []
    print(f"gaps: usage ledgers for {len(found)} worked builds", file=sys.stderr)
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as pool:
        futures = {pool.submit(triage.build_usage, binary, folder, token, refresh): (corpus, folder)
                   for corpus, folder in found}
        for future in as_completed(futures):
            corpus, folder = futures[future]
            try:
                results.append({"build": folder.name, "set": corpus,
                                "path": str(folder.relative_to(REPO)), **future.result()})
            except (Exception, SystemExit) as error:
                failures.append({"build": folder.name, "error": str(error)})
    return sorted(results, key=lambda row: (row["set"], row["build"])), failures


def line_buckets(line, reads, conditions):
    """Keep every unmet modifier's bucket; one working part cannot hide a gap.

    Only explicit registry metadata or the *reported* unmet config condition
    exempts an inactive modifier. Merely sharing its name with one does not.
    """
    if line.get("status") == "no_effect":
        return ["handled"]
    if line.get("status") in FATES:
        return ["dropped"]
    buckets = set()
    for modifier in line.get("modifiers") or []:
        status = modifier.get("status")
        if status in FATES:
            buckets.add("dropped")
        elif status == "unread":
            buckets.add("unused" if modifier.get("name") in reads else "dropped")
        elif status == "inactive":
            gate = modifier.get("read_gate")
            if gate:
                buckets.add("config-gated" if gate.get("config_option") else "state-gated")
            else:
                reason = modifier.get("reason") or ""
                condition = re.fullmatch(r"Condition (.+) (?:not met|holds \(applies only without it\))", reason)
                names = (condition.group(1).split(", ") if condition else
                         reason[len("none of Conditions "):-len(" met")].split(", ")
                         if reason.startswith("none of Conditions ") and reason.endswith(" met") else [])
                # Actor conditions use enemy.X in reasons, EnemyX in the ledger.
                names = ["Enemy" + name[6:] if name.startswith("enemy.") else
                         name[7:] if name.startswith("player.") else name for name in names]
                buckets.add("config-gated" if names and all(conditions.get(name) for name in names)
                            else "dropped")
    if not buckets and line.get("status") in ("unread", "inactive", "partial"):
        # A received line without modifiers has no proven reader/gate.
        if not line.get("modifiers"):
            buckets.add("dropped")
    return [bucket for bucket in BUCKETS if bucket in buckets] or ["handled"]


def bucket_counts(queue):
    """Template/build pairs, once per bucket (mixed rows can have several)."""
    return {bucket: sum(row["bucket_builds"][bucket] for row in queue) for bucket in BUCKETS}


def ranked_queue(audit, builds, kind=None, include_handled=False, include_all=False):
    """Group all variants before filtering; count affected builds once per path."""
    grouped = {}
    reads = triage.reader_union(build.get("usage") or {} for build in builds)

    def group(text):
        return grouped.setdefault(key(text), {"template": template(text), "variants": [],
                                              "sources": set(), "builds": []})

    for text, entry in (audit.get("entries") or {}).items():
        sources = entry.get("sources") or []
        row = group(text)
        row["variants"].append({"text": text, **entry})
        row["sources"].update(sources)
    for build in builds:
        usage = build.get("usage") or {}
        lines = usage.get("lines") or []
        conditions = {entry["condition"]: entry.get("config_option")
                      for entry in usage.get("conditions") or []}
        conditions.update(usage.get("condition_configs") or {})
        for line in lines:
            text = line.get("text") or ""
            if not text:
                continue
            if not include_handled and key(text) not in grouped:
                continue  # The queue ranks audited game text, not generated ledger labels.
            # These are generated calculation/config labels, not game text.
            # Audit-backed text still retains every carrier and its complete fate.
            if key(text) not in grouped and (
                    line.get("source_kind") in ("Config", "EnemyConfig", "SkillGem", "SupportGem")
                    or line.get("source_id") in ("mageblood", "base.movement_penalty")):
                continue
            row = group(text)
            identity = build.get("path") or f"{build.get('set')}/{build['build']}"
            carrier = next((b for b in row["builds"] if b["path"] == identity), None)
            if carrier is None:
                carrier = {"build": build["build"], "path": identity, "lines": []}
                row["builds"].append(carrier)
            carrier["lines"].append({**line, "buckets": line_buckets(line, reads, conditions)})
    queue = []
    for row in grouped.values():
        if not include_handled and row["sources"] and all(
                source.startswith("translation:") for source in row["sources"]):
            continue
        for build in row["builds"]:
            build["buckets"] = [bucket for bucket in BUCKETS if any(
                bucket in line["buckets"] for line in build["lines"])]
            build["affected"] = "dropped" in build["buckets"]
        affected = [build for build in row["builds"] if build["affected"]]
        row["bucket_builds"] = {bucket: sum(bucket in build["buckets"] for build in row["builds"])
                                for bucket in BUCKETS}
        if row["builds"] and not affected and not (include_handled or include_all):
            continue
        classes = {classify(v.get("status"), v.get("vendor_status"), bool(affected))
                   for v in row["variants"]}
        classes.discard(None)
        if not row["variants"] and affected:
            classes.add("ledger_gap")  # Build-only text has no audited vendor status.
        if not classes and (include_handled or include_all):
            classes.add("parsed")
        if not classes or (classes == {"no_effect"} and not affected
                           and not (include_handled or include_all)):
            continue
        row["classes"] = sorted(classes, key=lambda c: (*CLASSES, "ledger_gap", "parsed").index(c))
        row["class"] = row["classes"][0]
        row["sources"] = sorted(row["sources"])
        row["source_kinds"] = sorted({s.partition(":")[0] for s in row["sources"]},
                                      key=lambda k: (SOURCE_KINDS.index(k) if k in SOURCE_KINDS
                                                     else len(SOURCE_KINDS), k))
        row["affected_builds"] = len(affected)
        row["source_count"] = len(row["sources"])
        row["ledger_variants"] = sorted({line["text"] for build in row["builds"] for line in build["lines"]})
        row["builds"].sort(key=lambda b: b["path"])
        if kind is None or kind in row["source_kinds"]:
            queue.append(row)
    queue.sort(key=lambda row: (-row["affected_builds"],
                               tuple(-int(k in row["source_kinds"]) for k in SOURCE_KINDS),
                               -row["source_count"], row["template"]))
    for rank, row in enumerate(queue, 1):
        row["rank"] = rank
    return queue


def reverse_stats(data, texts):
    """Exact numeric-template reverse match; report absent base stats explicitly.

    A compound's individual rendered lines all point to its full member set.
    """
    needles = {key(text) for text in texts}
    existing = {stat.get("id") for stat in data.stats()}
    found = []
    for scope, kinds in (data.load("overlay/stat_descriptions.json", {}).get("scopes") or {}).items():
        descriptions = [(lines, [stat]) for stat, lines in (kinds.get("single") or {}).items()]
        descriptions += [([entry.get("template") or ""], entry.get("member_stats") or [stat])
                         for stat, entry in (kinds.get("compound") or {}).items()]
        for lines, ids in descriptions:
            for text in lines:
                variants = [text, *text.replace("\\n", "\n").splitlines()]
                if any(key(variant) in needles for variant in variants):
                    found.append({"scope": scope, "text": text,
                                  "stats": [{"id": stat, "base_exists": stat in existing} for stat in ids]})
    return found


class Sources:
    """Resolve audit source ids once, retaining the complete carrier text."""

    def __init__(self, data):
        self.data = data
        self.records = {}
        for record in data.uniques():
            self.records[f"unique:{record['name']}"] = (record["name"], lookup.describe_unique(record))
        for record in data.nodes():
            self.records[f"passive:{record['skill']}"] = (record.get("name"), lookup.describe_node(record))
        catalog = data.load("overlay/trade_catalog.json", {})
        for kind in ("mods", "search_mods", "bases"):
            for record in catalog.get(kind) or []:
                ident = record.get("id") or record.get("name")
                lines = record.get("implicits") if kind == "bases" else record.get("lines")
                self.records[f"{kind}:{ident}"] = (record.get("name") or ident, list(lines or []))
        jewels = data.load("overlay/passive_jewels.json", {})
        for ident, node in (jewels.get("nodes") or {}).items():
            self.records[f"jewel_node:{ident}"] = (node.get("name") or ident, node.get("stats") or [])
        for ident, family in (jewels.get("families") or {}).items():
            self.records[f"jewel_family:{ident}:attribute"] = (ident, family.get("attribute_additions") or [])

    def name(self, source):
        return self.records.get(source, (source.partition(":")[2], []))[0]

    def describe(self, source):
        kind, _, ident = source.partition(":")
        name, lines = self.records.get(source, (ident, []))
        result = {"source": source, "name": name, "lines": list(lines)}
        if kind in ("unique", "passive", "jewel_node"):
            result["poe2db_guess"] = "https://poe2db.tw/us/" + quote(name.replace(" ", "_"), safe="_'")
        if kind in ("stat", "compound"):
            scope, _, stat = ident.partition(":")
            descriptions = (self.data.load("overlay/stat_descriptions.json", {}).get("scopes") or {}).get(scope, {})
            result["lines"] = (descriptions.get("single") or {}).get(stat) or [
                json.dumps((descriptions.get("compound") or {}).get(stat), ensure_ascii=False)]
        if kind == "translation":
            result["lines"] = [ident]
        if kind in ("mods", "search_mods"):
            mod = next((m for m in self.data.mods() if m.get("id") == ident.rpartition(":")[2]), None)
            if mod:
                result["game_stats"] = mod.get("stats") or []
        if kind == "rule":
            result["lines"] = [json.dumps(rule, ensure_ascii=False)
                               for _, _, rule in rule_records(self.data) if rule.get("id") == ident]
        return result


def words(text):
    found = set(re.findall(r"[a-z]{3,}", re.sub(r"([a-z])([A-Z])", r"\1 \2", text).lower())) - STOPWORDS
    # Game text uses Charms/Charges/Enemies; engine names use Charm/Charge/Enemy.
    return {word[:-3] + "y" if word.endswith("ies") else
            word[:-1] if len(word) > 4 and word.endswith("s") and word not in ("chaos", "status")
            else word for word in found}


def rule_records(data):
    """Rules from every layer the engine can merge, keeping their provenance."""
    paths = [data.directory.parent / "overlay-common/special_mods.json",
             data.directory / "generated/special_derived.json", data.directory / "generated/special_vendor.json",
             data.directory / "overlay/special_mods.json", data.directory / "overlay/mod_parser_rules.json"]
    for path in paths:
        if not path.is_file():
            continue
        payload = json.loads(path.read_text(encoding="utf-8"))
        for section, entries in payload.items():
            if not isinstance(entries, list):
                continue
            for rule in entries:
                if not isinstance(rule, dict):
                    continue
                yield path, section, rule


def candidates(data, text):
    """Keyword suggestions, with locations, rather than claims of rule support."""
    wanted = words(text)
    names, rules = {}, []
    for path in sorted((REPO / "crates/pobr-core/src").rglob("*.rs")):
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for name in re.findall(r'"([A-Z][A-Za-z0-9_:]+)"', line):
                overlap = wanted & words(name)
                if overlap:
                    entry = names.setdefault(name, {"name": name, "score": len(overlap), "locations": []})
                    entry["locations"].append(f"{path.relative_to(REPO)}:{number}")
    for path, section, rule in rule_records(data):
        pattern = rule.get("pattern") or rule.get("literal") or ""
        overlap = wanted & words(pattern)
        if overlap:
            rules.append({"file": str(path), "section": section, "score": len(overlap), "rule": rule})
    return (sorted(names.values(), key=lambda r: (-r["score"], r["name"]))[:20],
            sorted(rules, key=lambda r: (-r["score"], r["file"], str(r["rule"])))[:20])


def research_card(data, audit, builds, query):
    # Include parsed lines too: their card is useful even if nothing is dropped.
    augmented = dict(audit)
    augmented["entries"] = {text: entry for text, entry in audit["entries"].items()
                            if key(text) == key(query)}
    matches = [row for row in ranked_queue(augmented, builds, include_handled=True)
               if key(row["template"]) == key(query)]
    if not matches:
        return None
    card = matches[0]
    card.pop("rank", None)
    sources = Sources(data)
    card["source_details"] = [sources.describe(source) for source in card["sources"]]
    card["stat_matches"] = reverse_stats(data, [query, *(v["text"] for v in card["variants"])])
    card["candidate_names"], card["nearby_rules"] = candidates(data, card["template"])
    return card


def render_card(card):
    rows = [f"template: {card['template']}", f"class: {', '.join(card['classes'])}", "variants:"]
    rows += [f"  {v['text']} [PoBR {v.get('status')}; PoB2 {v.get('vendor_status', 'unknown')}]"
             for v in card["variants"]] or ["  ledger only; no audit entry"]
    audited = {variant["text"] for variant in card["variants"]}
    rows += [f"  {text} [worked-build roll]" for text in card["ledger_variants"] if text not in audited]
    rows.append("game stat matches (overlay/stat_descriptions.json):")
    for match in card["stat_matches"]:
        rows.append(f"  {match['scope']}: {match['text']}")
        rows += [f"    {s['id']} [base/stats.json: {'exists' if s['base_exists'] else 'MISSING'}]" for s in match["stats"]]
    if not card["stat_matches"]:
        rows.append("  no exact template match")
    rows.append("sources (complete carrier text; poe2db URLs are guesses):")
    for source in card["source_details"]:
        rows.append(f"  {source['source']} — {source['name']}")
        rows += [f"    {line}" for line in source["lines"]]
        if source.get("game_stats"):
            rows.append("    game stats: " + json.dumps(source["game_stats"]))
        if source.get("poe2db_guess"):
            rows.append("    " + source["poe2db_guess"])
    rows.append(f"worked builds: {len(card['builds'])} carrying, {card['affected_builds']} with dropped input")
    for build in card["builds"]:
        rows.append(f"  {build['path']} [{', '.join(build['buckets'])}]")
        for line in build["lines"]:
            rows.append(f"    {line.get('status')} [{', '.join(line['buckets'])}] · {triage.line_source(line)} · {line.get('text')}")
            if line.get("reason"):
                rows.append(f"      {line['reason']}")
            for mod in line.get("modifiers") or []:
                rows.append(f"      {mod.get('name')}: {mod.get('status')}"
                            + (f" · {mod['reason']}" if mod.get("reason") else ""))
    rows.append("candidate engine names (keyword matches):")
    rows += [f"  {entry['name']} · {entry['locations'][0]}" for entry in card["candidate_names"]]
    rows.append("nearby rules (keyword matches):")
    rows += [f"  {entry['file']} [{entry['section']}] {json.dumps(entry['rule'], ensure_ascii=False)}"
             for entry in card["nearby_rules"]]
    return "\n".join(rows) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("gaps", "gap"))
    parser.add_argument("query", nargs="?")
    parser.add_argument("--json", type=Path, help="write the complete queue/card; --limit only limits text")
    parser.add_argument("--limit", type=int, default=30)
    parser.add_argument("--kind", help="source kind, e.g. unique, passive, mods, compound, jewel_node")
    parser.add_argument("--binary", type=Path, help="use an already built pobr executable (default: build CLI once)")
    parser.add_argument("--all", action="store_true", help="also show gated, unused and handled rows")
    parser.add_argument("--refresh", action="store_true", help="recalculate worked usage ledgers")
    args = parser.parse_args(argv)
    if args.command == "gap" and not args.query:
        parser.error("gap requires a template or text")
    if args.command == "gaps" and args.query:
        parser.error("gaps takes no query; use gap for a research card")
    if args.limit < 1:
        parser.error("--limit must be positive")
    data = lookup.Data(lookup.data_dir())
    audit = data.load("generated/modifier-audit.json")
    if not audit or not isinstance(audit.get("entries"), dict):
        parser.error("missing generated/modifier-audit.json; refresh the modifier audit first")
    binary = args.binary.resolve() if args.binary else triage.engine_binary()
    builds, failures = collect_usage(binary, args.refresh)
    if args.command == "gaps":
        complete = ranked_queue(audit, builds, args.kind, include_all=True)
        counts = bucket_counts(complete)
        queue = complete if args.all else ranked_queue(audit, builds, args.kind)
        sources = Sources(data)
        for row in queue:
            examples = [s for kind in SOURCE_KINDS for s in row["sources"]
                        if s.partition(":")[0] == kind]
            row["example_source"] = sources.name(examples[0]) if examples else next(iter(row["sources"]), "ledger")
        payload = {"data_version": data.directory.name, "builds_scanned": len(builds),
                   "failures": failures, "bucket_counts": counts, "queue": queue}
        print(f"data {data.directory.name} · {len(queue)} templates · {len(builds)} worked builds")
        print("template/build buckets (mixed rows count in each): " + " · ".join(
            f"{bucket}={counts[bucket]}" for bucket in BUCKETS))
        for row in queue[:args.limit]:
            print(f"{row['rank']:>3} {row['class']:<17} builds={row['affected_builds']:<3} "
                  f"{','.join(row['source_kinds']) or 'ledger'} | {row['example_source']} | {row['template']}"
                  + (" | " + ", ".join(f"{bucket}={count}" for bucket, count in row["bucket_builds"].items()
                                       if count) if args.all else ""))
    else:
        card = research_card(data, audit, builds, args.query)
        if card is None:
            print(f"gap: no audited or worked-build line matches {args.query!r}", file=sys.stderr)
            return 1
        payload = {"data_version": data.directory.name, "failures": failures, **card}
        print(render_card(card), end="")
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    for failure in failures:
        print(f"gaps: FAILED {failure['build']}: {failure['error']}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
