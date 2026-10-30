# Magpie workflow research

Snapshot: `yetone/magpie` main `6d154ef1e1fdad522b88ea617dc2e2f32eeeea38` (2026-10-30). Auxiliary signing/publishing repository: `yetone/magpie-releases` main `949467a6025f0855aa0299170d2a722c6fadca54`. Read-only GitHub APIs; connector used for metadata, source, PR search and review submissions, gh API used for full tree, actions history, rulesets, issue/comment pagination. The linked sources are pinned to immutable revisions; raw API snapshots were used during research but are not bundled here.

## Organization and authority

- A Go application, Go version from `go.mod`; major responsibilities under `internal/` (gateway, provider, agent integration, plugin, GUI, sessions, usage, library, update and many narrow support packages). Embedded plain JavaScript/CSS frontend under `internal/gui/assets`; browser regressions under `internal/gui/tests`; optional native model runtime under `native/privacy-filter`; website/install/update feed under `site/`; platform packaging/dev/icon/race tooling under `build/`.
- `Makefile` is the common build/test/release interface. `test` = vet then tests; `test-ui` = independent node tests, concurrency 2 by default. Native GUI builds versus `nogui` cross compilation are explicit product variants. Toolchain/build flags come from go.mod and Makefile rather than copied language versions.
- `AGENTS.md` routes to code standards, contributor policy, subsystem index, and lessons; `CLAUDE.md` imports AGENTS + LESSONS. Detailed subsystem references own responsibilities, state transitions, compatibility/failure contracts and source symbols. Changing a documented contract updates its owning reference in the same PR; internal refactor need not rewrite docs. Proposals belong to issue/PR, not current-contract pages.
- `LESSONS.md` is a long dated record of review/release failures. Repeated useful rules are promoted into code standards. Good learning loop, but copying the growing incident ledger into every task is expensive and can harden local habits into universal policy.

Pinned sources:
- https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/AGENTS.md
- https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/docs/subsystems/README.md
- https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/docs/code-standards.md
- https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/Makefile

## Complete committed CI inventory

