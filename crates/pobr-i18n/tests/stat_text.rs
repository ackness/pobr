//! `Translator::stat_text` maps stable `StatId`s to localized display names.

use pobr_data::prelude::{StatId, StatTextKey};
use pobr_i18n::{LanguageId, Translator, stat_text_key};

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
