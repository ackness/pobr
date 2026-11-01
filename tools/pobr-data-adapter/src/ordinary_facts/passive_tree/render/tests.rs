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
