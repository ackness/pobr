#!/usr/bin/env python3
"""Select checks from the entire tested range. Unknown inputs select full CI."""

import argparse
import json
import os
from pathlib import Path
import subprocess


AREAS = ("rust", "web", "scripts", "full")
FULL_FILES = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "mise.toml", "mise.lock",
    "web/package.json", "web/pnpm-lock.yaml", "web/pnpm-workspace.yaml",
    ".github/dependabot.yml", "devs/scripts/ci-plan.py",
    "devs/scripts/ci-gate.py", "devs/scripts/pr-ready.py",
    "devs/scripts/release-policy.py", "devs/scripts/release-check.sh",
    "devs/scripts/check-workflows.sh", "devs/scripts/driver.sh",
    "devs/scripts/tool-versions.py", "devs/scripts/install-gh.py",
    "devs/scripts/setup-environment.sh", "devs/scripts/pob2-luajit.sh",
}


def classify(files):
    plan = dict.fromkeys(AREAS, False)
    for name in files:
        if (name in FULL_FILES or name.endswith("/Cargo.toml")
                or name.startswith((".github/workflows/", ".github/actions/", ".cargo/"))
                or (name.startswith("pipeline/") and not name.endswith((".md", ".txt")))):
            plan["full"] = True
        elif name.startswith(("crates/", "apps/", "tools/", "data/", "examples/")):
            plan["rust"] = True
            if name.startswith("data/"):
                plan["scripts"] = True
        elif name.startswith(("devs/scripts/", "devs/ci/")):
            plan["scripts"] = True
        elif name.startswith("web/"):
            # Only known documentation is inert; text/Markdown fixtures can be inputs.
            if name in {"web/README.md", "web/README.zh-CN.md"}:
                continue
            plan["web"] = True
            if name.startswith(("web/scripts/", "web/public/", "web/worker-tests/")):
                plan["scripts"] = True
        elif name.endswith((".md", ".txt")) or (name.startswith((".assets/", "docs/")) and name.endswith((".png", ".jpg", ".jpeg", ".svg", ".webp"))):
            continue
        elif name.startswith(".github/ISSUE_TEMPLATE/") or name in {
            ".github/pull_request_template.md", ".gitignore", "LICENSE", "LICENSE.md",
        }:
            continue
        else:
            plan["full"] = True
    if plan["full"]:
        return {area: area == "full" for area in AREAS}
    return plan


def changed_files(base, head="HEAD", cwd=None):
    if not base or set(base) == {"0"}:
        return None
    result = subprocess.run(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, head, "--"],
        cwd=cwd, capture_output=True,
    )
    if result.returncode:
        print("Comparison base unavailable; selecting full CI.")
        return None
    return [name.decode("utf-8", errors="surrogateescape")
            for name in result.stdout.split(b"\0") if name]


def emit(plan):
    lines = [f"{key}={str(value).lower()}" for key, value in plan.items()]
    print("\n".join(lines))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write("\n".join(lines) + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as out:
            out.write("### Selected checks\n\n" + "\n".join(f"- `{line}`" for line in lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event", type=Path, default=os.environ.get("GITHUB_EVENT_PATH"))
    args = parser.parse_args()
    event = json.loads(args.event.read_text()) if args.event else {}
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "pull_request":
        # HEAD is the synthetic merge under test; its first parent is its base.
        base, head = "HEAD^1", "HEAD"
    elif event_name == "push":
        base, head = event.get("before"), event.get("after", "HEAD")
    else:
        base, head = None, "HEAD"
    files = changed_files(base, head)
    emit({area: area == "full" for area in AREAS} if files is None else classify(files))


if __name__ == "__main__":
    main()
