//! `Translator::stat_text` maps stable `StatId`s to localized display names.
//! `Translator::display_stat_text` maps `DisplayStatId`s to localized labels.

use pobr_data::prelude::{DisplayStatId, StatId, StatTextKey};
use pobr_i18n::{
    DISPLAY_STAT_PREFIX, LanguageId, Translator, display_stat_text_key, stat_text_key,
};

#[test]
fn stat_text_returns_en_us_display_name() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(t.stat_text(&StatId::new("life")), "Life");
    assert_eq!(
        t.stat_text(&StatId::new("fire_resistance")),
        "Fire Resistance"
    );
}

#[test]
fn stat_text_returns_translated_name_for_zh_tw() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    assert_eq!(t.stat_text(&StatId::new("life")), "生命");
    assert_eq!(t.stat_text(&StatId::new("mana")), "魔力");
    assert_eq!(t.stat_text(&StatId::new("fire_resistance")), "火焰抗性");
}

#[test]
fn stat_text_falls_back_to_en_us_when_untranslated() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    // `accuracy` exists in en-US stats but is not translated to zh-TW.
    assert_eq!(t.stat_text(&StatId::new("accuracy")), "Accuracy");
}

#[test]
fn stat_text_returns_id_when_unknown() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(t.stat_text(&StatId::new("not_a_stat")), "not_a_stat");
}

#[test]
fn stat_text_key_derives_stat_prefixed_bundle_key() {
    let key = stat_text_key(&StatId::new("life"));
    assert_eq!(key, StatTextKey::new("stat.life"));
    // The associated form on Translator agrees.
    assert_eq!(
        Translator::stat_text_key(&StatId::new("mana")).as_str(),
        "stat.mana"
    );
}

#[test]
fn stat_text_by_key_resolves_like_stat_text() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    let key = stat_text_key(&StatId::new("fire_resistance"));
    assert_eq!(t.stat_text_by_key(&key), "火焰抗性");
    // Untranslated key falls back to en-US.
    let acc = stat_text_key(&StatId::new("accuracy"));
    assert_eq!(t.stat_text_by_key(&acc), "Accuracy");
}

#[test]
fn stat_text_by_key_returns_key_when_unknown() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    let key = StatTextKey::new("stat.not_a_stat");
    assert_eq!(t.stat_text_by_key(&key), "stat.not_a_stat");
}

// display_stat_text: DisplayStatId → localized output-stat label

#[test]
fn display_stat_text_returns_en_us_labels() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("TotalDPS")),
        "Total DPS"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("TotalEHP")),
        "Total Effective Hit Points"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("CritChance")),
        "Critical Strike Chance"
    );
    assert_eq!(t.display_stat_text(&DisplayStatId::new("Life")), "Life");
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("EsRechargeRate")),
        "Energy Shield Recharge Rate"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ManaCost")),
        "Mana Cost"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ChillEffect")),
        "Chill Effect"
    );
}

#[test]
fn display_stat_text_returns_zh_tw_translations() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    // Translated entries.
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("TotalDPS")),
        "總 DPS"
    );
    assert_eq!(t.display_stat_text(&DisplayStatId::new("Life")), "生命");
    assert_eq!(t.display_stat_text(&DisplayStatId::new("Armour")), "護甲");
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("TotalEHP")),
        "有效血量"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ChillEffect")),
        "冰緩效果"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ManaCost")),
        "魔力消耗"
    );
}

#[test]
fn display_stat_text_falls_back_to_en_us_for_untranslated_zh_tw() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    // EsRechargeRate is in en-US but not zh-TW; should fall back.
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("EsRechargeRate")),
        "Energy Shield Recharge Rate"
    );
    // ActionRate not translated to zh-TW.
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ActionRate")),
        "Action Rate"
    );
}

#[test]
fn display_stat_text_returns_id_when_unknown() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    // An unmapped id returns the PascalCase id itself.
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("UnknownStat")),
        "UnknownStat"
    );
}

