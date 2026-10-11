# Product improvement loop

## Standing scope
- Work only in the assigned loop worktree and branch; commit only, never push.
- Focus: auto. The initial focus question received no selection.

## Long-term goal
Make PoBR a trustworthy browser workspace for comparing PoE2 upgrades: preserve player work across imports and stages, keep recommendation limitations visible, and make the next action understandable.

## First milestone: trustworthy build-workspace lifecycle
Status: completed in iteration 4, with the runtime evidence below. No release tag
created because this loop is explicitly commit-only.
Exit criteria:
1. Player-facing English and Chinese onboarding accurately distinguishes adding builds, replacing a stage, sharing a build and restoring a workspace backup.
2. Exercise actual import, stage switching, reload and backup/restore with real WASM; investigate any observed loss of notes, drafts, selection or planning context.
3. Address evidenced lifecycle defects with focused regressions, retaining existing calculation and unsupported-effect boundaries.
4. Review import/planning feedback for actionable errors without discarding prior results.

## Completed milestone: trustworthy whole-item comparison decisions
Status: completed in iteration 5. No tag under commit-only scope.
Exercise the primary README promise on a populated character: compare a complete
item before changing equipment, inspect assumptions and limitations, then apply
only the evaluated candidate.
Exit criteria:
1. Real-WASM comparison across compatible positions preserves the current build until explicit application.
2. Applied item text and displayed recalculation agree with the selected preview, including augment assumptions.
3. Baseline limits and new unsupported effects remain visible and are not mistaken for guaranteed gains.
4. Address only evidenced defects in this path and verify the corrected player-facing surface.

## Completed milestone: trustworthy support recommendation decisions
Status: completed in iteration 7 for the search/application/disclosure path; full mechanic correctness is not asserted. No tag under commit-only scope.
Use a populated skill group to verify eligible support comparisons, player
exclusions and explicit application without changing other skill groups.
Exit criteria:
1. Run real-WASM support search and inspect modeled-effect and bounded-search limits.
2. Excluding a support changes the candidate pool without removing the equipped baseline.
3. Applying a selected result preserves the intended skill group and agrees with previewed output.
4. Fix evidenced player-facing defects; do not widen support mechanic coverage without parity evidence.

## Completed milestone: support downside fidelity
Status: completed in iteration 8 for Controlled Destruction / NeverCrit. No tag under commit-only scope.
Investigate the source-described no-critical-hit downside of Controlled Destruction
before trusting its apparent gain on the populated Frost Bomb fixture.
Exit criteria:
1. Reproduce an isolated equipped/removed comparison and inspect effective hit/crit outputs and existing diagnostics.
2. Trace pinned PoB2 behavior through exported data and the Rust consumer; distinguish a calculation defect from a display-only statistic or source-text mismatch.
3. If a defect is proven, fix the actual consumer with source-backed parity/regression coverage and real-WASM verification. Do not infer mechanics from prose alone or silently suppress unsupported effects.

## Completed milestone: recommendation modeling-limit fidelity
Status: completed in iteration 10 for the two audited resource mechanics, not
exhaustive support-economy coverage. Maximum-resource loss and Life sacrifice
now have distinct source-attributed limits; no sustain formula was invented.
Exit criteria:
1. Reproduce the Life-sacrifice candidate's effects against the pinned source and actual calculated request.
2. Distinguish supported numeric effects from silent input/consumer omissions; do not infer coverage from prose or positive DPS.
3. Correct a proven omission through existing ingestion/diagnostic or calculation boundaries, with source-backed tests and real-WASM verification. Do not invent a sustain approximation or special-case a support name.

## Current milestone: trustworthy passive reallocation decisions
Extend the earlier one-point passive smoke to realistic multi-point routes and
refunds on the populated build. This is the next core upgrade workflow whose
preview/application boundaries have not been exercised deeply in this loop.
Exit criteria:
1. Real-WASM route/refund search respects the requested point budget and preserves the current build before explicit application.
2. Applying a selected plan agrees with its preview and preserves unrelated equipment, gems and workspace stages.
3. Existing/new modeling limits remain visible and distinguishable; fix only evidenced defects rather than inferring complete passive coverage.

