"""Materialize PoBR interpretation policies without storing full version mappings.

Only the reviewed source-manifest pointer is rebound. The external retained tree
bindings are data, not executable policy or an authority to invent semantics.
"""

import copy
from pathlib import Path

from pipeline.runtime.data_bundle import decode
from pipeline.imports.data_imports import encoded, pinned, regular, require, write

ROOT = Path(__file__).resolve().parents[3]
SOURCES = {
    "stat-descriptions/codec-policy": "pipeline/policies/stat-descriptions/0.5.5.5-plain-display-policy.json",
    "passive-tree/codec-policy": "pipeline/policies/passive-tree/0.5.5.5-tree-policy.template.json",
    "base-items/codec-policy": "pipeline/policies/base-items/0.5.5.5-inherited-tags-policy.json",
    "minions/codec-policy": "pipeline/policies/actors/0.5.5.5-minions-prepared-policy.json",
    "spectres/codec-policy": "pipeline/policies/actors/0.5.5.5-spectres-prepared-policy.json",
    "skill-overrides/codec-policy": "pipeline/policies/skill-overrides/0.5.5.5-implicit-ownership-policy.json",
}
TREE_BINDINGS = "retained-policy-bindings/passive-tree-v4.json"


def expand_template(value, bindings):
    if isinstance(value, dict):
        if set(value) == {"$retained"}:
            pointer = value["$retained"]
            # Both forms retain the original JSON value: the dotted key form is
            # portable for handoffs; the nested form mirrors the source policy.
            if pointer in bindings:
                return copy.deepcopy(bindings[pointer])
            current = bindings
            for key in pointer.split("."):
                current = current[key]
            return copy.deepcopy(current)
        return {key: expand_template(child, bindings) for key, child in value.items()}
    if isinstance(value, list):
        return [expand_template(child, bindings) for child in value]
    return value


def materialize(lock, inputs, source_root=None):
    source_root = source_root or ROOT
    handoff = decode(regular(inputs / "handoff.json/handoff.json"))
    for key, declaration in lock["policies"].items():
        require(key in SOURCES, "Unknown materialized policy")
        source = pinned(regular(Path(source_root) / SOURCES[key]), declaration["source"])
        spec = lock["inputs"][key]
        if key == "passive-tree/codec-policy":
            source = encoded(expand_template(decode(source), decode(regular(inputs / TREE_BINDINGS))))
        original = decode(source)
        if "policy_rebinding" in spec:
            rebind = spec["policy_rebinding"]
            write(inputs / key / rebind["original"], pinned(source, spec["files"][rebind["original"]]))
            current = copy.deepcopy(original)
            old, new = original, current
            for name in rebind["pointer"][:-1]:
                old, new = old[name], new[name]
            field = rebind["pointer"][-1]
            require(old[field] == handoff["original"]["manifest"]["sha256"], "Policy original binding mismatch")
            new[field] = handoff["facts_manifest"]["sha256"]
            source = encoded(current)
        write(inputs / key / spec["path"], pinned(source, spec["files"][spec["path"]]))
