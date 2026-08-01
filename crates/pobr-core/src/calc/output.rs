use pobr_data::prelude::DamageType;

use super::{DamageComponent, MinimalOutput, SkillUseTime};

#[derive(Debug, Clone, PartialEq)]
pub struct OutputTable {
    pub life: f64,
    pub mana: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub chance_to_be_hit: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub max_fire_resistance: f64,
    pub max_cold_resistance: f64,
    pub max_lightning_resistance: f64,
    pub fire_resistance_over_cap: f64,
    pub cold_resistance_over_cap: f64,
    pub lightning_resistance_over_cap: f64,
    pub crit_chance: f64,
    /// Crit chance (fraction) before the hit-chance downgrade / lucky /
    /// bifurcate / inevitable, but after the cap. Used to show overflow in breakdowns.
    pub pre_effective_crit_chance: f64,
    pub crit_multiplier: f64,
    /// Non-crit hit components split by damage type; summing gives total non-crit hit damage.
    pub damage_components: Vec<DamageComponent>,
    pub total_hit_avg: f64,
    pub hit_chance: f64,
    pub action_rate: f64,
    pub dps: f64,

    // Additional mechanics fields (written by perform's fill stage; Default 0/None)
    /// Skill use time / action rate resolution result.
    pub skill_use_time: Option<SkillUseTime>,
    /// Effective action rate after applying the server tick cap (actions/s).
    pub effective_action_rate: f64,
    /// Ailment DPS.
    pub bleed_dps: f64,
    pub ignite_dps: f64,
    pub poison_dps: f64,
    /// Shock's damage bonus magnitude (fraction, e.g. 0.20).
    pub shock_effect: f64,
    /// Maximum single hit each damage type can take (starting semantics =
    /// PoB2's view: TotalHitPool's pool-expansion layer + taken-as,
    /// CalcDefence.lua:3540-3697; `*_max_hit_pob2` is an alias with the same value).
    pub physical_max_hit: f64,
    pub fire_max_hit: f64,
    pub cold_max_hit: f64,
    pub lightning_max_hit: f64,
    pub chaos_max_hit: f64,
    /// Total EHP (starting semantics = PoB2's view:
    /// `TotalNumberOfHits × totalEnemyDamageIn`, CalcDefence.lua:3322;
    /// neutral at 0 when there's no enemy incoming damage. The old
    /// lowest-max-hit view is kept in `total_ehp_lowest_max_hit`).
    pub total_ehp: f64,
    /// Life / mana reservation and remainder.
    pub life_reserved: f64,
    pub life_unreserved: f64,
    pub mana_reserved: f64,
    pub mana_unreserved: f64,
    /// Per-second recovery.
    pub life_regen: f64,
    pub mana_regen: f64,
    pub energy_shield_regen: f64,
    /// Defence chance family.
    pub block_chance: f64,
    pub spell_block_chance: f64,

    // Defence extensions (Lane2: ES recharge / avoidance / taken multipliers / crit mitigation; written by perform fill)
    /// ES recharge rate (fraction recovered per second; 0 under ZealotsOath or when es=0).
    pub es_recharge_rate: f64,
    /// ES recharge start delay (seconds; default 4.0).
    pub es_recharge_delay: f64,
    /// ES recharge's absolute recovery per second (rate_fraction × energy_shield).
    pub es_recharge_per_second: f64,
    /// Avoidance chance family: hits / projectiles / each ailment (percentage).
    pub avoid_all_damage_from_hits: f64,
    pub avoid_projectile_damage: f64,
    pub avoid_stun: f64,
    pub avoid_ignite: f64,
    pub avoid_shock: f64,
    pub avoid_chill: f64,
    pub avoid_freeze: f64,
    pub avoid_poison: f64,
    pub avoid_bleeding: f64,
    /// Damage-taken multiplier (the damage-taken view, fraction; 1.0 = no mitigation/increase).
    pub taken_multi_physical: f64,
    pub taken_multi_fire: f64,
    pub taken_multi_cold: f64,
    pub taken_multi_lightning: f64,
    pub taken_multi_chaos: f64,
    /// Reduction of extra crit damage taken (percentage, 0-100).
    pub crit_extra_damage_reduction: f64,
    /// Enemy crit effect multiplier (weighted average damage multiplier, ≥ 1.0).
    pub enemy_crit_effect: f64,

