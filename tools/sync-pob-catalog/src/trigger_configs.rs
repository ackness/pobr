//! `gen-trigger-configs` subcommand:
//! produces `overlay/trigger_configs.json` (the 61 trigger configs from
//! vendor `Modules/CalcTriggers.lua:881-1417`'s configTable).
//!
//! configTable entries are **Lua closures that return a config table**
//! (roughly 90% declarative facts plus a handful of real logic). Running
//! them under luajit would need a full env mock, and fields like
//! `triggerSkillCond` are themselves still closures that can't be
//! serialized — so, following mirage_configs' precedent, **the config data
//! is embedded in this tool's source code** and the tool writes it out.
//! Unlike mirage, this adds a generation-time **configTable key scan
//! reconciliation** against the vendor source (regex-extracts the 61 keys
//! from `["<key>"] = function` and asserts set equality against the
//! embedded transcription's key set) — any key addition/removal/rename
//! fails generation outright, a finer-grained drift guard than a single
//! fingerprint; the fingerprint (line count + byte count) still goes into
//! `_meta` as a reminder to check for semantic drift within entry bodies.
//!
//! Restricted predicates obey the 20-doc §5 hard boundary (any/all/not
//! capped at three fields); real logic that can't be expressed that way
//! goes to `handler_id` (`trigger:` prefix, monitored under the global <100
//! handler-count gate across all phases). See [`pobr_data::catalog::triggers`] for the schema.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use pobr_data::catalog::triggers::{TriggerConfigDef, TriggerKeyDef, TriggerSkillCondDef};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};
use crate::extract_minions::to_pretty_json;

/// Total entry count in vendor's configTable (CalcTriggers.lua:881-1417, the value the drift guard asserts against).
pub const VENDOR_CONFIG_TABLE_COUNT: usize = 61;

/// `_meta` extension: the generic OverlayMeta plus a coarse vendor fingerprint plus the key-reconciliation result.
#[derive(Debug, Serialize)]
struct TriggerMeta {
    #[serde(flatten)]
    base: OverlayMeta,
    /// Coarse fingerprint of `Modules/CalcTriggers.lua` (`<lines>L:<bytes>B`)
    /// — a changed fingerprint after a vendor bump signals that entry
    /// bodies need a manual semantic review (the key set can stay equal while internal logic changes).
    vendor_fingerprint: String,
    /// The configTable key count scanned from the vendor source at
    /// generation time (always = 61; only written if reconciliation passes).
    vendor_config_table_keys: usize,
}

#[derive(Debug, Serialize)]
struct TriggerConfigsDoc {
    #[serde(rename = "_meta")]
    meta: TriggerMeta,
    configs: Vec<TriggerConfigDef>,
}

/// Entry skeleton: all declarative fields default (only key + vendor_ref
/// set), and each entry overrides its differing fields via struct-update
/// syntax, keeping the transcription readable entry by entry.
fn base(kind: &str, name: &str, vendor_ref: &str) -> TriggerConfigDef {
    TriggerConfigDef {
        key: TriggerKeyDef {
            kind: kind.to_string(),
            name: name.to_string(),
        },
        trigger_name: None,
        trigger_on_use: false,
        use_cast_rate: false,
        source_skill_cond: None,
        triggered_skill_cond: None,
        source_skill_name: None,
        requires_main_skill_name: None,
        trigger_chance_stat: None,
        source_rate_stat: None,
        cooldown_override_s: None,
        trigger_rate_cap_override: None,
        global_trigger: false,
        source_is_self: false,
        source_rate_is_final: false,
        ignores_tick_rate: false,
        assuming_every_hit_kills: false,
        ignore_source_rate: false,
        trigger_on_crit: false,
        requires_condition: None,
        match_effect_ids: Vec::new(),
        handler_id: None,
        note: None,
        vendor_ref: vendor_ref.to_string(),
        verified: false,
    }
}

/// Predicate shorthand.
fn cond(any: &[&str], all_flags: &[&str], not: &[&str]) -> Option<TriggerSkillCondDef> {
    Some(TriggerSkillCondDef {
        any_skill_types: any.iter().map(|s| s.to_string()).collect(),
        all_mod_flags: all_flags.iter().map(|s| s.to_string()).collect(),
        not_skill_types: not.iter().map(|s| s.to_string()).collect(),
    })
}

