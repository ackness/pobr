# Offensive Parity Diagnosis & Roadmap (2026-06-28)

> Source: ultracode diagnosis workflow (4 build-cluster diagnosers vs vendor `CalcOffence.lua`/`CalcPerform.lua` + feasibility agent). Read-only analysis; no code changed by the workflow. Each fix is vendor-anchored, not metric-tuning.

Baseline at time of diagnosis (effective mode, @5%): def25 84.2% / defcore 91.7% / **off 76.2% (panel 48.8%)** / dot 54.1%. The offensive gap is the target.

## Critical caution — compensating false-hits
Several builds pass `TotalDPS` only because component errors cancel. Most notably **monk-martial-artist-flicker-strike**: Speed +15% over × AverageDamage −10% under → TotalDPS 1.04x (false pass). The parity gate MUST keep asserting per-component (Speed/AvgDamage/CritMult), not just TotalDPS, or fixes that correct one component will *appear* to regress. Fixing flicker needs #1+#2 landed together.

## Ranked fixes

### #1 — Onslaught 'during effect' flask/charm flag fired unconditionally (phantom +20% INC attack/cast speed)  `[high/small]`
- **Builds:** witch-blood-mage-coiling-bolts, witch-abyssal-lich-detonate-dead, monk-martial-artist-flicker-strike
- **Mechanism:** item.rs parse_granted_buff_flag strips the 'during effect' suffix and emits a GLOBAL Modifier::flag("Onslaught"), which triggers the basic-form buff_definitions Onslaught entry adding +20% INC speed even when the granting flask is inactive. Vendor CalcPerform.lua:618-648 only emits Onslaught speed inside an active-flask loop; the demo goldens were exported with the flask OFF. Exact arithmetic proof: detonate-dead 1.25*(1+110%)=2.625 vendor vs PoBR 1.25*(1+130%)=2.875; coiling (1+135%)*0.85=2.00 vs (1+155%)*0.85=2.17 — the +20% is exactly the gap.
- **Fix:** item.rs:311 must NOT emit a global Onslaught flag for a flask/charm 'during effect' mod; emit a flask/charm-active-conditional flag (Condition:UsingFlask / providing-item active state) and finish the stubbed buff:onslaught_flask handler (buff_expander.rs:61-72), making it mutually exclusive with the basic-form Onslaught def so it does not double-fire. Read each build's flask-active config so a legitimately-toggled onslaught still applies; do NOT blanket-strip on text.
- **Expected gain:** detonate-dead Speed 1.09x -> ~1.00x (clean TotalDPS 1.08x -> 1.00x); flicker Speed 1.15x -> ~1.00x; coiling Speed 1.08x -> 1.00x (TotalDPS temporarily looks worse, exposing Fix #11).
- **Regression risk:** Medium. 7 demo builds carry 'Grants Onslaught during effect'; gating on text instead of flask-active config would wrongly strip legitimately-active onslaught. ninja_parity Speed columns (both directions) bound it. NOTE: fixing this UNMASKS coiling's hidden damage shortfall (Fix #11) so coiling TotalDPS headline drops 0.89x -> ~0.82x — that is the audit working as intended, not a regression; keep per-component Speed asserts.

### #2 — DistanceRamp tag rejected in translate_tag -> entire MORE-damage mod dropped (Close Combat / PointBlank / FarShot)  `[high/medium]`
- **Builds:** monk-martial-artist-twister, monk-martial-artist-flicker-strike, warrior-smith-of-kitava-shield-wall
- **Mechanism:** stat_map_engine.rs translate_tag has no DistanceRamp arm, so it falls to the default `other => UnsupportedTag`; the WHOLE mod entry is then marked Unsupported and injected as zero, dropping Close Combat's distance-ramped MORE damage. The data is present in skill_stat_map.json — it is rejected at translation, not missing. At enemyDistance=20 PoB applies +18% MORE (CC-II), +30% close (Shield Wall).
- **Fix:** Add a DistanceRamp arm to translate_tag (stat_map_engine.rs:1669) producing a new ModTag variant carrying the ramp points + per-stat scalar; add a damage-side evaluator that reads Multiplier:enemyDistance (effective mode) / 0 (panel mode), linearly interpolates between ramp points with clamp, multiplies by the constantStat, and feeds a Damage MORE into the hit aggregation. Same machinery unblocks PointBlank/FarShot (inverse ramp).
- **Expected gain:** monk-twister AvgDamage 0.80x -> ~0.85x (x1.18; full parity also needs Fix #8 crit-mult); Shield Wall 0.69x -> ~0.79x (full parity needs Fix #5 Heft); flicker AvgDamage lifted toward 1.0.
- **Regression risk:** Medium. Adds a previously-zero MORE to every CC/PointBlank/FarShot build; must gate by mode_effective and replicate vendor linear-interp + clamp exactly or other-distance builds over/undershoot. For flicker this alone pushes AvgDamage 0.90x -> ~1.06x over, so coordinate with Fix #1 and expect a temporary flicker overshoot; keep per-component asserts.

### #3 — Essence Drain (DoT-only skill) fabricates a spell hit and sums it into CombinedDPS  `[high/small]`
- **Builds:** sorceress-chronomancer-essence-drain
- **Mechanism:** Essence Drain is a pure chaos damage-over-time applicator in PoE2 (act_int.lua:6376, skillType DamageOverTime, no hit damage). PoBR routes its spell base chaos into the standard spell HIT pass and then combines: combined_dps = base_dps + total_dot_dps (skill_dot.rs:310), inventing AverageDamage 149.68 / TotalDPS 171.76 where PoB2 shows none (TotalDPS 0).
- **Fix:** Gate hit DPS to 0 for DoT-only skills: set a 'deals no hit damage' flag for EssenceDrainPlayer (statMap/overlay) or route spell_*_base_chaos_damage to ChaosDot only, so offence yields base_dps=0 and CombinedDPS=TotalDotDPS. Add a fixture asserting ED CombinedDPS==TotalDotDPS and that a hit+DoT skill keeps its hit.
- **Expected gain:** essence-drain CombinedDPS 1.51x -> 1.00x — largest single-build error in the set; TotalDotDPS already exact so this is a clean subtractive gate with no compensating-error exposure.
- **Regression risk:** Low-medium. Must stay specific to no-BASE-hit DoT skills; a blanket 'DamageOverTime -> no hit' gate would zero legitimate hybrid hits (comet hit + ailment DoT, bleed builds). dot/CombinedDPS parity columns catch over-zealous gating.

### #4 — Skill-applied (compounding) Elemental Exposure never injected — only flat config-20 path runs (frost-bomb)  `[high/small]`
- **Builds:** monk-invoker-frost-bomb
- **Mechanism:** Frost Bomb self-applies all-elemental exposure (act_int.lua:8925, magnitude 20, compounding +2 to cap 50, +Potent Exposure). Vendor maps active_skill_all_elemental_exposure_magnitude -> enemy {Fire,Cold,Lightning}Exposure BASE (SkillStatMap.lua:1732) and exposure takes the max source. PoBR's overlay skill_stat_map.json has NO entry for that stat (verified absent), so the skill exposure is never injected; only the hard-coded EXPOSURE_MAGNITUDE=20 config-condition path (calc_orchestrator/mod.rs:77/1156) runs. vs the +50% elem-res Pinnacle boss this swings cold-taken from ~0.83 to ~0.65.
- **Fix:** Add overlay mappings to skill_stat_map.json for active_skill_all_elemental_exposure_magnitude and _compounding_magnitude mirroring vendor SkillStatMap.lua:1732 ({Fire,Cold,Lightning}Exposure BASE) so the skill's own exposure becomes an enemy-side BASE mod that max-of in reduce_enemy_exposure can pick; model the compounding (+2/pulse to cap, +Q20 cap bonus) not a flat 20; keep EXPOSURE_MAGNITUDE=20 only as the conditionEnemy*Exposure config default; confirm Potent Exposure's exposure_effect_+% reaches player_db as {Elem}ExposureEffect INC.
- **Expected gain:** frost-bomb ~+1.28x of its gap (TotalDPS 0.52x -> ~0.67x); full parity still needs Fix #9 (residual ~1.5x).
- **Regression risk:** Low-moderate. Only self-exposure skills gain enemy-res reduction; clamp compounding to the documented cap or risk over-credit. Assert varashta-comet/druid-comet stay 1.00x (they emit no exposure stat -> no spillover).

### #5 — Heft MaxPhysicalDamage (max-only range) MORE not modeled (Shield Wall Smith)  `[high/medium]`
- **Builds:** warrior-smith-of-kitava-shield-wall
- **Mechanism:** Heft grants support_heft_maximum_physical_damage_+%_final 30 -> MaxPhysicalDamage MORE Hit (sup_str.lua:4205), scaling ONLY the max of the physical hit range. PoBR's damage-range calc has no max-only MORE handling, so +30%-on-max (~+15% avg) is dropped. Combined with the dropped Close Combat (Fix #2): 1.30 x 1.15 = 1.495 -> 0.67 ~= observed 0.69x.
- **Fix:** Model MaxPhysicalDamage MORE so it scales only the max of the physical hit range (not the average) in damage.rs/crit_pass.rs.
- **Expected gain:** With Fix #2, Shield Wall Smith 0.69x -> ~1.00x.
- **Regression risk:** Medium. Must hit only the range max or it over-credits the average. warrior-titan-shield-wall (same skill, no Heft/Close Combat) is a built-in control that must stay ~0.95x.

### #6 — Radius-jewel 'Passive Skills in Radius also grant <mod>' per-notable grant not injected (gemling crit)  `[medium/large]`
- **Builds:** mercenary-gemling-legionnaire-explosive-grenade
- **Mechanism:** gemling's radius jewel grants '+7% increased Crit Chance for Attacks' onto each allocated notable in radius. PoBR's mod_parser has no '(Notable|Small|Medium) Passive Skills in Radius also grant <inner mod>' rule, and radius_jewel.rs computes only geometry (which nodes fall in radius) — it never injects the granted sub-mod onto those notables. So the per-notable crit INC is dropped from the CriticalStrikeChance sum -> CritChance 8.55 vs 10.30 (0.83x).
- **Fix:** (1) mod_parser: add a rule family for '(Notable|Small|Medium) Passive Skills in Radius also grant <inner mod>' parsing the inner mod tagged as a radius grant. (2) radius_jewel.rs (~131-193): for each allocated in-radius node, inject the parsed inner mod honoring the tier filter and the 'for Attacks' condition; crit.rs:140-157 then aggregates unchanged. Verify decomposition base(5+3.8)*inc(17%) vs tools/pob2-oracle.
- **Expected gain:** gemling CritChance 0.83x -> ~1.00x, effective TotalDPS 0.96x -> ~1.00x.
- **Regression risk:** Medium. Adds crit/damage to ANY 'in Radius also grant' jewel build; gate strictly to allocated in-radius nodes of the correct tier and honor the inner condition or an 'for Attacks' grant could leak onto spell skills. parity_no_regression aggregate hit-rate gate catches over-application. deadeye-explosive-grenade (no crit sources) is a clean control proving the rest of the hit pipeline is correct.

### #7 — Ailment active-stacks estimate omits skillData.dpsMultiplier (gemling ignite uptime)  `[low/small]`
- **Builds:** mercenary-gemling-legionnaire-explosive-grenade
- **Mechanism:** Vendor CalcOffence.lua:5067 ailmentStacks = HitChance/100 * ailmentChance * skillData.dpsMultiplier, then min(_, maxStacks=1). PoBR's estimate_active_stacks (ailment.rs:1133) has NO dps_multiplier factor; for a multi-grenade skill the product lands below 1 so magnitude is scaled by the sub-1 value -> IgniteDPS 408 vs 614 (0.66x).
- **Fix:** Thread the active skill's dps_multiplier (already in offence.rs:351 / scaled_damage.rs:163) through resolve_stack_config (perform.rs:985,1318) into estimate_active_stacks and multiply it into the stacks product (ailment.rs:~1135). Confirm vs tools/pob2-oracle that dpsMultiplier — not effMult/penetration/ailmentRollAverage — is the gemling-vs-deadeye differentiator before landing.
- **Expected gain:** gemling IgniteDPS 0.66x -> ~1.00x, but ignite is ~1% of CombinedDPS so net parity move is tiny.
- **Regression risk:** Low. min(_, maxStacks) bounds the increase so it cannot inflate already-capped builds; only sub-cap DoT builds with dpsMultiplier>1 change.

### #8 — monk-twister missing ~0.28 crit multiplier (eff 0.95x)  `[low/small]`
- **Builds:** monk-martial-artist-twister
- **Mechanism:** PoBR CritMultiplier 4.88 vs 5.16 for monk-twister only; huntress-twister with the same Twister skill is 1.00x, so it is a per-source crit-mult mod, not the skill base. Candidate: a Dance with Death / Execute II / item crit-mult mod PoBR drops as unsupported or mis-scopes. It is the second factor (with Fix #2) that reproduces monk-twister AvgDamage 0.80x exactly (x1.18 CC x x1.055 crit).
- **Fix:** needs investigation — dump the CritMultiplier source list via tools/pob2-oracle and PoBR's AttributionReport, diff to find the missing ~0.28, then wire/repair that specific source. Likely small and localized once identified.
- **Expected gain:** monk-twister AvgDamage -> ~1.00x together with Fix #2.
- **Regression risk:** Low and localized — affects only builds carrying that mod; verify huntress-twister stays 1.00x to confirm correct scoping.

### #9 — Frost-bomb residual hit-damage shortfall ~1.5x beyond exposure (cold/spell MORE dropped)  `[medium/large]`
- **Builds:** monk-invoker-frost-bomb
- **Mechanism:** After Fix #4's exposure piece, frost-bomb's total damage multiplier pool is 13.3x vs PoB 25.6x; base cold read correctly (same pipeline that passes varashta-comet 0.99x), so the gap is a missing cold/spell MORE or an effective-only enemy debuff — most plausibly a Monk-Invoker ascendancy/tree 'more' (possibly conditional on chill/frozen/charges). Not isolatable from static analysis.
- **Fix:** needs oracle — run tools/pob2-oracle on the build and diff the cold-hit Damage MORE/INC list + enemy DamageTaken vs PoBR's TraceGraph; the missing entry names the source (likely an Invoker ascendancy notable or a 'more vs chilled/frozen' line). Then add the missing mod-parser rule / ascendancy ingest.
- **Expected gain:** frost-bomb remaining ~1.5x closed -> full parity together with Fix #4.
- **Regression risk:** Unknown until source identified; a keyword/condition-scoped Invoker-specific add is low-risk for other classes.

### #10 — Ember-fusillade per-hit fire-damage multiplier dropped (~0.78x hit+ignite, NOT exposure)  `[medium/large]`
- **Builds:** druid-oracle-ember-fusillade
- **Mechanism:** hit (TotalDPS 0.78x) and ignite (0.77x) scale by the SAME ratio while crit/mult/speed all match -> a purely per-hit fire multiplier propagating into ignite. Build sets no exposure so Fix #4 does not apply. Tempting leads (Zenith II FullMana, Rakiata's Flow on a spell, per-ember final, Swift Affliction) all eliminated; remaining candidate a conditional fire/spell 'more' or a Druid-Oracle tree fire-pen/res-reduction.
- **Fix:** needs oracle — run tools/pob2-oracle on the build and diff the fire-hit Damage MORE/INC list + enemy FireResist vs PoBR's trace; 1.28x is consistent with either a ~28% conditional 'more' or ~12% missing fire-pen/res-reduction. Add the matching mod-parser rule once the source line is identified.
- **Expected gain:** ember TotalDPS 0.78x + ignite 0.77x -> ~1.00x.
- **Regression risk:** Low if the added rule is keyword/condition-scoped (fire/spell). ninja_parity gates ember-fusillade.

### #11 — Coiling Bolts genuine AverageDamage ~0.82x (the masked half of the Onslaught false-hit)  `[low/medium]`
- **Builds:** witch-blood-mage-coiling-bolts
- **Mechanism:** TotalDPS 0.89x with Speed 1.08x over implies AvgDamage*hitChance ~0.82x under; once Fix #1 removes the phantom +20% speed, the real ~0.82x surfaces. Likely enemy curse 'increased damage taken' (Despair/Vulnerability on a heavy-curse Blood Mage) not applied to the effective-damage pass, and/or projectile chain / phys-to-chaos handling. Considered Casting's +35% is already mapped (not the gap).
- **Fix:** needs investigation — after Fix #1 lands, re-run coiling in isolation to expose the true ~0.82x and bisect curse-on-enemy damage-taken (buff_pass.rs + offence effective enemy_damage_multiplier) vs chain vs conversion (damage.rs). Track as a separate ticket so the two errors stop masking each other.
- **Expected gain:** coiling TotalDPS -> ~1.00x (paired with Fix #1; without it, the headline 0.89x is two errors cancelling).
- **Regression risk:** Unknown until source identified.

### #12 — Comet meta-trigger Speed jitter + stormweaver crit/ignite drift (mostly within tolerance)  `[low/medium]`
- **Builds:** druid-oracle-comet, sorceress-stormweaver-comet, sorceress-disciple-of-varashta-comet
- **Mechanism:** All three comet builds are meta-triggered (Spellslinger / Cast-on-Crit), so 'Speed' is the trigger rate; the +2%/-3% scatter around 1.00x points to trigger-interval rounding (server-tick cap), not a damage bug. stormweaver also shows CritChance 0.97x (it is the only comet build not forced to 100% effective) and ignite 0.86x (extra magnitude shortfall, possibly the same config-exposure theme).
- **Fix:** Lower priority (within/near the 5% gate). Audit trigger-rate quantization in trigger.rs vs vendor CalcPerform trigger-time; trace stormweaver crit aggregation in crit.rs for the ~3% source; re-check after Fix #4 (exposure path may lift stormweaver hit/ignite). varashta-comet (0.99x) is the clean no-regression anchor.
- **Expected gain:** Small — most components already within tolerance; mainly stormweaver CombinedDPS 0.93x -> ~1.00x.
- **Regression risk:** Medium. Speed-quantization changes ripple to EVERY triggered/cast build (deadeye, flicker, twister); gate by full ninja_parity. Keep crit changes scoped to aggregation order, not base values.

### #13 — Wolf Pack minion skill unmodeled — phantom player Speed, no MinionStat DPS  `[low/large]`
- **Builds:** mercenary-tactician-wolf-pack
- **Mechanism:** Wolf Pack is a minion-summon skill. PoB2 evaluates it in a separate minion env (MinionStat: AvgDamage 1238, Speed 1.475, TotalDPS 1826) and leaves the PLAYER panel at defaults (Speed 1.00, player DPS 0). PoBR has no minion DPS pipeline, evaluates it as a player attack -> phantom player Speed 1.43 (weapon/cast-rate artifact) and 0 damage; the real 1826 minion DPS is unmodeled.
- **Fix:** needs feature work — classify minion-summon skills as minion skills, suppress player-panel Speed/Damage for a minion main skill (default Speed to the PoB2 placeholder so the panel matches), and build a minion calc env emitting MinionStat DPS. No localized fix.
- **Expected gain:** None for the current parity gate (cosmetic player-Speed only) until a full minion pipeline exists — deprioritize for offensive parity; note this build's REAL problem is defensive (Armour 0.55x / EHP 0.28x), out of offensive scope.
- **Regression risk:** Zero for the current gate (player DPS=0 in both engines today). Keep additive and gate behind minion detection so non-minion builds are untouched.

## Top pick
Fix #1 — the Onslaught 'during effect' flask/charm flag. It is the single highest-leverage localized correction: one emission site (item.rs:311) plus finishing an already-stubbed handler, vendor-confirmed to the decimal (CalcPerform.lua:618-648; detonate-dead 1.25*(1+110%)=2.625 vs PoBR 1.25*(1+130%)=2.875, the +20% is exactly the gap), and it corrects Speed across 3 builds spanning two clusters (it also subsumes the cluster-1 flicker Speed 1.15x, which is +20pp INC on flicker's ~33% INC base, not the hypothesized extra MORE). Detonate-dead becomes a clean win immediately (1.08x -> 1.00x), and ninja_parity's per-component Speed columns (both directions) fully bound any over/under. Caveat to land it correctly: gate on each build's flask-active config rather than blanket-stripping by text (or you wrongly remove legitimately-active onslaught from the other 4 carriers), and expect coiling's headline TotalDPS to temporarily worsen (0.89x -> ~0.82x) as it UNMASKS the real damage shortfall (Fix #11) — that is the audit surfacing a hidden compensating error, exactly the intended outcome. If a zero-coordination, zero-side-effect first step is preferred instead, Fix #3 (Essence Drain DoT-only hit gating) is the cleanest fully-isolated win: largest single error (1.51x -> 1.00x), high confidence, TotalDotDPS already exact, no unmask.

## Deferred items (legacy / drift / spine)

- **legacy.rs retirement:** CURRENT STATE: Steps 2/3 + 3a already landed. `ParseCtx` is migrated out of legacy.rs into `crates/pobr-core/src/mod_parser/dispatch.rs` (read in full); engine modules are unconditionally compiled (`crates/pobr-core/src/mod_parser/mod.rs:26-42`, no feature gate). Engine is already the SOLE production parser: orchestrator always injects rules (`crates/pobr-build/src/build_data.rs:432-444` compiles them whenever the data pack ships `mod_parser_rules.json`; both `data/4.5.0.3.4/` and `data/4.5.2.1.3/` ship it), `session.parse_ctx()` prefers engine (`crates/pobr-core/src/calc/session.rs:135-143`). Only 3b (physical delete) remains. Authoritative blocker list: `audits/rearchitecture-2026-06-10/blueprints/m6-delete-legacy-3b-handoff.md`.

WHO USES THE NO-ENGINE (legacy) ParseCtx:
- `ParseCtx::none()` is the default of every non-`_with_ctx` ingest wrapper: `ingest_item` (item.rs:91), `ingest_flask_charm` (item.rs:219), `ingest_gem` (skill_source.rs:502), `ingest_active_gem` (skill_source.rs:545), skill_source.rs:638, `ingest_passive_nodes` (passive.rs:51). Used mostly by tests + bare `CalculationSession` (session.rs:135-143 returns none() only when neither parser_rules nor special_rules injected).
- Direct legacy `parse_mod()` PRODUCTION callers: `mod_cache.rs:33` (parse_or_insert — handoff says no prod caller, tests only), `calc_orchestrator/collect.rs:101` (granted_passive_defs — actually PRODUCES GrantedPassive mods, not just gating), collect.rs:160/169 (combine_wrapped_then_filter is_ok gating), collect.rs:513 (filter_parseable is_ok gating), corpus.rs:37.
- `parse_mod_with_rules()` prod callers: `apps/pobr-cli/src/lib.rs:216`, corpus.rs:54, `tools/precompile-mods/src/parsed.rs:109`.
- `parse_minion_modifier()` (legacy-ONLY, no engine equivalent fn): `calc_orchestrator/skill_resolve.rs:53` — but this is already a fallback; engine path is preferred at skill_resolve.rs:46-52 via `parse_mod_engine` + `extract_minion_modifier_entries` (engine.rs supports `add_to_minion`→MinionModifier LIST, minion.rs:210).

EXACT BLOCKER (file:line): The engine is proven byte-equal to legacy ONLY on the C1 18-build corpus + fixtures — `c1_diff_zero_gate` (crates/pobr-core/tests/parser/parser_dual_run.rs:392, pinned to GOLDEN_PARITY_DATA_VERSION). A rollback probe (handoff:17) swapped `tests/parser/mod_parser.rs`'s `parse_mod` to `parse_mod_engine` and hit 13/84 FAILURES — genuine engine-vs-legacy divergences NOT in the C1 corpus. These 13 must be adjudicated against vendor `ModParser.lua` before delete, because deleting legacy forces all currently-legacy test+ingest paths onto the engine. Most critical (handoff:30-41): (1) PerStat/Multiplier tag DROPPED — `+5 to maximum Mana per 10 Intelligence` etc.: engine emits the stat but does not attach `Multiplier{var,div}` tag → per-attribute scaling silently fails. Root cause located: (a) var name not vendor→PoBR aliased (`Int` vs `Intelligence`), (b) `per N <stat>` numeric form missing so div falls to 1 (this one can affect real builds); (2) weapon-class keyword-flag/condition bit encoding mismatches; (3) aggregate resistance expansion (`all Resistances` incl. chaos): engine 2 vs legacy 4 mods; (4) gain-as-per-grenade, bonded-enabler→condition, bracket-markup stripping; plus 2 design-semantic items (engine never returns Err / parses some immunity phrases legacy marks Unsupported — likely test updates, not engine fixes).

RETIREMENT PATH (handoff §81-90, 7 steps): (1) adjudicate the 13 divergences vs vendor ModParser.lua — fix engine/data for capability gaps, update assertions for design-semantic ones; re-run c1_diff_zero_gate green. (2) delete legacy.rs (4064 lines) + mod.rs re-exports (parse_mod/parse_mod_with_rules/parse_minion_modifier). (3) collapse `dispatch::ParseCtx` to engine-only (drop rules/registry/none/with_rules; decide no-rules semantics — recommend requiring rules always injected). (4) migrate prod direct-legacy callers (CLI, collect.rs granted_passive_defs+gating, corpus.rs, precompile-mods, mod_cache) to engine_ctx; delete the minion legacy fallback (skill_resolve.rs:53). (5) migrate ~15 test files to a shared dev-only rules loader, update assertions per adjudication. (6) delete parser_dual_run.rs (loses meaning) + fix mod_parser_bench legacy arm. (7) full nextest+clippy+fmt; parity_no_regression must not regress; any A-class engine/data change that moves parity → separate commit + owner review, no baseline bump. ENV NOTE: handoff said 3b needs local vendor+luajit; vendor IS now checked out (a82a33b) so that constraint is partly lifted — BUT re-extracting rules from a82a33b is entangled with the drift-gate/parity re-baseline (see drift_gate). Doing the 13-divergence fixes against the 2df5a74-pinned committed rules avoids that entanglement and is the cleaner first move.

- **parser-rules drift gate:** ROOT MISMATCH: committed `data/4.5.0.3.4/overlay/mod_parser_rules.json` `_meta.vendor_commit` = 2df5a7433... (2df5a74), but `vendor/.pob2-version.txt` = a82a33b4fbd... The ignored test (tools/sync-pob-catalog/tests/extract/extract_parser_rules.rs:42 `regenerated_matches_committed_artifact`) re-extracts from the live vendor and asserts byte-equality with the committed artifact → now drifts.

WHAT RESTORING REQUIRES — 2 coupled parts:
1. RE-EXTRACT (mechanical): `cargo run -p sync-pob-catalog -- extract-lua --what parser-rules --vendor-root vendor/PathOfBuilding-PoE2/src --out data/4.5.0.3.4/overlay/mod_parser_rules.json` (= the `_meta.regen_command`; also `soft_step mod_parser_rules` at pipeline/regen-all.sh:107). Needs luajit + complete vendor (the test already gates on both). a82a33b adds real new content the ignore note (extract_parser_rules.rs:30-39) calls out: IMMUNE form, `maimed` flag, `global evasion rating and energy shield` name_map.
2. PARITY GOLDEN RE-BASELINE (the real blocker, must happen TOGETHER): the a82a33b `global evasion rating and energy shield` name_map fixes PoB's old underestimate of `20% more Global Evasion Rating and Energy Shield`. So re-extracting changes REAL outputs that the parity goldens pin — but those goldens (`meta.json::player_stats`) were exported from OLD PoB2 (2df5a74-era, with the bug). Concretely affected builds/tests (file:line):
   - `crates/pobr-build/tests/parity/golden_canary.rs:135 canary_evasion_melee` — build `monk-martial-artist-twister`, asserts Evasion within DEF tolerance → evasion rises ~+20% → breaks.
   - `crates/pobr-build/tests/parity/defence_panels_golden.rs:101 deflection_matches_golden` — DeflectionRating = N% of Evasion, so the evasion change propagates: `monk-martial-artist-twister` (rating 11229.76) AND `huntress-spirit-walker-twister` (5666.7) both shift → break.
   - `crates/pobr-build/tests/parity/ninja_parity.rs parity_no_regression` — defence hit-rate counts (BASELINE_DEF_CORE_HIT5/HIT5/HIT10 at :514-516) include the monk evasion row; aggregate count can move.
   Also the dual-run gate pins GOLDEN_PARITY_DATA_VERSION (parser_dual_run.rs:78-91); since legacy is frozen handwritten and engine reads the NEW rules, the new name_map entries can create fresh DIFFs → `c1_diff_zero_gate` must be re-verified after re-extract.

STEPS: (1) confirm luajit + complete vendor (vendor already a82a33b). (2) re-extract → new mod_parser_rules.json (vendor_commit→a82a33b). (3) re-run tools/pob2-oracle to re-export meta.json player_stats goldens for affected builds (monk-martial-artist-twister, huntress-spirit-walker-twister, plus any carrying maimed/IMMUNE mods), oracle config aligned. (4) update canary_evasion_melee + deflection_matches_golden + ninja_parity BASELINE_DEF_* to new values in a SEPARATE baseline commit with owner dual-metric review (no silent bump — per ignore note + m6-switch-decision owner gate). (5) re-verify c1_diff_zero_gate green (or register new name-only DIFFs). (6) remove `#[ignore]` at extract_parser_rules.rs:41.

RISK: not a mechanical bump. monk-martial-artist-twister evasion +~20% and its deflection rating; huntress deflection; and the IMMUNE-form/maimed-flag additions could move OTHER builds' ailment/defence outputs beyond the evasion case — so a full ninja_parity re-measure (not just the evasion row) is needed. Owner dual-metric adjudication (hit-rate must not net-regress) is required, exactly why it was deferred.

- **994-line spine split:** VERDICT: There IS a safe seam structure. `calculate_with_data` (crates/pobr-build/src/calc_orchestrator/mod.rs:303-1297, ~994 lines) is a LINEAR injection pipeline, not a tangled data-flow graph. It decomposes into 5 phases with firm boundaries:

A. IMMUTABLE CONTEXT (308-584, ~280 lines): stat_map RAII guard; ring3 gating + gem-quality (rebinds `build` 3×, owns guards `ring3_gated`/`quality_adjusted`); resolve main_skill/main_effect/main_skill_types/skill_flags/skill_type_bits/dmg_keywords; resolve_config→base_cfg→cfg (the long `.with_*` chain + conditional conditions/flags, 404-528); base_input action rate; weapon/off_weapon/hand_weapon/off_hand_weapon/dmg_mult/bypasses_cooldown. Produces ~14 bindings consumed downstream.
B. SESSION CONSTRUCTION + STATIC INJECTION (586-648): `CalculationSession::new(base_input).with_config(cfg)` + set_constants/special_rules/parser_rules/buff_defs/curse/high_precision/hand_sources + cooldown-bypass flag.
C. SOURCE INJECTION PHASES (650-1112, ~460 lines): each phase = read-only context + `session.add_modifiers(...)`. character base; skill base/quality/support/triggers; damage-mult; weapon crit; defence base/shield/spirit/ward; equipment loop; jewels/flasks/radius/quest/config/custom; passive tree + small-passive scaling + notable copies + keystone; gems; buff/support-buff/herald/self-buff/exposure/spirit-reservation.
D. ENEMY SETUP (1114-1158).
E. POST-INJECTION BACKFILL (1160-1255): extra texts; attribute derivation (reads session.attribute_total AFTER all sources, 1183-1185); per-X multiplier backfill (reads session.base_sum/pool_total AFTER all sources, 1217-1238); condition bridges (session.has_flag, 1244-1254).
F. DIAGNOSTICS + minions + perform + output (1256-1296).

WHAT CAN BE EXTRACTED SAFELY:
- A → `fn build_calc_context(build, data, options) -> Result<CalcCtx>` returning a struct of the ~14 cross-phase bindings (cfg, base_input, main_skill, main_effect, main_skill_types, skill_flags, resolved_config, enemy_tier, weapon, off_weapon, hand/off_hand_weapon, dmg_mult, bypasses_cooldown). The cfg-building block (404-528) is the single biggest self-contained chunk.
- C → ~6-8 `fn inject_*(session: &mut CalculationSession, ctx: &CalcCtx, build, data, options)` phase fns. They are independent (share only read-only ctx + mut session) and mostly already delegate to submodule helpers (skill_base_modifiers, defence_base_modifiers, etc.) — the spine just sequences `session.add_modifiers(helper(...))`.
- E → `fn backfill_derived(session, build, data, options, ctx)` — firm natural boundary (must run after all of C+D because it reads aggregated session state). Clean.
- D, F → trivially extractable.

WHAT NEEDS CARE (not blockers, but the price):
1. The `Build` rebind ownership: A rebinds `build` to locals `ring3_gated`/`quality_adjusted` that own data `&build` borrows. Extracting A means the context builder must RETURN the owned Build (or Cow<Build>) so the borrow outlives the ctx — the single real friction point.
2. The RAII `_stat_map_guard` (311) must stay in the thin outer spine (its Drop resets a thread-local at fn exit) — cannot move into an extracted phase.
3. ORDERING is load-bearing: comments throughout assert "须在 add_item/add_passive_nodes/add_gem 之前/之后". Extraction must preserve exact call order. The backfill phase reading base_sum/pool_total enforces a hard "after all injection" firewall — that boundary is natural, not an obstacle.
4. One cross-phase local inside C: `passive_nodes` (959-964) feeds small-passive scaling (978-1024), notable copies (1021), and keystone_mod_map (1033) — keep those three together in one `inject_passives` phase. `bonus_scales` (769) and the equipment loop are fully self-contained. `off_weapon` crosses A→equipment-loop (used at 780) → carried in CalcCtx.

NET: introduce one `CalcCtx` struct + handle the Build-rebind ownership (Cow or owned-return); spine shrinks 994→~100 lines of named-phase orchestration. Behavior-preserving (pure call-order-preserving move), so it's gated by exact-ordering discipline + the existing parity/ring3 tests in the same file (mod.rs:1307-1431), not by genuine data-flow coupling.

- **Recommended order:** Recommended order: (1) Task 3 spine split FIRST — it is the only one that is purely local, behavior-preserving, needs no vendor/luajit/oracle, and is fully covered by existing in-file tests; doing it first makes the orchestrator legible for the later parser work. (2) Task 1 legacy retirement against the CURRENT 2df5a74-pinned committed rules — adjudicate the 13 probe divergences vs vendor ModParser.lua (vendor is now locally checked out), keeping it decoupled from the a82a33b re-extract. (3) Task 2 drift-gate restoration LAST — it is the heaviest: it requires re-extracting from a82a33b AND a coordinated parity-golden re-export (oracle) + owner dual-metric baseline re-base, touching evasion/deflection goldens for monk-martial-artist-twister + huntress-spirit-walker-twister and the ninja_parity defence counts. Note the entanglement: if Task 2's a82a33b re-extract is done first it would change the rules under Task 1's feet (new name_map entries → new dual-run DIFFs), so keep Task 1 on the frozen 2df5a74 rules and treat Task 2 as a separate vendor-bump workstream.

## Summary
Deduped 14 raw root causes across 4 clusters into 13 distinct fixes via two cross-cluster merges: (a) DistanceRamp tag drop = one stat_map_engine fix covering monk-twister, flicker, and Shield-Wall-Smith (cluster-1 + cluster-4); (b) Onslaught flask-flag = one item.rs fix covering coiling, detonate-dead, flicker (cluster-4 + the cluster-1 flicker-Speed symptom, which it explains better than the original 'extra MORE-speed' guess). Two dominant failure modes emerge: (1) MORE-damage mods silently dropped because a tag arm is missing in translate_tag (DistanceRamp / MaxPhysicalDamage), and (2) phantom additions inflating outputs (unconditional Onslaught flag, a fabricated hit on a DoT-only skill, plus a MISSING enemy-exposure injection that under-credits). Four high-confidence, vendor-anchored, localized fixes (#1 Onslaught, #2 DistanceRamp, #3 Essence Drain, #4 frost-bomb exposure) cover the bulk of the offensive gap and are all bounded by the ninja_parity per-component gate. The honest cautions: flicker-strike is a coordination case (needs #1 + #2 together or its three compensating errors flip the apparent parity), and four entries (#9-#11, plus #8) are genuinely not isolatable from static analysis and are marked low/medium confidence pending a tools/pob2-oracle dump — none of these are 'tune a number to hit the rate'; each names a concrete missing vendor mechanism. #12 (comet trigger jitter) is within tolerance and #13 (Wolf Pack minion) has zero current parity impact, so both are deprioritized.
---

## Fix #3 (Essence Drain) — deep-dive findings (2026-06-28)

Investigated for landing; de-risked but needs `tools/pob2-oracle` to implement safely.

**Confirmed via `POBR_DBG_ALLMODS` mod dump on `sorceress-chronomancer-essence-drain`:**
- Phantom hit source = `ChaosDamageMin/Max` = 62 / 115 (from statSet `spell_*_base_chaos_damage` → stat_map `SkillData{ChaosMin/ChaosMax}`) → hit DPS 171.76.
- Correct DoT source = `ChaosDot` = 179.56 (from `base_chaos_damage_to_deal_per_minute`) → TotalDot 335 (golden-exact, ratio 1.0000).
- **The hit and DoT come from SEPARATE stats** → a clean hit-only suppression is possible.

**The trap:** PoBR's existing `DealNoChaos` / `DealNoDamage` flag gates BOTH the hit (offence canDeal) AND the DoT (`skill_dot.rs:248`). So it CANNOT be used as-is — it would zero the golden-exact 335 DoT.

**Required to land correctly (next focused session):**
1. Run `tools/pob2-oracle` on this build to confirm PoB2's exact mechanism for `TotalDPS=0` (whether the `essence_drain` statDescriptionScope reinterprets `spell_*_base_chaos` as DoT-only, or a baseMod/flag disables the hit).
2. Implement a **hit-only** suppression that matches vendor (e.g. a new `DealNoHitDamage` flag checked in offence's hit pass but NOT in `skill_dot`, applied to essence drain), and verify it does NOT break hybrid hit+DoT skills (comet).
3. Per-component parity verify: essence-drain CombinedDPS 1.51x → 1.00x (clean +1 hit), no other build changes.

## Landing-order constraint discovered (2026-06-28)

**Fix #2 (DistanceRamp) must NOT be landed alone** — it would regress the parity gate:
- flicker-strike currently passes TotalDPS at 1.04x as a *false hit* (Speed +15% × AvgDamage −10% cancel).
- Adding Close Combat's +18% MORE damage pushes flicker AvgDamage 0.90x→1.06x → TotalDPS ~1.23x → **false-hit flips to a real miss (off −1)**.
- monk-twister 0.80x→0.85x and shield-wall 0.69x→0.79x both remain misses (need Fix #8 crit-mult / #5 Heft respectively).
- Net: **off hit count −1 → `parity_no_regression` gate FAILS**. Fix #2 must be bundled with #1 (flicker Speed) + #8 (twister crit-mult) + #5 (shield-wall Heft).

**Implication:** the only standalone, metric-improving offensive fix is **#3 (Essence Drain)**. The Close-Combat cluster (#1/#2/#5/#8) must land as a coordinated bundle.