    // Minion snapshots (Lane4: each minion's own offence/defence output; written by perform's multi-Actor pass)
    /// Key output snapshot for each minion (empty when there are no minions).
    pub minions: Vec<MinionOutput>,

    // Triggers (Lane4: trigger rate cap / actual trigger rate; written by perform, 0 when not applicable)
    /// Trigger rate cap (per second).
    pub trigger_rate_cap: f64,
    /// Actual trigger rate (per second) = min(cap, effective source rate).
    pub skill_trigger_rate: f64,

    // Defence recovery extensions (Lane A: charges / leech / Recoup; written
    // by perform fill, neutral 0 with no source)
    /// Charge current/max stacks (Power / Frenzy / Endurance).
    pub charge_power_current: u32,
    pub charge_power_maximum: u32,
    pub charge_frenzy_current: u32,
    pub charge_frenzy_maximum: u32,
    pub charge_endurance_current: u32,
    pub charge_endurance_maximum: u32,
    /// Leech panel rate (per second; 0 = no leech).
    pub life_leech_rate: f64,
    pub mana_leech_rate: f64,
    pub es_leech_rate: f64,
    /// Recoup return rate (per second; depends on the damage_taken estimate baseline).
    pub life_recoup_rate: f64,
    pub es_recoup_rate: f64,

    // Ailment extensions (Lane B: chill / freeze·shock buildup / bleed·poison stacks; written by perform's fill_ailments)
    /// Chill's action speed reduction (%, e.g. 30.0 = 30%; 0 = not applied).
    pub chill_effect: f64,
    /// Freeze buildup (% per hit; 0 = not accumulating).
    pub freeze_buildup_pct: f64,
    /// Shock buildup (% per hit; 0 = not accumulating).
    pub electrocute_buildup_pct: f64,
    /// Bleed/poison/ignite multi-stack DPS and active stack count (0 = no
    /// stacking configured; default max_stacks=1, stacked==single stack).
    pub bleed_stacked_dps: f64,
    pub bleed_active_stacks: f64,
    pub poison_stacked_dps: f64,
    pub poison_active_stacks: f64,
    pub ignite_stacked_dps: f64,
    pub ignite_active_stacks: f64,
    /// Stack ceiling for each damaging ailment (`max_stacks`; 1 = cannot
    /// stack). Used for diagnostics/panel.
    pub bleed_max_stacks: f64,
    pub poison_max_stacks: f64,
    pub ignite_max_stacks: f64,

    // Skill functionality (Lane C: AoE / projectiles / cooldown / cost;
    // written by perform fill, 0 with no base)
    /// Final AoE radius (internal coordinate units) and the area multiplier.
    pub aoe_radius: f64,
    pub aoe_area_mod: f64,
    /// Projectile count (0 without a projectile mod).
    pub projectile_count: f64,
    /// Cooldown (seconds; 0 = no cooldown) and the number of storable uses.
    pub cooldown: f64,
    pub cooldown_stored_uses: u32,
    /// Resource cost (final_cost; 0 without a base configuration).
    pub mana_cost: f64,
    pub life_cost: f64,
    pub spirit_reserved: f64,