| Workflow | Trigger | Jobs and actual evidence | Writes / cost controls |
|---|---|---|---|
| Test (`test.yml`) | push main, all PRs, manual | Ubuntu/macOS/Windows matrix; gofmt on Linux, vet all; full tests + focused race suites on Unix; Windows full test compile plus selected real native tests; separate linux/amd64 and android/arm64 nogui builds; site/installer tests on Linux | contents read; setup-go cache defaults; fail-fast false; main in-progress never cancelled, PR superseded run cancelled; apt step 15m timeout; test command 30m; race compiled once and sharded by CPU. No browser GUI suite here. |
| Docker (`docker.yml`) | push main and v* tags; PR only Dockerfile/.dockerignore/workflow; manual | native amd64/arm64 runners; build/load then real container smoke: gateway responds, unknown model returns structured 4xx, shell/healthcheck/config/provider CLI work; publish by digest then assemble manifest/tags; provenance attestation | packages write job; no login/push on PR; GHA per-platform build cache; digest artifacts retained 1 day; merge job id-token/attestations write; edge on main, version and major/minor/latest on stable tags. Tag pushes and main pushes may duplicate builds. |
| Privacy filter (`privacy-filter.yml`) | path-scoped main or pf/** push, pf-v* tags, path-scoped PR, manual | macOS arm64/amd64, Linux arm64/amd64, cross-built Windows; builds zipped helper; native runnable variants must start and print usage | default contents read, release job write; artifacts transferred to pf-v* prerelease with SHA256SUMS, latest=false. Separate helper lifecycle avoids moving app's update channel. |
| Release (`release.yml`) | v* tag or manual existing version | validates version character grammar, repository_dispatch to magpie-releases; mirror waits for >=16 assets, downloads notes/assets, creates/updates mirrored release; reports missing Chinese notes via one issue | cross-repo PAT scoped to release repo; source mirror contents/issues write; 75m timeout, up to 120 ×30s polling; mirror retry uses upload --clobber. It does NOT implement a Test-green gate. |
| UI preview (`ui-preview.yml`) | pull_request_target opened/synchronize/reopened/labeled; label event only ui-preview; manual PR | trusted detect computes changed GUI paths + optional label; uploads PR diff/title/body; record checks out exact head with no persisted creds, builds real nogui binary in temporary home, seeds fake usage/accounts/skills, records scripted AI-planned Chromium interaction; trusted publish receives artifacts, updates bounded block in PR body and static branch media | default permissions {}; detect PR write; record contents read but receives DEEPSEEK_API_KEY via environment; publish contents/PR write and never executes PR code. Per-PR concurrency cancellation; 3d metadata and 7d previews artifacts. |

Permanent workflow directory: https://github.com/yetone/magpie/tree/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/.github/workflows

Two additional dynamic workflows are listed by GitHub Actions API, not committed source: Copilot `dynamic/agents/copilot-pull-request-reviewer` and `pages-build-deployment`. Their presence alone does not establish required AI review or exact app configuration. No scheduled workflow, committed issue/PR templates, CODEOWNERS, Dependabot, labeler, stale bot or standard CONTRIBUTING/SECURITY file exists in the complete pinned source tree. `magpie-releases` separately has Dependabot.

## Signing/release chain (real downstream implementation)

https://github.com/yetone/magpie-releases/blob/949467a6025f0855aa0299170d2a722c6fadca54/.github/workflows/release.yml

`repository_dispatch` or manual ref/version/publish -> parallel mac arm64+amd64 signing/notarizing; seven CLI cross targets + Windows GUI; Linux native arm64+amd64; downloaded Windows binaries run on native Windows amd64 and arm64; Linux binaries launch under xvfb. Publish needs all platform checks -> checksums -> deterministic commit-list fallback if AI changelog unavailable -> optional Chinese translation -> version-aware Latest -> GitHub Release -> optional Discord webhook -> Homebrew update last. A manual build-only option exists. macOS signing optional; missing cert means ad-hoc unsigned-for-distribution behavior. Source read-only deploy key and signing secrets are kept in release repo.

Useful release principles: make artifact manifest explicit; test the binary to ship, not only its source; distinguish publication success from notification/tap failures; avoid a late older release stealing Latest; record optional service degradation visibly.

Limits: source docs mandate green Test at tagged commit or a later commit containing it, but neither release workflow implements that API/ancestry gate; it is an agent/maintainer procedure. Source release mirror tests an asset count, not exact names/checksums/SHA provenance. Separate repo is justified by signing secrets for this app; do not impose it on projects without that need. Neither source nor release uses overall release concurrency, so Latest and tap version checks can still race between simultaneous publications.

## Review and PR lifecycle: policy and observed practice

Policy in `docs/code-standards.md` and `docs/subsystems/README.md`:

1. Define one observable behavior and owning subsystem. Explain trigger + before/after + state/interface/compatibility/failure changes; no unrelated feature/refactor. Track whole path and sibling cases.
2. Before executing a community contribution, inspect scope and changes to automation/dependencies/credentials/network destinations. Sensitive surface or harmful content waits for maintainer judgment; never run arbitrary PR with real account configuration.
3. Review exact head merged onto freshly fetched current main. Trace runtime callers and state, not diff alone. Verify upstream behavior against source/real saved fixture instead of invented schema.
4. Regression test must fail meaningfully with old implementation. Tests sandbox config/cache, avoid network dependencies, clean up processes/goroutines, and assert user/caller behavior. Mutation checks catch tests that mirror implementation.
5. Run platform/native/race/browser checks appropriate to the change. Current magpie written acceptance policy is broad (full Go suite + native/Linux/Windows vet/build). Browser suite runs locally in Chromium and WebKit with relevant languages; CI Test does not run GUI browser suites. Do not transplant this broad expensive per-change policy into PoBR's explicitly focused-validation budget.
6. Record exact reviewed head/current base, commands, outcomes and not-run limits before merging. New head invalidates review. `gh pr merge --match-head-commit <reviewed SHA>` defends against author push while reviewing. Draft must be made ready.
7. For issue partially addressed use Refs rather than Fixes; await reporter confirmation when appropriate. Preserve issue patch attribution/coauthor credit. One behavior/cause per commit; durable validation evidence in commit body, not only chat.

Observed examples:

- PR #1294 https://github.com/yetone/magpie/pull/1294: first generic catalog toggle caused 373 provider-model rows to change. Formal CHANGES_REQUESTED review verified real catalogs and gateway request bodies. Later round found borrowed provider efforts leaked into unverified relays. Final review at `9ee185a1`, merged onto `c8c3ce9a9`, ran three intentional break checks. Real DeepSeek/Kimi requests failed due to quota/access and were explicitly disclosed; issue left open. Sources: https://github.com/yetone/magpie/pull/1294#pullrequestreview-5455369678, https://github.com/yetone/magpie/pull/1294#issuecomment-6135265435, https://github.com/yetone/magpie/pull/1294#issuecomment-6163254601.
- PR #1354 https://github.com/yetone/magpie/pull/1354: stale main caused conflicts and would restore DOM/logo blinking regression. Reviewer required author rebase and narrow scope. Final checks documented GUI failures and linked existing issues, not a blanket green. After merge users rejected layout; whole change reverted. Technical checks/AI recordings did not prove product acceptance. https://github.com/yetone/magpie/pull/1354#issuecomment-6136098180, https://github.com/yetone/magpie/pull/1354#issuecomment-6163212821, https://github.com/yetone/magpie/pull/1354#issuecomment-6165936194.
- PR #523 https://github.com/yetone/magpie/pull/523: two CHANGES_REQUESTED rounds caught duplicate quota alerts, backward-incompatible existing alert marks and inaccurate fixtures; final comment names reviewed head `cbb58c84`, merged-current-main checks and delivery timing. https://github.com/yetone/magpie/pull/523#issuecomment-5971788408.
- PR #1045 tests-only https://github.com/yetone/magpie/pull/1045#issuecomment-6022464962: independently ran every package test alone and shuffle seeds, reproduced base failure, then checked combined batch. No app release for tests-only policy.

These examples support a maintained review habit; they do not prove all PRs follow it. Review often appears as top-level conversation comments, not APPROVED submissions; #1354 and #1045 have no formal review objects, #1294 formal record still contains CHANGES_REQUESTED plus later accepting comment. A requirement for N formal approvals cannot be inferred.

## Issues, protection and automation governance

- No issue form/PR template exists. Standard labels (bug, enhancement, question etc.) exist but sampled newest issues have no labels/assignee. Docs say PRs currently open to collaborators only; source snapshot does not expose the server setting itself. Reporter fixes may come as issue patches/fork links.
- Current defect report example #4410 gives exact main SHA, browser/language matrix, reproduction command, failure output, observed DOM/root cause, impact and suggested minimal repair. https://github.com/yetone/magpie/issues/4410. This is good content to turn into a template, not evidence a template already exists.
- Release translation absence became a user issue #2122 and later deduplicated automation for a single open incident. https://github.com/yetone/magpie/issues/2122. Warning buried in logs became actionable owner-visible state.
- Legacy branch protection API returns 404. Public rulesets ARE readable: https://github.com/yetone/magpie/rules/24232762 active main/default-branch rules include only deletion and non_fast_forward. No required status checks or pull-request approval rule in this ruleset. Do not claim review/green policy is enforced by it; inaccessible organization/app settings remain unknown.
- Actions run observation on 2026-10-30: source Release https://github.com/yetone/magpie/actions/runs/41024222227 in progress while same-SHA Test https://github.com/yetone/magpie/actions/runs/41024204851 pending. Several immediately preceding Tests cancelled. This snapshot illustrates declared gates need enforced dependencies; do not generalize a time-bounded sample into historical failure rate.

## Portable synthesis, not a clone

Adopt: short entrypoint instructions -> one owner document per contract -> existing task runner -> explicitly named verification tiers -> exact commit-bound review -> artifacts and runtime evidence -> independently gated publication -> issue feedback feeding narrow guard/test fixes.

Adapt: choose OS/browser matrix from supported targets; choose focused versus full checks from cost/risk; make default-branch/full-gate cadence explicit; use templates to collect reproducible input and semantic PR explanation; choose review independence compatible with team size; keep server settings in a documented setup checklist and verify separately from YAML.

Reject as defaults: per-change full multi-platform builds; high-frequency release cadence; personal contributor watch/block lists; unbounded incident text in hot agent instructions; API keys supplied to arbitrary fork PR code; AI preview as functional or product approval; asset count as identity proof; documented release policy without machine gate; Copilot app presence as proof review is required.

UI-preview security source explicitly admits arbitrary PR code can read DEEPSEEK_API_KEY: https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/.github/ui-preview/README.md. Preserve trusted detect/untrusted execute/trusted publish separation, but use mock providers and no live secrets for portable fork-safe preview. Bind generated evidence to current PR head and reject stale results before publishing; source publisher displays requested SHA but does not explicitly compare it to latest `p.head.sha`.
