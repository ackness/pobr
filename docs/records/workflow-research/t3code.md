# T3 Code workflow research

Snapshot: `pingdotgg/t3code`, default branch `main`, commit `5ef56853ef1854da50bc1909c6c0a32700f2cb13`, researched 2026-10-30. Read-only research: no branches, edits, comments, or external mutations. Linked source files use immutable revisions. Raw tree, PR, issue and run API snapshots were inspected during research but are not bundled here.

## Architecture and operating principles

- PNPM/Vite+ monorepo: `apps/{server,web,desktop,mobile,marketing}`, shared contracts/runtime/providers in `packages/`, infrastructure in `infra/`, Rust/native helpers in `native/`, model-facing regression evals in `evals/`, reusable dev/release code in `scripts/`, read-only reference repositories in `.repos/`.
- Root manifest and workspace catalog pin toolchains and central dependency versions, patched dependencies and allowed install scripts. Do not migrate its Vite+, PNPM 12, Effect, private caches, Blacksmith infrastructure or reference-repository machinery merely to reproduce the workflow.
- `AGENTS.md` carries architecture/invariants, focused verification, PR and docs boundaries. `CONTRIBUTING.md` is the outside-contributor eligibility contract. `docs/operations/` holds maintainer runbooks; `docs/internals/` explains decisions/hidden constraints; `docs/user/` serves end users. Scratch plans are outside git; the merged PR is the implementation record.
- Local `vp run check:changed` reuses the same `.github/scripts/ci-changes.cjs` package/dependent mapping as CI, but runs adjacent/focused tests rather than promising a full CI pass. Changes to code, install or generated files invalidate its local result reuse. CI remains the full-suite backstop.
- Test policy favors the lowest layer that reproduces the bug, extending an existing case. Logic/data/protocol fixes get regression tests; cosmetic-only fixes get before/after captures, no mirrored UI tests. Backend change evidence can be focused tests plus a short real runtime transcript. Main agent performs integrated client verification once after integration.

Sources:
- [AGENTS.md](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/AGENTS.md)
- [Development runbook](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/docs/operations/development.md)
- [Testing/review policy](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/docs/internals/testing.md)
- [Package manifest](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/package.json)

## Workflow inventory: all 24 YAML workflows

