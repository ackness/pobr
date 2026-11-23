#!/usr/bin/env python3
"""Read-only final PR preflight; print the current head for an atomic merge."""

import json
import subprocess
import sys

REQUIRED = {"PR gate"}
FULL_GATE = "Full gate"
FULL_REQUIRED = REQUIRED | {FULL_GATE}


def gh(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def flatten_check_runs(payload):
    if isinstance(payload, dict):
        pages = [payload]
    elif isinstance(payload, list):
        pages = payload
    else:
        raise ValueError("unexpected commit check-runs payload")
    runs = []
    for page in pages:
        runs.extend(page.get("check_runs", []))
    return runs


def assess_full_gate(commit_check_runs):
    if any(run.get("status") != "completed" for run in commit_check_runs):
        raise ValueError("head commit still has pending check runs")
    full_runs = [run for run in commit_check_runs if run.get("name") == FULL_GATE]
    if not full_runs:
        raise ValueError("Full gate must exist on the PR head commit")
    latest = max(
        enumerate(full_runs),
        key=lambda item: (item[1].get("id") or 0, item[0]),
    )[1]
    if latest.get("conclusion") != "success":
        raise ValueError("Full gate on the PR head commit must pass, not fail, cancel or skip")


def assess(pr, checks, require_full=False, commit_checks=None):
    if pr.get("draft") or pr.get("state") != "open" or pr.get("mergeable") is not True:
        raise ValueError("PR must be open, ready and conflict-free; unknown mergeability needs a later retry")
    required = FULL_REQUIRED if (require_full and commit_checks is None) else REQUIRED
    by_name = {check["name"]: check for check in checks}
    missing = sorted(required - by_name.keys())
    if missing:
        raise ValueError(f"{', '.join(missing)} must exist on this PR")
    not_passing = sorted(name for name in required if by_name[name].get("bucket") != "pass")
    if not_passing:
        raise ValueError(f"{', '.join(not_passing)} must pass, not skip or remain pending")
    if any(check.get("bucket") in {"fail", "cancel", "pending"} for check in checks):
        raise ValueError("PR still has failing, cancelled or pending checks")
    if require_full and commit_checks is not None:
        assess_full_gate(commit_checks)


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
        commit_checks = None
        if require_full:
            head_sha = before["head"]["sha"]
            commit_checks = flatten_check_runs(
                gh("api", "--paginate", "--slurp", f"repos/{repo}/commits/{head_sha}/check-runs?per_page=100")
            )
        assess(before, checks, require_full=require_full, commit_checks=commit_checks)
        after = gh("api", endpoint)
        assess(after, checks, require_full=require_full, commit_checks=commit_checks)
        if any(before[side]["sha"] != after[side]["sha"] for side in ("head", "base")):
            raise ValueError("PR changed during preflight; repeat review/checks for the new version")
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"PR not ready: {error}") from error
    head = after["head"]["sha"]
    print(f"Current head: {head}\nBase: {after['base']['sha']}")
    print(f"If merging is authorized: gh pr merge {number} --merge --match-head-commit {head}")


if __name__ == "__main__":
    main()