fn s(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// The 61 trigger configs (hand-transcribed from the
/// `Modules/CalcTriggers.lua:881-1417` configTable closures as of vendor
/// commit `2df5a74`; see each `vendor_ref` for its line-range anchor;
/// transcription order matches vendor source order, sorted ascending by `key.name` before writing).
#[allow(clippy::too_many_lines)]
fn builtin_configs() -> Vec<TriggerConfigDef> {
    let v = "Modules/CalcTriggers.lua";
    let mut configs = vec![
        // :882-888 Summon Spectral Wolf (Claw melee/attack source, not a totem).
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee", "Attack"], &["Claw"], &["SummonsTotem"]),
            ..base("unique_item", "law of the wilds", &format!("{v}:882-888"))
        },
        // :889-897 Active only when the main skill is Storm Cascade (Melee/Attack source).
        TriggerConfigDef {
            requires_main_skill_name: s("Storm Cascade"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "the rippling thoughts",
                &format!("{v}:889-897"),
            )
        },
        // :898-906 Same as above (the Surging variant).
        TriggerConfigDef {
            requires_main_skill_name: s("Storm Cascade"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "the surging thoughts",
                &format!("{v}:898-906"),
            )
        },
        // :907-921 Unseen Strike: global + rate cap 2/s, requires Phasing (otherwise disabled).
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            trigger_rate_cap_override: Some(2.0),
            requires_condition: s("Phasing"),
            ..base("unique_item", "the hidden blade", &format!("{v}:907-921"))
        },
        // :922-925 global + source = self.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base(
                "unique_item",
                "replica eternity shroud",
                &format!("{v}:922-925"),
            )
        },
        // :926-929 global + source = self.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base(
                "unique_item",
                "shroud of the lightless",
                &format!("{v}:926-929"),
            )
        },
        // :930-932 Gore Shockwave (Melee/Attack source).
        TriggerConfigDef {
            trigger_name: s("Gore Shockwave"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "limbsplit", &format!("{v}:930-932"))
        },
        // :933-935 Same as Limbsplit.
        TriggerConfigDef {
            trigger_name: s("Gore Shockwave"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "the cauteriser", &format!("{v}:933-935"))
        },
        // :936-938 Stalking Pustule (Damage/Attack source).
        TriggerConfigDef {
            trigger_name: s("Stalking Pustule"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "duskblight", &format!("{v}:936-938"))
        },
        // :939-944 Rain of Arrows on bow attack: cooldown override 1s, triggerOnUse.
        TriggerConfigDef {
            trigger_on_use: true,
            cooldown_override_s: Some(1.0),
            source_skill_cond: cond(&["Attack"], &["Bow"], &[]),
            ..base("unique_item", "lioneye's paws", &format!("{v}:939-944"))
        },
        // :945-950 Same as above (Replica).
        TriggerConfigDef {
            trigger_on_use: true,
            cooldown_override_s: Some(1.0),
            source_skill_cond: cond(&["Attack"], &["Bow"], &[]),
            ..base(
                "unique_item",
                "replica lioneye's paws",
                &format!("{v}:945-950"),
            )
        },
        // :951-955 Lightning Warp: cooldown override 1s.
        TriggerConfigDef {
            trigger_name: s("Lightning Warp"),
            cooldown_override_s: Some(1.0),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "moonbender's wing", &format!("{v}:951-955"))
        },
        // :956-958 Molten Burst.
        TriggerConfigDef {
            trigger_name: s("Molten Burst"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "ngamahu's flame", &format!("{v}:956-958"))
        },
        // :959-961 Icicle Burst.
        TriggerConfigDef {
            trigger_name: s("Icicle Burst"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "cameria's avarice", &format!("{v}:959-961"))
        },
        // :962-964 Bone Nova.
        TriggerConfigDef {
            trigger_name: s("Bone Nova"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "uul-netol's embrace",
                &format!("{v}:962-964"),
            )
        },
        // :965-968 on-kill: sourceRateIsFinal + assumingEveryHitKills.
        TriggerConfigDef {
            source_rate_is_final: true,
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "rigwald's crest", &format!("{v}:965-968"))
        },
        // :969-972 Same as above.
        TriggerConfigDef {
            source_rate_is_final: true,
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "jorrhast's blacksteel",
                &format!("{v}:969-972"),
            )
        },
        // :973-976 Same as above.
        TriggerConfigDef {
            source_rate_is_final: true,
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "ashcaller", &format!("{v}:973-976"))
        },
        // :977-979 on-kill (no sourceRateIsFinal).
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "arakaali's fang", &format!("{v}:977-979"))
        },
        // :980-982 Same as above.
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "sporeguard", &format!("{v}:980-982"))
        },
        // :983-985 Same as above.
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "mark of the elder", &format!("{v}:983-985"))
        },
        // :986-988 Same as above.
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "mark of the shaper", &format!("{v}:986-988"))
        },
        // :989-997 Spell triggered on a Wand attack; the triggered skill is the same-socket triggeredByUnique spell.
        TriggerConfigDef {
            trigger_on_use: true,
            source_skill_cond: cond(&["Damage", "Attack"], &["Wand"], &[]),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("unique_item", "poet's pen", &format!("{v}:989-997"))
        },
        // :998-1011 Real logic: regex-extracts the unique's trigger name from modSource, plus a Replica branch.
        TriggerConfigDef {
            trigger_on_use: true,
            triggered_skill_cond: cond(&["RangedAttack"], &[], &[]),
            handler_id: s("trigger:maloneys_mechanism_unique_name"),
            note: s("源谓词依赖 Replica 判定（弓攻击 vs 法术二选一），落 handler"),
            ..base(
                "unique_item",
                "maloney's mechanism",
                &format!("{v}:998-1011"),
            )
        },
        // :1012-1020 A bow attack triggers the same-socket spell.
        TriggerConfigDef {
            trigger_on_use: true,
            source_skill_cond: cond(&["Damage", "Attack"], &["Bow"], &[]),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("unique_item", "asenath's chant", &format!("{v}:1011-1019"))
        },
        // :1021-1026 Hex source + cast rate.
        TriggerConfigDef {
            use_cast_rate: true,
            source_skill_cond: cond(&["Hex"], &[], &[]),
            ..base(
                "unique_item",
                "vixen's entrapment",
                &format!("{v}:1020-1025"),
            )
        },
        // :1027-1032 Source = Queen's Demand (exact name match).
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_name: s("Queen's Demand"),
            ..base("skill", "flames of judgement", &format!("{v}:1026-1031"))
        },
        // :1033-1038 Same as above.
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_name: s("Queen's Demand"),
            ..base("skill", "storm of judgement", &format!("{v}:1032-1037"))
        },
        // :1039-1061 Craft-mod trigger: a global scan plus totem/golem rate special-casing, real logic.
        TriggerConfigDef {
            handler_id: s("trigger:trigger_craft_scan"),
            note: s("triggeredByCraft 全局源扫描 + totem/golem/banner 源速率特判"),
            ..base("triggered_by", "trigger craft", &format!("{v}:1038-1060"))
        },
        // :1062-1075 Real logic: a mana-cost-gated comparer (KitavaRequiredManaCost).
        TriggerConfigDef {
            trigger_name: s("Kitava's Thirst"),
            trigger_chance_stat: s("KitavaTriggerChance"),
            handler_id: s("trigger:kitavas_thirst_mana_gate"),
            note: s("源比较器要求 ManaCost > KitavaRequiredManaCost，落 handler"),
            ..base("unique_item", "kitava's thirst", &format!("{v}:1061-1074"))
        },
        // :1076-1083 Real logic: dual two-handed sources (the sources must be in **different** sockets, vendor's not slotMatch).
        TriggerConfigDef {
            source_skill_cond: cond(&["Damage", "Attack"], &["Mace", "Weapon1H"], &[]),
            handler_id: s("trigger:mjolner_dual_source"),
            note: s("vendor band(bor(Mace,1H)) 任一位命中 + 非同槽源约束，落 handler"),
            ..base("unique_item", "mjolner", &format!("{v}:1075-1082"))
        },
        // :1084-1089 Melee + one-handed-sword source; the triggered skill is the same-socket triggeredByCospris.
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &["Sword", "Weapon1H"], &[]),
            note: s("vendor band(bor(Sword,1H)) 字面任一位命中，按条目意图转写 all-of"),
            ..base("unique_item", "cospri's malice", &format!("{v}:1083-1088"))
        },
        // :1090-1093 CoC: attack source (same socket), folds the source's crit chance in (triggerOnCrit).
        TriggerConfigDef {
            source_skill_cond: cond(&["Attack"], &[], &[]),
            trigger_on_crit: true,
            match_effect_ids: vec!["MetaCastOnCritPlayer".to_string()],
            note: s("trigger_on_crit 来自 SkillStatMap 对 triggeredByCoc 的固定旗标"),
            ..base(
                "triggered_by",
                "cast on critical strike",
                &format!("{v}:1089-1092"),
            )
        },
        // :1094-1105 on-melee-kill: requires KilledRecently, otherwise degrades to self-cast.
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Melee"], &[], &[]),
            requires_condition: s("KilledRecently"),
            match_effect_ids: vec!["MetaCastOnMeleeKillPlayer".to_string()],
            note: s("vendor 源谓词 Attack and Melee；Melee 蕴含 Attack，转写 any[Melee]"),
            ..base(
                "triggered_by",
                "cast on melee kill",
                &format!("{v}:1093-1104"),
            )
        },
        // :1106-1113 Holy Relic Nova: real logic for the minion actor.
        TriggerConfigDef {
            trigger_name: s("Summon Holy Relic"),
            source_skill_cond: cond(&["Attack"], &[], &[]),
            handler_id: s("trigger:holy_relic_minion_actor"),
            note: s("actor = env.minion（召唤物触发域），M5a minion 框架后接"),
            ..base("skill", "nova", &format!("{v}:1105-1112"))
        },
        // :1114-1128 CWDT: global + source is self; the threshold output CWDTThreshold is panel info.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            match_effect_ids: vec!["MetaCastWhenDamageTakenPlayer".to_string()],
            note: s("CWDTThreshold 面板输出（threshold × CWDTThreshold mod）未建模"),
            ..base(
                "triggered_by",
                "cast when damage taken",
                &format!("{v}:1113-1127"),
            )
        },
        // :1129-1133 Trigger chance = skillData.chanceToTriggerOnStun.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            trigger_chance_stat: s("chanceToTriggerOnStun"),
            match_effect_ids: vec!["MetaCastWhenStunnedPlayer".to_string()],
            ..base(
                "triggered_by",
                "cast when stunned",
                &format!("{v}:1128-1132"),
            )
        },
        // :1134-1153 The triggered side: source = the Automation gem (by name), not subject to frame rounding.
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Automation"),
            note: s("主技能 = Automation 自身时 vendor 走 global+self 分支（自速率）"),
            ..base("triggered_by", "automation", &format!("{v}:1133-1152"))
        },
        // :1154-1169 The triggered side: source = Spellslinger (by name); the source side is a Wand attack.
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            source_skill_name: s("Spellslinger"),
            note: s("主技能 = Spellslinger 自身时源谓词为 Wand 攻击（weaponTypes 判定）"),
            ..base("triggered_by", "spellslinger", &format!("{v}:1153-1168"))
        },
        // :1170-1182 Same shape as Automation.
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Call to Arms"),
            ..base("triggered_by", "call to arms", &format!("{v}:1169-1181"))
        },
        // :1183-1195 Same shape as Automation.
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Autoexertion"),
            ..base("triggered_by", "autoexertion", &format!("{v}:1182-1194"))
        },
        // :1196-1198 Attack source.
        TriggerConfigDef {
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("triggered_by", "mark on hit", &format!("{v}:1195-1197"))
        },
        // :1199-1204 Attack source (same socket).
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("triggered_by", "hextouch", &format!("{v}:1198-1203"))
        },
        // :1205-1210 Attack source.
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("unique_item", "oskarm", &format!("{v}:1204-1209"))
        },
        // :1211-1214 global + source is self.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base("skill", "tempest shield", &format!("{v}:1210-1213"))
        },
        // :1215-1230 Real logic: duration acts as a pseudo-cooldown (needs a self-sub-calculation of Duration).
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            handler_id: s("trigger:shattershard_duration_cd"),
            note: s("triggerRateCapOverride = 1/Duration（取自自身子计算输出）"),
            ..base("skill", "shattershard", &format!("{v}:1214-1229"))
        },
        // :1231-1241 Real logic: BattlemageUpTimeRatio comparer.
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            handler_id: s("trigger:battlemage_uptime_comparer"),
            ..base("skill", "battlemage's cry", &format!("{v}:1230-1240"))
        },
        // :1242-1261 Real logic: brand activation frequency (repeatFrequency × the frequency multiplier zone).
        TriggerConfigDef {
            ignores_tick_rate: true,
            source_rate_is_final: true,
            source_skill_name: s("Arcanist Brand"),
            handler_id: s("trigger:arcanist_brand_activation_freq"),
            ..base("triggered_by", "arcanist brand", &format!("{v}:1241-1260"))
        },
        // :1262-1266 On-death trigger: global, DPS is display-only info.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            note: s("vendor 仅置 triggered + infoMessage（on-death 无稳态 DPS 语义）"),
            ..base("triggered_by", "cast on death", &format!("{v}:1261-1265"))
        },
        // :1267-1274 Real logic: InfernalUpTimeRatio comparer.
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            handler_id: s("trigger:infernal_uptime_comparer"),
            ..base("skill", "combust", &format!("{v}:1266-1273"))
        },
        // :1275-1277 Attack source (same socket).
        TriggerConfigDef {
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("skill", "prismatic burst", &format!("{v}:1274-1276"))
        },
        // :1278-1280 Melee source (same socket).
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            ..base("skill", "shockwave", &format!("{v}:1277-1279"))
        },
        // :1281-1285 Bow attack source.
        TriggerConfigDef {
            trigger_on_use: true,
            trigger_name: s("Manaforged Arrows"),
            source_skill_cond: cond(&["Attack"], &["Bow"], &[]),
            ..base(
                "triggered_by",
                "manaforged arrows",
                &format!("{v}:1280-1284"),
            )
        },
        // :1286-1295 Real logic: CurseOverlaps multiplication plus a config-input branch.
        TriggerConfigDef {
            use_cast_rate: true,
            ignores_tick_rate: true,
            source_skill_cond: cond(&["Hex"], &[], &[]),
            handler_id: s("trigger:doom_blast_overlaps"),
            ..base("skill", "doom blast", &format!("{v}:1285-1294"))
        },
        // :1296-1298 CWC: customHandler (pobr already models this via the CWCTriggerTime channel).
        TriggerConfigDef {
            handler_id: s("trigger:cwc_handler"),
            match_effect_ids: vec!["MetaCastWhileChannellingPlayer".to_string()],
            note: s("vendor customHandler = CWCHandler；pobr 经 perform CWC 分支建模"),
            ..base(
                "triggered_by",
                "cast while channelling",
                &format!("{v}:1295-1297"),
            )
        },
        // :1299-1301 Helmet Focus enchant trigger: customHandler.
        TriggerConfigDef {
            handler_id: s("trigger:helmet_focus_handler"),
            ..base("triggered_by", "focus", &format!("{v}:1298-1300"))
        },
        // :1302-1377 Real logic: Snipe's charge-up stage count / hitTimeMultiplier / trigger list.
        TriggerConfigDef {
            handler_id: s("trigger:snipe_stages"),
            ..base("triggered_by", "snipe", &format!("{v}:1301-1376"))
        },
        // :1378-1384 Real logic: TotemLife comparer + ignoreSourceRate.
        TriggerConfigDef {
            ignore_source_rate: true,
            handler_id: s("trigger:avenging_flame_totem_comparer"),
            note: s("源谓词 skillFlags.totem（pobr 无图腾域）+ 图腾血量比较器"),
            ..base("skill", "avenging flame", &format!("{v}:1377-1384"))
        },
        // :1385-1399 Link trigger: source rate = the IntuitiveLinkSourceRate stat.
        TriggerConfigDef {
            use_cast_rate: true,
            source_skill_name: s("Intuitive Link"),
            source_rate_stat: s("IntuitiveLinkSourceRate"),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("triggered_by", "intuitive link", &format!("{v}:1385-1399"))
        },
        // :1400-1406 Svalinn shield: global + source is self; the triggered-skill filter goes through support applicability.
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            handler_id: s("trigger:svalinn_support_filter"),
            note: s("triggeredSkillCond 调 canGrantedEffectSupportActiveSkill，落 handler"),
            ..base(
                "triggered_by",
                "supporttriggerelementalspellonblock",
                &format!("{v}:1400-1406"),
            )
        },
        // :1407-1416 Settlers enchant: a Melee source triggers the same-socket enchanted skill.
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            ..base(
                "triggered_by",
                "supporttriggerfirespellonhit",
                &format!("{v}:1407-1416"),
            )
        },
    ];
    configs.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    configs
}