| Workflow file | Trigger | Role and important boundary |
| --- | --- | --- |
| `ci.yml` | PR; push main | Affected-package/dependent selection; lint; build+typecheck; other-package tests; web browser tests plus production boot; eight server shards; Rust; native-mobile lint; release-smoke; final `Check` aggregator. Main runs the full Linux backstop. |
| `agent-context-budget.yml` | PR opened/sync/reopen/label/unlabel | Prevent model-facing context budget growth against main unless maintainer adds `agent-context-increase`. Domain-specific performance gate. |
| `windows-tests.yml` | Manual | Optional package/file filters for Windows investigation; not ordinary required CI. |
| `mobile-fingerprint-check.yml` | PR opened/sync/reopen/edited | Compare native fingerprints for merge result/base; emit artifact, read-only token. |
| `mobile-fingerprint-label.yml` | workflow_run of fingerprint check | Trusted writer validates completed PR run and updates native-change label; no PR code execution. |
| `thread-transfer-report.yml` | workflow_run of CI | Trusted publisher compares bounded-schema measurement artifact to successful main baseline; updates one PR comment. |
| `pr-size.yml` | pull_request_target opened/reopen/sync/ready/draft | Metadata-only size labels; checkout base, effective line counts; test exclusions for mixed PRs. Size is review context, not eligibility. |
| `pr-vouch.yml` | PR target; `/recheck-vouch` issue comment; main updates to trust file/workflow | Reconcile one of `vouch:trusted/unvouched/denounced`. Repository permission/vouch status is separate from triage exemptions. |
| `issue-labels.yml` | Main changes to issue forms/workflow; manual | Upsert owned issue labels; doesn't delete user labels. |
| `stale-needs-more-info.yml` | Daily; manual | Only issues already labeled `needs more info`; close after seven days with remedy/reopen message. Does not stale or close general PRs/issues. |
| `web-preview.yml` | PR labeled/sync/reopen | Same-repo PRs with `preview:web`; Vercel deployment and PR URL comment. New PR head cancels prior build. |
| `relay-preview.yml` | PR labeled/sync/reopen | Same-repo PRs with `relay:preview`; protected environment; verify still open/labeled. Serial deployments aren't canceled. |
| `relay-preview-cleanup.yml` | PR target unlabeled/closed; daily; manual | Trusted main code tears down preview resources, uses labels as retry markers, sweeps closed previews; open-PR recheck before teardown. |
| `desktop-macos-preview.yml` | PR labeled | On one-shot `preview:mac`, build JS without signing secrets; upload `js-bundle`. |
| `desktop-macos-preview-publish.yml` | Completed build workflow; PR target closed/unlabeled | Trusted main validator requires exact current PR SHA/run/artifact, label and trusted author; consumes label; package/sign trusted code treating PR bundle as data; publish rolling preview; cleanup removed/closed previews. |
| `mobile-eas-preview.yml` | Same-repo PR opened/reopen/sync/label | Label-opt-in EAS native/fingerprint preview; preview-specific credential. |
| `mobile-eas-preview-fork.yml` | Fork PR target labeled | Explicit maintainer opt-in for exactly label-event head SHA; consumes label; persist-credentials false; no shared caches; executes reviewed PR app/plugins with EXPO_PREVIEW_TOKEN and preview signing credentials. This is an exceptional trust elevation, not a generic safe fork-CI template. |
| `mobile-eas-production.yml` | Main path changes; manual build/update/profile/platform/version | Store builds/submission vs OTA chosen using fingerprint; source-map upload; optional manual version override uses release-app token. |
| `mobile-android-apk.yml` | Hourly; manual | Mirror newest Play APK; skip cleanly if no Play credential. |
| `mobile-showcase-screenshots.yml` | Manual platform/appearance/theme | iOS simulator and Android emulator screenshot capture; validate store assets and upload. |
| `deploy-relay.yml` | Main push; manual force | Production relay environment; no canceled publishers; publish deployment commit status. |
| `release.yml` | Stable tags; every 30 minutes; manual channel/version | Resolve exact SHA, preflight, full quality/tests, build one JS bundle, six desktop targets, GitHub release, npm OIDC, optional AUR/web/marketing/finalize/Discord. |
| `release-desktop.yml` | Reusable workflow_call | Per-platform package/sign/smoke and target-architecture verification; receives explicit ref/version/channel. |
| `publish-aur.yml` | Reusable; manual | Serialized AUR package publish; SSH credential only in publishing context; unprivileged packaging user. |