## Verified support evidence
- Iteration 6 searched the populated Frost Bomb group (index 4, level 19/quality 20, four support slots). The initial result reported 824 complete recalculations, 8 baseline limits and 8 excluded complete edits.
- Excluded Atziri's Impatience from a recommendation, then excluded equipped Efficiency II through the checklist. Neither altered equipped socket groups or baseline DPS 7347.1; subsequent recommendations contained neither support.
- Applied Considered Casting, Concentrated Area, Controlled Destruction and Elemental Focus. The preview's 19799.8 DPS became displayed hit DPS 19800. Other 12 groups were deep-equal; Frost Bomb remained level 19/quality 20 with main group index 4.
- Reload retained both exclusion IDs under the FrostBombPlayer preference key; Efficiency II remained visibly unchecked.
- Iteration 6 found no search/application defect, but chips lacked effect descriptions. Iteration 7 added a native disclosure using optional verbatim English descriptions from the pinned PoB2 skill definitions, with localized labels and explicit numeric/coverage limits.
- A throwaway assertion incorrectly assumed GemInput carries is_support; corrected the verification to inspect the actual ID/level/quality contract. This was a smoke-script error, not an application failure.
- Regeneration added 538 descriptions to 567 support entries; 28 lack known compatibility and Feeding Frenzy II lacks source prose. Deleting the new fields reproduces the original catalog exactly; the companion trade map regenerated byte-identically.
- Real-WASM search produced 931 complete recalculations. Source-described costs are visible; keyboard toggling, 390px no-overflow layout, all three UI languages and description-free catalog fallback were verified. English prose carries lang=en. Eighteen support tests and TypeScript checking passed; the optional benchmark was skipped.
- Iteration 8 proved the Controlled Destruction discrepancy is a real calculation defect: global_cannot_crit was absent from implicit extraction, NeverCrit was rejected by mapping, and the shared crit resolver had no consumer. All three links are now fixed; no-crit wins over other crit bonuses and its source remains in attribution.
- Both permanent regressions failed before the fix. The pinned PoB2 oracle confirmed crit chance/multiplier zero; after rebuilding WASM, the minimal support fixture reports zero/zero with its noncritical damage benefit retained. Twenty-one crit tests, sixteen support-gating tests, two no-regression parity gates, and scoped fmt/Clippy passed. Independent review approved.
- The populated fixture's formerly inflated 19799.75 DPS is now 4419.6 with Controlled Destruction equipped. Top eight recommendations omit it. Applied the top replacement (Considered Casting, Concentrated Area, Elemental Focus, Atziri's Impatience): preview 28546.6 became displayed 28547, crit returned to 100%, other twelve groups were unchanged, and reload retained the result.
- Exact regeneration added only five implicit flag entries: Controlled Destruction and four Elemental Storm stat sets. Storm's raw hit components remain present in sets 2–4 and all four show zero crit; trigger throughput/full-DPS parity was not established by the direct fixture.
- Iteration 9 proved Atziri's resource-loss input survives extraction but has no upstream or native on-use consumer. Canonical stat prose specifies maximum Life/Mana/Energy Shield. A curated raw-stat classifier now discloses that proven missing mechanic through the existing compatible-host limit observer; costs, recovery and DPS are unchanged.
- Equipped Atziri now appears among nine baseline limits. After removing it and searching with four slots, top eight plans omit it; two excluded complete sets show attributed loss receipts and no Apply controls. Applied Cooldown Recovery II instead: preview 19670.8 displayed 19671, other twelve groups remained unchanged, and reload retained it.
- Nineteen support-gating tests, five WASM support contracts, eighteen planner tests and scoped fmt/Clippy passed. Rebuilt WASM and inspected the real warning surface; independent review approved. Tests preserve unknown/control/manual-consumer behavior and verify the raw mechanic under another support name.
- The temporary numeric oracle probe did not include the lineage support (supports was empty), so it was not used as parity evidence. Source payload/map/consumer inspection establishes the diagnostic, not an inferred sustain model. Maximum-resource-loss disclosure is not exhaustive sacrifice/resource-economy coverage.
- Iteration 10 reproduced Extraction's 10% Life sacrifice missing from diagnostics while its damage benefit applied. Pinned description entry 764 says Life sacrifice without maximum/current semantics. Added a separate raw-stat classification; the same stat is retained for Guatelitzi's Ablation (25) and Sacrificial Offering (15), without support-name filtering.
- Extended the existing three resource-effect regressions to both canonical mechanics, preserving all previous assertions. Two failed before the addition; all nineteen support-gating tests, five WASM support contracts, eighteen planner tests and scoped fmt/Clippy passed afterward. Independent review approved without findings.
- Rebuilt WASM: the minimal Extraction fixture now reports a precise value-10 sacrifice receipt, with DPS46.561230576 and ordinary costs unchanged. Populated search shows two excluded Extraction complete sets with no Apply controls; it finds no better modeled plan than the current Cooldown Recovery II setup. Screenshot inspected; no build edits were applied this iteration.