#[test]
fn display_stat_text_key_derives_display_prefixed_key() {
    let key = display_stat_text_key(&DisplayStatId::new("TotalDPS"));
    assert_eq!(key, StatTextKey::new("display.TotalDPS"));
    // Prefix constant matches.
    assert_eq!(DISPLAY_STAT_PREFIX, "display");
    // Associated method on Translator agrees.
    assert_eq!(
        Translator::display_stat_text_key(&DisplayStatId::new("CritChance")).as_str(),
        "display.CritChance"
    );
}

#[test]
fn display_stat_text_wave2_charge_labels() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ChargePowerCurrent")),
        "Power Charges"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ChargeFrenzyMaximum")),
        "Maximum Frenzy Charges"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("ChargeEnduranceCurrent")),
        "Endurance Charges"
    );
}

#[test]
fn display_stat_text_wave2_avoidance_labels() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("AvoidIgnite")),
        "Avoid Ignite"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("AvoidFreeze")),
        "Avoid Freeze"
    );
    assert_eq!(
        t.display_stat_text(&DisplayStatId::new("AvoidBleeding")),
        "Avoid Bleeding"
    );
    // zh-TW translations
    let zh = Translator::new(LanguageId::new("zh-TW")).unwrap();
    assert_eq!(
        zh.display_stat_text(&DisplayStatId::new("AvoidIgnite")),
        "規避點燃"
    );
    assert_eq!(
        zh.display_stat_text(&DisplayStatId::new("AvoidFreeze")),
        "規避冰凍"
    );
}

