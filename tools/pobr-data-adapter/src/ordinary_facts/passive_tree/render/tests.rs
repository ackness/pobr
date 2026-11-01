use super::*;
const GLOBAL: &str = "Data/StatDescriptions/stat_descriptions.csd";
fn descriptor(ids: &[&str], text: &str) -> Value {
    json!({"kind":"description","ids":ids,"raw_slice_sha256":"first","groups":[{
        "language_origin":"implicit-default","declared_language":null,"branches":[{
            "condition_tokens_raw":ids.iter().map(|_|"#").collect::<Vec<_>>(),"template_raw":text,
            "template_syntax_diagnostics":[],"quantifiers_lexical":[],"quantifier_tokens_raw":[],"restrictions_raw":[],"span":{}}]}]})
}
fn doc(records: Vec<Value>) -> Value {
    json!({"files":{GLOBAL:{"path":GLOBAL,"records":records}}})
}
#[test]
fn numeric_units_match_pinned_reference_for_signed_limits_rounding_and_zero() {
    let fixture: Value = serde_json::from_str(include_str!("number-cases.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut n = Number::new(case["value"].as_i64().unwrap());
        n.transform(case["handler"].as_str().unwrap()).unwrap();
        assert_eq!(n.display(false).unwrap(), case["expected"], "{case}");
    }
}
#[test]
fn numeric_transforms_preserve_limits_reject_unknowns_and_bad_indices() {
    let mut n = Number::new(i64::MIN);
    assert!(n.transform("negate").is_err());
    let mut n = Number::new(i64::MAX);
    n.transform("milliseconds_to_seconds").unwrap();
    assert!(n.display(false).is_err());
    for q in [
        json!({"name_raw":"unknown","argument_tokens_raw":["1"]}),
        json!({"name_raw":"canonical_line","argument_tokens_raw":["1"]}),
        json!({"name_raw":"negate","argument_tokens_raw":["0"]}),
        json!({"name_raw":"negate","argument_tokens_raw":["2"]}),
    ] {
        assert!(number::apply(&mut [Number::new(7)], &[q]).is_err());
    }
}
#[test]
fn unit_formats_and_canonical_line_keep_raw_evidence_and_change_with_source() {
    let mut d = descriptor(&["duration"], "Lasts {0} seconds");
    d["groups"][0]["branches"][0]["quantifiers_lexical"] = json!([
        {"name_raw":"milliseconds_to_seconds_1dp","argument_tokens_raw":["1"]},
        {"name_raw":"canonical_line","argument_tokens_raw":[]}]);
    d["groups"][0]["branches"][0]["quantifier_tokens_raw"] =
        json!(["milliseconds_to_seconds_1dp", "1", "canonical_line"]);
    let d = doc(vec![d]);
    let r = Renderer::global(&d).unwrap();
    for (v, text, number) in [
        (1500, "Lasts 1.5 seconds", "1.5"),
        (2000, "Lasts 2.0 seconds", "2.0"),
    ] {
        let (lines, evidence) = r.render(&[("duration".into(), v)]).unwrap();
        assert_eq!(lines, vec![text]);
        assert_eq!(evidence[0]["values"], json!([v]));
        assert_eq!(evidence[0]["formatted_values"], json!([number]));
    }
}
#[test]
fn identical_english_owners_retain_all_sources_but_conflicts_fail() {
    let a = descriptor(&["a", "b"], "{0} and {1}");
    let mut b = a.clone();
    b["span"] = json!({"line_start":99});
    b["raw_slice_sha256"] = json!("second");
    b["groups"][0]["branches"][0]["span"] = json!({"line_start":100});
    b["groups"]
        .as_array_mut()
        .unwrap()
        .push(json!({"language_origin":"explicit","declared_language":"Other","branches":[]}));
    let stats = vec![("a".into(), 2), ("b".into(), 3)];
    let d = doc(vec![a.clone(), b.clone()]);
    let (lines, e) = Renderer::global(&d).unwrap().render(&stats).unwrap();
    assert_eq!(lines, vec!["2 and 3"]);
    assert_eq!(
        e[0]["equivalent_english_descriptors"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        e[0]["equivalent_english_descriptors"][1]["raw_slice_sha256"],
        "second"
    );
    for key in [
        "template_raw",
        "condition_tokens_raw",
        "quantifier_tokens_raw",
        "restrictions_raw",
        "unknown_semantics",
    ] {
        let mut changed = b.clone();
        changed["groups"][0]["branches"][0][key] = json!("changed");
        let d = doc(vec![a.clone(), changed]);
        assert!(
            Renderer::global(&d)
                .unwrap()
                .render(&stats)
                .unwrap_err()
                .contains("ambiguous")
        );
    }
}
#[test]
fn only_known_inapplicable_quality_restriction_is_skipped() {
    let mut r = descriptor(&["a"], "{0} quality");
    let mut normal = r["groups"][0]["branches"][0].clone();
    r["groups"][0]["branches"][0]["restrictions_raw"] = json!(["gem_quality"]);
    normal["template_raw"] = json!("{0} ordinary");
    r["groups"][0]["branches"]
        .as_array_mut()
        .unwrap()
        .push(normal);
    let d = doc(vec![r.clone()]);
    let (lines, e) = Renderer::global(&d)
        .unwrap()
        .render(&[("a".into(), 7)])
        .unwrap();
    assert_eq!(lines, vec!["7 ordinary"]);
    assert_eq!(e[0]["branch"], 1);
    r["groups"][0]["branches"][0]["restrictions_raw"] = json!(["unknown"]);
    assert!(
        Renderer::global(&doc(vec![r]))
            .unwrap()
            .render(&[("a".into(), 7)])
            .is_err()
    );
}
#[test]
fn duplicates_do_not_supply_absent_compound_members_or_erase_no_description() {
    let a = descriptor(&["a", "b"], "{0} and {1}");
    let d = doc(vec![a.clone(), a]);
    assert!(
        Renderer::global(&d)
            .unwrap()
            .render(&[("a".into(), 1)])
            .unwrap_err()
            .contains("incomplete compound")
    );
    let a = json!({"kind":"no_description","ids":["a"]});
    let d = doc(vec![a.clone(), a]);
    assert!(
        Renderer::global(&d)
            .unwrap()
            .render(&[("a".into(), 1)])
            .is_err()
    );
    let d = doc(vec![descriptor(&["a", "a"], "duplicate")]);
    assert!(Renderer::global(&d).is_err());
}

#[test]
fn later_numeric_handler_owns_format_but_preserves_the_scaled_value() {
    let mut values = [Number::new(2000)];
    number::apply(
        &mut values,
        &[
            json!({"name_raw":"milliseconds_to_seconds_1dp","argument_tokens_raw":["1"]}),
            json!({"name_raw":"negate","argument_tokens_raw":["1"]}),
        ],
    )
    .unwrap();
    assert_eq!(values[0].display(false).unwrap(), "-2");
}

#[test]
fn implicit_placeholders_exclusions_table_only_and_compound_order_are_source_semantics() {
    let mut a = descriptor(&["a", "b"], "{1:+d} then {} then {0:-d} at 10%%");
    a["groups"][0]["branches"][0]["template_raw"] = json!("{} then {:+d} at 10%%");
    a["groups"][0]["branches"][0]["condition_tokens_raw"] = json!(["!0", "#"]);
    let mut table = a["groups"][0]["branches"][0].clone();
    table["restrictions_raw"] = json!(["table_only"]);
    table["template_raw"] = json!("table@{0}");
    a["groups"][0]["branches"]
        .as_array_mut()
        .unwrap()
        .insert(0, table);
    let d = doc(vec![a]);
    let renderer = Renderer::global(&d).unwrap();
    let (lines, evidence) = renderer
        .render(&[("b".into(), 3), ("a".into(), 2)])
        .unwrap();
    assert_eq!(lines, vec!["2 then +3 at 10%"]);
    assert_eq!(evidence[0]["branch"], 1);
    assert!(
        renderer
            .render(&[("a".into(), 0), ("b".into(), 3)])
            .unwrap_err()
            .contains("no CSD branch")
    );
    assert!(
        renderer
            .render(&[("a".into(), 2)])
            .unwrap_err()
            .contains("incomplete")
    );
    let (catalog, _) = renderer.catalog("a").unwrap();
    assert_eq!(catalog.branches[0].template, "{0} then {1:+d} at 10%");
    assert_eq!(catalog.branches[0].conditions[0].not, Some(0));
}

#[test]
fn offsets_and_composed_units_are_exact_and_unknowns_remain_errors() {
    let mut number = Number::new(50);
    number.transform("plus_two_hundred").unwrap();
    number.transform("divide_by_one_hundred_2dp").unwrap();
    assert_eq!(number.display(false).unwrap(), "2.50");
    assert_eq!(number.format.offset, 200);
    assert_eq!(number.format.divisor, 100);
    let mut number = Number::new(-1550);
    number.transform("milliseconds_to_seconds_0dp").unwrap();
    assert_eq!(number.display(false).unwrap(), "-1");
    assert!(number.transform("passive_hash").is_err());
}

#[test]
fn client_string_arguments_and_escaped_braces_use_one_template_parser() {
    assert_eq!(
        format_template("Grants {0} Passive Skill Points", &["100"]).unwrap(),
        "Grants 100 Passive Skill Points"
    );
    assert_eq!(
        format_template("Grants Skill: <underline>{{{0}}}", &["Fire Spell on Hit"]).unwrap(),
        "Grants Skill: <underline>{Fire Spell on Hit}"
    );
    assert_eq!(
        format_template("{} {:+d}", &["raw{unparsed}", "-3"]).unwrap(),
        "raw{unparsed} -3"
    );
    assert!(format_template("{0:unknown}", &["100"]).is_err());
    assert!(format_template("{0:d}", &["Name"]).is_err());
    assert!(format_template("{1}", &["Name"]).is_err());
}

#[test]
fn supplied_relational_names_preserve_signed_keys_and_never_fall_back_to_numeric_ids() {
    let mut d = descriptor(&["key", "amount"], "{1} for {0}");
    d["groups"][0]["branches"][0]["quantifier_tokens_raw"] = json!(["passive_hash", "1"]);
    d["groups"][0]["branches"][0]["quantifiers_lexical"] =
        json!([{"name_raw":"passive_hash","argument_tokens_raw":["1"]}]);
    let d = doc(vec![d]);
    let values = BTreeMap::from([(
        "passive_hash".into(),
        BTreeMap::from([(-32768, "Named passive".into())]),
    )]);
    let renderer = Renderer::global(&d).unwrap().with_lookup_values(&values);
    assert_eq!(
        renderer
            .render(&[("key".into(), -32768), ("amount".into(), 5)])
            .unwrap()
            .0,
        vec!["5 for Named passive"]
    );
    assert!(
        renderer
            .render(&[("key".into(), 7), ("amount".into(), 5)])
            .unwrap_err()
            .contains("unknown CSD lookup key")
    );
    assert!(
        Renderer::global(&d)
            .unwrap()
            .render(&[("key".into(), -32768), ("amount".into(), 5)])
            .is_err()
    );
    let (catalog, _) = renderer.catalog("key").unwrap();
    assert_eq!(
        catalog.branches[0].lookups,
        vec![Some("passive_hash".into()), None]
    );
    assert_eq!(catalog.branches[0].formats[0], Default::default());
}
