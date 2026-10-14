#!/usr/bin/env python3
"""The game data records an id or a name has.

    mise run lookup <query> [--level N]

The query is matched exactly (ignoring case) against granted effect ids
(`IceShotPlayer`; a gem's id or effect id such as `IceShot` finds the effect
it grants), stat ids, base item names and ids, passive node ids and names,
and unique names. Every kind that matches is printed:

- granted effect: its skill types, the gems that grant it, each stat set's
  constant and implicit stats, and with `--level N` the level row and each
  stat set's stats at that level;
- stat: its stat record, the description text PoB2's export renders for it,
  where the engine gives it a meaning (`rules/skill_stats.rs`, the extracted
  stat map) and which effects and mods carry it;
- base item: class, tags, implicit mods and their stats, defences or weapon
  numbers, and what it grants when socketed (runes, soul cores, idols);
- passive node: its stats and kind;
- unique: its item text.

Nothing exact gives the names that contain the query. The data version is the
one the engine uses: `POBR_DATA_ROOT` (default `data/`) and
`POBR_DATA_VERSION` (default that root's `CURRENT`), or `--version`.
Standard library only.
"""

import argparse
import json
import os
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[2]
# The engine's own meaning of skill stats, keyed by the game's stat id.
SKILL_STATS_RS = REPO / "crates/pobr-core/src/rules/skill_stats.rs"
# How many records of a kind, and how many names of a list, are printed.
LIMIT = 10
# A unique's search tags in PoB2's item text.
SEARCH_TAGS = re.compile(r"\{tags:[^}]*\}")


def data_dir(version=None) -> Path:
    """The data directory the engine reads: `POBR_DATA_ROOT` (default the
    repository's `data/`), version `POBR_DATA_VERSION` or that root's `CURRENT`."""
    root = Path(os.environ.get("POBR_DATA_ROOT") or REPO / "data")
    version = version or os.environ.get("POBR_DATA_VERSION")
    if not version:
        current = root / "CURRENT"
        lines = current.read_text(encoding="utf-8").split() if current.is_file() else []
        if not lines:
            raise SystemExit(f"lookup: {current} names no version; set POBR_DATA_VERSION")
        version = lines[0]
    directory = root / version
    if not directory.is_dir():
        raise SystemExit(f"lookup: no data directory {directory}")
    return directory


class Data:
    """One version's data files, each read when first asked for."""

    def __init__(self, directory: Path):
        self.directory = directory
        self.files = {}

    def load(self, relative: str, default=None):
        if relative not in self.files:
            path = self.directory / relative
            self.files[relative] = (json.loads(path.read_text(encoding="utf-8"))
                                    if path.is_file() else default)
        return self.files[relative]

    def records(self, relative: str, key: str = "") -> list:
        """A list file, or the list under `key` of an overlay file."""
        value = self.load(relative, [] if not key else {})
        if key:
            value = value.get(key, []) if isinstance(value, dict) else []
        return value if isinstance(value, list) else []

    def effects(self) -> list:
        return self.records("base/granted_effects.json")

    def gems(self) -> list:
        return self.records("base/skill_gems.json")

    def stats(self) -> list:
        return self.records("base/stats.json")

    def bases(self) -> list:
        return self.records("base/base_items.json")

    def nodes(self) -> list:
        return self.records("base/passive_tree.json")

    def uniques(self) -> list:
        return self.records("overlay/uniques.json", "uniques")

    def mods(self) -> list:
        return self.records("base/mods.json")

    def stat_sets(self) -> list:
        return self.records("base/granted_effect_stat_sets.json")

    def levels(self) -> dict:
        value = self.load("base/granted_effect_levels.json", {})
        return value if isinstance(value, dict) else {}

    def runes(self) -> list:
        return self.records("overlay/runes.json", "runes")

    def cost_types(self) -> list:
        return self.records("base/cost_types.json")


# Matching


def same(a, b) -> bool:
    return isinstance(a, str) and isinstance(b, str) and a.casefold() == b.casefold()