    // --- Defence extensions (W0.2 contract fields: default neutral 0; wired
    //     up incrementally by tracks A-F. golden reference =
    //     examples/demo-bd-test/builds/*/meta.json::player_stats's same-named keys) ---
    /// Base Spirit pool value (base × inc × more + Override, mirroring PoB2's doActorLifeManaSpirit).
    pub spirit: f64,
    /// Unreserved Spirit remainder (= spirit − spirit_reserved; vendor
    /// CalcDefence.lua:337 has no floor, so over-reservation goes negative
    /// -- golden's `SpiritUnreserved` has values like −130, so W0.2's
    /// original "floors at 0" comment didn't match vendor/golden and was corrected when D-3 wired this up).
    pub spirit_unreserved: f64,
    /// Block chance ceiling (%; PoB2 `BlockChanceMax`, CalcDefence.lua:961-966).
    pub block_chance_max: f64,
    /// Spell block chance ceiling (%; PoB2 `SpellBlockChanceMax`).
    pub spell_block_chance_max: f64,
    /// Effective block chance (%; after the lucky/unlucky power, PoB2
    /// `EffectiveBlockChance`, CalcDefence.lua:1030-1058).
    pub effective_block_chance: f64,
    /// Effective spell block chance (%; PoB2 `EffectiveSpellBlockChance`).
    pub effective_spell_block_chance: f64,
    /// Effective projectile attack block chance (%; PoB2 `EffectiveProjectileBlockChance`).
    /// The EHP average block chance is the mean of all four variants (vendor CalcDefence.lua:1067).
    pub effective_projectile_block_chance: f64,
    /// Effective spell projectile block chance (%; PoB2
    /// `EffectiveSpellProjectileBlockChance`, vendor :1013 takes the max with ProjectileBlock).
    pub effective_spell_projectile_block_chance: f64,
    /// Block damage-taken share (%; the fraction of damage still taken from
    /// a blocked hit, PoB2 `BlockEffect`, ModParser.lua:2479; 0 = fully blocked).
    pub block_effect: f64,
    /// Deflection rating (PoB2 `DeflectionRating` = BASE + Evasion/Armour
    /// GainAsDeflection, CalcDefence.lua:1487-1490).
    pub deflection_rating: f64,
    /// Deflection chance (%; PoB2 `DeflectChance` = deflectChance(rating,
    /// enemyAccuracy), CalcDefence.lua:48-54, :1491).
    pub deflect_chance: f64,
    /// Overall evade chance (%; PoB2 `EvadeChance`, CalcDefence.lua:1396-1466).
    pub evade_chance: f64,
    /// Melee evade chance (%; PoB2 `MeleeEvadeChance`, an independent inc factor per variant).
    pub melee_evade_chance: f64,
    /// Projectile evade chance (%; PoB2 `ProjectileEvadeChance`).
    pub projectile_evade_chance: f64,
    /// Spell evade chance (%; PoB2 `SpellEvadeChance`).
    pub spell_evade_chance: f64,
    /// Spell projectile evade chance (%; PoB2 `SpellProjectileEvadeChance`).
    pub spell_projectile_evade_chance: f64,
    /// Stun threshold (PoB2 `StunThreshold`, base Life/ES/Mana mod
    /// switching, CalcDefence.lua:2525-2643).
    pub stun_threshold: f64,
    /// Self-stun chance (%; PoB2 `SelfStunChance` = StunBaseMult × effective damage/threshold).
    pub self_stun_chance: f64,
    /// Stun duration (seconds; rounded up per ServerTickRate).
    pub stun_duration: f64,
    /// Ward pool (PoB2 `Ward`, per-slot aggregation + EnergyShieldToWard,
    /// CalcDefence.lua:1144-1273).
    pub ward: f64,
    /// Recoverable life pool (PoB2 `LifeRecoverable`, the life pool value used by the EHP loop).
    pub life_recoverable: f64,
    /// ES recovery cap pool (PoB2 `EnergyShieldRecoveryCap`, the ES pool value used by the EHP loop).
    pub energy_shield_recovery_cap: f64,
    /// Panel physical damage reduction (%; PoB2 `PhysicalDamageReduction`, the armour DR under a reference hit).
    pub physical_damage_reduction: f64,
    /// Number of hits needed to be lethal (PoB2 `NumberOfDamagingHits`, CalcDefence.lua:2979-3153).
    pub number_of_damaging_hits: f64,
    /// Number of lethal hits after accounting for the not-hit/block/deflect
    /// probability layer (PoB2 `NumberOfMitigatedDamagingHits`, CalcDefence.lua:3246-3247).
    pub number_of_mitigated_hits: f64,
    /// Old-view total EHP (the min of each type's max hit). F-3 switched
    /// `total_ehp`'s semantics to the PoB2 view (mitigatedHits ×
    /// totalEnemyDamageIn, :3322); the old value is kept here as a
    /// supplementary metric.
    pub total_ehp_lowest_max_hit: f64,

    // --- PoB2-view fields (produced by F-1's dual-run in parallel; after F-3
    //     switches over, these equal the canonical `total_ehp`/`*_max_hit`
    //     values, kept as aliases for dual-run reports/downstream compatibility). ---
    /// New-view total EHP (PoB2 `TotalEHP = TotalNumberOfHits × totalEnemyDamageIn`,
    /// CalcDefence.lua:3322; neutral at 0 when there's no enemy incoming damage).
    pub total_ehp_pob2: f64,
    /// Enemy single-hit total incoming damage (PoB2 `totalEnemyDamageIn`,
    /// the Σ placeholder before mult/crit, CalcDefence.lua:2136).
    pub total_enemy_damage_in: f64,
    /// New-view maximum hit taken per type (PoB2 `<X>MaximumHitTaken`,
    /// TotalHitPool's pool-expansion layer + taken-as, CalcDefence.lua:3540-3697).
    pub physical_max_hit_pob2: f64,
    pub fire_max_hit_pob2: f64,
    pub cold_max_hit_pob2: f64,
    pub lightning_max_hit_pob2: f64,
    pub chaos_max_hit_pob2: f64,

