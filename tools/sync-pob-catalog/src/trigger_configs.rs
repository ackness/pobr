//! `gen-trigger-configs` 子命令（M4-T5 W-E1，缺口 14-G1）：
//! 产 `overlay/trigger_configs.json`（vendor `Modules/CalcTriggers.lua:881-1417`
//! configTable 的 61 项触发配置）。
//!
//! configTable 条目是**返回配置表的 Lua 闭包**（约 90% 声明性事实 + 少数真逻辑），
//! luajit 执行需完整 env mock 且 `triggerSkillCond` 等字段本身仍是闭包、无法序列化
//! ——故沿用 mirage_configs（M5a-D2）先例：**配置数据内嵌于本工具源码**、由工具落盘。
//! 与 mirage 不同的增强：生成时对 vendor 源做 **configTable key 扫描对账**（正则抽取
//! `["<key>"] = function` 的 61 个 key，与内嵌转写的 key 集合做集合相等断言），
//! key 增删改 = 生成即失败，比单一指纹更细的 drift 防线；指纹（行数+字节数）仍落
//! `_meta` 提醒条目体语义漂移。
//!
//! 受限谓词遵守 20 号 §5 硬边界（any/all/not 三字段封顶，R1 纪律）；表达不了的
//! 真逻辑落 `handler_id`（`trigger:` 前缀，全阶段 <100 总闸计数监控）。
//! schema 见 [`pobr_data::catalog::triggers`]。

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use pobr_data::catalog::triggers::{TriggerConfigDef, TriggerKeyDef, TriggerSkillCondDef};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};
use crate::extract_minions::to_pretty_json;

/// vendor configTable 的条目总数（CalcTriggers.lua:881-1417，drift 防线断言值）。
pub const VENDOR_CONFIG_TABLE_COUNT: usize = 61;

/// `_meta` 扩展：通用 OverlayMeta + vendor 粗粒度指纹 + key 对账结果。
#[derive(Debug, Serialize)]
struct TriggerMeta {
    #[serde(flatten)]
    base: OverlayMeta,
    /// `Modules/CalcTriggers.lua` 粗粒度指纹（`<行数>L:<字节数>B`）——vendor bump
    /// 后指纹变化 = 提示人工复核条目体语义（key 集合相等仍可能改条目内逻辑）。
    vendor_fingerprint: String,
    /// 生成时从 vendor 源扫描出的 configTable key 数（恒 = 61，对账通过才落盘）。
    vendor_config_table_keys: usize,
}

#[derive(Debug, Serialize)]
struct TriggerConfigsDoc {
    #[serde(rename = "_meta")]
    meta: TriggerMeta,
    configs: Vec<TriggerConfigDef>,
}

/// 条目骨架：全部声明性字段缺省（仅 key + vendor_ref），各条目以 struct-update
/// 语法覆盖差异字段，保持转写逐条可读。
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

/// 谓词速记。
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