def find(data: Data, query: str) -> list:
    """Every record `query` names exactly, as (kind, record)."""
    found = []
    effects = {effect.get("id"): effect for effect in data.effects()}
    effect_ids = [effect_id for effect_id in effects if same(effect_id, query)]
    for gem in data.gems():
        gem_id = gem.get("id") or ""
        if any(same(name, query) for name in (gem_id, gem_id.rsplit("/", 1)[-1], gem.get("gem_effect_id"))):
            effect_ids += [gem.get("granted_effect_id"), *(gem.get("additional_granted_effect_ids") or [])]
    for effect_id in dict.fromkeys(effect_ids):
        if effect_id in effects:
            found.append(("granted effect", effects[effect_id]))
    found += [("stat", stat) for stat in data.stats() if same(stat.get("id"), query)]
    found += [("base item", base) for base in data.bases()
              if same(base.get("name"), query) or same(base.get("id"), query)]
    if query.isdigit():
        # A class's variant of a node has its own number in a build's tree.
        number = int(query)
        found += [("passive node", node) for node in data.nodes()
                  if node.get("skill") == number
                  or any(variant.get("variant_skill") == number for variant in node.get("variants") or [])]
    else:
        found += [("passive node", node) for node in data.nodes()
                  if same(node.get("name"), query) or same(node.get("id"), query)]
    found += [("unique", unique) for unique in data.uniques() if same(unique.get("name"), query)]
    return found


def suggest(data: Data, query: str) -> list:
    """(kind, name) of every record whose id or name contains `query`."""
    needle = query.casefold()

    def matches(value):
        return isinstance(value, str) and needle in value.casefold()

    names = [("granted effect", effect.get("id")) for effect in data.effects() if matches(effect.get("id"))]
    names += [("stat", stat.get("id")) for stat in data.stats() if matches(stat.get("id"))]
    names += [("base item", base.get("name")) for base in data.bases() if matches(base.get("name"))]
    names += [("passive node", f"{node.get('skill')} {node.get('name')}") for node in data.nodes()
              if matches(node.get("name"))]
    names += [("unique", unique.get("name")) for unique in data.uniques() if matches(unique.get("name"))]
    return list(dict.fromkeys(names))


# Records


def value_text(value) -> str:
    if isinstance(value, float):
        return str(int(value)) if value.is_integer() else str(round(value, 6))
    if isinstance(value, list):
        return ", ".join(value_text(entry) for entry in value)
    return str(value)


def plain(text: str) -> str:
    """Display text without the tree's link markup (`[Attributes|Attribute]`)
    or line breaks."""
    text = re.sub(r"\[([^\]|]*)\|([^\]]*)\]", r"\2", text)
    return re.sub(r"\[([^\]|]*)\]", r"\1", text).replace("\n", " ")


def cost_text(data: Data, effect: dict, amounts) -> str:
    """A level row's costs with the effect's cost types (`6 Mana`)."""
    names = [entry.get("id") for entry in data.cost_types()]
    kinds = effect.get("cost_types") or []
    costs = []
    for index, amount in enumerate(amounts or []):
        kind = kinds[index] if index < len(kinds) else None
        name = names[kind] if isinstance(kind, int) and 0 <= kind < len(names) else f"cost type {kind}"
        costs.append(f"{value_text(amount)} {name}")
    return ", ".join(costs)


def stat_values(stats) -> str:
    return ", ".join(f"{entry.get('stat')} = {value_text(entry.get('value'))}" for entry in stats or [])


def listed(names, limit: int = LIMIT) -> str:
    names = list(names)
    text = ", ".join(str(name) for name in names[:limit])
    return text + (f", … ({len(names)} in all)" if len(names) > limit else "")