    // --- Curse panel fields (produced by buff_pass, copied back via
    //     Env::curse_pass_output; "not extending display_catalog, only
    //     OutputTable fields"; default 0/empty, neutral) ---
    /// Enemy curse limit (PoB2 `output.EnemyCurseLimit`, CalcPerform.lua:2830).
    /// Stays at 0 when buff_pass didn't run (mode_buffs off / no buff spec).
    pub enemy_curse_limit: f64,
    /// Curse slot occupancy list (order = vendor's `curseSlots` merge order:
    /// hex slots → mark slots → `ignoreCurseLimit` slots appended after, CalcPerform.lua:2878-2896).
    pub curse_slots: Vec<String>,

    // Skill DoT / combined-DPS family contract fields (item 6, naming
    // frozen). Neutral at 0 during the skeleton stage; written by perform's
    // fill section via `skill_dot::fill_skill_dot` once calc is wired up.
    /// Single-instance skill DoT DPS (PoB2 `TotalDotInstance`, CalcOffence.lua:5831-5929).
    pub skill_dot_instance: f64,
    /// Skill DoT DPS after accumulating stackable instances (PoB2 `TotalDot`, `:5931`, clamped to DotDpsCap).
    pub skill_total_dot: f64,
    /// Total DPS across all DoT sources (PoB2 `TotalDotDPS`, `:6093-6234`:
    /// skill dot + poison/caustic/ignite/burning/bleed/corrupting/decay, clamped to DotDpsCap).
    pub total_dot_dps: f64,
    /// Hit DPS + DoT (PoB2 `WithDotDPS`).
    pub with_dot_dps: f64,
    /// Combined DPS (PoB2 `CombinedDPS`).
    pub combined_dps: f64,
    // Per-hand sub-tables (decision D4: strongly-typed sub-tables, with the
    // flat PoB key `MainHand.X` resolved through display_catalog's pob_key).
    // The existing top-level fields' semantics = after combineStat (a
    // single-hand build passes through the OR mode unchanged).
    /// Main hand pass sub-result. None for non-attack skills; Some for
    /// attacks (a single-hand attack has off_hand=None).
    pub main_hand: Option<HandOutput>,
    /// Off hand pass sub-result (Some when dual wielding/shield charging).
    pub off_hand: Option<HandOutput>,
}

/// Mirrors vendor's `Stored<Type>{Hit,Crit}{Min,Max}` (CalcOffence.lua:4050-4056,
/// pre-resist, includes allMult; the crit leg additionally has
/// ×CritMultiplier) -- the min/max input surface for damaging ailment source
/// damage (consumed by `calcMinMaxUnmitigatedAilmentSourceDamage` `:4833-4857`,
/// with RollAverage interpolation `:5125-5126` operating on min/max).
///
/// One entry = one damage type's two-leg range (non-crit leg hit_min/max,
/// crit leg crit_min/max). min/max are not folded by lucky (vendor's lucky
/// only folds the `*Avg` family, `:4035-4046`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredDamageRange {
    /// Damage type (types still present after the canDeal gate).
    pub damage_type: DamageType,
    /// Non-crit leg hit lower bound (vendor `Stored<Type>HitMin`, `:4056`).
    pub hit_min: f64,
    /// Non-crit leg hit upper bound (vendor `Stored<Type>HitMax`, `:4057`).
    pub hit_max: f64,
    /// Crit leg hit lower bound (vendor `Stored<Type>CritMin`, `:4051`).
    pub crit_min: f64,
    /// Crit leg hit upper bound (vendor `Stored<Type>CritMax`, `:4052`).
    pub crit_max: f64,
}

