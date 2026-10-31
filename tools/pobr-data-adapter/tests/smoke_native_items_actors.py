"""Differential consumer check against fixed, reviewed pure producer projections.

Only this opt-in verifier executes the two byte-pinned Python views. The Rust
adapter has no Python/source-checkout dependency. This does not re-audit those
producer views or certify their gameplay semantics.
"""

from contextlib import contextmanager
import hashlib
from pathlib import Path
import sys
import types


BC_MANIFEST = "56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0"
BC_PRODUCER = "e41401224a085a676c84b8f14b144f20fe37c79a"
BC_TABLES = (
    "ArmourTypes", "AttributeRequirements", "BuffDefinitions", "BuffTemplates", "ClientStrings2",
    "ComponentCharges", "EssenceMods", "EssenceReplacementType", "EssenceTargetItemCategories", "Essences",
    "Flasks", "ItemInherentSkills", "ItemSpirit", "ModFamily", "ModType", "Mods", "ShieldTypes",
    "SoulCoreLimits", "SoulCoreStatCategories", "SoulCoreStats", "SoulCoreTypes", "SoulCores",
    "UtilityFlaskBuffs", "WeaponClasses", "WeaponTypes", "DefaultMonsterStats", "DisplayMinionMonsterType",
    "GameConstants", "MinionGemLevelScaling", "MinionStats", "MonsterResistances", "MonsterTypes",
    "MonsterVarieties", "PlayerMinionIntrinsicStats", "SkillTotemVariations", "SpectreOverrides",
    "SummonedSpecificMonsters", "SummonedSpecificMonstersOnDeath",
)
BC_COUNTS = {"mod_rows": 16784, "base_item_rows": 5496, "soul_core_rows": 313, "soul_core_stat_rows": 526,
             "soul_core_category_rows": 30, "utility_buff_rows": 13, "essence_mod_rows": 178, "constant_rows": 121,
             "default_monster_level_rows": 100, "monster_variety_rows": 2794, "monster_type_rows": 1571,
             "monster_resistance_rows": 22, "summoned_monster_rows": 170, "intrinsic_minion_stat_rows": 4,
             "minion_gem_scaling_rows": 40, "spectre_override_rows": 206}
PINS = {
    "RePoE.native_items": "9350c81674a332670eb22e24aca5cb114e7f09f74dea26cf556b707f4166f1ee",
    "RePoE.native_actor_facts": "a0845e3866c5dd0fb21a8edb42f68fc9eec3affef19b426631c0066bd5e51f58",
}


@contextmanager
def projectors(root):
    """Execute only verified buffers, not package init, .pyc or caller JSON code."""
    buffers = {}
    for name, digest in PINS.items():
        path = Path(root).joinpath(*name.split(".")).with_suffix(".py")
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, ("wrong fixed producer source", path)
        buffers[name] = data
    names = ["RePoE", *PINS]
    previous = {name: sys.modules.get(name) for name in names}
    try:
        package = types.ModuleType("RePoE")
        package.__path__ = []
        sys.modules["RePoE"] = package
        for name, data in buffers.items():
            module = types.ModuleType(name)
            sys.modules[name] = module
            setattr(package, name.rsplit(".", 1)[-1], module)
            exec(compile(data, f"<{name}:pinned>", "exec"), module.__dict__)
        yield sys.modules["RePoE.native_items"], sys.modules["RePoE.native_actor_facts"]
    finally:
        for name in reversed(names):
            if previous[name] is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = previous[name]


def stable_reference(tables, table, row, context):
    assert type(row) is int and 0 <= row < len(tables[table]["rows"]), (context, table, row)
    values = tables[table]["rows"][row]["values"]
    result = {"raw": {"payload": f"tables/{table}.json", "pointer": f"/rows/{row}/values"}}
    if isinstance(values.get("Id"), str):
        result["id"] = values["Id"]
    return result


def compare_items_actors(candidate, receipt, documents, equal_facts, repoe_source):
    assert repoe_source is not None, "B/C smoke requires explicit --repoe-source for fixed pure projector buffers"
    tables = {name.removeprefix("tables/").removesuffix(".json"): value
              for name, value in documents.items() if name.startswith("tables/")}
    with projectors(repoe_source) as (items_view, actors_view):
        items, items_summary, _ = items_view.project(tables, stable_reference)
        actors, actors_summary, _ = actors_view.project(tables, stable_reference)
    equal_facts(items, candidate["items"])
    equal_facts(actors, candidate["actors"])
    assert items_summary | actors_summary == BC_COUNTS == candidate["items_actors_summary"] == receipt["items_actors_summary"]
