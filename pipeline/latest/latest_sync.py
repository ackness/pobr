"""Propose and conditionally merge latest-data changes using trusted default code.

No candidate code is executed with credentials. Completed workflow callbacks
recheck both exact-head gates, owner freshness and the adopted data lock.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlencode

from pipeline.imports import data_imports as imports
from pipeline.latest import latest_data as latest
from pipeline.imports.data_imports import encoded, regular, require, sha, write


REPOSITORY = "ackness/pobr"
DEFAULT = "master"
BRANCH = "automation/data-latest"
ALLOWED = set(latest.CANDIDATE_FILES)
MARKER = "chore: synchronize complete data publication"


class GateIncomplete(ValueError):
    """A candidate stays pending until both exact-head checks pass."""


def oid(value):
    require(isinstance(value, str) and re.fullmatch(r"[a-f0-9]{40}", value), "Invalid Git identity")
    return value


def api(method, path, body=None):
    command = ["gh", "api", "--method", method, path]
    if body is not None:
        command += ["--input", "-"]
    result = subprocess.run(command, input=None if body is None else encoded(body),
                            stdout=subprocess.PIPE, check=True, timeout=60)
    return json.loads(result.stdout) if result.stdout.strip() else None


def endpoint(suffix):
    return f"repos/{REPOSITORY}/{suffix}"


def branch_state(request=api):
    records = request("GET", endpoint("git/matching-refs/heads/" + BRANCH))
    exact = [value for value in records if value["ref"] == "refs/heads/" + BRANCH]
    require(len(exact) <= 1, "Ambiguous automation branch")
    return oid(exact[0]["object"]["sha"]) if exact else None


def pull_requests(request=api):
    query = urlencode({"state": "open", "base": DEFAULT,
                       "head": REPOSITORY.split("/")[0] + ":" + BRANCH, "per_page": 100})
    values = request("GET", endpoint("pulls?" + query))
    require(len(values) <= 1, "Duplicate automation PRs")
    return values


def protected(settings):
    checks = settings.get("required_status_checks") or {}
    names = set(checks.get("contexts", [])) | {value["context"] for value in checks.get("checks", [])}
    require(checks.get("strict") is True and "PR gate" in names
            and (settings.get("enforce_admins") or {}).get("enabled") is True,
            "needs-strict-branch-checks: require up-to-date PR gate including administrators")


def assess_pr(value):
    require(value.get("state") == "open" and not value.get("draft") and value.get("mergeable") is True,
            "PR is not ready or mergeability is unknown")
    require(value["base"]["ref"] == DEFAULT and value["base"]["repo"]["full_name"] == REPOSITORY
            and value["head"]["ref"] == BRANCH and value["head"]["repo"]["full_name"] == REPOSITORY,
            "Untrusted automation PR")
    oid(value["head"]["sha"])
    oid(value["base"]["sha"])


def gate(workflow, expected, event, name, request=api):
    query = urlencode({"head_sha": expected, "event": event, "per_page": 100})
    runs = request("GET", endpoint(f"actions/workflows/{workflow}/runs?{query}"))["workflow_runs"]
    runs = [run for run in runs if run.get("head_sha") == expected and run.get("event") == event
            and run.get("head_branch") == BRANCH
            and (run.get("head_repository") or {}).get("full_name") == REPOSITORY]
    if not runs:
        raise GateIncomplete(f"Missing {name} run at candidate SHA")
    run = max(runs, key=lambda value: int(value["id"]))
    if run.get("status") != "completed" or run.get("conclusion") != "success":
        raise GateIncomplete(f"{name} is incomplete, failed, cancelled or approval-required")
    query = urlencode({"filter": "latest", "per_page": 100})
    jobs = request("GET", endpoint(f"actions/runs/{run['id']}/jobs?{query}"))
    require(jobs["total_count"] <= 100, "Unexpectedly large workflow job graph")
    selected = [job for job in jobs["jobs"] if job.get("name") == name]
    if not (len(selected) == 1 and selected[0].get("status") == "completed"
            and selected[0].get("conclusion") == "success"):
        raise GateIncomplete(f"Missing successful {name} job")
    return {"run": run["id"], "attempt": run["run_attempt"], "head": expected,
            "job": selected[0]["id"], "conclusion": "success"}


def git(root, *arguments, payload=None, env=None):
    return subprocess.check_output(["git", "--no-lazy-fetch", "--no-replace-objects", *arguments],
                                   cwd=root, input=payload, env=env, timeout=60)


def read_tree(root, revision, directory):
    """Read fixed small metadata only; never check out or run candidate scripts."""
    oid(revision)
    for name in sorted(ALLOWED):
        size = int(git(root, "cat-file", "-s", revision + ":" + name))
        require(0 <= size <= latest.MAX_METADATA, "Oversized candidate metadata")
        raw = git(root, "show", revision + ":" + name)
        require(len(raw) <= latest.MAX_METADATA, "Oversized candidate metadata")
        write(directory / name, raw)
    return latest.read_lock(directory)


def changed_files(base, head, request=api):
    diff = request("GET", endpoint(f"compare/{oid(base)}...{oid(head)}"))
    files = diff.get("files", [])
    require(0 < len(files) <= len(ALLOWED) and all(value["filename"] in ALLOWED
            and value.get("status") == "modified" and "previous_filename" not in value for value in files),
            "Automation branch changes code, policies outside the closure, or unexpected paths")
    return files


def write_access():
    require(os.environ.get("DATA_SYNC_AUTH_READY") == "true",
            "needs-data-sync-token: configure an App installation token or PAT for unattended PR CI")


def create_commit(root, candidate, base):
    oid(base)
    receipt = latest.verify_candidate(candidate)
    with tempfile.TemporaryDirectory(prefix="pobr-latest-index-") as temporary:
        env = {**os.environ, "GIT_INDEX_FILE": str(Path(temporary) / "index"),
               "GIT_AUTHOR_NAME": "pobr-data-sync", "GIT_AUTHOR_EMAIL": "data-sync@users.noreply.github.com",
               "GIT_COMMITTER_NAME": "pobr-data-sync", "GIT_COMMITTER_EMAIL": "data-sync@users.noreply.github.com"}
        git(root, "read-tree", base, env=env)
        for name in sorted(ALLOWED):
            raw = regular(candidate / name)
            checksum = oid(git(root, "hash-object", "-w", "--stdin", payload=raw, env=env).decode().strip())
            git(root, "update-index", "--add", "--cacheinfo", f"100644,{checksum},{name}", env=env)
        tree = oid(git(root, "write-tree", env=env).decode().strip())
        message = (MARKER + "\n\nData-lock: " + receipt["lock_id"]
                   + "\nPrevious-data-lock: " + receipt["previous_id"] + "\n")
        return oid(git(root, "commit-tree", tree, "-p", base, payload=message.encode(), env=env).decode().strip())


def push(root, revision, expected):
    # gh's credential helper reads GH_TOKEN privately; no credential in arguments,
    # URLs, persisted Git config, files, or the process's captured output.
    expected = "" if expected is None else oid(expected)
    subprocess.run(["git", "-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential",
                    "-c", "credential.interactive=never", "push",
                    f"--force-with-lease=refs/heads/{BRANCH}:{expected}",
                    "origin", f"{oid(revision)}:refs/heads/{BRANCH}"], cwd=root, check=True, timeout=60)


def dispatch_full(head, request=api):
    query = urlencode({"head_sha": oid(head), "event": "workflow_dispatch", "per_page": 100})
    runs = request("GET", endpoint("actions/workflows/ci.yml/runs?" + query))["workflow_runs"]
    matching = [run for run in runs if run.get("head_sha") == head and run.get("head_branch") == BRANCH]
    latest_run = max(matching, key=lambda value: int(value["id"])) if matching else None
    if latest_run is None or (latest_run.get("status") == "completed" and latest_run.get("conclusion") != "success"):
        request("POST", endpoint("actions/workflows/ci.yml/dispatches"), {"ref": BRANCH, "inputs": {"e2e": "false"}})


def propose(root, temporary, fetcher, official, request=api):
    base = oid(request("GET", endpoint("git/ref/heads/" + DEFAULT))["object"]["sha"])
    require(git(root, "rev-parse", "HEAD").decode().strip() == base, "Default checkout advanced; retry on current code")
    output = temporary / "candidate"
    status = latest.propose(root, output, fetcher, official=official)
    if status["status"] != "update-available":
        return status
    require(status["auto_eligible"], "New publication requires target/scope review")
    write_access()
    old = branch_state(request)
    head = None
    if old is not None:
        changed_files(base, old, request)
        commit = request("GET", endpoint("git/commits/" + old))
        require(commit["message"].splitlines()[0] == MARKER and len(commit["parents"]) == 1,
                "Automation branch contains an unowned commit")
        if commit["parents"][0]["sha"] == base:
            previous = request("GET", endpoint(f"contents/{latest.LOCK}?ref={old}"))
            import base64
            if sha(base64.b64decode(previous["content"], validate=False)) == status["candidate_lock"]:
                head = old
    # Recheck the default data lock and branch lease immediately before push.
    latest.recheck(output, root, fetcher, official=official)
    require(request("GET", endpoint("git/ref/heads/" + DEFAULT))["object"]["sha"] == base,
            "Default branch advanced during proposal")
    if head is None:
        head = create_commit(root, output, base)
        push(root, head, old)
    prs = pull_requests(request)
    body = ("Synchronize the complete immutable owner publication combination.\n\n"
            f"Data lock: `{status['candidate_lock']}`. Previous: `{status['adopted_lock']}`.\n\n"
            "PR gate and separately dispatched Full gate must pass at this exact head. Full validation includes "
            "the final compact candidate with this run's WASM. Adoption rechecks source frontiers, official "
            "version and the baseline. Source semantic certification and upstream generation remain separate.")
    if prs:
        pr = request("PATCH", endpoint(f"pulls/{prs[0]['number']}"), {"title": MARKER, "body": body})
    else:
        pr = request("POST", endpoint("pulls"), {"title": MARKER, "body": body, "head": BRANCH, "base": DEFAULT})
    dispatch_full(head, request)
    # Scheduling also reconciles an existing candidate. A lost/failed completion
    # callback must not require repeating already successful expensive CI.
    try:
        return accept(root, temporary / "accept", fetcher, official, request)
    except GateIncomplete as error:
        return {**status, "status": "candidate-proposed-awaiting-gates", "head": head,
                "pr": pr["html_url"], "reason": str(error)}


def accept(root, temporary, fetcher, official, request=api):
    prs = pull_requests(request)
    if not prs:
        return {"status": "no-open-data-candidate", "adoption": "unchanged"}
    pr = request("GET", endpoint(f"pulls/{prs[0]['number']}"))
    assess_pr(pr)
    head, base = pr["head"]["sha"], pr["base"]["sha"]
    require(git(root, "rev-parse", "HEAD").decode().strip() == base, "Default checkout advanced")
    changed_files(base, head, request)
    protected(request("GET", endpoint("branches/" + DEFAULT + "/protection")))
    gates = {"pr": gate("pr.yml", head, "pull_request", "PR gate", request),
             "full": gate("ci.yml", head, "workflow_dispatch", "Full gate", request)}
    # Fetch one immutable object, never a candidate script or installation step.
    git(root, "-c", "protocol.file.allow=never", "fetch", "--no-tags", "--depth=1", "origin", head)
    proposed = temporary / "candidate-tree"
    composition, raw = read_tree(root, head, proposed)
    current = latest.read_lock(root)
    require(composition["previous_id"] == sha(current[1]), "Expected data lock changed")
    regenerated = temporary / "resolved"
    status = latest.propose(root, regenerated, fetcher, official=official)
    require(status["status"] == "update-available" and status["auto_eligible"], "Candidate freshness/scope no longer eligible")
    # A code-only owner HEAD advance may leave its immutable publication unchanged.
    frozen = composition["owner_publications"]
    require(all(all(frozen[owner][key] == status["published_frontier"][owner][key]
                        for key in ("sequence", "descriptor")) for owner in latest.OWNERS),
            "Newer owner publication supersedes candidate")
    import shutil
    shutil.rmtree(regenerated)
    latest.propose(root, regenerated, fetcher,
                   heads=lambda owner, branch: frozen[owner]["index_revision"], official=official)
    wanted = latest.verify_candidate(regenerated)
    require(sha(raw) == wanted["lock_id"] and all(regular(proposed / name) == regular(regenerated / name)
            for name in ALLOWED), "Candidate metadata differs from trusted resolver")
    write_access()
    after = request("GET", endpoint(f"pulls/{pr['number']}"))
    assess_pr(after)
    require(all(after[side]["sha"] == pr[side]["sha"] for side in ("base", "head")), "PR changed during acceptance")
    protected(request("GET", endpoint("branches/" + DEFAULT + "/protection")))
    require(gates == {"pr": gate("pr.yml", head, "pull_request", "PR gate", request),
                      "full": gate("ci.yml", head, "workflow_dispatch", "Full gate", request)},
            "Checks changed during acceptance")
    result = request("PUT", endpoint(f"pulls/{pr['number']}/merge"), {"sha": head, "merge_method": "squash"})
    require(result.get("merged") is True, "Host rejected protected conditional merge")
    return {"status": "adopted", "lock_id": wanted["lock_id"], "head": head,
            "merge": oid(result["sha"]), "checks": gates, "pr": pr["html_url"],
            "runtime_selection": "Accepted default metadata; local installations update explicitly"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("propose", "accept"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root = latest.ROOT
    with tempfile.TemporaryDirectory(prefix="pobr-data-sync-") as temporary:
        temporary = Path(temporary).resolve()
        fetcher = imports.GitInputs(temporary / "git", timeout=30)
        try:
            config = latest.channels(root)
            official = latest.observe_official(config, temporary / "observation", temporary / "cache", fetcher)
            result = globals()[args.action](root, temporary, fetcher, official)
        except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
            write(args.out, encoded({"schema": "pobr-latest-sync-result/v1", "status": "incomplete",
                                    "adoption": "unchanged", "reason": str(error)}))
            raise
    write(args.out, encoded({"schema": "pobr-latest-sync-result/v1", **result}))
    print(encoded(result).decode(), end="")


if __name__ == "__main__":
    main()
