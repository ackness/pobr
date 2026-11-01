#!/usr/bin/env python3
"""Use gh to measure Actions queue, preparation and validation separately."""

import argparse
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
import csv
from datetime import datetime
import json
from pathlib import Path
import re
import statistics
import subprocess


def gh(*args):
    return subprocess.check_output(["gh", *args])


def seconds(start, end):
    if not start or not end:
        return 0.0
    return max(0.0, (datetime.fromisoformat(end.replace("Z", "+00:00"))
                     - datetime.fromisoformat(start.replace("Z", "+00:00"))).total_seconds())


def validation(name):
    if re.search(r"^(Post |Set up |Complete |Install|Build|Sync |Upload|Package )", name):
        return False
    return bool(re.search(
        r"tests?|doctests|smoke|typecheck|clippy|Format|cargo (check|fmt)|pnpm (test|typecheck)"
        r"|check-workflows|contract|compatibility|reproducibility|catalog lint|Require every"
        r"|plan.py|Validate version|Dependabot", name, re.I))


def measure(run, job, log=""):
    elapsed = seconds(job.get("started_at"), job.get("completed_at"))
    skipped = job.get("conclusion") == "skipped"
    checks = compilation = 0.0
    # Runner/step clocks can differ from API job clocks. Use only durations
    # within the same clock, never subtract a step timestamp from a job timestamp.
    for step in job.get("steps", []):
        if step.get("conclusion") == "skipped" or not validation(step["name"]):
            continue
        duration = seconds(step.get("started_at"), step.get("completed_at"))
        compile_time = 0.0
        if "Workspace tests" in step["name"]:
            for line in log.splitlines():
                clean = re.sub(r"\x1b\[[0-9;]*m", "", line)
                if "Finished `test` profile" in clean:
                    stamp = clean.split(" ", 1)[0]
                    if seconds(step["started_at"], stamp) <= duration:
                        compile_time = seconds(step["started_at"], stamp)
                        break
        compilation += compile_time
        checks += max(0, duration - compile_time)
    checks = min(elapsed, checks)
    return {
        "run_id": run["id"], "workflow": run["name"], "event": run["event"],
        "branch": run["head_branch"], "sha": run["head_sha"],
        "job_id": job["id"], "job": job["name"], "result": job.get("conclusion"),
        "dependency_wait_s": round(seconds(run["created_at"], job.get("created_at")), 3) if not skipped else 0,
        "queue_s": round(seconds(job.get("created_at"), job.get("started_at")), 3) if not skipped else 0,
        "preparation_s": round(elapsed - checks, 3),
        "validation_s": round(checks, 3), "cargo_test_compile_s": round(compilation, 3),
        "job_s": round(elapsed, 3), "url": job.get("html_url", ""),
    }


def report(data, output, logs):
    rows = [measure(x["run"], j, (logs / f"log-{j['id']}.txt").read_text()
                    if (logs / f"log-{j['id']}.txt").exists() else "")
            for x in data for j in x["jobs"]]
    with (output / "jobs.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    groups = defaultdict(list)
    for row in rows:
        if row["result"] == "success":
            groups[row["job"]].append(row)
    summary = []
    for name, group in sorted(groups.items()):
        summary.append({"job": name, "count": len(group), **{
            key: round(statistics.median(x[key] for x in group), 1)
            for key in ("queue_s", "preparation_s", "validation_s", "job_s")}})
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    header = (f"{len(data)} runs, {len(rows)} jobs (skips/failures retained in jobs.csv).\n\n"
              "Queue: job created → started. Preparation: checkout, tools, dependencies, builds, uploads/cleanup. "
              "Validation: test/lint/check steps; Cargo test compilation is reassigned to preparation when logs exist. "
              "Dependency wait is separate from runner queue. Successful-job medians below, seconds.\n\n"
              "| Job | n | Queue | Preparation | Validation | Total |\n|---|---:|---:|---:|---:|---:|\n")
    table = "".join(f"| {s['job']} | {s['count']} | {s['queue_s']} | {s['preparation_s']} | {s['validation_s']} | {s['job_s']} |\n" for s in summary)
    (output / "summary.md").write_text(header + table)
    print(header + table)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="ackness/pobr")
    parser.add_argument("--limit", type=int, default=50)
    parser.add_argument("--run", type=int, action="append", help="Collect particular runs instead of latest runs")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--input", type=Path, help="Analyze an already collected raw.json snapshot")
    parser.add_argument("--logs", type=Path, help="Reuse/download timestamped job logs in this directory")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    logs = args.logs or args.output / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    if args.input:
        data = json.loads(args.input.read_text())
    else:
        runs = ([json.loads(gh("api", f"repos/{args.repo}/actions/runs/{run}")) for run in args.run]
                if args.run else json.loads(gh("run", "list", "--repo", args.repo, "--limit", str(args.limit),
                    "--json", "databaseId")))
        def collect(run):
            run = run if "id" in run else json.loads(gh("api", f"repos/{args.repo}/actions/runs/{run['databaseId']}"))
            pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{args.repo}/actions/runs/{run['id']}/jobs?per_page=100"))
            return {"run": run, "jobs": [job for page in pages for job in page["jobs"]]}
        with ThreadPoolExecutor(max_workers=6) as pool:
            data = list(pool.map(collect, runs))
    (args.output / "raw.json").write_text(json.dumps(data, indent=2) + "\n")
    def download(job):
        path = logs / f"log-{job['id']}.txt"
        if job.get("conclusion") in ("success", "failure", "cancelled") and not path.exists():
            # Recent gh protects terminals from raw ANSI. Logs are written only
            # to a file and stripped for analysis, never rendered in a terminal.
            result = subprocess.run(["gh", "api", "--allow-escape-sequences", f"repos/{args.repo}/actions/jobs/{job['id']}/logs"], capture_output=True)
            if result.returncode == 0:
                path.write_bytes(result.stdout)
            else:
                print(f"Logs unavailable for {job['id']}; using step boundaries.")
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(download, [j for x in data for j in x["jobs"]]))
    report(data, args.output, logs)


if __name__ == "__main__":
    main()