/// 61 条触发配置（人工从 `Modules/CalcTriggers.lua:881-1417` configTable 闭包
/// 转写，vendor commit `2df5a74` 时点；行段锚点见各 `vendor_ref`，转写顺序 =
/// vendor 源顺序，落盘前按 `key.name` 升序排序）。
#[allow(clippy::too_many_lines)]
fn builtin_configs() -> Vec<TriggerConfigDef> {
    let v = "Modules/CalcTriggers.lua";
    let mut configs = vec![
        // :882-888 召唤幽魂狼（Claw 近战/攻击源，非图腾）。
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee", "Attack"], &["Claw"], &["SummonsTotem"]),
            ..base("unique_item", "law of the wilds", &format!("{v}:882-888"))
        },
        // :889-897 仅当主技能 = Storm Cascade 时生效（Melee/Attack 源）。
        TriggerConfigDef {
            requires_main_skill_name: s("Storm Cascade"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "the rippling thoughts",
                &format!("{v}:889-897"),
            )
        },
        // :898-906 同上（Surging 变体）。
        TriggerConfigDef {
            requires_main_skill_name: s("Storm Cascade"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "the surging thoughts",
                &format!("{v}:898-906"),
            )
        },
        // :907-921 Unseen Strike：global + 速率上限 2/s，需 Phasing（否则 disable）。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            trigger_rate_cap_override: Some(2.0),
            requires_condition: s("Phasing"),
            ..base("unique_item", "the hidden blade", &format!("{v}:907-921"))
        },
        // :922-925 global + 源 = 自身。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base(
                "unique_item",
                "replica eternity shroud",
                &format!("{v}:922-925"),
            )
        },
        // :926-929 global + 源 = 自身。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base(
                "unique_item",
                "shroud of the lightless",
                &format!("{v}:926-929"),
            )
        },
        // :930-932 Gore Shockwave（Melee/Attack 源）。
        TriggerConfigDef {
            trigger_name: s("Gore Shockwave"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "limbsplit", &format!("{v}:930-932"))
        },
        // :933-935 同 Limbsplit。
        TriggerConfigDef {
            trigger_name: s("Gore Shockwave"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "the cauteriser", &format!("{v}:933-935"))
        },
        // :936-938 Stalking Pustule（Damage/Attack 源）。
        TriggerConfigDef {
            trigger_name: s("Stalking Pustule"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "duskblight", &format!("{v}:936-938"))
        },
        // :939-944 Rain of Arrows on bow attack：cd 覆盖 1s，triggerOnUse。
        TriggerConfigDef {
            trigger_on_use: true,
            cooldown_override_s: Some(1.0),
            source_skill_cond: cond(&["Attack"], &["Bow"], &[]),
            ..base("unique_item", "lioneye's paws", &format!("{v}:939-944"))
        },
        // :945-950 同上（Replica）。
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
        // :951-955 Lightning Warp：cd 覆盖 1s。
        TriggerConfigDef {
            trigger_name: s("Lightning Warp"),
            cooldown_override_s: Some(1.0),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "moonbender's wing", &format!("{v}:951-955"))
        },
        // :956-958 Molten Burst。
        TriggerConfigDef {
            trigger_name: s("Molten Burst"),
            source_skill_cond: cond(&["Melee", "Attack"], &[], &[]),
            ..base("unique_item", "ngamahu's flame", &format!("{v}:956-958"))
        },
        // :959-961 Icicle Burst。
        TriggerConfigDef {
            trigger_name: s("Icicle Burst"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "cameria's avarice", &format!("{v}:959-961"))
        },
        // :962-964 Bone Nova。
        TriggerConfigDef {
            trigger_name: s("Bone Nova"),
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base(
                "unique_item",
                "uul-netol's embrace",
                &format!("{v}:962-964"),
            )
        },
        // :965-968 on-kill：sourceRateIsFinal + assumingEveryHitKills。
        TriggerConfigDef {
            source_rate_is_final: true,
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "rigwald's crest", &format!("{v}:965-968"))
        },
        // :969-972 同上。
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
        // :973-976 同上。
        TriggerConfigDef {
            source_rate_is_final: true,
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "ashcaller", &format!("{v}:973-976"))
        },
        // :977-979 on-kill（无 sourceRateIsFinal）。
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "arakaali's fang", &format!("{v}:977-979"))
        },
        // :980-982 同上。
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "sporeguard", &format!("{v}:980-982"))
        },
        // :983-985 同上。
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "mark of the elder", &format!("{v}:983-985"))
        },
        // :986-988 同上。
        TriggerConfigDef {
            assuming_every_hit_kills: true,
            source_skill_cond: cond(&["Damage", "Attack"], &[], &[]),
            ..base("unique_item", "mark of the shaper", &format!("{v}:986-988"))
        },
        // :989-997 法术触发于 Wand 攻击；被触发 = 同槽 triggeredByUnique 法术。
        TriggerConfigDef {
            trigger_on_use: true,
            source_skill_cond: cond(&["Damage", "Attack"], &["Wand"], &[]),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("unique_item", "poet's pen", &format!("{v}:989-997"))
        },
        // :998-1011 真逻辑：从 modSource 正则提取 unique 触发名 + Replica 分流。
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
        // :1012-1020 弓攻击触发同槽法术。
        TriggerConfigDef {
            trigger_on_use: true,
            source_skill_cond: cond(&["Damage", "Attack"], &["Bow"], &[]),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("unique_item", "asenath's chant", &format!("{v}:1011-1019"))
        },
        // :1021-1026 Hex 源 + 施法速率。
        TriggerConfigDef {
            use_cast_rate: true,
            source_skill_cond: cond(&["Hex"], &[], &[]),
            ..base(
                "unique_item",
                "vixen's entrapment",
                &format!("{v}:1020-1025"),
            )
        },
        // :1027-1032 源 = Queen's Demand（按名精确匹配）。
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_name: s("Queen's Demand"),
            ..base("skill", "flames of judgement", &format!("{v}:1026-1031"))
        },
        // :1033-1038 同上。
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_name: s("Queen's Demand"),
            ..base("skill", "storm of judgement", &format!("{v}:1032-1037"))
        },
        // :1039-1061 工艺词条触发：全局扫描 + 图腾/魔像速率特判，真逻辑。
        TriggerConfigDef {
            handler_id: s("trigger:trigger_craft_scan"),
            note: s("triggeredByCraft 全局源扫描 + totem/golem/banner 源速率特判"),
            ..base("triggered_by", "trigger craft", &format!("{v}:1038-1060"))
        },
        // :1062-1075 真逻辑：mana cost 门控 comparer（KitavaRequiredManaCost）。
        TriggerConfigDef {
            trigger_name: s("Kitava's Thirst"),
            trigger_chance_stat: s("KitavaTriggerChance"),
            handler_id: s("trigger:kitavas_thirst_mana_gate"),
            note: s("源比较器要求 ManaCost > KitavaRequiredManaCost，落 handler"),
            ..base("unique_item", "kitava's thirst", &format!("{v}:1061-1074"))
        },
        // :1076-1083 真逻辑：双手双源（源须**不同**槽位，vendor not slotMatch）。
        TriggerConfigDef {
            source_skill_cond: cond(&["Damage", "Attack"], &["Mace", "Weapon1H"], &[]),
            handler_id: s("trigger:mjolner_dual_source"),
            note: s("vendor band(bor(Mace,1H)) 任一位命中 + 非同槽源约束，落 handler"),
            ..base("unique_item", "mjolner", &format!("{v}:1075-1082"))
        },
        // :1084-1089 Melee + 单手剑源；被触发 = 同槽 triggeredByCospris。
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &["Sword", "Weapon1H"], &[]),
            note: s("vendor band(bor(Sword,1H)) 字面任一位命中，按条目意图转写 all-of"),
            ..base("unique_item", "cospri's malice", &format!("{v}:1083-1088"))
        },
        // :1090-1093 CoC：攻击源（同槽），折源暴击率（triggerOnCrit）。
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
        // :1094-1105 on-melee-kill：需 KilledRecently，否则退化自施法。
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
        // :1106-1113 Holy Relic Nova：minion actor 真逻辑。
        TriggerConfigDef {
            trigger_name: s("Summon Holy Relic"),
            source_skill_cond: cond(&["Attack"], &[], &[]),
            handler_id: s("trigger:holy_relic_minion_actor"),
            note: s("actor = env.minion（召唤物触发域），M5a minion 框架后接"),
            ..base("skill", "nova", &format!("{v}:1105-1112"))
        },
        // :1114-1128 CWDT：global + 源自身；阈值输出 CWDTThreshold（面板信息）。
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
        // :1129-1133 触发几率 = skillData.chanceToTriggerOnStun。
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
        // :1134-1153 被触发侧：源 = Automation 宝石（按名），不受帧取整。
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Automation"),
            note: s("主技能 = Automation 自身时 vendor 走 global+self 分支（自速率）"),
            ..base("triggered_by", "automation", &format!("{v}:1133-1152"))
        },
        // :1154-1169 被触发侧：源 = Spellslinger（按名）；源侧 = Wand 攻击。
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            source_skill_name: s("Spellslinger"),
            note: s("主技能 = Spellslinger 自身时源谓词为 Wand 攻击（weaponTypes 判定）"),
            ..base("triggered_by", "spellslinger", &format!("{v}:1153-1168"))
        },
        // :1170-1182 同 Automation 形态。
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Call to Arms"),
            ..base("triggered_by", "call to arms", &format!("{v}:1169-1181"))
        },
        // :1183-1195 同 Automation 形态。
        TriggerConfigDef {
            trigger_on_use: true,
            use_cast_rate: true,
            source_rate_is_final: true,
            ignores_tick_rate: true,
            source_skill_name: s("Autoexertion"),
            ..base("triggered_by", "autoexertion", &format!("{v}:1182-1194"))
        },
        // :1196-1198 攻击源。
        TriggerConfigDef {
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("triggered_by", "mark on hit", &format!("{v}:1195-1197"))
        },
        // :1199-1204 攻击源（同槽）。
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("triggered_by", "hextouch", &format!("{v}:1198-1203"))
        },
        // :1205-1210 攻击源。
        TriggerConfigDef {
            source_rate_is_final: true,
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("unique_item", "oskarm", &format!("{v}:1204-1209"))
        },
        // :1211-1214 global + 源自身。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            ..base("skill", "tempest shield", &format!("{v}:1210-1213"))
        },
        // :1215-1230 真逻辑：duration 作伪冷却（需自身子计算 Duration）。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            handler_id: s("trigger:shattershard_duration_cd"),
            note: s("triggerRateCapOverride = 1/Duration（取自自身子计算输出）"),
            ..base("skill", "shattershard", &format!("{v}:1214-1229"))
        },
        // :1231-1241 真逻辑：BattlemageUpTimeRatio comparer。
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            handler_id: s("trigger:battlemage_uptime_comparer"),
            ..base("skill", "battlemage's cry", &format!("{v}:1230-1240"))
        },
        // :1242-1261 真逻辑：brand 激活频率（repeatFrequency × 频率乘区）。
        TriggerConfigDef {
            ignores_tick_rate: true,
            source_rate_is_final: true,
            source_skill_name: s("Arcanist Brand"),
            handler_id: s("trigger:arcanist_brand_activation_freq"),
            ..base("triggered_by", "arcanist brand", &format!("{v}:1241-1260"))
        },
        // :1262-1266 死亡触发：global，DPS 仅信息展示。
        TriggerConfigDef {
            global_trigger: true,
            source_is_self: true,
            note: s("vendor 仅置 triggered + infoMessage（on-death 无稳态 DPS 语义）"),
            ..base("triggered_by", "cast on death", &format!("{v}:1261-1265"))
        },
        // :1267-1274 真逻辑：InfernalUpTimeRatio comparer。
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            handler_id: s("trigger:infernal_uptime_comparer"),
            ..base("skill", "combust", &format!("{v}:1266-1273"))
        },
        // :1275-1277 攻击源（同槽）。
        TriggerConfigDef {
            source_skill_cond: cond(&["Attack"], &[], &[]),
            ..base("skill", "prismatic burst", &format!("{v}:1274-1276"))
        },
        // :1278-1280 Melee 源（同槽）。
        TriggerConfigDef {
            source_skill_cond: cond(&["Melee"], &[], &[]),
            ..base("skill", "shockwave", &format!("{v}:1277-1279"))
        },
        // :1281-1285 弓攻击源。
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
        // :1286-1295 真逻辑：CurseOverlaps 倍增 + config 输入分流。
        TriggerConfigDef {
            use_cast_rate: true,
            ignores_tick_rate: true,
            source_skill_cond: cond(&["Hex"], &[], &[]),
            handler_id: s("trigger:doom_blast_overlaps"),
            ..base("skill", "doom blast", &format!("{v}:1285-1294"))
        },
        // :1296-1298 CWC：customHandler（pobr 已建模 CWCTriggerTime 通道）。
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
        // :1299-1301 头盔 Focus 附魔触发：customHandler。
        TriggerConfigDef {
            handler_id: s("trigger:helmet_focus_handler"),
            ..base("triggered_by", "focus", &format!("{v}:1298-1300"))
        },
        // :1302-1377 真逻辑：Snipe 蓄力层数 / hitTimeMultiplier / 触发列表。
        TriggerConfigDef {
            handler_id: s("trigger:snipe_stages"),
            ..base("triggered_by", "snipe", &format!("{v}:1301-1376"))
        },
        // :1378-1384 真逻辑：TotemLife comparer + ignoreSourceRate。
        TriggerConfigDef {
            ignore_source_rate: true,
            handler_id: s("trigger:avenging_flame_totem_comparer"),
            note: s("源谓词 skillFlags.totem（pobr 无图腾域）+ 图腾血量比较器"),
            ..base("skill", "avenging flame", &format!("{v}:1377-1384"))
        },
        // :1385-1399 链接触发：源速率 = IntuitiveLinkSourceRate stat。
        TriggerConfigDef {
            use_cast_rate: true,
            source_skill_name: s("Intuitive Link"),
            source_rate_stat: s("IntuitiveLinkSourceRate"),
            triggered_skill_cond: cond(&["Spell"], &[], &[]),
            ..base("triggered_by", "intuitive link", &format!("{v}:1385-1399"))
        },
        // :1400-1406 Svalinn 盾：global + 源自身；被触发筛选走 support 适用性。
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
        // :1407-1416 Settlers 附魔：Melee 源触发同槽被附魔技能。
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