/// A single hand's pass output (the field set = combineStat's input
/// surface, **frozen** -- per review C6c: crossbow fields
/// (FiringRate/ReloadTime family) and ailment extension fields are appended
/// in their own separate future commits, avoiding repeated pob_key changes
/// in display_catalog. `accuracy` isn't on the current MinimalOutput
/// surface; it'll be appended alongside its own commit once display needs it.
///  `stored_ranges` was appended per the C6c convention -- the min/max input surface for ailment magnitude).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HandOutput {
    /// This hand's hit chance (fraction; vendor `HitChance`, the AVERAGE input).
    pub hit_chance: f64,
    /// This hand's crit chance (fraction; vendor `CritChance`, the CRIT input).
    pub crit_chance: f64,
    /// Crit chance before the hit downgrade, after the cap (fraction; vendor `PreEffectiveCritChance`).
    pub pre_effective_crit_chance: f64,
    /// This hand's crit damage multiplier (vendor `CritMultiplier`, the AVERAGE input).
    pub crit_multiplier: f64,
    /// This hand's attack speed (vendor `Speed`, the HARMONICMEAN input).
    pub speed: f64,
    /// This hand's per-type hit components (before merging).
    pub damage_components: Vec<DamageComponent>,
    /// Average hit after CritBlend (vendor `AverageHit`).
    pub average_hit: f64,
    /// After × hit_chance (vendor `AverageDamage`, the DPS input).
    pub average_damage: f64,
    /// This hand's own DPS (before merging; vendor `TotalDPS`, the DPS input).
    pub total_dps: f64,
    /// The `Stored<Type>CritAvg` family (a resolved value; vendor
    /// `:4047-4057`, the ailment magnitude input -- pre-resist, includes
    /// allMult and the crit leg's ×CritMultiplier).
    pub stored_crit_avg: Vec<(DamageType, f64)>,
    /// The `Stored<Type>HitAvg` family (non-crit leg).
    pub stored_hit_avg: Vec<(DamageType, f64)>,
    /// The `Stored<Type>CombinedAvg` family (both legs accumulated weighted
    /// by crit chance; merged with the DPS mode at the outer level, :4588).
    pub stored_combined_avg: Vec<(DamageType, f64)>,
    /// The `Stored<Type>{Hit,Crit}{Min,Max}` family (appended; the damaging
    /// ailment source damage input, resolved by vendor `:4050-4056` / consumed at `:4833-4857`).
    pub stored_ranges: Vec<StoredDamageRange>,
}

/// A single minion's output snapshot (same structure as the subset of the player's key offence/defence output).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionOutput {
    /// Minion level (monster level).
    pub level: u32,
    /// Minion DPS (runs the same offence pipeline as the player).
    pub dps: f64,
    /// Life pool.
    pub life: f64,
    /// Armour.
    pub armour: f64,
    /// Evasion.
    pub evasion: f64,
    /// Energy shield.
    pub energy_shield: f64,
}

