"""Consume completed ordinary outputs without shipping their build inputs.

The producer authenticates raw inputs. This boundary checks the exact declared
job/lock, the pinned handoff, producer receipts and the eight replacement bytes.
It also works on the outputs-only artifact transferred between CI jobs.
"""
from pathlib import Path

from pipeline.runtime.ordinary import ordinary_inputs as ordinary
from pipeline.runtime.data_bundle import decode, relative_path
from pipeline.imports.data_imports import encoded, pinned, regular, require, validate_pin
from pipeline.runtime.ordinary.ordinary_inputs import pin


def overlay(files, version, target, config, config_dir, source):
    require(isinstance(config, dict) and set(config) == {"requirements", "lock", "outputs"},
            "Invalid ordinary runtime selection")
    request, lock = [decode(regular(Path(config_dir) / relative_path(config[key])))
                     for key in ("requirements", "lock")]
    ordinary.plan(request, lock)
    require(request["target"] == target and target["game_patch"] == version,
            "Ordinary runtime target mismatch")
    declared = {job["domain"]: sorted(ordinary.PROFILES[job["domain"]][1]) for job in request["domains"]}
    require(config["outputs"] == declared, "Ordinary output ownership mismatch")
    require(source is not None, "Declared ordinary production requires --ordinary")
    source = Path(source)
    completed_raw = regular(source / "completed.json")
    completed = decode(completed_raw)
    require(completed.get("schema") == "pobr-ordinary-production/v1"
            and completed.get("status") == "completed-declared-domains"
            and completed.get("target") == target
            and completed.get("requirements") == pin(encoded(request))
            and completed.get("lock") == pin(encoded(lock))
            and completed.get("production_ready") is False and completed.get("adoption") == "not-adopted"
            and completed.get("retained") == request["retained"], "Ordinary completion identity mismatch")
    if "source_verifier" in lock:
        require(completed.get("source_verifier") == {"schema": "pobr-source-verifier-use/v1",
                                                   "library": lock["source_verifier"]},
                "Ordinary source verifier identity mismatch")
    validate_pin(completed["binary"])
    handoff_name = ordinary.HANDOFF_INPUT
    handoff_raw = pinned(regular(source / "inputs" / handoff_name / handoff_name),
                         lock["inputs"][handoff_name]["files"][handoff_name])
    handoff = decode(handoff_raw)
    require(handoff.get("schema") == "pobr-ordinary-source-handoff/v1"
            and completed["facts_manifest"] == handoff["facts_manifest"]
            and completed["source_lock"] == handoff["source_lock"], "Ordinary source handoff mismatch")
    require(set(completed["domains"]) == set(declared), "Incomplete ordinary domains")
    result, replacements = dict(files), {}
    receipt_target = {"game": target["game"], "realm": target["realm"], "version": version}
    for job in request["domains"]:
        domain = job["domain"]
        summary = completed["domains"][domain]
        receipt = decode(pinned(regular(source / domain / "producer-receipt.json"), summary["receipt"]))
        require(receipt.get("format") == "pobr-ordinary-facts-receipt/v1"
                and receipt.get("status") == "domain-candidate"
                and receipt.get("target") == receipt_target
                and receipt.get("complete_runtime") is False and receipt.get("adoption") == "not-adopted"
                and receipt.get("adapter") == summary["adapter"], "Ordinary receipt identity mismatch")
        origin = receipt["input"]
        ordinary.identity(origin["source_identity"], target)
        require(origin["manifest_sha256"] == completed["facts_manifest"]["sha256"]
                and set(origin["selected_payloads"]) == set(job["selected_facts"])
                and origin["selected_payloads"]["source-lock.json"] == completed["source_lock"]
                and receipt["files"].get("source-manifest.json") == completed["facts_manifest"]
                and receipt["files"].get("source-lock.json") == completed["source_lock"],
                "Ordinary receipt source mismatch")
        outputs = {name: value for name, value in receipt["files"].items()
                   if name.startswith(("base/", "overlay/"))}
        require(set(outputs) == set(declared[domain]) and outputs == summary["outputs"],
                "Ordinary receipt output mismatch")
        for name, expected in outputs.items():
            full_name = f"data/{version}/{name}"
            require(full_name in files and name not in replacements, "Ordinary output is not a unique replacement")
            raw = pinned(regular(source / domain / relative_path(name)), expected)
            # Do not parse/re-encode game numbers here. The assembler's lossless
            # projection will validate JSON and preserve exact number tokens.
            result[full_name] = raw
            replacements[name] = {"domain": domain, "previous": pin(files[full_name]), "output": expected}
    evidence = {"completion": pin(completed_raw), "requirements": completed["requirements"],
                "lock": completed["lock"], "handoff": pin(handoff_raw), "binary": completed["binary"],
                "facts_manifest": completed["facts_manifest"], "source_lock": completed["source_lock"],
                "domains": completed["domains"], "replacements": replacements,
                "scope": "Only declared business outputs replace frozen runtime paths; all others retained.",
                "adoption": "not-adopted"}
    return result, evidence
