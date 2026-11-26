---
name: release-pobr
description: >-
  Fast, version-agnostic PoBR application release runbook: pre-check live state
  and UI health, draft CHANGELOG.md from complete commit/PR history, overlap
  local e2e release with cloud PR + Full CI, recover cleanly from E2E/CI/fetch
  pitfalls, and verify GitHub Release, Cloudflare Pages deployment, and strict
  display audit. Use when asked to release a new PoBR version or troubleshoot
  release and deployment steps.
---

# Release PoBR (`v0.x.y`)

Fast, repeatable runbook for shipping a PoBR application release. Canonical
policy lives in [RELEASING.md](../../../RELEASING.md#4-发版上线) and
[docs/development-workflow.md](../../../docs/development-workflow.md#发布与部署);
do not hardcode app or data versions here — always read live repository state.

## Hard Guardrails

- **Live truth only:** Read current version/patch directly from `Cargo.toml`
  (`[workspace.package].version`), `git tag --sort=-v:refname | head -n 5`,
  `data/CURRENT`, and `pipeline/data.lock.json`. Never infer versions from prose.
- **Release PR purity:** A release PR touches ONLY `Cargo.toml`, `Cargo.lock`,
  and `CHANGELOG.md`. Any bug or test failure goes into a separate PR against
  `origin/master` first, then `release/v0.x.y` resets onto the updated
  `origin/master` and cites the fix PR in `CHANGELOG.md`.
- **Immutable tags & merge commits:** Merge release PRs with `--merge` and
  `--match-head-commit`; never move or delete a pushed `v*` tag.

## Fast-Release Runbook

### 1. Single-Shot Preflight (Live State, Clean Tree, Chromium)

```bash
git fetch origin
git tag --sort=-v:refname | head -n 5
cat data/CURRENT
git status --porcelain --untracked-files=normal
gh run list --branch master --limit 5
pnpm --dir web exec playwright install --dry-run chromium
```

- Confirm next SemVer (`v0.x.y`), active patch in `pipeline/data.lock.json`, and
  green `master` CI.
- **Clean tree:** If `git status --porcelain --untracked-files=normal` is not
  empty, temporarily move unrelated untracked files/dirs (especially under
  `web/public/`) into `.cache/` and restore them in Step 7.
- **Chromium:** If the dry-run install location does not exist on disk, run
  `mise run setup:e2e` now.
- **Fast UI pre-check (saves ~12 min on regressions):** If Web/CSS/E2E files
  changed since `v<prev>`, run `mise run e2e smoke` (~35s) or the directly
  affected specs BEFORE bumping `Cargo.toml` so failures are fixed without
  flipping `Cargo.toml` version and recompiling WASM twice.

### 2. Inspect Complete Change Scope Since `v<prev>`

Inspect BOTH merge commits (every PR number) AND individual commits / API diffs
(so follow-up commits pushed after a PR description are never missed):

```bash
git log --first-parent --format='=== %h %s ===%n%b' v<prev>..origin/master
git log --no-merges --oneline v<prev>..origin/master
git diff --stat v<prev>..origin/master -- apps/ crates/pobr-data/ pipeline/data.lock.json
```

Check whether any breaking changes occurred (WASM exports, `SCHEMA_VERSION`,
`CONTRACT_VERSION`, CLI flags/subcommands, or saved-build compatibility).

### 3. Bump Version, Draft `CHANGELOG.md`, Validate & Commit

```bash
git switch -c release/v0.x.y origin/master
# Edit Cargo.toml ([workspace.package].version) and CHANGELOG.md:
# - English, single line per paragraph/bullet (no manual line wraps)
# - Order: Breaking changes (required; "- None." if none), Added, Changed, Fixed, Data, For developers, Internal
# - Cite every PR in v<prev>..origin/master at least once; add bottom compare link
mise exec -- cargo metadata --offline --format-version 1 > /dev/null || mise exec -- cargo metadata --format-version 1 > /dev/null
mise exec -- bash devs/scripts/release-check.sh origin/master   # must print release=true
mise exec -- python3 devs/scripts/release-policy.py notes v0.x.y .cache/release-notes.md
git diff --stat   # MUST list ONLY Cargo.toml, Cargo.lock, CHANGELOG.md
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release v0.x.y"
```

### 4. Overlap Cloud CI with Local `e2e release` (Key Speedup)

Cloud `PR gate` + `Full validation` (~5–6 min) and local `e2e release` (~8–12
min) are independent on the committed tree. Launch cloud CI first via a draft PR
so it runs concurrently with local Playwright:

```bash
git push -u origin HEAD
gh pr create --draft --title "chore: release v0.x.y" --body-file .cache/release-notes.md
mise run ci release/v0.x.y
```

Immediately run the full local browser release suite (increase `--workers` on
dedicated multi-core hosts, e.g., `--workers 6` or `8`; `--workers` is handled
by `e2e.py` and preserves release receipt generation):

```bash
mise run e2e release --workers 6
mise run release:e2e-check HEAD   # must print PASS
```

- Never mark the PR ready or merge until BOTH local `release:e2e-check` AND
  cloud `Contribution checks` + `Full validation` succeed.
- If `e2e release` fails: close or keep the draft PR parked, inspect
  `.cache/e2e/runs/<suite>/report.json`, land the fix in a separate PR against
  `origin/master`, then `git fetch origin && git switch -C release/v0.x.y origin/master`,
  re-apply the bump + updated `CHANGELOG.md` (citing the fix PR), force-push the
  draft branch, re-dispatch `mise run ci release/v0.x.y`, and re-run `e2e release`.

### 5. Verify Gates, Merge, and Push Annotated Tag

```bash
gh pr ready <PR#>
mise run pr-ready <PR#> --require-full
gh pr merge <PR#> --merge --match-head-commit <head-sha> --delete-branch
git fetch origin
git log -1 --format='%H %s' origin/master      # must be the release PR merge commit
mise run release:e2e-check origin/master       # must PASS (if master moved, detach & re-run e2e release)
git tag -a v0.x.y -m "Release v0.x.y" <merge-sha>
git push origin v0.x.y
```

### 6. Verify `Release` Workflow, Assets, Deployment & Strict Display Audit

- Watch `Release` workflow (`gh run list --workflow release.yml --limit 5`,
  `gh run watch <run-id> --exit-status`).
- Confirm via `gh run view <run-id> --json jobs` that all 4 jobs passed AND
  `deploy stable v0 to Cloudflare Pages` actually ran
  `cloudflare/wrangler-action` (not skipped).
- Confirm `gh release view v0.x.y --json tagName,url,assets` contains
  `pobr-wasm-*-web.tar.gz`, `SHA256SUMS`, `wasm-size.json`, `wasm-size.md`, and
  `git show v0.x.y:pipeline/data.lock.json` pins the expected packages.
- Run strict display audit:
  ```bash
  mise run builds:fetch .cache/ninja-$(date +%F)
  mise run audit:display --release v0.x.y --strict .cache/ninja-$(date +%F)
  ```
  Must exit 0 with empty `Errors`/`Raw ids` and 0 raw-ID gem names (`en-US`,
  `zh-CN`, `zh-TW`). See Pitfall #10 below if `builds:fetch` fails upstream.

### 7. Cleanup

- Switch to `master`, `git merge --ff-only origin/master`, remove temporary
  branches, and restore any untracked files moved into `.cache/` in Step 1.

## Pitfalls & Recovery

1. **Outdated version examples in docs:** Always read `Cargo.toml`, `git tag`,
   `data/CURRENT`, and `pipeline/data.lock.json` instead of trusting doc text.
2. **Untracked files prevent the E2E receipt & enter `web/dist/`:** `e2e.py`
   checks `git status --porcelain --untracked-files=normal == ""` before and
   after the run; move unrelated untracked dirs (e.g., under `web/public/`) into
   `.cache/` first and restore them after release.
3. **Locked Playwright Chromium missing (`STOP ... setup:e2e`):** Run
   `mise run setup:e2e` during preflight before starting `e2e release`.
4. **Missing follow-up commits in `CHANGELOG.md`:** PR descriptions can be stale
   after extra commits are pushed; always review `git log --no-merges` and
   `git diff --stat`.
5. **E2E failure during release:** Never commit fixes to `release/v0.x.y`; merge
   a separate fix PR to `master` first, then recreate `release/v0.x.y`.
6. **Extra WASM rebuilds when flipping branches:** Changing `Cargo.toml` version
   between `0.x.y` and `0.x.(y-1)` invalidates WASM build inputs; run quick UI
   checks before bumping `Cargo.toml` or develop a fix PR in a separate worktree.
7. **macOS Playwright keyboard shortcuts:** Browser-native paste/select must use
   `ControlOrMeta+...`, never hardcoded `Control+...`.
8. **Missing Full CI gate on release PR:** `Cargo.toml`/`Cargo.lock` edits
   require `mise run ci release/v0.x.y` and `mise run pr-ready <PR#> --require-full`.
9. **Transient GitHub API HTTP 502 during `gh run watch`:** Simply re-run
   `gh run watch <run-id> --exit-status`; do not treat a 502 watch drop as CI failure.
10. **`builds:fetch` fails upstream (`missing or ambiguous latest-season snapshot` / 429):**
    Fall back to the freshest cached `.cache/ninja-*` directory with
    `builds/*/code.txt` (or `examples/reference-builds/tuned`) for
    `mise run audit:display --release v0.x.y --strict`, and note the fallback in
    the release report.
11. **macOS sandbox `xcode-select` error on `/usr/bin/git`:** On macOS agent
    hosts where `/var/db/xcode_select_link` is sandbox-restricted, run git,
    `gh`, and Playwright commands outside the sandbox.