/// 从 vendor `Modules/CalcTriggers.lua` 扫描 configTable 的 key 集合
/// （`local configTable = {` 到首个行首 `}` 之间的 `["<key>"] = function` 行）。
fn scan_vendor_config_keys(vendor_root: &Path) -> io::Result<BTreeSet<String>> {
    let path = vendor_root.join("Modules/CalcTriggers.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("无法读取 {}：{error}", path.display()),
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
        // 形如：\t["law of the wilds"] = function()
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

/// key 对账（drift 防线，蓝图 §4.1 T5「61 项抽取计数断言」的生成时刻面）：
/// 内嵌转写 key 集合必须与 vendor 源扫描结果**集合相等**且 = 61。
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
            "trigger configTable key 对账失败（vendor drift，须更新内嵌转写）：\n  vendor 有而转写缺 {missing:?}\n  转写有而 vendor 无 {extra:?}"
        )));
    }
    if vendor_keys.len() != VENDOR_CONFIG_TABLE_COUNT {
        return Err(io::Error::other(format!(
            "trigger configTable 条目数 {} ≠ 预期 {VENDOR_CONFIG_TABLE_COUNT}",
            vendor_keys.len()
        )));
    }
    Ok(vendor_keys.len())
}

/// 生成 `overlay/trigger_configs.json` 文本（byte-stable；先过 key 对账）。
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