impl Default for OutputTable {
    fn default() -> Self {
        Self {
            life: 0.0,
            mana: 0.0,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            chance_to_be_hit: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            max_fire_resistance: 0.0,
            max_cold_resistance: 0.0,
            max_lightning_resistance: 0.0,
            fire_resistance_over_cap: 0.0,
            cold_resistance_over_cap: 0.0,
            lightning_resistance_over_cap: 0.0,
            crit_chance: 0.0,
            pre_effective_crit_chance: 0.0,
            crit_multiplier: 0.0,
            damage_components: Vec::new(),
            total_hit_avg: 0.0,
            hit_chance: 0.0,
            action_rate: 0.0,
            dps: 0.0,
            skill_use_time: None,
            effective_action_rate: 0.0,
            bleed_dps: 0.0,
            ignite_dps: 0.0,
            poison_dps: 0.0,
            shock_effect: 0.0,
            physical_max_hit: 0.0,
            fire_max_hit: 0.0,
            cold_max_hit: 0.0,
            lightning_max_hit: 0.0,
            chaos_max_hit: 0.0,
            total_ehp: 0.0,
            life_reserved: 0.0,
            life_unreserved: 0.0,
            mana_reserved: 0.0,
            mana_unreserved: 0.0,
            life_regen: 0.0,
            mana_regen: 0.0,
            energy_shield_regen: 0.0,
            block_chance: 0.0,
            spell_block_chance: 0.0,
            es_recharge_rate: 0.0,
            es_recharge_delay: 0.0,
            es_recharge_per_second: 0.0,
            avoid_all_damage_from_hits: 0.0,
            avoid_projectile_damage: 0.0,
            avoid_stun: 0.0,
            avoid_ignite: 0.0,
            avoid_shock: 0.0,
            avoid_chill: 0.0,
            avoid_freeze: 0.0,
            avoid_poison: 0.0,
            avoid_bleeding: 0.0,
            // Taken multipliers / enemy crit effect default to neutral (1.0 = no mitigation/increase).
            taken_multi_physical: 1.0,
            taken_multi_fire: 1.0,
            taken_multi_cold: 1.0,
            taken_multi_lightning: 1.0,
            taken_multi_chaos: 1.0,
            crit_extra_damage_reduction: 0.0,
            enemy_crit_effect: 1.0,
            minions: Vec::new(),
            trigger_rate_cap: 0.0,
            skill_trigger_rate: 0.0,
            // Lane A: charges / leech / Recoup default to neutral 0 (no effect on the panel without a source mod).
            charge_power_current: 0,
            charge_power_maximum: 0,
            charge_frenzy_current: 0,
            charge_frenzy_maximum: 0,
            charge_endurance_current: 0,
            charge_endurance_maximum: 0,
            life_leech_rate: 0.0,
            mana_leech_rate: 0.0,
            es_leech_rate: 0.0,
            life_recoup_rate: 0.0,
            es_recoup_rate: 0.0,
            // Lane B: ailment extensions default to 0 (not applied without a matching hit/mod).
            chill_effect: 0.0,
            freeze_buildup_pct: 0.0,
            electrocute_buildup_pct: 0.0,
            bleed_stacked_dps: 0.0,
            bleed_active_stacks: 0.0,
            poison_stacked_dps: 0.0,
            poison_active_stacks: 0.0,
            ignite_stacked_dps: 0.0,
            ignite_active_stacks: 0.0,
            bleed_max_stacks: 0.0,
            poison_max_stacks: 0.0,
            ignite_max_stacks: 0.0,
            // Lane C: skill functionality defaults to 0 (not written without a base configuration).
            aoe_radius: 0.0,
            aoe_area_mod: 0.0,
            projectile_count: 0.0,
            cooldown: 0.0,
            cooldown_stored_uses: 0,
            mana_cost: 0.0,
            life_cost: 0.0,
            spirit_reserved: 0.0,
            // Defence extensions (W0.2): all neutral 0 until wired up
            // (ParityStatus=Planned, not included in extract_display_values).
            spirit: 0.0,
            spirit_unreserved: 0.0,
            block_chance_max: 0.0,
            spell_block_chance_max: 0.0,
            effective_block_chance: 0.0,
            effective_spell_block_chance: 0.0,
            effective_projectile_block_chance: 0.0,
            effective_spell_projectile_block_chance: 0.0,
            block_effect: 0.0,
            deflection_rating: 0.0,
            deflect_chance: 0.0,
            evade_chance: 0.0,
            melee_evade_chance: 0.0,
            projectile_evade_chance: 0.0,
            spell_evade_chance: 0.0,
            spell_projectile_evade_chance: 0.0,
            stun_threshold: 0.0,
            self_stun_chance: 0.0,
            stun_duration: 0.0,
            ward: 0.0,
            life_recoverable: 0.0,
            energy_shield_recovery_cap: 0.0,
            physical_damage_reduction: 0.0,
            number_of_damaging_hits: 0.0,
            number_of_mitigated_hits: 0.0,
            total_ehp_lowest_max_hit: 0.0,
            // (F-1): dual-run parallel fields, default neutral 0.
            total_ehp_pob2: 0.0,
            total_enemy_damage_in: 0.0,
            physical_max_hit_pob2: 0.0,
            fire_max_hit_pob2: 0.0,
            cold_max_hit_pob2: 0.0,
            lightning_max_hit_pob2: 0.0,
            chaos_max_hit_pob2: 0.0,
            //  Curse panel defaults to neutral (no effect on any existing output when buff_pass hasn't run).
            enemy_curse_limit: 0.0,
            curse_slots: Vec::new(),
            //  Skill DoT / combined-DPS family skeleton fields, always neutral 0 until wired up.
            skill_dot_instance: 0.0,
            skill_total_dot: 0.0,
            total_dot_dps: 0.0,
            with_dot_dps: 0.0,
            combined_dps: 0.0,
            //  Per-hand sub-tables default to None (always None in the fallback state; consumers skip on None).
            main_hand: None,
            off_hand: None,
        }
    }
}

