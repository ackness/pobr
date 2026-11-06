#!/usr/bin/env python3
"""Run local PR checks plus affected-package Rust lints and tests, offline."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ci_plan", ROOT / "devs/scripts/ci-plan.py")
planner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(planner)
CLIPPY_FEATURES = {
    "pobr-core": "benchmarks",
    "pobr-build": "benchmarks",
    "pobr-wasm": "diagnostics",
}


def touched_packages(files, root=ROOT, full=False):
    result = subprocess.run(
        ["cargo", "metadata", "--offline", "--no-deps", "--format-version", "1"],
        cwd=root, capture_output=True, text=True,
    )
    if result.returncode:
        raise ValueError(f"cargo metadata failed: {result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    members = {
        Path(package["manifest_path"]).parent.resolve(): package
        for package in metadata["packages"] if package["id"] in metadata["workspace_members"]
    }
    # Prefer the closest enclosing member, including nested non-member inputs.
    paths = sorted(members, key=lambda path: len(path.parts), reverse=True)
    touched = set()
    for name in files:
        if not planner.classify([name])["rust"]:
            continue
        changed = (root / name).resolve()
        member = next((path for path in paths if changed.is_relative_to(path)), None)
        if member is None:
            # Data, examples and non-member tools can affect every consumer.
            full = True
        else:
            touched.add(members[member]["name"])
    if full:
        return sorted(package["name"] for package in members.values())

    dependents = {package["name"]: set() for package in members.values()}
    for package in members.values():
        # Include normal, build, dev, optional and target-specific dependencies.
        for dependency in package["dependencies"]:
            path = dependency.get("path")
            member = members.get(Path(path).resolve()) if path else None
            if member:
                dependents[member["name"]].add(package["name"])
    pending = list(touched)
    while pending:
        for name in dependents[pending.pop()] - touched:
            touched.add(name)
            pending.append(name)
    return sorted(touched)


def steps_for(files, plan, root=ROOT, packages=None):
    steps = []
    if plan["rust"] or plan["full"]:
        if packages is None:
            packages = touched_packages(files, root, full=plan["full"])
        selectors = [arg for name in packages for arg in ("-p", name)]
        features = [f"{name}/{feature}" for name, feature in CLIPPY_FEATURES.items()
                    if name in packages]
        steps += [
            ("Rust fmt", root, ["cargo", "fmt", "--all", "--check"]),
            ("Rust check", root, ["cargo", "check", "--offline", "--locked", "--workspace", "--all-targets"]),
            ("Rust clippy", root, ["cargo", "clippy", "--offline", "--locked", *selectors,
                                   "--all-targets", *(["--features", ",".join(features)] if features else []),
                                   "--", "-D", "warnings"]),
            ("Rust tests", root, ["cargo", "test", "--offline", "--locked", *selectors]),
        ]
    if plan["web"] or plan["full"]:
        steps += [(f"Web {task}", root / "web", ["pnpm", *args]) for task, args in (
            ("i18n:check", ["i18n:check"]),
            ("typecheck", ["typecheck"]),
            ("test", ["test", "--", "--maxWorkers=4"]),
            ("test:worker", ["test:worker"]),
        )]
    if plan["scripts"] or plan["full"]:
        steps.append(("Automation contracts", root, ["bash", "devs/scripts/check-workflows.sh"]))
    return steps


def run_steps(steps, root=ROOT):
    results = []
    env = {**os.environ, "CARGO_TARGET_DIR": str(root / "target")}
    for label, cwd, command in steps:
        print(f"\n{label}: {shlex.join(command)}", flush=True)
        started = time.monotonic()
        try:
            code = subprocess.run(command, cwd=cwd, env=env).returncode
        except OSError as error:
            print(error, flush=True)
            code = 1
        results.append((label, code, time.monotonic() - started))
        if label == "Rust fmt" and code:
            print("Format check failed; stopping before compilation.", flush=True)
            break
    print("\nLocal check summary:", flush=True)
    for label, code, elapsed in results:
        print(f"  {'PASS' if code == 0 else 'FAIL'} {label} ({elapsed:.1f}s)", flush=True)
    return int(any(code for _, code, _ in results))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan-only", action="store_true",
                        help="print selected areas and packages without running checks")
    args = parser.parse_args()
    plan = {}
    try:
        files = planner.local_changed_files(ROOT)
        plan = planner.classify(files)
        print(f"Local plan ({len(files)} changed files): "
              + (", ".join(area for area, selected in plan.items() if selected) or "docs only / no checks"),
              flush=True)
        packages = touched_packages(files, full=plan["full"]) if plan["rust"] or plan["full"] else []
        print("Rust packages: " + (", ".join(packages) or "none"), flush=True)
        if args.plan_only:
            return 0
        if not any(plan.values()):
            print("PASS documentation / no executable changes (no builds needed).", flush=True)
            return 0
        print("SKIP network steps: dependency installation, pinned owner-data downloads "
              "and remote release-tag validation. "
              "Use already prepared dependencies, WASM and fixtures; missing inputs fail their checks.", flush=True)
        return run_steps(steps_for(files, plan, packages=packages))
    except (ValueError, OSError, KeyError) as error:
        print(f"FAIL local planning: {error}", flush=True)
        return 1
    finally:
        if plan.get("full"):
            print("Full change: mise run ci <branch> must be green before merging.", flush=True)


if __name__ == "__main__":
    raise SystemExit(main())