Permanent directory source: [workflows](https://github.com/pingdotgg/t3code/tree/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/workflows).

## CI control logic worth extracting

1. **One affected graph for local and CI.** Map files to owning workspace package, take transitive reverse dependencies, explicitly register cross-package file reads absent from manifests. Unknown paths/manifests/global test setup run everything; failed diff resolution runs everything. Rename detection lists old+new paths. Documentation is inert for package suites, but always-on formatting still covers it.
2. **Stable result aggregation with explained skips.** `Check` runs with `always()`. Every listed dependency must succeed, or be skipped only when every gate associated with that job is explicitly `false`. Cancelled, failed, detector failure/missing outputs and unexplained skips fail. Server jobs self-gate after checkout instead of waiting for detector job; this saves scheduling latency but still reports successful empty shards when irrelevant.
3. **Parallelism respects test isolation.** Eight server shards distribute by recorded duration across machines, preserving `fileParallelism:false` on each. Package tests run with bounded concurrency. Don't apply this costly topology to small Rust changes.
4. **Cache identity follows inputs.** Toolchain cache includes Vite+, Node, PNPM, OS/arch. PNPM store distinguishes filtered/full install namespace and lock/workspace/root manifest. Browser download cache includes browser version. Private task cache is disabled for fork PRs; legitimate same-repo workflow uses its own rules. Cache is an optimization, never an approval signal.
5. **Separate measurement from privileged publication.** CI writes measurement artifacts, trusted `workflow_run` scripts validate bounded JSON/schema and current PR/run/SHA before posting. An artifact is untrusted data; do not execute it in the writer job. Measurement report and pass/fail threshold remain distinct.
6. **Cancel obsolete validation; serialize publication.** PR CI cancels old head, main SHA checks remain separately addressable. Release/deploy never cancel an active publisher; release uses `queue:max` and channel groups.

Sources: [CI](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/workflows/ci.yml), [detector](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/scripts/ci-changes.cjs), [diff adapter](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/actions/ci-changes/action.yml), [dependency setup](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/actions/setup-ci-deps/action.yml), [trusted report](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/scripts/thread-transfer-report.cjs).

## Contribution, review, issue flow

- Limited outside review capacity is explicit. One underlying problem per external PR, even if the solution crosses contracts/server/client/tests/docs. Drafts receive the same eligibility check. Very small obvious bug fixes and narrow configuration of existing capabilities can qualify without prior product approval. New product behavior needs explicit linked maintainer approval of direction/scope; a linked issue alone isn't approval. Maintainers have a documented batching exception.
- PR template asks for Problem, Change, Scope and approval, Verification. Verification names exact commands/manual steps and observations/unchecked limits, not just checkboxes. Visible changes attach before/after images; motion/timing gets video. PR-only assets are uploaded, not committed. Conventional title; agent model/harness attribution in body.
- Bug Issue Form asks component, deterministic repro, expected/actual, impact, version/environment/logs/uploads/workaround. Feature proposals go to Ideas Discussions; blank issues disabled. Separate `via-triage` form records diagnosis/evidence/fix/workaround/model for agent-investigated support reports. Security reports use security@ping.gg and external policy.
- `npx t3 triage` support playbook is a different workflow from contribution eligibility. It collects machine/version facts, uses version-pinned source, grounds diagnosis in logs/code, searches duplicates/fixes, redacts credentials, and prepares an issue for explicit user authorization. Useful reusable distinction: a support report is evidence, not a feature implementation request.
- Contribution triage skill loads all policy and exemption files from one freshly resolved trusted main SHA. Fork/PR modifications cannot change authority. Missing/malformed trusted policy yields unresolved routing, never exemption/auto-closure. It fetches cumulative current PR state and complete/paginated evidence. It doesn't execute PR code to decide eligibility.
- `TRIAGE_EXEMPTIONS.td` alone controls bypass; VOUCHED, organization/write status or previous success do not. `triage:keep-open` or an exempt maintainer's historical comment/review protects against automated closure but does not approve eligibility/correctness.
- Eligibility, review requested, review complete and merge authorization remain distinct. Intended Macroscope handoff label is `macroscope-review`, but CONTRIBUTING says integration/native event rollout remains partly unverified; unsupported events and no periodic sweep can leave stale eligibility until a supported event/explicit assessment.
- Bot review is advisory: CodeRabbit path instructions reference the same maintained Effect/UI/testing policies. Its TypeScript config auto-reviews and changes pre-merge behavior based on author association; Macroscope is separately observed in PRs. No CI workflow in this tree automatically runs contribution-triage skill or proves its full deployment.
- T3-specific bans on some code-generation models, mandatory attributions, vouch trust infrastructure and outside-contribution scarcity are product policies, not portable defaults.

Sources: [CONTRIBUTING](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/CONTRIBUTING.md), [PR template](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/pull_request_template.md), [Issue forms](https://github.com/pingdotgg/t3code/tree/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/ISSUE_TEMPLATE), [contribution triage skill](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.agents/skills/contribution-triage/SKILL.md), [CodeRabbit config](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.coderabbit.config.ts).

## Release flow and artifact lineage

- Resolve release commit once and pass that explicit ref to all stages. Manual stable promotes latest published nightly commit, not moving main. Tag release builds exact tagged commit; scheduled nightly checks whether a new release is warranted. Publication waits for all release quality/test/package jobs.
- Compile common JS once, fan out six desktop targets. Same-arch macOS/Linux native builds; Windows packaging/signing on Linux and target-architecture Windows verification. CLI archives are smoke tested; Windows embeds exact same-arch Linux archive for WSL, with checksums.
- Preview is manual and defaults in dispatch; full real build/sign/publish train, isolated `preview` npm tag and GitHub prerelease, no updater feed/manifests, hosted-web aliases/AUR/Discord skipped. It still publishes and uses signing secrets: do not call it a dry run. Nightly and stable are user-facing distribution channels.
- npm publishing uses OIDC in its own job; release writes, production environments, release-app token and platform signing credentials are scoped to dependent publish/build jobs. Signing-sensitive actions are commit pinned in reusable workflow. Missing signing credentials produces unsigned builds, not non-publishing execution.
- Documentation drift exists: later release runbook sections still discuss only stable/nightly and say use nightly as the release test train despite earlier/current source adding preview. Source/tests win; rewrite stale paragraphs rather than preserve contradictory history.

Sources: [release workflow](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/workflows/release.yml), [reusable packaging](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/workflows/release-desktop.yml), [release runbook](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/docs/operations/release.md).

## Observed practice, not inferred enforcement

Public ruleset [Passing CI on main #13856153](https://github.com/pingdotgg/t3code/rules/13856153) was retrievable. It requires eight contexts: `Test`, `Check`, `Mobile Native Static Analysis`, `Release Smoke`, `Test Server 1/2/3`, `Rust`. This contradicts CI's comment claiming only `Check` needs protection configuration. Drift warning: the aggregator is sound, but migration requires auditing/removing stale individual required contexts separately. Ruleset requires zero approving reviews, no codeowner review, no last-push approval, no thread-resolution gate; allows squash/rebase. Do not claim enforced human approval. Legacy `/branches/main/protection` returned 404; ruleset endpoint succeeded.

Bounded samples (not proof of universal behavior):

- [PR #21878](https://github.com/pingdotgg/t3code/pull/21878): external cosmetic fix, scope-exception explanation and before/after screenshots. Macroscope approved an earlier head. Maintainer merged current main, relocated exact icon substitution after upstream refactor, recorded focused checks and real user flow at final SHA `acd4f1508490b51458d1c3192ad4dd75a943596c`, then merged. Final Check success; Rust/release-smoke/mobile-native appropriately skipped. Shows verify final version, preserve contributor intent, no broad UI regression suite for a two-line icon change.
- [PR #19941](https://github.com/pingdotgg/t3code/pull/19941): initial maintainer changes requested with explicit product direction; many CodeRabbit/Macroscope iterations; final maintainer APPROVED on head `d700ada0192f99288f0ed786c935da9bb57c40e5`, merged by juliusmarminge. Shows bot comments are not product approval.
- [PR #27115](https://github.com/pingdotgg/t3code/pull/27115): final Macroscope APPROVED on `66e436831b623f6c46454155d38c39b1d39a3b39`, earlier bot approvals dismissed, maintainers merged. Final Check and relevant suites success. Shows approval must be attributed to a SHA, while required human approval cannot be assumed.
- [PR #27127](https://github.com/pingdotgg/t3code/pull/27127): focused database-copy isolation repair, existing regression fails on main then seven pass. Maintainer could not push to fork despite maintainerCanModify, rebuilt with coauthor credit on replacement, documented original/replacement comparison and real CLI behavior; original closed in favor of replacement. Good external-PR handling pattern, no need to automate takeovers by default.
- [Scheduled release run 41021531810](https://github.com/pingdotgg/t3code/actions/runs/41021531810) was successful; source SHA `97e35bcf23e4dcf34c2dcffadd37c08db0e01e52`. Other recent scheduled runs succeeded. This proves workflow execution, not that every package published: preflight can skip no-change runs.
- Recent issues #27431 and #27437 use expected/actual or diagnosis/source evidence structures; sampled labels were empty, showing Issue Form declarations do not prove every report was filed through the form or auto-labeled.

## Suggested migration to PoBR

Reuse ideas: a single affected-check plan; stable fail-closed result aggregator; focused local check versus full protected CI versus explicit release/publish; merge evidence pinned to SHA; test only risky behavior at existing layer; simple evidence-oriented PR and issue templates; owned label reconciliation; privileged metadata operations separated from fork code; one maintainer runbook with contributor/agent docs linking to it.

Avoid importing: eight shards, paid/private runners/caches, mobile/native matrices, VOUCHED/model bans, automated PR closures, mandatory feature-discussion approval for ordinary maintainer work, preview deployment infra without a concrete product need, or a second scheduler. Check PoBR's actual cost profile, artifact lineage, existing release jobs and commands first.

Provenance: T3 Code is MIT licensed at this snapshot. This report extracts design, not substantial source code. Any future copied source requires preserving MIT notice.
