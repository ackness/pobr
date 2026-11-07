#!/usr/bin/env python3
"""Run the Playwright suite locally in tiers, after checking that its inputs are current.

  mise run e2e [smoke|affected|default|release] [--workers N] [--plan-only] [<playwright args>]
  mise run release:e2e-check [<rev>]

`affected` (the default) runs the @smoke tests plus the specs mapped from the files changed
against origin/master. `release` runs every matrix cell and the three isolated configs, and
on a clean tree writes a receipt keyed by the tree hash that release:e2e-check looks for.
"""

import argparse
from datetime import datetime, timezone
import fnmatch
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import socket
import subprocess
import sys
import time
from typing import NamedTuple


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ci_plan", ROOT / "devs/scripts/ci-plan.py")
planner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(planner)

TIERS = ("smoke", "affected", "default", "release")
WORKERS = 4  # The machine is shared; each worker is a Chromium renderer running real WASM.
PORTS = range(4200, 5000)  # Outside the fixed defaults of the configs (4173, 4177, 4184, 4186).
RECEIPT = "pobr-e2e-release/v1"


class Suite(NamedTuple):
    name: str
    args: tuple = ()
    env: tuple = ()
    config: str = "playwright.config.ts"
    main: bool = True  # The main config takes --workers; isolated configs keep their own.


# Component fixtures on the Vite dev server with the mock backend, one config each.
ISOLATED = {
    name: Suite(name, config=f"e2e/{name}.config.ts", main=False) for name in ("calcs-config", "item-display", "controls")
}
RELEASE = (Suite("matrix", ("--grep-invert", "@quarantine"), (("POBR_E2E_ALL", "1"),)), *ISOLATED.values())
SPEC = ".spec.ts"

# Spec groups for `affected`: spec file stems under web/e2e (globs), or "@<isolated config>".
TREE = ("passive-*", "tree-*", "weapon-set-tree", "compare-trees", "node-power", "scratch-build", "upgrade-guidance")
ITEMS = ("item-*", "affix-local-properties", "unique-catalog", "market-*", "custom-modifier-blocks",
         "utility-selection", "limbs", "@item-display")
SKILLS = ("skill-*", "support-*", "gem-corrupted", "minion-selection", "unresolved-gem-persistence", "@controls")
CALCS = ("calcs-breakdown", "workbench", "@calcs-config")
CONFIG = ("config-*", "party", "@calcs-config")
TRADE = ("trade-*", "upgrade-guidance", "replacement-continuity", "market-*")
COMPARE = ("compare-*",)
# The engine and its data reach every page through calculation and import.
ENGINE = ("calcs-breakdown", "import-planning-errors", "json-import-identity", "build-roundtrip",
          "hosted-import", "wegame-import", "planner-file", "data-bump-smoke")
DEFAULT = ("default",)
NONE = ()