impl From<&MinimalOutput> for OutputTable {
    fn from(value: &MinimalOutput) -> Self {
        Self {
            life: value.life,
            mana: value.mana,
            fire_resistance: value.fire_resistance,
            cold_resistance: value.cold_resistance,
            lightning_resistance: value.lightning_resistance,
            max_fire_resistance: value.max_fire_resistance,
            max_cold_resistance: value.max_cold_resistance,
            max_lightning_resistance: value.max_lightning_resistance,
            fire_resistance_over_cap: value.fire_resistance_over_cap,
            cold_resistance_over_cap: value.cold_resistance_over_cap,
            lightning_resistance_over_cap: value.lightning_resistance_over_cap,
            crit_chance: value.crit_chance,
            pre_effective_crit_chance: value.pre_effective_crit_chance,
            crit_multiplier: value.crit_multiplier,
            damage_components: value.damage_components.clone(),
            total_hit_avg: value.total_hit_avg,
            hit_chance: value.hit_chance,
            action_rate: value.action_rate,
            dps: value.dps,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod m2_default_neutral_tests {
    use super::OutputTable;

    /// Neutrality invariant: the new defence extension fields are all 0
    /// under `Default` (no effect on any existing output/comparison until
    /// wired up; written by perform fill once tracks A-F wire them in).
    #[test]
    fn m2_defence_extension_fields_default_to_zero() {
        let out = OutputTable::default();
        for (name, v) in [
            ("spirit", out.spirit),
            ("spirit_unreserved", out.spirit_unreserved),
            ("block_chance_max", out.block_chance_max),
            ("spell_block_chance_max", out.spell_block_chance_max),
            ("effective_block_chance", out.effective_block_chance),
            (
                "effective_spell_block_chance",
                out.effective_spell_block_chance,
            ),
            ("block_effect", out.block_effect),
            ("deflection_rating", out.deflection_rating),
            ("deflect_chance", out.deflect_chance),
            ("evade_chance", out.evade_chance),
            ("melee_evade_chance", out.melee_evade_chance),
            ("projectile_evade_chance", out.projectile_evade_chance),
            ("spell_evade_chance", out.spell_evade_chance),
            (
                "spell_projectile_evade_chance",
                out.spell_projectile_evade_chance,
            ),
            ("stun_threshold", out.stun_threshold),
            ("self_stun_chance", out.self_stun_chance),
            ("stun_duration", out.stun_duration),
            ("ward", out.ward),
            ("life_recoverable", out.life_recoverable),
            ("energy_shield_recovery_cap", out.energy_shield_recovery_cap),
            ("physical_damage_reduction", out.physical_damage_reduction),
            ("number_of_damaging_hits", out.number_of_damaging_hits),
            ("number_of_mitigated_hits", out.number_of_mitigated_hits),
            ("total_ehp_lowest_max_hit", out.total_ehp_lowest_max_hit),
            ("total_ehp_pob2", out.total_ehp_pob2),
            ("total_enemy_damage_in", out.total_enemy_damage_in),
            ("physical_max_hit_pob2", out.physical_max_hit_pob2),
            ("fire_max_hit_pob2", out.fire_max_hit_pob2),
            ("cold_max_hit_pob2", out.cold_max_hit_pob2),
            ("lightning_max_hit_pob2", out.lightning_max_hit_pob2),
            ("chaos_max_hit_pob2", out.chaos_max_hit_pob2),
        ] {
            assert_eq!(v, 0.0, "{name} 默认应为 0（中性）");
        }
    }
}

#[cfg(test)]
mod m4_t4_default_neutral_tests {
    use super::OutputTable;

    /// Neutrality invariant: the skill DoT / combined-DPS family contract
    /// fields are all 0 under `Default` (no effect on any existing output/comparison until calc wires them in).
    #[test]
    fn m4_t4_skill_dot_fields_default_to_zero() {
        let out = OutputTable::default();
        for (name, v) in [
            ("skill_dot_instance", out.skill_dot_instance),
            ("skill_total_dot", out.skill_total_dot),
            ("total_dot_dps", out.total_dot_dps),
            ("with_dot_dps", out.with_dot_dps),
            ("combined_dps", out.combined_dps),
        ] {
            assert_eq!(v, 0.0, "{name} 默认应为 0（中性）");
        }
    }
}
