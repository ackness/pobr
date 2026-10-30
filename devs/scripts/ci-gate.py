#!/usr/bin/env python3
"""Fail unless every selected check succeeded and every skip was planned."""

import argparse
import json
import os

from pathlib import Path
import importlib.util

_spec = importlib.util.spec_from_file_location("ci_plan", Path(__file__).with_name("ci-plan.py"))
_policy = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_policy)


def check(needs, mode):
    if mode == "full":
        expected = {name: True for name in ("lint", "ci", "web", "scripts")}
        if set(needs) != set(expected):
            raise ValueError("Full gate dependencies are missing or unexpected")
    else:
        if set(needs) != {"changes", *_policy.AREAS}:
            raise ValueError("PR gate dependencies are missing or unexpected")
        changes = needs["changes"]
        if changes.get("result") != "success":
            raise ValueError("Change planning did not succeed")
        outputs = changes.get("outputs", {})
        if any(outputs.get(area) not in ("true", "false") for area in _policy.AREAS):
            raise ValueError("Missing or malformed check plan")
        expected = {area: outputs[area] == "true" for area in _policy.AREAS}
        if expected["full"] and any(expected[area] for area in ("rust", "web", "scripts")):
            raise ValueError("Full validation must replace lightweight jobs")
    for name, selected in expected.items():
        result = needs[name].get("result")
        if result != "success" and not (result == "skipped" and not selected):
            raise ValueError(f"{name}: expected {'success' if selected else 'success or planned skip'}, got {result}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pr", "full"))
    args = parser.parse_args()
    try:
        check(json.loads(os.environ["NEEDS_JSON"]), args.mode)
    except (ValueError, KeyError, TypeError) as error:
        raise SystemExit(f"Gate rejected: {error}") from error
    print("All selected checks passed.")


if __name__ == "__main__":
    main()