def describe_effect(data: Data, effect: dict, level) -> list:
    effect_id = effect.get("id")
    rows = [f"== granted effect {effect_id}"]
    kind = "support" if effect.get("is_support") else f"active skill {effect.get('active_skill')}"
    cast = effect.get("cast_time")
    rows.append(f"   {kind}" + (f" · cast time {cast} ms" if cast is not None else ""))
    for field, label in (("skill_types", "skill types"), ("add_skill_types", "adds types"),
                         ("require_skill_types", "requires types"),
                         ("exclude_skill_types", "excludes types"),
                         ("cannot_be_supported", "cannot be supported by")):
        if effect.get(field):
            rows.append(f"   {label}: {', '.join(map(str, effect[field]))}")
    if effect.get("additional_stat_set_ids"):
        rows.append(f"   additional stat sets: {', '.join(effect['additional_stat_set_ids'])}")
    for gem in data.gems():
        if effect_id == gem.get("granted_effect_id") or effect_id in (gem.get("additional_granted_effect_ids") or []):
            rows.append(f"   gem {gem.get('id')} · tags {', '.join(gem.get('gem_tags') or [])}")

    level_rows = data.levels().get(effect_id) or []
    if level_rows:
        numbers = [row.get("level") for row in level_rows]
        rows.append(f"   levels {numbers[0]}–{numbers[-1]} ({len(level_rows)} rows)"
                    + ("" if level is not None else "; --level N prints one"))
    if level is not None:
        row = next((row for row in level_rows if row.get("level") == level), None)
        if row is None:
            rows.append(f"   level {level}: no row")
        else:
            fields = [f"cost {cost_text(data, effect, row['cost_amounts'])}"] if row.get("cost_amounts") else []
            fields += [f"{key} {value_text(value)}" for key, value in row.items()
                       if key not in ("level", "cost_amounts")]
            rows.append(f"   level {level}: {', '.join(fields) or 'no numbers'}")

    for record in data.stat_sets():
        if record.get("effect_id") != effect_id:
            continue
        for stat_set in record.get("sets") or []:
            rows.append(f"   stat set {stat_set.get('set_id')} · base effectiveness "
                        f"{value_text(stat_set.get('base_effectiveness'))}")
            if stat_set.get("constant_stats"):
                rows.append(f"     constant: {stat_values(stat_set['constant_stats'])}")
            if stat_set.get("implicit_stats"):
                rows.append(f"     implicit: {', '.join(stat_set['implicit_stats'])}")
            set_levels = stat_set.get("levels") or []
            if level is None:
                per_level = dict.fromkeys(entry.get("stat") for row in set_levels
                                          for entry in row.get("stats") or [])
                if per_level:
                    rows.append(f"     per level: {', '.join(per_level)}")
            else:
                row = next((row for row in set_levels if row.get("gem_level") == level), None)
                if row is None:
                    rows.append(f"     level {level}: no row")
                else:
                    multiplier = row.get("damage_multiplier")
                    head = (f"damage multiplier {value_text(multiplier)}; "
                            if multiplier is not None else "")
                    rows.append(f"     level {level}: {head}{stat_values(row.get('stats')) or 'no stats'}")
    return rows


def descriptions(data: Data, stat_id: str) -> list:
    """The texts PoB2's export renders for `stat_id` (at value 1), by scope."""
    scopes = (data.load("overlay/stat_descriptions.json", {}) or {}).get("scopes") or {}
    found = []
    for scope, kinds in scopes.items():
        single = (kinds.get("single") or {}).get(stat_id)
        if single:
            found += [f"{scope}: {plain(text)}" for text in single]
        for compound in (kinds.get("compound") or {}).values():
            if stat_id in (compound.get("member_stats") or []):
                found.append(f"{scope}, with {', '.join(compound['member_stats'])}: "
                             f"{plain(compound.get('template') or '')}")
        if stat_id in (kinds.get("unrendered") or []):
            found.append(f"{scope}: listed, not rendered")
    return found


def engine_lines(stat_id: str) -> list:
    """`skill_stats.rs` lines that name `stat_id` (the engine's own meaning)."""
    if not SKILL_STATS_RS.is_file():
        return []
    quoted = f'"{stat_id}"'
    text = SKILL_STATS_RS.read_text(encoding="utf-8").splitlines()
    try:
        place = SKILL_STATS_RS.relative_to(REPO)
    except ValueError:
        place = SKILL_STATS_RS
    return [f"{place}:{number}" for number, line in enumerate(text, 1) if quoted in line]


def describe_stat(data: Data, stat: dict) -> list:
    stat_id = stat.get("id")
    rows = [f"== stat {stat_id}"]
    fields = [f"{key} {value}" for key, value in stat.items() if key != "id"]
    if fields:
        rows.append("   " + " · ".join(fields))
    texts = descriptions(data, stat_id)
    rows += [f"   description: {text}" for text in texts] or [
        "   description: none in overlay/stat_descriptions.json"]
    for place in engine_lines(stat_id):
        rows.append(f"   engine meaning: {place}")
    stat_map = data.load("overlay/skill_stat_map.json", {}) or {}
    entry = (stat_map.get("global") or {}).get(stat_id)
    if entry:
        rows.append(f"   stat map, global: {json.dumps(entry.get('mods'), ensure_ascii=False)}")
    per_set = [f"{effect}/{set_key}" for effect, sets in (stat_map.get("per_stat_set") or {}).items()
               if isinstance(sets, dict)
               for set_key, stats in sets.items() if isinstance(stats, dict) and stat_id in stats]
    if per_set:
        rows.append(f"   stat map, per stat set: {listed(per_set)}")
    carriers = []
    for record in data.stat_sets():
        for stat_set in record.get("sets") or []:
            constant = any(entry.get("stat") == stat_id for entry in stat_set.get("constant_stats") or [])
            leveled = any(entry.get("stat") == stat_id for row in stat_set.get("levels") or []
                          for entry in row.get("stats") or [])
            implicit = stat_id in (stat_set.get("implicit_stats") or [])
            if constant or leveled or implicit:
                carriers.append(stat_set.get("set_id"))
    if carriers:
        rows.append(f"   in stat sets: {listed(dict.fromkeys(carriers))}")
    mods = [mod.get("id") for mod in data.mods()
            if any(entry.get("stat_id") == stat_id for entry in mod.get("stats") or [])]
    if mods:
        rows.append(f"   in mods: {listed(mods)}")
    return rows


