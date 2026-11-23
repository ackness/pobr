#!/usr/bin/env python3
"""Read-only final PR preflight; print the current head for an atomic merge."""

import json
import subprocess
import sys

REQUIRED = {"PR gate"}
FULL_REQUIRED = REQUIRED | {"Full gate"}


def gh(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def assess(pr, checks, require_full=False):
    if pr.get("draft") or pr.get("state") != "open" or pr.get("mergeable") is not True:
        raise ValueError("PR must be open, ready and conflict-free; unknown mergeability needs a later retry")
    required = FULL_REQUIRED if require_full else REQUIRED
    by_name = {check["name"]: check for check in checks}
    missing = sorted(required - by_name.keys())
    if missing:
        raise ValueError(f"{', '.join(missing)} must exist on this PR")
    not_passing = sorted(name for name in required if by_name[name].get("bucket") != "pass")
    if not_passing:
        raise ValueError(f"{', '.join(not_passing)} must pass, not skip or remain pending")
    if any(check.get("bucket") in {"fail", "cancel", "pending"} for check in checks):
        raise ValueError("PR still has failing, cancelled or pending checks")


def parse_args(argv):
    usage = "usage: mise run pr-ready <PR-number> [--require-full] (read-only; does not merge)"
    require_full = False
    positional = []
    for arg in argv:
        if arg == "--require-full":
            require_full = True
        elif arg.startswith("-"):
            raise SystemExit(usage)
        else:
            positional.append(arg)
    if len(positional) != 1 or not positional[0].isdigit():
        raise SystemExit(usage)
    return positional[0], require_full


def main():
    number, require_full = parse_args(sys.argv[1:])
    try:
        repo = gh("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
        endpoint = f"repos/{repo}/pulls/{number}"
        before = gh("api", endpoint)
        checks = gh("pr", "checks", number, "--json", "name,state,bucket,link")
        assess(before, checks, require_full=require_full)
        after = gh("api", endpoint)
        assess(after, checks, require_full=require_full)
        if any(before[side]["sha"] != after[side]["sha"] for side in ("head", "base")):
            raise ValueError("PR changed during preflight; repeat review/checks for the new version")
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"PR not ready: {error}") from error
    head = after["head"]["sha"]
    print(f"Current head: {head}\nBase: {after['base']['sha']}")
    print(f"If merging is authorized: gh pr merge {number} --merge --match-head-commit {head}")


if __name__ == "__main__":
    main()
