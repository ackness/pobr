// Generated from Rust request DTOs by ts-rs. Do not edit manually.
// Regenerate: cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored

export type SlotIssue = {
/**
 * The equipment slot id / `Flask N`·`Charm N` slot name / `Jewel@<socket_node>`.
 */
slot: string, message: string, };

export type CharacterOverride = { level?: number, class_name?: string, ascendancy_name?: string, };

export type GemInput = { enabled?: boolean, count?: number | null, enable_global_1?: boolean | null, enable_global_2?: boolean | null, skill_id: string, level: number, quality: number,
/**
 * PoB stat-set selection (1-based); omitted selects the primary form.
 */
stat_set_index?: number | null,
/**
 * Source display metadata for unresolved slots; never used to resolve calculation effects.
 */
name_spec?: string | null,
/**
 * Whether the gem is corrupted; omitted means it is not.
 */
corrupted?: boolean,
/**
 * The monster a minion gem summons (PoB `skillMinion`); a Companion's
 * beast and a Spectre's monster set the Spirit it reserves.
 */
minion?: string | null,
/**
 * Which of the minion's skills it uses (PoB `skillMinionSkill`,
 * 1-based); omitted is the first.
 */
minion_skill?: number | null, };

export type SocketGroupInput = {
/**
 * Legacy clients used this before masking enabled by weapon set. New
 * requests preserve user enablement independently of weapon-set activity.
 */
full_dps_enabled?: boolean | null, include_in_full_dps?: boolean, group_count?: number | null, weapon_set?: 1 | 2 | null, slot?: string | null, enabled: boolean,
/**
 * PoB's 1-based selection among non-support gems; absent selects the first.
 */
main_active_skill?: number | null,
/**
 * The source marker for an equipment-granted skill group (passed
 * through from decode; `None` for a manual group).
 */
source?: string | null, gems: Array<GemInput>, };

export type SlotItemInput = { slot: string, text: string, };

export type WeaponSwap = {
/**
 * The weapon set in use, 1 or 2.
 */
active: 1 | 2,
/**
 * The other set's weapons. A group bound to that set (a skill one of
 * them grants) is used with them.
 */
alternate_items: Array<SlotItemInput>,
/**
 * The passives allocated in only one set, per set (export only).
 */
exclusive_nodes: [Array<number>, Array<number>], };

export type JewelInput = { socket_node: number, text: string, };
