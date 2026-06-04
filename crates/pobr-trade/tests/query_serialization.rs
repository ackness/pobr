//! `TradeQuery` / `StatFilter` 的 serde 回环（roundtrip）测试。
//! 离线、纯数据，不联网。

use pobr_trade::{StatFilter, TradeQuery};

#[test]
fn trade_query_roundtrips_through_json() {
    let query = TradeQuery {
        league: "Standard".to_string(),
        min_price: Some(1.0),
        max_price: Some(50.0),
        stat_filters: vec![
            StatFilter {
                stat: "life".to_string(),
                min: Some(80.0),
                max: None,
            },
            StatFilter {
                stat: "fire_resistance".to_string(),
                min: Some(30.0),
                max: Some(48.0),
            },
        ],
    };

    let json = serde_json::to_string(&query).expect("serialize");
    let decoded: TradeQuery = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(query, decoded);
}

#[test]
fn trade_query_with_no_price_bounds_roundtrips() {
    let query = TradeQuery {
        league: "Hardcore".to_string(),
        min_price: None,
        max_price: None,
        stat_filters: vec![],
    };

    let json = serde_json::to_string(&query).expect("serialize");
    let decoded: TradeQuery = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(query, decoded);
    assert!(decoded.stat_filters.is_empty());
}

#[test]
fn stat_filter_roundtrips_independently() {
    let filter = StatFilter {
        stat: "movement_speed".to_string(),
        min: Some(20.0),
        max: Some(35.0),
    };

    let json = serde_json::to_string(&filter).expect("serialize");
    let decoded: StatFilter = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(filter, decoded);
}

#[test]
fn trade_query_deserializes_from_explicit_json() {
    let json = r#"{
        "league": "Settlers",
        "min_price": 5.0,
        "max_price": null,
        "stat_filters": [
            { "stat": "chaos_resistance", "min": 10.0, "max": null }
        ]
    }"#;

    let query: TradeQuery = serde_json::from_str(json).expect("deserialize explicit json");

    assert_eq!(query.league, "Settlers");
    assert_eq!(query.min_price, Some(5.0));
    assert_eq!(query.max_price, None);
    assert_eq!(query.stat_filters.len(), 1);
    assert_eq!(query.stat_filters[0].stat, "chaos_resistance");
    assert_eq!(query.stat_filters[0].min, Some(10.0));
}
