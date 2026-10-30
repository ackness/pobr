"""Independent actual-a candidate comparison; no decoder or runtime certification."""

import argparse
import hashlib
import json
from pathlib import Path


EXPECTED_MANIFEST = "519364561e03e351b36c3aca0d9a986cf240e420183baa25d3c22ff1513f8257"
EXPECTED_COUNTS = {
    "passive_definitions": 9731,
    "graph_instances": 5152,
    "graph_stat_entries": 5957,
    "class_overrides": 61,
    "effects": 8349,
    "qualities": 429,
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(root, name):
    return json.loads((root / name).read_bytes())


def compare(source, first, second):
    manifest_bytes = (source / "manifest.json").read_bytes()
    assert sha(manifest_bytes) == EXPECTED_MANIFEST, "wrong external source receipt"
    receipt = load(first, "candidate-receipt.json")
    assert receipt["input"]["manifest_sha256"] == EXPECTED_MANIFEST
    assert receipt["production_ready"] is False
    assert receipt["status"] == "partial-candidate"
    assert {p.name for p in first.iterdir()} == {
        "candidate-receipt.json", *receipt["files"]
    }
    assert {p.name for p in second.iterdir()} == {p.name for p in first.iterdir()}
    total_bytes = 0
    for path in first.iterdir():
        data = path.read_bytes()
        assert data == (second / path.name).read_bytes(), f"nondeterministic {path.name}"
        total_bytes += len(data)
        if path.name != "candidate-receipt.json":
            assert receipt["files"][path.name] == {"bytes": len(data), "sha256": sha(data)}
    assert (first / "source-manifest.json").read_bytes() == manifest_bytes
    assert sha((first / "source-lock.json").read_bytes()) == receipt["input"]["source_identity"]["source_lock_sha256"]
    assert (first / "source-diagnostics.json").read_bytes() == (source / "diagnostics.json").read_bytes()

    candidate = load(first, "candidate.json")
    passives = load(source, "passives.json")
    graph = load(source, "graph.json")
    skills = load(source, "skills.json")
    tables = {p.stem: load(source, f"tables/{p.name}") for p in (source / "tables").iterdir()}

    def raw(ref):
        assert ref["payload"].startswith("tables/")
        table = tables[Path(ref["payload"]).stem]
        parts = ref["pointer"].split("/")
        assert parts[1] == "rows" and parts[3] == "values" and len(parts) == 4
        return table["rows"][int(parts[2])]["values"]

    assert candidate["input_manifest_sha256"] == EXPECTED_MANIFEST
    assert candidate["passive_order"] == [r["values"]["Id"] for r in tables["PassiveSkills"]["rows"]]
    assert set(candidate["passives"]) == set(candidate["passive_order"])
    for definition in passives["definitions"]:
        key = definition["definition"]["id"]
        node = candidate["passives"][key]
        physical = raw(node["raw"])
        assert physical["Id"] == key
        assert node["graph_id_raw"] == physical["PassiveSkillGraphId"] == definition["graph_id"]
        assert node["in_default_graph"] == definition["in_default_graph"]
        assert len(node["stats"]) == len(physical["Stats"])
        for slot, pair in enumerate(node["stats"], 1):
            assert pair["slot"] == slot and pair["value_field"] == f"Stat{slot}Value"
            assert pair["value"] == physical[pair["value_field"]]
            assert pair["stat_id"] == tables["Stats"]["rows"][physical["Stats"][slot - 1]]["values"]["Id"]
            assert raw(pair["raw_stat"])["Id"] == pair["stat_id"]
    fifth = candidate["passives"]["energy_shield_and_armour44"]["stats"][4]
    assert (fifth["stat_id"], fifth["slot"], fifth["value"]) == ("recover_X_life_on_block", 5, 10)
    assert candidate["passives"]["energy_shield_and_armour44"]["graph_id_raw"] == 31955

    assert len(candidate["instances"]) == len(passives["default_graph_instances"])
    for instance, original in zip(candidate["instances"], passives["default_graph_instances"], strict=True):
        assert instance == {
            "graph_id": original["graph_id"], "definition_id": original["definition"]["id"],
            "group": original["group"], "radius_raw": original["radius"],
            "position_raw": original["position"], "connections": original["connections"], "splines": original["splines"],
        }
    normalized_graph = candidate["graph"]
    assert normalized_graph["raw"] == {"payload": "graph.json", "pointer": ""}
    assert normalized_graph["tree_id"] == "Default"
    assert raw(normalized_graph["raw_tree"])["Id"] == "Default"
    assert normalized_graph["header"] == graph["header"]
    assert normalized_graph["skills_per_orbit_raw"] == graph["skills_per_orbit"]
    assert normalized_graph["roots"] == graph["roots"]
    for group, original in zip(normalized_graph["groups"], graph["groups"], strict=True):
        for field in ["x", "y", "flag"]:
            assert group[f"{field}_raw"] == original[field]
        for field in ["unknown1", "unknown2"]:
            assert group[field] == original[field]
        assert group["node_order"] == [n["graph_id"] for n in original["nodes"]]
    assert sum(g["unknown1"] != 0 for g in normalized_graph["groups"]) == 29

    for relation, original in zip(candidate["class_relations"], passives["class_overrides"], strict=True):
        assert relation["character_id"] == original["character"]["id"] == raw(relation["raw_character"])["Id"]
        assert relation["base_definition_id"] == original["base"]["id"]
        assert relation["replacement_definition_id"] == original["replacement"]["id"]
        for field in ["base_graph_id", "replacement_graph_id", "base_in_default_graph", "replacement_in_default_graph"]:
            assert relation[field] == original[field]
        assert raw(relation["raw"]) == tables["ClassPassiveSkillOverrides"]["rows"][original["row"]]["values"]
    absent = [(r["base_graph_id"], r["replacement_graph_id"]) for r in candidate["class_relations"] if not r["base_in_default_graph"]]
    assert absent == [(55194, 57601), (19680, 40837)]
    assert all(not r["replacement_in_default_graph"] for r in candidate["class_relations"])

    assert candidate["stat_set_order"] == [r["values"]["Id"] for r in tables["GrantedEffectStatSets"]["rows"]]
    assert len(candidate["stat_sets"]) == 8283
    for key, stat_set in candidate["stat_sets"].items():
        assert raw(stat_set["raw"])["Id"] == key
    assert candidate["effect_order"] == [r["values"]["Id"] for r in tables["GrantedEffects"]["rows"]]
    for original in skills["effects"]:
        key = original["effect"]["id"]
        effect = candidate["effects"][key]
        assert raw(effect["raw"])["Id"] == key
        for link, old in zip(effect["ordered_stat_sets"], original["ordered_stat_sets"], strict=True):
            assert link == {"stat_set_id": old["stat_set"]["id"], **{k: old[k] for k in ["source_field", "source_slot", "local_index_hypothesis"]}}
    for quality, original in zip(candidate["qualities"], skills["qualities"], strict=True):
        assert quality["effect_id"] == original["effect"]["id"]
        physical = raw(quality["raw"])
        assert tables["GrantedEffects"]["rows"][physical["GrantedEffect"]]["values"]["Id"] == quality["effect_id"]
        for channel, fields in [("main", ["ApplyToStatSets", "Stats", "StatsValuesPermille"]), ("alternate", ["AltApplyToStatSets", "AltStats", "AltStatValuesPermille"])]:
            assert quality[channel]["scope_raw"] == original[channel]["scope_raw"] == physical[fields[0]]
            assert len(quality[channel]["stats"]) == len(physical[fields[1]])
            for pair, old in zip(quality[channel]["stats"], original[channel]["stats"], strict=True):
                assert pair["stat_id"] == old["stat"]["id"] == raw(pair["raw_stat"])["Id"]
                assert pair["slot"] == old["slot"]
                assert pair["value_permille"] == old["value_permille"] == physical[fields[2]][pair["slot"]]
    snipe = candidate["qualities"][69]
    assert snipe["effect_id"] == "SnipePlayer" and snipe["raw"]["pointer"] == "/rows/69/values"
    assert snipe["main"]["scope_raw"] == [1, 2, 3] and snipe["alternate"]["scope_raw"] == [0]
    assert [s["value_permille"] for s in snipe["main"]["stats"]] == [200]
    assert [s["value_permille"] for s in snipe["alternate"]["stats"]] == [1000, 1000]
    assert [s["stat_set_id"] for s in candidate["effects"]["SnipePlayer"]["ordered_stat_sets"]] == ["SnipePlayer", "SnipeExplosionPlayer", "SnipeFrozenExplosionPlayer"]
    diagnostics = load(first, "source-diagnostics.json")["diagnostics"]
    assert len(diagnostics) == 61
    snipe_diagnostics = [d for d in diagnostics if d["code"] == "quality_scope_out_of_range_under_local_index_hypothesis" and d.get("quality_row") == 69]
    assert snipe_diagnostics, "lost Snipe discrepancy"

    measured = {
        "passive_definitions": len(candidate["passives"]),
        "graph_instances": len(candidate["instances"]),
        "graph_stat_entries": sum(len(candidate["passives"][i["definition_id"]]["stats"]) for i in candidate["instances"]),
        "class_overrides": len(candidate["class_relations"]),
        "effects": len(candidate["effects"]), "qualities": len(candidate["qualities"]),
    }
    assert measured == EXPECTED_COUNTS == candidate["summary"] == receipt["summary"]
    print(json.dumps({"measured": measured, "stat_sets": len(candidate["stat_sets"]),
        "source_diagnostics": len(diagnostics), "output_bytes": total_bytes,
        "receipt_sha256": sha((first / "candidate-receipt.json").read_bytes()),
        "identical_independent_outputs": True, "snipe_discrepancy_retained": True}, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-facts", type=Path, required=True)
    parser.add_argument("--candidate-a", type=Path, required=True)
    parser.add_argument("--candidate-b", type=Path, required=True)
    args = parser.parse_args()
    compare(args.native_facts, args.candidate_a, args.candidate_b)
