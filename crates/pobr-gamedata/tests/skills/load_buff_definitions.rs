//! `overlay/buff_definitions.json` load tests.
//!
//! The storage anchor for the hand-curated table: each buff's formula
//! parameters are cross-checked against vendor
//! `CalcPerform.lua doActorMisc` (line numbers in each entry's vendor_ref).

use pobr_data::catalog::buffs::{BuffDef, BuffDefinitionsDef, BuffModValue, Rounding};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> BuffDefinitionsDef {
    GameData::new(repo_data_root().join(version()))
        .buff_definitions()
        .expect("buff_definitions 可加载")
        .expect("仓库数据包含 overlay/buff_definitions.json")
}

/// A missing file (an old data pack) = `Ok(None)`, no error (backward compatible).
#[test]
fn missing_overlay_file_is_none() {
    let dir = std::env::temp_dir().join(format!(
        "pobr-gamedata-buff-defs-missing-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("建临时版本目录");
    let loaded = GameData::new(&dir)
        .buff_definitions()
        .expect("缺文件不应报错");
    assert!(loaded.is_none());
}

fn find<'a>(doc: &'a BuffDefinitionsDef, id: &str) -> &'a BuffDef {
    doc.buffs
        .iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("缺 buff {id}"))
}

/// Sort order + first-batch coverage.
#[test]
fn sorted_and_first_batch_coverage() {
    let doc = load();
    for pair in doc.buffs.windows(2) {
        assert!(pair[0].id <= pair[1].id, "未按 id 排序：{}", pair[1].id);
    }
    for id in [
        "Onslaught",
        "Adrenaline",
        "Convergence",
        "UnholyMight",
        "ChaoticMight",
        "HerEmbrace",
        "Fortify",
        "Fanaticism",
        "Elusive",
    ] {
        find(&doc, id);
    }
}

/// The buff-handler budget ≤8 (B2; currently 4:
/// fortify/fanaticism/elusive/onslaught_flask).
#[test]
fn handler_budget() {
    let doc = load();
    let handlers: Vec<_> = doc
        .buffs
        .iter()
        .filter_map(|b| b.handler_id.as_deref())
        .collect();
    assert!(handlers.len() <= 8, "buff handler 超预算：{handlers:?}");
    assert!(handlers.contains(&"buff:fortify"));
    assert!(handlers.contains(&"buff:onslaught_flask"));
}

/// Onslaught's formula parameters (vendor :614-645's basic shape).
#[test]
fn onslaught_formula() {
    let doc = load();
    let def = find(&doc, "Onslaught");
    assert_eq!(def.trigger_flag, "Onslaught");
    let effect = def.effect.as_ref().expect("有公式");
    assert_eq!(effect.base, 10.0);
    assert_eq!(
        effect.inc_stats,
        vec![
            "OnslaughtEffect".to_string(),
            "BuffEffectOnSelf".to_string()
        ]
    );
    assert_eq!(effect.rounding, Rounding::Floor);
    // Speed INC 2e ×2 (Attack/Cast) + WarcrySpeed 2e + MovementSpeed e
    assert_eq!(def.mods.len(), 4);
    assert_eq!(def.mods[0].value, BuffModValue::PerEffect { coeff: 2.0 });
    assert_eq!(def.mods[3].value, BuffModValue::PerEffect { coeff: 1.0 });
}

/// Adrenaline's per-mod floor (vendor :661-668).
#[test]
fn adrenaline_per_mod_rounding() {
    let doc = load();
    let def = find(&doc, "Adrenaline");
    assert_eq!(def.mods.len(), 5);
    for template in &def.mods {
        match &template.value {
            BuffModValue::ScaledRounded { rounding, .. } => {
                assert_eq!(*rounding, Rounding::Floor);
            }
            other => panic!("Adrenaline mod 应为 scaled_rounded，实际 {other:?}"),
        }
    }
    assert_eq!(
        def.mods[0].value,
        BuffModValue::ScaledRounded {
            coeff: 100.0,
            rounding: Rounding::Floor
        }
    );
}

/// UnholyMight: a Multiplier literal + the 0.3-coefficient per-multiplier
/// term (vendor :652-656).
#[test]
fn unholy_might_shape() {
    let doc = load();
    let def = find(&doc, "UnholyMight");
    assert_eq!(def.mods[0].name, "Multiplier:UnholyMightMagnitude");
    assert_eq!(def.mods[0].value, BuffModValue::Literal { value: 100.0 });
    assert_eq!(def.mods[1].name, "DamageGainAsChaos");
    assert_eq!(
        def.mods[1].value,
        BuffModValue::ScaledRounded {
            coeff: 0.3,
            rounding: Rounding::None
        }
    );
    assert_eq!(def.mods[1].tags.len(), 1);
}

/// vendor_ref completeness: a valid line-number range + hash format +
/// matching filename.
#[test]
fn vendor_refs_well_formed() {
    let doc = load();
    // Allowed vendor line ranges: doActorMisc (CalcPerform :578-850, the
    // main body) + the Arcane Surge injection section (CalcDefence
    // :1606-1617 — vendor writes this buff after doActorLifeManaSpirit and
    // before recovery, an odd-one-out entry).
    let allowed: &[(&str, u32, u32)] = &[
        ("Modules/CalcPerform.lua", 578, 850),
        ("Modules/CalcDefence.lua", 1606, 1617),
    ];
    for buff in &doc.buffs {
        assert!(
            allowed
                .iter()
                .any(|(file, lo, hi)| buff.vendor_ref.file == *file
                    && buff.vendor_ref.line_start >= *lo
                    && buff.vendor_ref.line_end <= *hi),
            "{} 行段 {}:{}–{} 超出允许范围",
            buff.id,
            buff.vendor_ref.file,
            buff.vendor_ref.line_start,
            buff.vendor_ref.line_end
        );
        assert!(
            buff.vendor_ref.segment_hash.starts_with("fnv1a64:")
                && buff.vendor_ref.segment_hash.len() == 8 + 16,
            "{} 行段 hash 格式非法：{}",
            buff.id,
            buff.vendor_ref.segment_hash
        );
    }
}

/// Template entries and handler entries are mutually exclusive (having a
/// handler_id means no mods/effect).
#[test]
fn handler_entries_have_no_templates() {
    let doc = load();
    for buff in &doc.buffs {
        if buff.handler_id.is_some() {
            assert!(
                buff.mods.is_empty() && buff.effect.is_none(),
                "{} 同时携带 handler 与模板",
                buff.id
            );
        } else {
            assert!(!buff.mods.is_empty(), "{} 模板条目无 mods", buff.id);
        }
    }
}