#[test]
fn sidebar_extensions_have_both_labels() {
    let en = Translator::new(LanguageId::new("en-US")).unwrap();
    let zh = Translator::new(LanguageId::new("zh-TW")).unwrap();
    for (id, english, chinese) in [
        ("AverageDamage", "Average Damage", "平均傷害"),
        ("AverageBurstDamage", "Average Burst Damage", "平均爆發傷害"),
        ("CombinedAvg", "Combined Average", "合計平均傷害"),
        ("HitSpeed", "Hit Rate", "命中速率"),
        ("IgniteDamage", "Ignite Damage", "點燃傷害"),
        ("BleedDamage", "Bleed Damage", "流血傷害"),
        ("PoisonDamage", "Poison Damage", "中毒傷害"),
        ("WithIgniteDPS", "DPS including Ignite", "包含點燃 DPS"),
        ("WithBleedDPS", "DPS including Bleed", "包含流血 DPS"),
        ("WithPoisonDPS", "DPS including Poison", "包含中毒 DPS"),
        ("MainHandAccuracy", "Main Hand Accuracy", "主手命中值"),
        ("OffHandAccuracy", "Off Hand Accuracy", "副手命中值"),
        (
            "AccuracyHitChanceUncapped",
            "Uncapped Chance to Hit",
            "無上限命中率",
        ),
        ("QuantityMultiplier", "Quantity Multiplier", "數量倍率"),
        ("Duration", "Skill Duration (s)", "技能持續時間（秒）"),
        (
            "DurationSecondary",
            "Secondary Duration (s)",
            "次要持續時間（秒）",
        ),
        ("AuraDuration", "Aura Duration (s)", "光環持續時間（秒）"),
        (
            "ReserveDuration",
            "Reservation Duration (s)",
            "保留持續時間（秒）",
        ),
        (
            "AreaOfEffectRadiusMetres",
            "Area Radius (m)",
            "範圍半徑（公尺）",
        ),
        ("PierceCount", "Pierce Count", "穿透次數"),
        ("ForkCount", "Fork Count", "分岔次數"),
        ("ChainMax", "Maximum Chains", "最大連鎖次數"),
        ("SplitCount", "Split Count", "分裂數量"),
        ("BounceCount", "Bounce Count", "彈跳次數"),
        ("ReqStr", "Strength Required", "力量需求"),
        ("ReqDex", "Dexterity Required", "敏捷需求"),
        ("ReqInt", "Intelligence Required", "智慧需求"),
        ("ManaPercentCost", "Mana Cost (%)", "魔力消耗（%）"),
        (
            "ManaPercentPerSecondCost",
            "Mana Cost (%/s)",
            "魔力每秒消耗（%）",
        ),
        ("ManaPerSecondCost", "Mana Cost / s", "魔力每秒消耗"),
        ("LifePercentCost", "Life Cost (%)", "生命消耗（%）"),
        (
            "LifePercentPerSecondCost",
            "Life Cost (%/s)",
            "生命每秒消耗（%）",
        ),
        ("LifePerSecondCost", "Life Cost / s", "生命每秒消耗"),
        ("ESCost", "Energy Shield Cost", "能量護盾消耗"),
        (
            "ESPercentCost",
            "Energy Shield Cost (%)",
            "能量護盾消耗（%）",
        ),
        (
            "ESPercentPerSecondCost",
            "Energy Shield Cost (%/s)",
            "能量護盾每秒消耗（%）",
        ),
        (
            "ESPerSecondCost",
            "Energy Shield Cost / s",
            "能量護盾每秒消耗",
        ),
        ("WardCost", "Runic Ward Cost", "符文守護消耗"),
        (
            "WardPercentCost",
            "Runic Ward Cost (%)",
            "符文守護消耗（%）",
        ),
        (
            "WardPercentPerSecondCost",
            "Runic Ward Cost (%/s)",
            "符文守護每秒消耗（%）",
        ),
        (
            "WardPerSecondCost",
            "Runic Ward Cost / s",
            "符文守護每秒消耗",
        ),
        ("RageCost", "Rage Cost", "盛怒消耗"),
        ("RagePerSecondCost", "Rage Cost / s", "盛怒每秒消耗"),
        (
            "TotalTotemPlacementTime",
            "Totem Placement Time (s)",
            "圖騰放置時間（秒）",
        ),
        (
            "TrapThrowingTime",
            "Trap Throwing Time (s)",
            "陷阱投擲時間（秒）",
        ),
        ("TrapThrowCount", "Traps per Throw", "每次投擲陷阱數"),
        (
            "MineLayingTime",
            "Mine Throwing Time (s)",
            "地雷投擲時間（秒）",
        ),
        ("MineThrowCount", "Mines per Throw", "每次投擲地雷數"),
        ("WarcryCastTime", "Warcry Time (s)", "戰吼施放時間（秒）"),
        ("ChannelTime", "Channel Time (s)", "引導時間（秒）"),
        ("Str", "Strength", "力量"),
        ("Dex", "Dexterity", "敏捷"),
        ("Int", "Intelligence", "智慧"),
        ("Rage", "Rage", "盛怒"),
        (
            "FireResistOverCap",
            "Fire Res. Over Max",
            "超過上限火焰抗性",
        ),
        (
            "ColdResistOverCap",
            "Cold Res. Over Max",
            "超過上限冰冷抗性",
        ),
        (
            "LightningResistOverCap",
            "Lightning Res. Over Max",
            "超過上限閃電抗性",
        ),
        (
            "ChaosResistOverCap",
            "Chaos Res. Over Max",
            "超過上限混沌抗性",
        ),
        ("LifeUnreservedPercent", "Unreserved Life", "未保留生命"),
        ("ManaUnreservedPercent", "Unreserved Mana", "未保留魔力"),
        ("SpiritUnreservedPercent", "Unreserved Spirit", "未保留精魂"),
        (
            "PreEffectiveCritChance",
            "Crit Chance before Accuracy",
            "命中確認前暴擊率",
        ),
        ("StoredUses", "Stored Uses", "儲存使用次數"),
        ("TotalDot", "Skill DoT DPS", "技能持續傷害 DPS"),
        (
            "WithDotDPS",
            "Hit and Skill DoT DPS",
            "擊中與技能持續傷害 DPS",
        ),
        ("FiringRate", "Firing Rate", "射擊速率"),
        ("ReloadTime", "Reload Time (s)", "裝填時間（秒）"),
        ("CullingDPS", "Culling DPS", "撲殺 DPS"),
        (
            "ProjectileSpeedMod",
            "Projectile Speed Multiplier",
            "投射物速度倍率",
        ),
    ] {
        let id = DisplayStatId::new(id);
        assert_eq!(en.display_stat_text(&id), english);
        assert_eq!(zh.display_stat_text(&id), chinese);
    }
}
