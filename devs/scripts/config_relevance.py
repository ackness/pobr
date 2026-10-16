"""Conservative, reproducible config relevance from the active game snapshot.

Missing required skills are conclusive for that control. Otherwise look for all
mechanic words in game stat/name/text records. This is lexical evidence, not
proof of a working mechanic: Stats retains legacy entries, which we keep rather
than silently declaring them PoE1-only. No option/mechanic exclusion list is used.
"""
import json
from pathlib import Path
import re

RULE = ("Exclude a control if none of its required if_skill names/ids exist in "
        "base/granted_effects.json or the game's skill-gem item names; otherwise exclude only when its mechanic "
        "identifiers lack complete word evidence in base stats/skills/mods/passives or "
        "inherited overlay item/stat-description text (even across separate records). Keep controls "
        "without a searchable mechanic. Legacy "
        "stats and exporter-derived overlay text are retained evidence, not proof of a playable PoE2 mechanic.")
# Grammar/namespace words, not a list of mechanics. Keep damage types and named
# mechanics intact; normalize the same way on the option and the game side.
GRAMMAR = set("condition multiplier config enemy enemies player self have has been "
              "is are was were by on in of to with for from if and or the a an "
              "you your can use using used recently count stack stacks active "
              "override val min max minimum maximum total number checks inflict "
              "apply applying applied affecting affected last past seconds second sec "
              "be always final low high two eight ten avg full s no uses average".split())


def words(value):
    value = re.sub(r"([a-z])([A-Z])", r"\1 \2", value)
    tokens = re.findall(r"[a-z]+", value.lower())
    def stem(word):
        if word.endswith("ies"):
            return word[:-3] + "y"
        if word.endswith("s") and not word.endswith(("ss", "us")):
            word = word[:-1]
        if word.endswith("ed"):
            word = word[:-2]
        if word.endswith("ing"):
            word = word[:-3]
        if word.endswith("e"):
            word = word[:-1]
        return word
    return {stem(word) for word in tokens if word not in GRAMMAR}


def text_values(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from text_values(item)
    elif isinstance(value, dict):
        for key, item in value.items():
            if key != "_meta":
                yield from text_values(item)


class GameEvidence:
    def __init__(self, directory):
        self.skills = {}
        self.records = []
        self.index = {}
        for relative in ("base/stats.json", "base/granted_effects.json", "base/mods.json",
                         "base/base_items.json", "base/passive_tree.json", "overlay/uniques.json",
                         "overlay/stat_descriptions.json"):
            path = directory / relative
            if not path.exists():
                continue
            data = json.loads(path.read_text())
            if relative == "base/granted_effects.json":
                for row in data:
                    self.skills[row["id"].casefold()] = relative
                    if row.get("displayed_name"):
                        self.skills[row["displayed_name"].casefold()] = relative
            if relative == "base/base_items.json":
                data = [row for row in data if row.get("item_class", "").endswith("Skill Gem")]
                self.skills.update((row["name"].casefold(), relative) for row in data
                                   if row.get("name"))
            for value in sorted(set(text_values(data))):
                number = len(self.records)
                self.records.append({"file": relative, "text": value})
                for word in words(value):
                    self.index.setdefault(word, set()).add(number)

    def matches(self, query):
        tokens = words(query)
        if not tokens:
            return []
        terms = [self.index.get(word, set()) for word in tokens]
        matches = set.intersection(*terms)
        # Split descriptions and engine shorthand can name a real mechanic in
        # different records. Keep this uncertain case in the denominator.
        if not matches and all(terms):
            matches = set.union(*terms)
        return [self.records[i] for i in sorted(matches)]

    def classify(self, option):
        visibility = option.get("visibility") or {}
        skills = visibility.get("if_skill") or []
        if skills and not any(skill.casefold() in self.skills for skill in skills):
            return {"class": "not_in_game", "reason": "required_skill_absent", "queries": skills,
                    "sources": []}
        # A required skill present in the game suffices, even if the engine uses
        # a different internal name for its mechanic.
        if skills:
            return {"class": "relevant", "reason": "required_skill_present", "queries": skills,
                    "sources": [{"file": self.skills[skill.casefold()], "text": skill}
                                for skill in skills if skill.casefold() in self.skills]}
        queries = [name for family, names in visibility.items()
                   if family in ("if_flag", "if_cond", "if_enemy_cond", "if_mult", "if_enemy_mult")
                   for name in names if words(name)]
        if not queries:
            effects = list(option.get("effects") or [])
            effects += [effect for group in (option.get("option_effects") or {}).values()
                        for effect in group]
            queries = [effect["name"] for effect in effects
                       if words(effect["name"]) and effect["name"] != "SkillData"]
        # Empty effects and skill-data keys can be scalar controls whose names
        # are implementation details; absence of a query is not an exclusion.
        if not queries:
            return {"class": "relevant", "reason": "no_searchable_mechanic", "queries": [], "sources": []}
        evidence = [source for query in queries for source in self.matches(query)]
        return {"class": "relevant" if evidence else "not_in_game",
                "reason": "game_text_match" if evidence else "mechanic_absent",
                "queries": queries, "sources": evidence[:12]}


def catalog_options(directory):
    catalog = json.loads((directory / "overlay/config_options.json").read_text())
    options = {option["var"]: option for option in catalog["options"]}
    # Match GameData::config_options' bundled override, without changing the
    # independent energy-options file another unit owns.
    own = directory.parent / "overlay-common/archon_config_options.json"
    if own.exists():
        options.update((o["var"], o) for o in json.loads(own.read_text())["options"])
    return sorted(options.values(), key=lambda option: option["var"])


def relevance(directory, options=None):
    game = GameEvidence(directory)
    return {option["var"]: game.classify(option)
            for option in (options if options is not None else catalog_options(directory))}


def web_manifest(directory):
    return {"data_version": directory.name, "rule": RULE,
            "excluded": {var: row for var, row in relevance(directory).items()
                         if row["class"] == "not_in_game"}}


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[2]
    directory = root / "data" / (root / "data/CURRENT").read_text().strip()
    destination = root / "web/src/lib/configGameRelevance.json"
    destination.write_text(json.dumps(web_manifest(directory), indent=2) + "\n")
    print(destination)