/// Scan vendor `Modules/CalcTriggers.lua` for configTable's key set (the
/// `["<key>"] = function` lines between `local configTable = {` and the first line-leading `}`).
fn scan_vendor_config_keys(vendor_root: &Path) -> io::Result<BTreeSet<String>> {
    let path = vendor_root.join("Modules/CalcTriggers.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("failed to read {}: {error}", path.display()),
        )
    })?;

    let mut keys = BTreeSet::new();
    let mut in_table = false;
    for line in text.lines() {
        if !in_table {
            if line.starts_with("local configTable = {") {
                in_table = true;
            }
            continue;
        }
        if line.starts_with('}') {
            break;
        }
        // Shape: \t["law of the wilds"] = function()
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("[\"")
            && let Some(end) = rest.find("\"]")
            && rest[end..].contains("= function")
        {
            keys.insert(rest[..end].to_string());
        }
    }
    Ok(keys)
}

/// Key reconciliation (a drift guard: the generation-time counterpart of the
/// 61-entry extraction count assertion): the embedded transcription's key
/// set must be **set-equal** to the vendor-source scan result, and equal 61.
fn check_key_drift(vendor_root: &Path) -> io::Result<usize> {
    let vendor_keys = scan_vendor_config_keys(vendor_root)?;
    let embedded_keys: BTreeSet<String> = builtin_configs()
        .iter()
        .map(|c| c.key.name.clone())
        .collect();

    if vendor_keys != embedded_keys {
        let missing: Vec<&String> = vendor_keys.difference(&embedded_keys).collect();
        let extra: Vec<&String> = embedded_keys.difference(&vendor_keys).collect();
        return Err(io::Error::other(format!(
            "trigger configTable key reconciliation failed (vendor drift, embedded transcription needs updating):\n  present in vendor but missing from transcription {missing:?}\n  present in transcription but missing from vendor {extra:?}"
        )));
    }
    if vendor_keys.len() != VENDOR_CONFIG_TABLE_COUNT {
        return Err(io::Error::other(format!(
            "trigger configTable entry count {} != expected {VENDOR_CONFIG_TABLE_COUNT}",
            vendor_keys.len()
        )));
    }
    Ok(vendor_keys.len())
}

