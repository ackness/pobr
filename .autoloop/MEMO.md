# PoBR autonomous improvement loop

## Scope and standing decisions

- Loop: `loop-20261011-1650`; branch: `autoloop/loop-20261011-1650`.
- Work only in `/Users/wuyong/codes/game/pobr/.omp/worktrees/autoloop-loop-20261011-1650`.
- Focus: auto. The initial focus question was cancelled; the restarted instruction explicitly leaves selection to the assistant.
- Commit only. The driver handles publication; do not push, open PRs or create release tags.
- Leave the paused `/goal` unchanged as instructed.
- No inbox steering at initialization.
- Prior-loop observations are historical leads, not verification of this branch. Do not duplicate its pending changes without checking current evidence.

## Product and survey

PoBR is a browser-based PoE2 planner using a Rust/WASM engine, versioned game data and three UI languages. Its primary promise is comparing equipment, support sets and connected passive upgrades before changing a build. Calculation coverage is explicitly incomplete.

Current starting commit: `6bd4cee` (`fix: identify complete attribution skill groups`). Recent history emphasizes import recovery, deduplicated item libraries and preservation of imported weapon-hand candidates. `gh issue list --state open --limit 15` completed successfully with no open issues returned.

Survey sources: README product promise; CLAUDE architecture/validation table; `docs/web-contracts.md` support/passive/workspace contracts; `web/src/lib/passivePlanner.ts`; `web/e2e/passive-continuity.spec.ts`; current git history. The existing planner combines connected routes, bounded refunds, atomic travel attributes, unsupported-effect receipts and explicit application. Existing browser coverage includes synthetic three-point routes and jewel continuity; realistic populated-build behavior still needs direct observation in this loop.

## Long-term goal

Make PoBR a trustworthy decision workspace for real PoE2 builds: comparisons must show their assumptions and limits, preserve player work, and produce the same result when applied as when previewed.

## First milestone: Trustworthy passive reallocation decisions

Prioritize passive planning because it combines topology, scarce point budgets, travel attributes, refunds, calculation limits and persisted state in one player decision. This is a complementary scope to recent import work and prior-loop support-mechanic investigation.

Exit criteria:

1. Observe real shipped-WASM multi-point route searches on a populated imported build, including total/new/refunded point counts and no mutation before Apply.
2. Exercise a valid bounded refund/reallocation and confirm connectivity and protected allocations are retained; if the imported topology correctly disables refunds, verify the explanation and use a connected fixture for the positive path.
3. Applied nodes and travel attributes agree with the chosen preview, with unrelated equipment, gems and stages preserved through navigation/reload.
4. Baseline limitations and excluded edits remain visible; stale results cannot apply after relevant edits.
5. Fix only evidenced defects, with targeted regression checks and a direct runtime proof. Do not infer complete passive mechanic coverage from ranking or numerical parity.

## Iteration 1 plan

Establish this branch's runnable browser surface and inspect the passive planning workflow against the contract. Record concrete evidence and implement one bounded defect if observed. Do not assume previous-worktree WASM or running servers represent this branch.

## Candidate next priorities

- Continue realistic route/refund and preview/application verification until milestone exit criteria are met.
- Evaluate import/workspace discovery only when an observed workflow problem warrants it; existing history already contains substantial correctness work.
- Revisit broader calculation limits separately, without treating undocumented raw stats as uniformly unsupported.

## Iteration 1 evidence and handoff

- Branch-local frozen-lockfile dependencies installed; release WASM built from this branch; `sync-data` copied 67 runtime files and 17 dated references. No vendor clone was needed.
- Vite service `pobr-passives-1650` serves this worktree at `http://127.0.0.1:5182/`. Do not use the prior loop's port 5181 as evidence for this branch.
- Real browser imported `examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt`: level 98 Invoker, 131 allocated nodes, 13 skill groups, 10 equipment slots.
- Three-point search with the existing Balanced DPS / EHP objective evaluated 391 plans and displayed 10 recommendations, 10 baseline calculation limits, and an explicit bounded-search notice. No excluded edits appeared in this particular search; it does not verify the exclusion path.
- Best route: Cooldown Recovery Rate, Temporal Mastery, Elemental Damage; highlighted node IDs `17796`, `12412`, `45497`. Both searching and Show on tree left the entire saved envelope unchanged.
- Apply changed only `allocatedNodes`, from 131 to 134. Every other state field, including all groups, equipment and attribute choices, remained unchanged; the other saved build was also unchanged. This route contained no new travel attributes, so atomic attribute changes remain unverified here.
- Direct calculation through the same branch's real backend returned TotalDPS `7560.833856375`, TotalEHP `23661.486537325`, Life `1`, matching the displayed preview `7560.83`, `23661.49`, `1`. Sidebar displayed Hit DPS `7,561`.
- A reload assertion based on JSON string equality failed because restored object keys use a different order. A fresh import/search/Apply/reload run compared every state field with Node's `isDeepStrictEqual`: semantic equality true, zero changed fields. No product fix is warranted for serialization ordering.
- Reallocate mode correctly disabled search for this imported fixture. Weapon-exclusive node counts are 23 and 22; the visible explanation names weapon-set passives and other unsafe imported topology. Positive refunds are not verified by a disabled control.
- Visually inspected route preview and disabled-refund screenshots; temporary screenshots removed. Browser task space 29 finished. No permanent tests or product code changed during this investigation.
- Browser automation caveat: automatic click scrolling hung on the nested planner drawer; after inspecting the unchanged state, `scrollIntoView({block: 'center'})` followed by a real click completed the search in seconds. Tool issue reported; do not attribute that timeout to calculation performance.

Next iteration: use a connected shared-tree fixture for a positive bounded refund/reallocation, including a replacement travel attribute. Verify exact preview/apply values, protected sockets, unrelated state and reload. Then cover stale-plan invalidation, other-stage isolation and a real excluded-edit receipt before closing the milestone.
