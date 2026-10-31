"""Independent final-profile candidate comparison; no decoder or runtime certification."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import struct


EXPECTED_MANIFEST = "30b42aa2b79e1810bf44f856b924d929a3e36fffe5a9afc264b7712c5dd2f968"
EXPECTED_PRODUCER = "b6def2152dfc48d8ec49c7a237b2e973c3830e97"
TABLES = (
    "Characters", "ClassPassiveSkillOverrides", "GrantedEffectQualityStats",
    "GrantedEffectStatSets", "GrantedEffects", "PassiveSkillTrees", "PassiveSkills", "Stats",
)
PAYLOADS = {*(f"tables/{table}.json" for table in TABLES),
            "diagnostics.json", "graph.json", "passives.json", "skills.json"}
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


def decode(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            assert key not in result, f"duplicate JSON key: {key}"
            result[key] = value
        return result

    def nonfinite(value):
        raise AssertionError(f"nonfinite JSON number: {value}")

    return json.loads(data, object_pairs_hook=unique, parse_constant=nonfinite)


def read_regular(path):
    assert stat.S_ISREG(path.lstat().st_mode), f"not a regular file: {path}"
    # Bytes, not a subsequent reopen of this pathname, become the comparison evidence.
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
    with os.fdopen(fd, "rb") as stream:
        assert stat.S_ISREG(os.fstat(stream.fileno()).st_mode), f"not a regular file: {path}"
        return stream.read()


def verify_source(source):
    source = source.absolute()
    assert ".." not in source.parts, "source traversal is forbidden"
    for directory in [*reversed(source.parents), source, source / "tables"]:
        assert stat.S_ISDIR(directory.lstat().st_mode), f"not a directory (symlinks forbidden): {directory}"
    manifest_bytes = read_regular(source / "manifest.json")
    assert sha(manifest_bytes) == EXPECTED_MANIFEST, "wrong external source receipt"
    manifest = decode(manifest_bytes)
    assert set(manifest["files"]) == PAYLOADS, "unexpected source manifest inventory"
    actual = set()
    for directory in [source, source / "tables"]:
        for child in directory.iterdir():
            name = child.relative_to(source).as_posix()
            mode = child.lstat().st_mode
            assert (stat.S_ISDIR(mode) if name == "tables" else stat.S_ISREG(mode)), f"invalid source entry: {name}"
            actual.add(name)
    assert actual == {"manifest.json", "tables", *PAYLOADS}, "unexpected source path inventory"
    retained = {}
    for name in sorted(PAYLOADS):
        data = read_regular(source / name)
        assert manifest["files"][name] == {"bytes": len(data), "sha256": sha(data)}, f"source payload fingerprint mismatch: {name}"
        retained[name] = data
    # Authenticate the entire inventory before parsing even one canonical payload.
    documents = {name: decode(data) for name, data in retained.items()}
    return manifest_bytes, manifest, retained, documents


def compare(source, first, second):
    manifest_bytes, manifest, source_bytes, documents = verify_source(source)
    output_bytes = {p.name: read_regular(p) for p in first.iterdir()}
    receipt = decode(output_bytes["candidate-receipt.json"])
    assert receipt["input"]["manifest_sha256"] == EXPECTED_MANIFEST
    assert receipt["input"]["producer_revision"] == EXPECTED_PRODUCER
    assert receipt["input"]["source_identity"] == manifest["source_identity"]
    assert receipt["production_ready"] is False
    assert receipt["status"] == "partial-candidate"
    assert set(output_bytes) == {
        "candidate-receipt.json", *receipt["files"]
    }
    assert {p.name for p in second.iterdir()} == set(output_bytes)
    total_bytes = 0
    for name, data in output_bytes.items():
        assert data == read_regular(second / name), f"nondeterministic {name}"
        total_bytes += len(data)
        if name != "candidate-receipt.json":
            assert receipt["files"][name] == {"bytes": len(data), "sha256": sha(data)}
    assert output_bytes["source-manifest.json"] == manifest_bytes
    assert sha(output_bytes["source-lock.json"]) == receipt["input"]["source_identity"]["source_lock_sha256"]
    assert output_bytes["source-diagnostics.json"] == source_bytes["diagnostics.json"]

    candidate = decode(output_bytes["candidate.json"])
    passives = documents["passives.json"]
    graph = documents["graph.json"]
    skills = documents["skills.json"]
    tables = {name: documents[f"tables/{name}.json"] for name in TABLES}

    def check_refs(value):
        if isinstance(value, dict):
            if set(value) == {"payload", "pointer"}:
                target = documents[value["payload"]]
                for part in value["pointer"].split("/")[1:]:
                    part = part.replace("~1", "/").replace("~0", "~")
                    target = target[int(part)] if isinstance(target, list) else target[part]
                return 1
            return sum(check_refs(child) for child in value.values())
        if isinstance(value, list):
            return sum(check_refs(child) for child in value)
        return 0

    reference_count = check_refs(candidate)

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
    physical_by_graph = {}
    for row in tables["PassiveSkills"]["rows"]:
        physical_by_graph.setdefault(row["values"]["PassiveSkillGraphId"], []).append(row)
    instances = {i["graph_id"]: i for i in candidate["instances"]}
    assert len(instances) == len(candidate["instances"])
    for instance, original in zip(candidate["instances"], passives["default_graph_instances"], strict=True):
        physical = physical_by_graph[instance["graph_id"]]
        assert len(physical) == 1
        assert original["definition"] == {"row": physical[0]["_row"], "id": physical[0]["values"]["Id"]}
        assert instance == {
            "graph_id": original["graph_id"], "definition_id": original["definition"]["id"],
            "group": original["group"], "radius_raw": original["radius"],
            "position_raw": original["position"], "connections": original["connections"], "splines": original["splines"],
        }
    normalized_graph = candidate["graph"]
    assert normalized_graph["raw"] == {"payload": "graph.json", "pointer": ""}
    assert normalized_graph["tree_id"] == "Default"
    assert raw(normalized_graph["raw_tree"])["Id"] == "Default"
    assert raw(normalized_graph["raw_tree"])["PassiveSkillGraph"] + ".psg" == graph["raw_resource"]
    assert normalized_graph["header"] == graph["header"]
    assert normalized_graph["skills_per_orbit_raw"] == graph["skills_per_orbit"]
    assert normalized_graph["roots"] == graph["roots"]
    seen = set()
    for index, (group, original) in enumerate(zip(normalized_graph["groups"], graph["groups"], strict=True)):
        for field in ["x", "y"]:
            assert struct.pack("!d", group[f"{field}_raw"]) == struct.pack("!d", original[field])
        assert group["flag_raw"] == original["flag"]
        for field in ["unknown1", "unknown2"]:
            assert group[field] == original[field]
        assert group["node_order"] == [n["graph_id"] for n in original["nodes"]]
        for node in original["nodes"]:
            assert node["graph_id"] not in seen
            seen.add(node["graph_id"])
            instance = instances[node["graph_id"]]
            assert instance["group"] == index == original["group"]
            assert instance["radius_raw"] == node["radius"] and instance["position_raw"] == node["position"]
            assert instance["connections"] == node["connections"] and instance["splines"] == node["splines"]
            assert len(node["connections"]) == len(node["splines"])
            assert all(edge in instances for edge in node["connections"])
    assert seen == set(instances)
    assert all(root["graph_id"] in instances for root in graph["roots"])
    for definition in candidate["passives"].values():
        assert definition["in_default_graph"] == (definition["graph_id_raw"] in instances)
    assert sum(g["unknown1"] != 0 for g in normalized_graph["groups"]) == 29

    for relation, original in zip(candidate["class_relations"], passives["class_overrides"], strict=True):
        assert relation["character_id"] == original["character"]["id"] == raw(relation["raw_character"])["Id"]
        assert relation["base_definition_id"] == original["base"]["id"]
        assert relation["replacement_definition_id"] == original["replacement"]["id"]
        for field in ["base_graph_id", "replacement_graph_id", "base_in_default_graph", "replacement_in_default_graph"]:
            assert relation[field] == original[field]
        physical = raw(relation["raw"])
        assert physical == tables["ClassPassiveSkillOverrides"]["rows"][original["row"]]["values"]
        assert relation["character_id"] == tables["Characters"]["rows"][physical["CharacterToOverrideFor"]]["values"]["Id"]
        for prefix, field in [("base", "SkillToOverride"), ("replacement", "Override")]:
            definition = tables["PassiveSkills"]["rows"][physical[field]]["values"]
            assert relation[f"{prefix}_definition_id"] == definition["Id"]
            assert relation[f"{prefix}_graph_id"] == definition["PassiveSkillGraphId"]
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
        physical = raw(effect["raw"])
        assert physical["Id"] == key
        links = []
        if physical["StatSet"] is not None:
            links.append(("StatSet", None, physical["StatSet"]))
        links.extend(("AdditionalStatSets", slot, row) for slot, row in enumerate(physical["AdditionalStatSets"]) if row is not None)
        assert [(link["source_field"], link["source_slot"], link["stat_set_id"]) for link in effect["ordered_stat_sets"]] == [
            (field, slot, tables["GrantedEffectStatSets"]["rows"][row]["values"]["Id"]) for field, slot, row in links]
        for link, old in zip(effect["ordered_stat_sets"], original["ordered_stat_sets"], strict=True):
            assert link == {"stat_set_id": old["stat_set"]["id"], **{k: old[k] for k in ["source_field", "source_slot", "local_index_hypothesis"]}}
    for quality, original in zip(candidate["qualities"], skills["qualities"], strict=True):
        assert quality["effect_id"] == original["effect"]["id"]
        physical = raw(quality["raw"])
        assert tables["GrantedEffects"]["rows"][physical["GrantedEffect"]]["values"]["Id"] == quality["effect_id"]
        for channel, fields in [("main", ["ApplyToStatSets", "Stats", "StatsValuesPermille"]), ("alternate", ["AltApplyToStatSets", "AltStats", "AltStatValuesPermille"])]:
            assert quality[channel]["scope_raw"] == original[channel]["scope_raw"] == physical[fields[0]]
            assert len(quality[channel]["stats"]) == len(physical[fields[1]]) == len(physical[fields[2]])
            for pair, old in zip(quality[channel]["stats"], original[channel]["stats"], strict=True):
                assert pair["stat_id"] == old["stat"]["id"] == raw(pair["raw_stat"])["Id"]
                assert pair["stat_id"] == tables["Stats"]["rows"][physical[fields[1]][pair["slot"]]]["values"]["Id"]
                assert pair["slot"] == old["slot"]
                assert pair["value_permille"] == old["value_permille"] == physical[fields[2]][pair["slot"]]
    snipe = candidate["qualities"][69]
    assert snipe["effect_id"] == "SnipePlayer" and snipe["raw"]["pointer"] == "/rows/69/values"
    assert snipe["main"]["scope_raw"] == [1, 2, 3] and snipe["alternate"]["scope_raw"] == [0]
    assert [s["value_permille"] for s in snipe["main"]["stats"]] == [200]
    assert [s["value_permille"] for s in snipe["alternate"]["stats"]] == [1000, 1000]
    assert [s["stat_set_id"] for s in candidate["effects"]["SnipePlayer"]["ordered_stat_sets"]] == ["SnipePlayer", "SnipeExplosionPlayer", "SnipeFrozenExplosionPlayer"]
    diagnostics = documents["diagnostics.json"]["diagnostics"]
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
        "receipt_sha256": sha(output_bytes["candidate-receipt.json"]),
        "sealed_payloads": len(source_bytes), "resolved_raw_references": reference_count,
        "identical_independent_outputs": True, "snipe_discrepancy_retained": True}, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-facts", type=Path, required=True)
    parser.add_argument("--candidate-a", type=Path, required=True)
    parser.add_argument("--candidate-b", type=Path, required=True)
    args = parser.parse_args()
    compare(args.native_facts, args.candidate_a, args.candidate_b)
