#!/usr/bin/env python3
"""Read-only final PR preflight; print the current head for an atomic merge."""

import json
import subprocess
import sys

REQUIRED = {"PR gate"}


def gh(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def assess(pr, checks):
    if pr.get("draft") or pr.get("state") != "open" or pr.get("mergeable") is not True:
        raise ValueError("PR must be open, ready and conflict-free; unknown mergeability needs a later retry")
    by_name = {check["name"]: check for check in checks}
    if not REQUIRED <= by_name.keys():
        raise ValueError("PR gate must exist on this PR")
    if any(by_name[name].get("bucket") != "pass" for name in REQUIRED):
        raise ValueError("PR gate must pass, not skip or remain pending")
    if any(check.get("bucket") in {"fail", "cancel", "pending"} for check in checks):
        raise ValueError("PR still has failing, cancelled or pending checks")


def main():
    if len(sys.argv) != 2 or not sys.argv[1].isdigit():
        raise SystemExit("usage: mise run pr-ready <PR-number> (read-only; does not merge)")
    number = sys.argv[1]
    try:
        repo = gh("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
        endpoint = f"repos/{repo}/pulls/{number}"
        before = gh("api", endpoint)
        checks = gh("pr", "checks", number, "--json", "name,state,bucket,link")
        assess(before, checks)
        after = gh("api", endpoint)
        assess(after, checks)
        if any(before[side]["sha"] != after[side]["sha"] for side in ("head", "base")):
            raise ValueError("PR changed during preflight; repeat review/checks for the new version")
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"PR not ready: {error}") from error
    head = after["head"]["sha"]
    print(f"Current head: {head}\nBase: {after['base']['sha']}")
    print(f"If merging is authorized: gh pr merge {number} --squash --match-head-commit {head}")


if __name__ == "__main__":
    main()
