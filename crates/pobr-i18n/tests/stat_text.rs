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

// ---------------------------------------------------------------------------
// display_stat_text: DisplayStatId → localized output-stat label
// ---------------------------------------------------------------------------

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
