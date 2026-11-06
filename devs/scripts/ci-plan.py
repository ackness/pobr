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
    "devs/scripts/install-actionlint.py",
    "devs/scripts/ci-rust-tests.py",
    "devs/scripts/setup-environment.sh", "devs/scripts/pob2-luajit.sh",
}
DOCUMENTATION_SIDECARS = {"apps/pobr-wasm/tests/fixtures/pob2-planner.md"}
DATA_LOCK = "pipeline/data.lock.json"
DATA_SMOKE_EVENTS = ("push", "workflow_dispatch")
# Web files generated from game data; the automation contracts check them
# against the prepared snapshot (test_workflows.py).
DATA_DERIVED_WEB = {"web/src/lib/configGameRelevance.json"}


def documentation(name):
    # Markdown fixtures may be executable test inputs; ordinary guides are inert
    # regardless of whether they live under crates/, apps/, examples/ or web/.
    if name.endswith(".md"):
        return ("/fixtures/" not in name
                or Path(name).name.lower().startswith("readme")
                or name in DOCUMENTATION_SIDECARS)
    return name.startswith((".assets/", "docs/")) and name.endswith((".png", ".jpg", ".jpeg", ".svg", ".webp"))


def classify(files):
    plan = dict.fromkeys(AREAS, False)
    for name in files:
        if documentation(name):
            continue
        if (name in FULL_FILES or name.endswith("/Cargo.toml")
                or name.startswith((".github/workflows/", ".github/actions/", ".cargo/"))
                or name.startswith("pipeline/")):
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
            if (name.startswith(("web/scripts/", "web/public/", "web/worker-tests/"))
                    or name in DATA_DERIVED_WEB):
                plan["scripts"] = True
        elif name.endswith(".txt") and "/fixtures/" not in name:
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


def plan_for_event(files, event):
    if files is None:
        if event == "pull_request":
            raise ValueError("PR comparison base unavailable; cannot select checks safely")
        return {area: area == "full" for area in AREAS}
    affected = classify(files)
    if event == "pull_request":
        # Shared/unknown inputs get every quick check. Integration runs on master
        # or explicitly on this pushed ref, rather than rebuilding every PR edit.
        if affected["full"]:
            return {area: area != "full" for area in AREAS}
        return affected
    if event == "push":
        return {area: area == "full" and any(affected.values()) for area in AREAS}
    return {area: area == "full" for area in AREAS}


def data_smoke(files, event, ref_type):
    """Whether full CI also runs the data-bump Playwright smoke (web/e2e/data-bump-smoke.spec.ts).

    It runs when the tested range changes the data lock: a branch push (master integration)
    or a manual run on a branch (`mise run ci <ref>`, compared with the default branch).
    Tags, nightly runs and calls without a branch range never select it; an unknown range on
    a branch does, because a data bump must not slip through unchecked.
    """
    if ref_type != "branch" or event not in DATA_SMOKE_EVENTS:
        return False
    return files is None or DATA_LOCK in files


def changed_files(base, head="HEAD", cwd=None):
    if not base or set(base) == {"0"}:
        return None
    result = subprocess.run(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, head, "--"],
        cwd=cwd, capture_output=True,
    )
    if result.returncode:
        print("Comparison base unavailable.")
        return None
    return [name.decode("utf-8", errors="surrogateescape")
            for name in result.stdout.split(b"\0") if name]


def local_changed_files(cwd=None):
    """Include the branch range, index, working tree and non-ignored new files."""
    def git(*args):
        result = subprocess.run(["git", *args], cwd=cwd, capture_output=True)
        if result.returncode:
            raise ValueError(f"git {' '.join(args)} failed: "
                             + result.stderr.decode(errors="replace").strip())
        return result.stdout

    try:
        base = git("merge-base", "origin/master", "HEAD").decode().strip()
    except ValueError as error:
        raise ValueError(f"{error}; run `git fetch origin master` to refresh the comparison base") from error
    files = set()
    for args in (
        ("diff", "--no-renames", "--name-only", "-z", base, "HEAD", "--"),
        ("diff", "--no-renames", "--name-only", "-z", "--cached", "HEAD", "--"),
        ("diff", "--no-renames", "--name-only", "-z", "--"),
        ("ls-files", "--others", "--exclude-standard", "-z"),
    ):
        files.update(name.decode("utf-8", errors="surrogateescape")
                     for name in git(*args).split(b"\0") if name)
    return sorted(files)


def emit(plan, title="Selected checks"):
    lines = [f"{key}={str(value).lower()}" for key, value in plan.items()]
    print("\n".join(lines))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write("\n".join(lines) + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as out:
            out.write(f"### {title}\n\n" + "\n".join(f"- `{line}`" for line in lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event", type=Path, default=os.environ.get("GITHUB_EVENT_PATH"))
    parser.add_argument("--data-smoke", metavar="BASE", nargs="?", const="",
                        help="instead of the check plan, decide the data-bump smoke against BASE")
    args = parser.parse_args()
    event = json.loads(args.event.read_text()) if args.event else {}
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if args.data_smoke is not None:
        ref_type = os.environ.get("GITHUB_REF_TYPE")
        ranged = ref_type == "branch" and event_name in DATA_SMOKE_EVENTS
        files = changed_files(args.data_smoke) if ranged else []
        emit({"run": data_smoke(files, event_name, ref_type)}, "Data-bump smoke")
        return
    if event_name == "pull_request":
        # HEAD is the synthetic merge under test; its first parent is its base.
        base, head = "HEAD^1", "HEAD"
    elif event_name == "push":
        base, head = event.get("before"), event.get("after", "HEAD")
    else:
        base, head = None, "HEAD"
    files = changed_files(base, head)
    try:
        emit(plan_for_event(files, event_name))
    except ValueError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
