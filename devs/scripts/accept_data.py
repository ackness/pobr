#!/usr/bin/env python3
"""Accept a new data patch: version smoke, L2 corpus, strict display audit and browser smoke.

  mise run data:accept [--skip multi-version|corpus|display|e2e] ...

Skips are explicit in the summary. This checks that builds work, without comparing
numbers or writing snapshots; L2 attribution and full release E2E remain separate.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import time

import e2e
from triage import build_dirs


ROOT = Path(__file__).resolve().parents[2]
STEPS = ("multi-version", "corpus", "display", "e2e")


def corpus_codes(root):
    # The snapshot owns the L2 corpus selection; include newly added builds too.
    snapshot = json.loads((root / "examples/output-snapshot.json").read_text(encoding="utf-8"))
    corpora = sorted({Path(name).parent for name in snapshot["builds"]})
    if not corpora:
        raise ValueError("the L2 snapshot has no corpora")
    codes = []
    for corpus in corpora:
        try:
            found = build_dirs([str(root / corpus)])
        except SystemExit as error:
            raise ValueError(str(error)) from error
        if not found:
            raise ValueError(f"empty L2 corpus: {corpus}")
        codes.extend(folder / "code.txt" for _, folder in found)
    return codes


def compute_corpus(inputs):
    codes = corpus_codes(inputs.root)
    if not inputs.call(["mise", "run", "build"]):
        return False
    binary = str(inputs.root / "target/debug/pobr")
    failed = 0
    for code in codes:
        ok = inputs.call([binary, "calculate-build", "--file", str(code)], quiet=True)
        failed += not ok
        print(f"  {'PASS' if ok else 'FAIL'} {code.parent.relative_to(inputs.root)}", flush=True)
    print(f"  Corpus: {len(codes) - failed}/{len(codes)} builds computed", flush=True)
    return failed == 0


def audit_display(inputs):
    codes = corpus_codes(inputs.root)
    # Reuse E2E's preparation and freshness checks before auditing the local WASM.
    inputs.node_modules()
    inputs.data()
    inputs.wasm()
    return inputs.call(["mise", "run", "audit:display", "--strict",
                        *(str(code.relative_to(inputs.root)) for code in codes)])


def main(argv=None, root=ROOT, run=subprocess.run):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--skip", action="append", choices=STEPS, default=[],
                        help="omit one step (repeat to skip more); summary will be incomplete")
    args = parser.parse_args(argv)
    # Acceptance must build this checkout, even if the shell names another target.
    inputs = e2e.Inputs(root, env={**os.environ, "CARGO_TARGET_DIR": str(root / "target")}, run=run)
    actions = (
        lambda: inputs.call(["mise", "run", "test", "build", "parity", "multi_version::"]),
        lambda: compute_corpus(inputs),
        lambda: audit_display(inputs),
        lambda: inputs.call(["mise", "run", "e2e", "smoke", "--grep", "data-bump-smoke.spec.ts"]),
    )
    results = []
    for name, action in zip(STEPS, actions, strict=True):
        if name in args.skip:
            results.append((name, "SKIP", 0.0))
            continue
        print(f"\nData acceptance — {name}:", flush=True)
        started = time.monotonic()
        try:
            ok = action()
        except (OSError, ValueError, KeyError, e2e.Stop) as error:
            print(f"  FAIL {error}", flush=True)
            ok = False
        results.append((name, "PASS" if ok else "FAIL", time.monotonic() - started))

    print("\n| Step | Result | Time |\n| --- | --- | ---: |", flush=True)
    for name, status, seconds in results:
        print(f"| {name} | {status} | {seconds:.1f}s |", flush=True)
    failed = any(status == "FAIL" for _, status, _ in results)
    verdict = "FAIL" if failed else "INCOMPLETE (explicit skips)" if args.skip else "PASS"
    print(f"\nData acceptance: {verdict}", flush=True)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