# Changed path (glob; `*` crosses directories) → what `affected` runs; the first match wins.
# A web path below matching nothing runs the default tier; other paths run nothing.
AFFECTED = (
    # No browser effect: unit tests, Node/Worker tests, maintenance scripts, docs.
    ("web/src/*.test.ts", NONE), ("web/src/*.test.tsx", NONE), ("web/src/testing/*", NONE),
    ("web/worker-tests/*", NONE), ("web/README*", NONE),
    # Inputs of the WASM engine (web/scripts/wasm-build-inputs.mjs) and of the synced data.
    ("crates/*/src/*", ENGINE), ("crates/*/Cargo.toml", ENGINE), ("crates/*/build.rs", ENGINE),
    ("apps/pobr-wasm/src/*", ENGINE), ("apps/pobr-wasm/Cargo.toml", ENGINE), ("apps/pobr-wasm/build.rs", ENGINE),
    ("Cargo.toml", ENGINE), ("Cargo.lock", ENGINE), ("rust-toolchain.toml", ENGINE), (".cargo/*", ENGINE),
    (planner.DATA_LOCK, ENGINE), ("data/*", ENGINE),
    ("web/scripts/build-wasm.mjs", ENGINE), ("web/scripts/wasm-build-inputs.mjs", ENGINE),
    ("web/scripts/sync-data.mjs", ENGINE), ("web/scripts/sync-build-references.mjs", ENGINE),
    ("web/scripts/*", NONE),
    # Isolated configs and their fixtures.
    ("web/e2e/calcs-config.*", ("@calcs-config",)), ("web/e2e/fixtures/calcs-config.*", ("@calcs-config",)),
    ("web/e2e/item-display.*", ("@item-display",)), ("web/e2e/global-display.spec.ts", ("@item-display",)),
    ("web/e2e/fixtures/item-display.*", ("@item-display",)),
    ("web/e2e/controls.*", ("@controls",)), ("web/e2e/ranges.check.ts", ("@controls",)),
    ("web/e2e/fixtures/controls.*", ("@controls",)), ("web/e2e/fixtures/ranges.*", ("@controls",)),
    # Pages and their components.
    ("web/src/components/tree/*", TREE), ("web/src/app/pages-tree.css", TREE),
    ("web/src/components/items/*", ITEMS),
    ("web/src/components/skills/*", SKILLS), ("web/src/app/pages/skills/*", SKILLS),
    ("web/src/app/pages-skills.css", SKILLS),
    ("web/src/components/calcs/*", CALCS), ("web/src/app/pages/calcs/*", CALCS),
    ("web/src/app/pages-calcs.css", CALCS), ("web/src/app/inspector/*", CALCS),
    ("web/src/components/config/*", CONFIG), ("web/src/app/pages/config/*", CONFIG),
    ("web/src/app/pages-config.css", CONFIG), ("web/src/app/config-workflow.css", CONFIG),
    ("web/src/components/trade/*", TRADE), ("web/src/app/pages-trade.css", TRADE),
    ("web/src/components/compare/*", COMPARE), ("web/src/app/pages/compare/*", COMPARE),
    ("web/src/app/pages-compare.css", COMPARE),
    ("web/src/components/guidance/*", ("build-guidance",)),
    ("web/src/app/Overview.tsx", ("overview-ux", "calcs-breakdown")),
    ("web/src/app/CommandPalette.tsx", ("workbench",)),
)


class Stop(Exception):
    """A problem the runner cannot fix cheaply; the message says what to run."""


class Selection(NamedTuple):
    suites: tuple  # Suites to run, in order; empty when nothing is affected.
    reasons: tuple  # Lines explaining the choice.


def match(name, patterns):
    return any(fnmatch.fnmatchcase(name, pattern) for pattern in patterns)


def spec_stems(e2e):
    return {path.name.removesuffix(SPEC) for path in e2e.glob(f"*{SPEC}")}


def spec_inputs(e2e):
    """Repository files the specs read (`'../../examples/…'` literals) → the spec stems reading them."""
    inputs = {}
    for path in sorted(e2e.glob(f"*{SPEC}")):
        for literal in re.findall(r"'(\.\./[^'\s]+)'", path.read_text(encoding="utf-8")):
            # Specs resolve these from their own directory or from web/ (the working directory).
            for base in (e2e, e2e.parent):
                target = (base / literal).resolve()
                if target.is_file() and target.is_relative_to(ROOT):
                    inputs.setdefault(target.relative_to(ROOT).as_posix(), set()).add(path.name.removesuffix(SPEC))
    return inputs


def map_file(name, stems, inputs):
    """What one changed file runs: spec patterns, "@isolated" names, DEFAULT, or NONE."""
    if planner.documentation(name):
        return NONE
    if name in inputs:
        return tuple(sorted(inputs[name]))
    for pattern, target in AFFECTED:
        if fnmatch.fnmatchcase(name, pattern):
            return target
    if name.startswith("web/e2e/") and name.endswith(SPEC):
        stem = name.removeprefix("web/e2e/").removesuffix(SPEC)
        # A deleted spec has nothing left to run.
        return (stem,) if stem in stems else NONE
    return DEFAULT if name.startswith("web/") else NONE


def grep_for(patterns, stems):
    """One --grep that keeps the @smoke tests and every test of the selected spec files."""
    files = sorted(stem for stem in stems if match(stem, patterns))
    if not files:
        return "@smoke"
    # Playwright matches against "<project> <file> <describe…> <title> <tags>".
    return "@smoke|(^| )(" + "|".join(re.escape(stem + SPEC) for stem in files) + ")( |$)"