def mod_text(mod: dict) -> str:
    stats = []
    for entry in mod.get("stats") or []:
        low, high = entry.get("min"), entry.get("max")
        values = value_text(low) if low == high else f"{value_text(low)}–{value_text(high)}"
        stats.append(f"{entry.get('stat_id')} {values}")
    return "; ".join(stats) or "no stats"


def describe_base(data: Data, base: dict) -> list:
    rows = [f"== base item {base.get('name')} ({base.get('id')})"]
    rows.append(f"   class {base.get('item_class')} · drop level {base.get('drop_level')} · "
                f"{base.get('width')}×{base.get('height')} · tags {', '.join(base.get('tags') or []) or 'none'}")
    for field in ("armour", "weapon"):
        if base.get(field):
            rows.append(f"   {field}: " + ", ".join(
                f"{key} {value_text(value)}" for key, value in base[field].items()))
    implicits = base.get("implicits") or []
    if implicits:
        mods = {mod.get("id"): mod for mod in data.mods()}
        for implicit in implicits:
            mod = mods.get(implicit)
            rows.append(f"   implicit {implicit}: {mod_text(mod) if mod else 'not in base/mods.json'}")
    else:
        rows.append("   implicits: none")
    for rune in data.runes():
        if same(rune.get("name"), base.get("name")):
            for slot, socketed in (rune.get("slots") or {}).items():
                rows.append(f"   socketed in {slot} ({socketed.get('kind')}): "
                            f"{' / '.join(socketed.get('lines') or [])}")
    return rows


def describe_node(node: dict) -> list:
    rows = [f"== passive node {node.get('skill')} {node.get('name')} ({node.get('id')}, {node.get('kind')})"]
    if node.get("ascendancy_id"):
        rows.append(f"   ascendancy {node['ascendancy_id']}")
    rows += [f"   {plain(line)}" for line in node.get("stats") or []] or ["   no stats"]
    for variant in node.get("variants") or []:
        rows.append(f"   as {variant.get('class')} ({variant.get('variant_skill')}): {variant.get('name')} — "
                    f"{' / '.join(plain(line) for line in variant.get('stats') or [])}")
    if node.get("connections"):
        rows.append(f"   connects to {listed(node['connections'])}")
    return rows


def describe_unique(unique: dict) -> list:
    rows = [f"== unique {unique.get('name')} ({unique.get('base')}, {unique.get('item_type')})"]
    # The item text as PoB2 keeps it, without its search tags (`{tags:life}`).
    rows += ["   " + SEARCH_TAGS.sub("", line) for line in (unique.get("raw") or "").splitlines()]
    return rows


def describe(data: Data, kind: str, record: dict, level) -> list:
    if kind == "granted effect":
        return describe_effect(data, record, level)
    if kind == "stat":
        return describe_stat(data, record)
    if kind == "base item":
        return describe_base(data, record)
    if kind == "passive node":
        return describe_node(record)
    return describe_unique(record)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="mise run lookup",
        description="The game data records of a granted effect, stat, base item, passive node or unique.")
    parser.add_argument("query", help="an id or a name: IceShotPlayer, a stat id, Fox Idol, 38537, a unique")
    parser.add_argument("--level", type=int, help="a granted effect's numbers at this level")
    parser.add_argument("--version", help="the data version (default: the one the engine uses)")
    args = parser.parse_args(argv)

    directory = data_dir(args.version)
    data = Data(directory)
    print(f"data {directory.name}")
    found = find(data, args.query)
    if not found:
        names = suggest(data, args.query)
        if not names:
            print(f"nothing is named {args.query!r}")
            return 1
        print(f"nothing is named {args.query!r} exactly; the names that contain it ({len(names)}):")
        for kind, name in names[:LIMIT * 3]:
            print(f"   {kind:<15}{name}")
        if len(names) > LIMIT * 3:
            print(f"   … {len(names) - LIMIT * 3} more")
        return 1
    counts = {}
    for kind, record in found:
        counts[kind] = counts.get(kind, 0) + 1
        if counts[kind] > LIMIT:
            continue
        print()
        print("\n".join(describe(data, kind, record, args.level)))
    for kind, count in counts.items():
        if count > LIMIT:
            print(f"\n… {count - LIMIT} more {kind} records with this name")
    return 0


if __name__ == "__main__":
    sys.exit(main())
