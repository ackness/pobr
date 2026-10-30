#!/usr/bin/env python3
"""Validate an agent PR handoff; this proves record freshness, not independence."""

import json
import os
from pathlib import Path
import re
import subprocess


MARKER = re.compile(r"<!--\s*pobr-audit\s*\n(.*?)\n\s*-->", re.DOTALL)
SECTIONS = ("Problem", "Change", "Validation", "Remaining limits")


def validate(pr):
    if pr.get("draft"):
        return "Draft: complete the handoff before marking ready for review."
    body = pr.get("body") or ""
    prose = re.sub(r"<!--.*?-->", "", body, flags=re.DOTALL)
    for section in SECTIONS:
        match = re.search(rf"^## {section}\s*\n(.*?)(?=^## |\Z)", prose, re.MULTILINE | re.DOTALL)
        value = match.group(1).strip() if match else ""
        if not value or re.fullmatch(r"(?:todo|tbd|n/?a|[-\[\] x\s])+", value, re.IGNORECASE):
            raise ValueError(f"Complete the '{section}' section with concrete evidence (limits may be 'none')")
    records = MARKER.findall(body)
    if len(records) != 1:
        raise ValueError("Ready PR needs exactly one pobr-audit JSON record")
    audit = json.loads(records[0])
    required = {"head", "base", "reviewer", "verdict", "findings"}
    if not isinstance(audit, dict) or set(audit) != required:
        raise ValueError("Audit fields must be head, base, reviewer, verdict and findings")
    for side in ("head", "base"):
        if audit[side] != pr[side]["sha"]:
            raise ValueError(f"Audit {side} is stale; review the current head/base and update the record")
    if not isinstance(audit["reviewer"], str) or not audit["reviewer"].strip():
        raise ValueError("Name the reviewer model/session in the audit record")
    if audit["verdict"] != "pass" or not isinstance(audit["findings"], list):
        raise ValueError("Audit must pass with an explicit findings array")
    for finding in audit["findings"]:
        if (not isinstance(finding, dict) or set(finding) != {"severity", "status", "summary"}
                or finding["severity"] not in {"blocking", "suggestion"}
                or finding["status"] not in {"open", "resolved"}
                or not isinstance(finding["summary"], str) or not finding["summary"].strip()):
            raise ValueError("Each finding needs severity, status and a concrete summary")
        if finding["severity"] == "blocking" and finding["status"] != "resolved":
            raise ValueError("Resolve blocking review findings before marking ready")
    return "Handoff is complete and audit matches this head/base. Independence remains a maintainer responsibility."


def validate_current(event_pr, current):
    if event_pr["head"]["sha"] != current["head"]["sha"]:
        raise ValueError("This run belongs to an older PR head; use the current run")
    return validate(current)


def main():
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    try:
        event_pr = event["pull_request"]
        endpoint = f"repos/{event['repository']['full_name']}/pulls/{event_pr['number']}"
        current = json.loads(subprocess.check_output(["gh", "api", endpoint], text=True))
        print(validate_current(event_pr, current))
    except (ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"PR handoff rejected: {error}") from error


if __name__ == "__main__":
    main()