/// CalcTriggers.lua 粗粒度指纹：行数 + 字节数。
fn vendor_fingerprint(vendor_root: &Path) -> io::Result<String> {
    let path = vendor_root.join("Modules/CalcTriggers.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("无法读取 {}：{error}", path.display()),
        )
    })?;
    Ok(format!("{}L:{}B", text.lines().count(), text.len()))
}

#[cfg(test)]
mod tests {
    use super::{VENDOR_CONFIG_TABLE_COUNT, builtin_configs};

    /// 61 条、key 唯一且升序（蓝图 §4.1 T5：61 项抽取计数断言的内嵌侧）。
    #[test]
    fn builtin_configs_count_and_order() {
        let configs = builtin_configs();
        assert_eq!(configs.len(), VENDOR_CONFIG_TABLE_COUNT);
        let names: Vec<&str> = configs.iter().map(|c| c.key.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted, "key.name 必须唯一且升序");
    }

    /// handler 条目带 `trigger:` 前缀（20 号 §5 全局台账按前缀分域计数）；
    /// handler 总数计入监控（当前 15 条，全阶段 <100 总闸）。
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
                "handler id {handler} 缺 trigger: 前缀"
            );
        }
        assert_eq!(handlers.len(), 15, "handler 条目数变化须同步监控台账");
        assert!(handlers.len() < 100, "handler 总数超 20 号 §5 监控闸");
    }

    /// 每条都有 key 类别 + vendor 行段锚点；谓词非空字段遵守三字段封顶
    /// （schema 即三字段，此处断言非空谓词必至少约束一个字段）。
    #[test]
    fn entries_have_anchor_and_meaningful_cond() {
        for config in builtin_configs() {
            assert!(
                matches!(
                    config.key.kind.as_str(),
                    "skill" | "triggered_by" | "unique_item"
                ),
                "{} 的 kind 非法：{}",
                config.key.name,
                config.key.kind
            );
            assert!(
                config.vendor_ref.starts_with("Modules/CalcTriggers.lua:"),
                "{} 缺 vendor 行段锚点",
                config.key.name
            );
            for cond in [&config.source_skill_cond, &config.triggered_skill_cond]
                .into_iter()
                .flatten()
            {
                assert!(!cond.is_empty(), "{} 带空谓词", config.key.name);
            }
        }
    }

    /// PoE2 join 键（match_effect_ids）当前映射的 5 个 Meta 触发宝石。
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

    /// CoC 条目方向性事实：trigger_on_crit 置位（W-E2 暴击折入的数据前提）。
    #[test]
    fn coc_entry_folds_crit() {
        let configs = builtin_configs();
        let coc = configs
            .iter()
            .find(|c| c.key.name == "cast on critical strike")
            .expect("缺 CoC 条目");
        assert!(coc.trigger_on_crit);
        assert!(coc.source_skill_cond.is_some());
    }
}