## Verified whole-item evidence
- Both ring positions were compared without mutating workspace bytes. Ring 2 applied the exact evaluated candidate, preserving all other item slots; preview DPS 6654.85 displayed as 6655 after recalculation.
- A synthetic unmodeled line remained in diagnostics (11 versus baseline 10) and no recommended badge appeared. Manual application remains available under explicit partial-calculation warnings.
- A bare Ancestral Tiara comparison inherited Perfect Iron Rune and disclosed one assumed socket plus excluded costs. As-listed EHP was 24099.31; inherited setup was 24699.76. Applying inherited setup stored the named rune/socket payload and displayed EHP 24700, preserving other slots.
- Identified missing post-apply feedback. Added a localized status naming the evaluated item and destination, gated on matching equipped text/weapon set and settled session work. It does not claim complete mechanic modeling.
- Real browser verified equipment, jewel and flask confirmations; English/Traditional/Simplified messages; absence before apply; clearing on text edit and navigation. Comparison/augment unit tests (39) and typecheck passed.

## Initial survey, 2026-10-11
- README positions the product around comparing gear, supports and passive upgrades; incomplete mechanics are explicitly disclosed.
- docs/web-contracts.md describes independent builds and stages, additive default imports, explicit current-stage replacement and whole-workspace restoration.
- web/src/lib/buildWorkspace.ts implements workspace creation, stage selection, atomic envelope serialization and rejecting malformed collections.
- web/e2e/build-workspace.spec.ts covers independent builds/stages, sharing, reload, restoration and unapplied drafts. Coverage exists; execution is not yet verified in this worktree.
- Recent history concentrates on rejected-import preservation, library deduplication, hand-role candidates and attribution identity. This makes lifecycle reliability a concrete first milestone rather than a speculative new feature.
- `gh issue list --state open --limit 15` succeeded with no open issue rows.
- Confirmed documentation gap: README says importing another build replaces the current one, contradicting the current workspace contract and test scenarios. Correct this first without claiming runtime verification.

## Verified lifecycle evidence
- Iteration 2 built release WASM and synced data 4.5.5.3 in this worktree, then ran Vite on 127.0.0.1:5181.
- Real browser: Starter level 30 and Endgame level 90 retained distinct notes across stage selection and reload. Importing a synthetic level-75 Deadeye added a second build without overwriting either stage.
- Downloaded workspace backup restored two builds after creating a third temporary build; confirmation appeared before replacement.
- Rejected arbitrary JSON preserved the active level, notes and collection. The raw `invalid .build json: missing field passives` error remained visible across pages; actionable/localized import guidance merits review.
- An unapplied +123-life ring draft survived page navigation and reload without becoming equipped. The downloaded workspace backup contained it; the shared build file excluded it.
- Corrected the observed obsolete editor notice in all three languages. English surface visually verified; workspace unit tests (5) and TypeScript check passed.
- Iteration 3 used the committed monk-invoker-frost-bomb code: level 98, 13 socket groups, 10 equipped items, 131 passives. Replacement cancellation preserved exact workspace bytes. Confirmed replacement changed only its copied target stage to level 45; the original populated stage remained intact.
- Sharing and reimporting the two-stage build added an independent build with fresh build/stage IDs and semantically equal saved stages. An initial JSON-string comparison failed only because property order changed; deepStrictEqual passed.
- A one-point passive search produced a 6265.34 → 6488.01 DPS recommendation. Rejecting arbitrary JSON preserved the workspace bytes, displayed stats and completed plan text. Applying the retained recommendation gave 132 allocated nodes and displayed hit DPS 6488.
- Iteration 4 added recovery advice after rejected pastes in all three languages. Recognized PoBR envelopes point to file import with a restore warning; other inputs point to complete supported sources. The original diagnostic remains intact.
- Browser checks verified file/source advice, unchanged workspace bytes after rejected backup paste, advice clearing on edit, language switching and successful populated-code retry with no remaining error or advice. Existing import tests (17) and typecheck passed. No decoder or calculation contract changed.

## Operational notes
- Driver state reports PR publication failure resolving base branch `autoloop-base`. No publishing commands were run; repair remains outside commit-only scope.
- Browser confirmation handling worked when upload and acceptDialog occurred in one ego-browser invocation; splitting across rounds did not restore. Reported tooling discrepancy, not an application defect.
- Pinned PoB2 vendor ce566eac45ea8a86477f513c7ee65a1ebe60014e is now cloned inside this worktree. Iteration 7 regenerated trade metadata, repacked the snapshot manifest and synced 67 runtime files; existing release WASM was reused because no engine/API behavior changed.

## Priority queue
1. Exercise multi-point passive routes/refunds and preview/application integrity on the populated fixture.
2. Keep resource-loss/sacrifice warnings visible; complete support economics and other unknown stat families remain unclaimed.
3. Any further mechanic coverage needs canonical input/consumer evidence and a failing-before regression, not prose or oracle DPS alone.
