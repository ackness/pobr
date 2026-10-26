# Flame Wall projectile buff evidence

Only ninja-top10-2026-10-26 code.txt builds with archived quality output; no holdout inspected. Config-true candidates are evidence inventory, not after-run classification.

Game snapshot: 0.5.5.4.2/base/granted_effect_stat_sets.json / FlameWallPlayer / FlameWallProjectileBuffPlayer
Game tooltip: https://poe2db.tw/us/Flame_Wall

The existing flameWallAddedDamage config declares that the projectile passed through a wall. Only the two native added-fire stats are admitted. No lingering ignite, infused lightning, cast history, rate, or uptime is inferred. Mastery/global level bonuses affect native values; wall damage/quality modifiers do not multiply the outgoing flat buff. The existing buff merge keeps the strongest matching modifier across copies.

Config-true candidates: 15; main skill has native Projectile type: 8. Full cohort after measurements remain the parent gate's work.

| Build | Main skill | Projectile type | Flame Wall level(s) |
|---|---|---|---|
| chronomancer-01 | CometPlayer | False | 35 |
| chronomancer-02 | CometPlayer | False | 35 |
| chronomancer-03 | CometPlayer | False | 32 |
| disciple-of-varashta-09 | FrostboltPlayer | True | 21 |
| gemling-legionnaire-08 | SparkPlayer | True | 19, 34 |
| gemling-legionnaire-10 | ArcPlayer | True | 35 |
| lich-03 | FlameWallPlayer | False | 9 |
| oracle-02 | SparkPlayer | True | 31 |
| oracle-08 | CometPlayer | False | 37 |
| pathfinder-08 | FlameWallPlayer | False | 14 |
| shaman-01 | SparkPlayer | True | 36 |
| shaman-08 | SparkPlayer | True | 15 |
| smith-of-kitava-01 | DetonateDeadPlayer | False | 19 |
| smith-of-kitava-03 | FrostboltPlayer | True | 25 |
| smith-of-kitava-10 | FrostboltPlayer | True | 26 |

gemling-legionnaire-08 cached native hit sources contain Lightning 103..1949 and no Fire; cached oracle lists Flame Wall levels 19/34 and fire base 71..107 (the native level-34 payload). Native Lightning source has an existing 408% gain-as-Lightning modifier, so the missing Fire also removes its downstream gain-as contribution. Exact cache records and raw groups are in the JSON.

Focused regression measurements (source-changed code, no CLI/oracle execution): L20 fire 27..41; L20 + Fire Mastery +1 + global Fire level +2 gives 34..51; duplicates L20/L34, L34/L20 and L34/L34 all give 71..107, including each selected stat set. Existing 50% gain-as-Lightning receives half the fire range. Missing/false config, absent/disabled source and alternate-only source give zero; non-projectile Ice Nova gets zero. Attribution retains SkillGem and winning effective level.

Deliberate scope limits: infused lightning remains unimplemented by this patch; no LingeringIgnite contribution is introduced. Ordinary recipient ignite potential still follows its existing damage/ailment path. No full-row discrepancy is declared explained before the parent after run.

Local commit: 1d890661 (parent e5b04af6 merged before commit). Validation: cargo test -q -p pobr-build --test skills flame_wall:: (3 passed); cargo clippy -q -p pobr-core -p pobr-build --lib --tests -- -D warnings; cargo fmt --check; git diff --check. All ran via mise on this worktree target. No own rule/parser edits, so no regeneration required. No new in-game measurement; strongest-copy handling uses the existing buff convention. Parent owns batch validation and documentation/classification.