def select(tier, files=None, stems=frozenset(), inputs=None):
    if tier == "smoke":
        return Selection((Suite("smoke", ("--grep", "@smoke")),), ("@smoke tests",))
    if tier == "default":
        return Selection((Suite("default"),), ("default run (no @extended or @quarantine)",))
    if tier == "release":
        return Selection(RELEASE, ("every matrix cell except @quarantine, then the isolated configs",))
    targets = {}
    for name in files:
        if target := map_file(name, stems, inputs or {}):
            targets.setdefault(target, []).append(name)
    if not targets:
        return Selection((), (f"{len(files)} changed files, none with a browser or engine effect",))
    items = {item for target in targets for item in target}
    patterns = {item for item in items if not item.startswith("@") and item != "default"}
    main = (Suite("default") if DEFAULT in targets
            else Suite("affected", ("--grep", grep_for(patterns, stems))))
    reasons = [f"{', '.join(target)} ← {', '.join(names[:3])}" + (f" and {len(names) - 3} more" if len(names) > 3 else "")
               for target, names in targets.items()]
    if main.name == "affected":
        reasons.insert(0, "@smoke tests")
    return Selection((main, *(ISOLATED[name] for name in ISOLATED if f"@{name}" in items)), tuple(reasons))


def preferred_port(root):
    digest = hashlib.sha256(str(Path(root).resolve()).encode()).digest()
    return PORTS.start + int.from_bytes(digest[:8], "big") % len(PORTS)


def port_free(port):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        # As Node does: closed connections in TIME_WAIT don't block a new listener, a live one does.
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind(("127.0.0.1", port))
        except OSError:
            return False
    return True


def free_port(root, is_free=port_free):
    """The worktree's own port, or the next free one in PORTS."""
    start = preferred_port(root) - PORTS.start
    for offset in range(len(PORTS)):
        port = PORTS.start + (start + offset) % len(PORTS)
        if is_free(port):
            return port
    raise Stop(f"no free port in {PORTS.start}-{PORTS.stop - 1}; stop old preview servers")


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def tree_digest(root, paths, skip=()):
    """Content hash of files under `paths` (relative to root), skipping relative prefixes."""
    entries = []
    for relative in paths:
        path = root / relative
        files = [path] if path.is_file() else sorted(p for p in path.rglob("*") if p.is_file()) if path.is_dir() else []
        for file in files:
            name = file.relative_to(root).as_posix()
            if not name.startswith(tuple(skip)):
                entries.append((name, sha256(file.read_bytes())))
    return sha256(json.dumps(entries).encode())