/// Generate `overlay/trigger_configs.json` text (byte-stable; runs the key reconciliation first).
pub fn run_gen_trigger_configs(args: &ExtractLuaArgs) -> io::Result<String> {
    let key_count = check_key_drift(&args.vendor_root)?;
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let fingerprint = vendor_fingerprint(&args.vendor_root)?;
    let mut regen = "cargo run -p sync-pob-catalog -- gen-trigger-configs --vendor-root vendor/PathOfBuilding-PoE2/src".to_string();
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    let meta = TriggerMeta {
        base: OverlayMeta {
            schema: "trigger_configs/v1".to_string(),
            generator:
                "sync-pob-catalog gen-trigger-configs（数据内嵌于工具源码 + vendor key 对账）"
                    .to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: commit,
            vendor_commit_subject: subject,
            extracted_files: vec!["Modules/CalcTriggers.lua".to_string()],
            regen_command: regen,
        },
        vendor_fingerprint: fingerprint,
        vendor_config_table_keys: key_count,
    };
    Ok(to_pretty_json(&TriggerConfigsDoc {
        meta,
        configs: builtin_configs(),
    }))
}

/// Coarse fingerprint of CalcTriggers.lua: line count + byte count.
fn vendor_fingerprint(vendor_root: &Path) -> io::Result<String> {
    let path = vendor_root.join("Modules/CalcTriggers.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    Ok(format!("{}L:{}B", text.lines().count(), text.len()))
}

#[cfg(test)]
mod tests {
    use super::{VENDOR_CONFIG_TABLE_COUNT, builtin_configs};

    /// 61 entries, keys unique and ascending (the embedded-side counterpart of the 61-entry extraction count assertion).
    #[test]
    fn builtin_configs_count_and_order() {
        let configs = builtin_configs();
        assert_eq!(configs.len(), VENDOR_CONFIG_TABLE_COUNT);
        let names: Vec<&str> = configs.iter().map(|c| c.key.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted, "key.name must be unique and ascending");
    }

    /// handler entries carry the `trigger:` prefix (20-doc §5's global ledger
    /// counts by prefix domain); the total handler count is monitored (currently 15, under the global <100 gate across all phases).
    #[test]
    fn handler_ids_prefixed_and_counted() {
        let configs = builtin_configs();
        let handlers: Vec<&str> = configs
            .iter()
            .filter_map(|c| c.handler_id.as_deref())
            .collect();
        for handler in &handlers {
            assert!(
                handler.starts_with("trigger:"),
                "handler id {handler} is missing the trigger: prefix"
            );
        }
        assert_eq!(
            handlers.len(),
            15,
            "handler entry count changes must be synced to the monitoring ledger"
        );
        assert!(
            handlers.len() < 100,
            "total handler count exceeds doc-20 §5's monitoring gate"
        );
    }

    /// Every entry has a key category plus a vendor line-range anchor;
    /// non-empty predicate fields obey the three-field cap (the schema
    /// itself has three fields, and this asserts a non-empty predicate constrains at least one of them).
    #[test]
    fn entries_have_anchor_and_meaningful_cond() {
        for config in builtin_configs() {
            assert!(
                matches!(
                    config.key.kind.as_str(),
                    "skill" | "triggered_by" | "unique_item"
                ),
                "{}'s kind is invalid: {}",
                config.key.name,
                config.key.kind
            );
            assert!(
                config.vendor_ref.starts_with("Modules/CalcTriggers.lua:"),
                "{} is missing a vendor line-range anchor",
                config.key.name
            );
            for cond in [&config.source_skill_cond, &config.triggered_skill_cond]
                .into_iter()
                .flatten()
            {
                assert!(
                    !cond.is_empty(),
                    "{} carries an empty predicate",
                    config.key.name
                );
            }
        }
    }

    /// The 5 Meta trigger gems currently mapped by the PoE2 join key (match_effect_ids).
    #[test]
    fn match_effect_ids_mapped_meta_gems() {
        let configs = builtin_configs();
        let mapped: Vec<&str> = configs
            .iter()
            .flat_map(|c| c.match_effect_ids.iter().map(String::as_str))
            .collect();
        assert!(mapped.contains(&"MetaCastOnCritPlayer"));
        assert!(mapped.contains(&"MetaCastWhenDamageTakenPlayer"));
        assert!(mapped.contains(&"MetaCastWhileChannellingPlayer"));
    }

    /// A directional fact about the CoC entry: trigger_on_crit is set (the data precondition for folding in crit chance).
    #[test]
    fn coc_entry_folds_crit() {
        let configs = builtin_configs();
        let coc = configs
            .iter()
            .find(|c| c.key.name == "cast on critical strike")
            .expect("missing CoC entry");
        assert!(coc.trigger_on_crit);
        assert!(coc.source_skill_cond.is_some());
    }
}