class Inputs:
    """What the browser tests run on: node_modules, Chromium, data, WASM, synced data and dist."""

    def __init__(self, root=ROOT, env=None, run=subprocess.run):
        self.root, self.web = root, root / "web"
        self.env = dict(os.environ if env is None else env)
        self.run = run
        self.state_path = root / ".cache/e2e/inputs.json"
        try:
            self.state = json.loads(self.state_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            self.state = {}
        self.notes = []

    def call(self, command, cwd=None, quiet=False):
        print(f"  $ {shlex.join(str(part) for part in command)}", flush=True)
        output = subprocess.DEVNULL if quiet else None
        return self.run(command, cwd=cwd or self.root, env=self.env, stdout=output).returncode == 0

    def save(self, **values):
        self.state.update(values)
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self.state_path.write_text(json.dumps(self.state, indent=2) + "\n", encoding="utf-8")

    def node_modules(self):
        marker = self.web / "node_modules/.modules.yaml"
        lock = self.web / "pnpm-lock.yaml"
        if marker.is_file() and marker.stat().st_mtime >= lock.stat().st_mtime:
            return
        if not self.call(["pnpm", "--dir", "web", "install", "--offline", "--frozen-lockfile"]):
            raise Stop("web dependencies are missing and not in the pnpm store: run `pnpm --dir web install --frozen-lockfile`")

    def chromium(self):
        result = self.run([str(self.web / "node_modules/.bin/playwright"), "install", "--dry-run", "chromium"],
                          cwd=self.web, env=self.env, capture_output=True, text=True)
        locations = re.findall(r"Install location:\s+(\S+)", result.stdout)
        if result.returncode or not locations or not all(Path(path).is_dir() for path in locations):
            raise Stop("the Playwright Chromium for the locked version is not installed: run `mise run setup:e2e`")

    def data_key(self):
        lock = (self.root / planner.DATA_LOCK).read_bytes()
        version = json.loads(lock)["target"]["game_patch"]
        current = self.root / "data/CURRENT"
        manifest = self.root / f"data/{version}/manifest.json"
        if not current.is_file() or current.read_text().strip() != version or not manifest.is_file():
            return None
        return sha256(json.dumps([sha256(lock), sha256(manifest.read_bytes()),
                                  tree_digest(self.root, ["data/overlay-common"]),
                                  tree_digest(self.web, ["scripts/sync-data.mjs", "scripts/sync-build-references.mjs"])]).encode())

    def data(self):
        key = self.data_key()
        if key and key == self.state.get("data") and (self.web / "public/data").is_dir():
            return key
        # The pinned install verifies every byte against the lock; offline it only reuses verified downloads.
        # (check-data.sh also demands a clean worktree, which an uncommitted change never has.)
        if not self.call([sys.executable, "-m", "pipeline.runtime.build_data", "--install", "data", "--offline"], quiet=True):
            raise Stop("data/ does not match pipeline/data.lock.json: run `mise run data:prepare`")
        if not self.call([sys.executable, "-m", "pipeline.imports.check_data_ownership"], quiet=True):
            raise Stop("data/ ownership check failed: run `mise run data:check` for details")
        key = self.data_key()
        if not key or not self.call(["pnpm", "sync-data"], cwd=self.web, quiet=True):
            raise Stop("syncing browser data failed: run `pnpm --dir web sync-data` for details")
        self.save(data=key)
        return key

    def wasm(self):
        result = self.run(["node", "web/scripts/wasm-cache-key.mjs"], cwd=self.root, env=self.env,
                          capture_output=True, text=True)
        key = result.stdout.strip().removeprefix("key=")
        if result.returncode or not key:
            raise Stop("computing the WASM identity failed: run `node web/scripts/wasm-cache-key.mjs` for details")
        if key == self.state.get("wasm") and (self.web / "src/wasm/pkg/build-receipt.json").is_file():
            return
        # build-wasm.mjs reuses verified bindings itself and recompiles only stale ones (about a minute).
        if not self.call(["node", "web/scripts/build-wasm.mjs"]):
            raise Stop("the WASM build failed: run `node web/scripts/build-wasm.mjs` for details")
        self.save(wasm=key)

    def dist_key(self, data):
        vite = sorted((name, value) for name, value in self.env.items() if name.startswith("VITE_"))
        sources = ["src", "public", "index.html", "vite.config.ts", "package.json", "pnpm-lock.yaml",
                   "tsconfig.json", "tsconfig.app.json", "tsconfig.node.json"]
        return sha256(json.dumps([data, vite, sha256((self.root / "Cargo.toml").read_bytes()),
                                  tree_digest(self.web, sources, skip=("public/data/", "public/build-references/"))]).encode())

    def dist(self, data):
        key = self.dist_key(data)
        index = self.web / "dist/index.html"
        if (key == self.state.get("dist") and index.is_file()
                and sha256(index.read_bytes()) == self.state.get("dist_index")):
            return
        # Typecheck is not the runner's job: bundle only.
        if not self.call(["pnpm", "exec", "vite", "build"], cwd=self.web, quiet=True):
            raise Stop("vite build failed: run `pnpm --dir web exec vite build` for details")
        self.save(dist=key, dist_index=sha256(index.read_bytes()))

    def rust_hint(self):
        git = lambda *args: self.run(["git", *args], cwd=self.root, capture_output=True, text=True).stdout.strip()
        common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir"))
        target = self.root / "target"
        if common.parent != self.root and not (target.is_dir() and any(target.iterdir())):
            self.notes.append(f"Rust target/ is empty in this worktree: `mise run warm {common.parent}` "
                              "copies that checkout's debug build for later cargo checks")

    def prepare(self):
        print("Preflight:", flush=True)
        self.rust_hint()
        self.node_modules()
        self.chromium()
        data = self.data()
        self.wasm()
        self.dist(data)
        for note in self.notes:
            print(f"  NOTE {note}", flush=True)
        print("  inputs current: node_modules, Chromium, data, WASM, synced data, dist", flush=True)


class Result(NamedTuple):
    suite: str
    code: int
    passed: int
    failed: int
    flaky: int
    skipped: int
    seconds: float

    @property
    def ok(self):
        return self.code == 0 and self.failed == 0 and self.passed > 0


def run_suite(suite, port, workers, extra, env, root=ROOT, run=subprocess.run):
    out = root / ".cache/e2e/runs" / suite.name
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    command = [str(root / "web/node_modules/.bin/playwright"), "test", "-c", suite.config, *suite.args,
               "--reporter=list,json,html", "--output", str(out / "artifacts"),
               # Extra arguments (spec filters, --grep, --headed) are for the main config only.
               *(["--workers", str(workers), *extra] if suite.main else [])]
    env = {**env, **dict(suite.env), "POBR_E2E_PORT": str(port),
           "PLAYWRIGHT_JSON_OUTPUT_FILE": str(out / "report.json"),
           "PLAYWRIGHT_HTML_OUTPUT_DIR": str(out / "html"), "PLAYWRIGHT_HTML_OPEN": "never"}
    print(f"\n{suite.name}: {shlex.join(command[1:])} (port {port})", flush=True)
    started = time.monotonic()
    code = run(command, cwd=root / "web", env=env).returncode
    seconds = time.monotonic() - started
    try:
        stats = json.loads((out / "report.json").read_text(encoding="utf-8"))["stats"]
    except (OSError, ValueError, KeyError):
        stats = {}
    return Result(suite.name, code, stats.get("expected", 0), stats.get("unexpected", 0),
                  stats.get("flaky", 0), stats.get("skipped", 0), seconds)


def summary(results, env, root=ROOT):
    print("\nE2E summary:", flush=True)
    for result in results:
        print(f"  {'PASS' if result.ok else 'FAIL'} {result.suite}: {result.passed} passed, {result.failed} failed, "
              f"{result.flaky} flaky, {result.skipped} skipped ({result.seconds:.1f}s)"
              + ("" if result.ok or result.failed else f" (exit {result.code}, no test result)"), flush=True)
    for result in results:
        if not result.ok:
            out = (root / ".cache/e2e/runs" / result.suite).relative_to(root)
            print(f"  {result.suite} report: pnpm --dir web exec playwright show-report ../{out}/html; "
                  f"screenshots and traces: {out}/artifacts", flush=True)
    if not all(result.ok for result in results) and env.get("POBR_E2E_TRACE") != "1":
        print("  Traces are off locally; rerun with POBR_E2E_TRACE=1 to record them.", flush=True)


def git(*args, root=ROOT):
    result = subprocess.run(["git", *args], cwd=root, capture_output=True, text=True)
    if result.returncode:
        raise Stop(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def snapshot(root=ROOT):
    """The commit, tree and clean state a release receipt is keyed by."""
    return {"commit": git("rev-parse", "HEAD", root=root), "tree": git("rev-parse", "HEAD^{tree}", root=root),
            "clean": git("status", "--porcelain", "--untracked-files=normal", root=root) == ""}


def receipt_path(tree, root=ROOT):
    return root / ".cache/e2e" / f"release-{tree}.json"


def release_receipt(before, after, results, workers, started, finished, root=ROOT):
    lock = (root / planner.DATA_LOCK).read_bytes()
    version = json.loads((root / "web/node_modules/@playwright/test/package.json").read_text())["version"]
    return {
        "schema": RECEIPT, "passed": all(result.ok for result in results) and before == after and before["clean"],
        "commit": before["commit"], "tree": before["tree"], "clean": before["clean"] and after["clean"],
        "data_lock_sha256": sha256(lock), "playwright": version, "workers": workers,
        "suites": [{"name": r.suite, "passed": r.passed, "failed": r.failed, "flaky": r.flaky,
                    "skipped": r.skipped, "seconds": round(r.seconds, 1)} for r in results],
        "started": started, "finished": finished,
    }


def passing_receipt(path, tree):
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    suites = {suite.get("name"): suite for suite in receipt.get("suites", [])}
    complete = all(name in suites and suites[name].get("failed") == 0 and suites[name].get("passed", 0) > 0
                   for name in (suite.name for suite in RELEASE))
    ok = receipt.get("schema") == RECEIPT and receipt.get("tree") == tree and receipt.get("passed") is True
    return receipt if ok and complete and receipt.get("clean") is True else None


def worktrees(root=ROOT):
    """This checkout first, then the repository's other worktrees (the receipt stays where it ran)."""
    listed = [Path(line.removeprefix("worktree ")) for line in
              git("worktree", "list", "--porcelain", root=root).splitlines() if line.startswith("worktree ")]
    return [root, *(path for path in listed if path.resolve() != root.resolve())]


def release_check(rev, root=ROOT):
    try:
        tree = git("rev-parse", "--verify", f"{rev}^{{tree}}", root=root)
        for checkout in worktrees(root):
            receipt = passing_receipt(receipt_path(tree, checkout), tree)
            if receipt:
                counts = ", ".join(f"{suite['name']} {suite['passed']}" for suite in receipt["suites"])
                print(f"PASS release E2E for {rev} (tree {tree[:12]}): commit {receipt['commit'][:12]}, "
                      f"finished {receipt['finished']}, {counts} passed; {receipt_path(tree, checkout)}")
                return 0
    except Stop as error:
        print(f"FAIL release E2E check: {error}")
        return 1
    print(f"FAIL no passing release E2E receipt for {rev} (tree {tree[:12]}): "
          "run `mise run e2e release` on a clean checkout of that tree")
    return 1


def parse(argv):
    """Runner options, then Playwright's: after `--`, or any argument the runner does not know (mise drops `--`)."""
    if "--" in argv:
        index = argv.index("--")
        argv, extra = argv[:index], argv[index + 1:]
    else:
        extra = []
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tier", nargs="?", default="affected", choices=TIERS)
    parser.add_argument("--workers", type=int, default=WORKERS, help=f"Playwright workers (default {WORKERS})")
    parser.add_argument("--plan-only", action="store_true", help="print the selection without preflight or tests")
    args, unknown = parser.parse_known_args(argv)
    if args.workers < 1:
        parser.error("--workers must be at least 1")
    return args, [*unknown, *extra]


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["release-check"]:
        if len(argv) > 2:
            print("usage: mise run release:e2e-check [<rev>]", file=sys.stderr)
            return 2
        return release_check(argv[1] if len(argv) == 2 else "HEAD")
    args, extra = parse(argv)
    env = dict(os.environ)
    try:
        files = planner.local_changed_files(ROOT) if args.tier == "affected" else None
        e2e = ROOT / "web/e2e"
        stems = spec_stems(e2e)
        selection = select(args.tier, files, stems, spec_inputs(e2e) if files else None)
        print(f"E2E {args.tier}: " + ("; ".join(suite.name for suite in selection.suites) or "nothing to run"), flush=True)
        for reason in selection.reasons:
            print(f"  {reason}", flush=True)
        if args.plan_only or not selection.suites:
            return 0
        before = snapshot() if args.tier == "release" else None
        Inputs(env=env).prepare()
        port = int(env["POBR_E2E_PORT"]) if env.get("POBR_E2E_PORT") else None
        if port is not None and not port_free(port):
            raise Stop(f"POBR_E2E_PORT={port} is already in use")
        started = datetime.now(timezone.utc).isoformat(timespec="seconds")
        results = [run_suite(suite, port or free_port(ROOT), args.workers, extra, env) for suite in selection.suites]
        finished = datetime.now(timezone.utc).isoformat(timespec="seconds")
        summary(results, env)
        if before is not None:
            receipt = release_receipt(before, snapshot(), results, args.workers, started, finished)
            if extra:
                print("No release receipt: extra Playwright arguments change what the release run covers.", flush=True)
            elif not receipt["clean"]:
                print("No release receipt: the tree had uncommitted changes (commit them and rerun).", flush=True)
            elif receipt["passed"]:
                path = receipt_path(before["tree"])
                path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
                print(f"Release receipt: {path.relative_to(ROOT)}", flush=True)
            else:
                print("No release receipt: a suite failed or HEAD changed during the run.", flush=True)
        return int(not all(result.ok for result in results))
    except (Stop, ValueError) as error:
        print(f"STOP {error}", flush=True)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
