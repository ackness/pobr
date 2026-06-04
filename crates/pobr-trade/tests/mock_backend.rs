//! `MockBackend` 行为测试：search/fetch 返回预置数据；可模拟 RateLimit / NotFound 错误。
//! 离线，不联网。

use pobr_trade::{MockBackend, StatFilter, TradeBackend, TradeError, TradeItem, TradeQuery};

fn sample_items() -> Vec<TradeItem> {
    vec![
        TradeItem {
            id: "item-1".to_string(),
            name: "Tabula Rasa".to_string(),
            price_chaos: 12.0,
        },
        TradeItem {
            id: "item-2".to_string(),
            name: "Goldrim".to_string(),
            price_chaos: 3.5,
        },
    ]
}

fn sample_query() -> TradeQuery {
    TradeQuery {
        league: "Standard".to_string(),
        min_price: None,
        max_price: Some(20.0),
        stat_filters: vec![StatFilter {
            stat: "life".to_string(),
            min: Some(50.0),
            max: None,
        }],
    }
}

#[test]
fn mock_search_returns_preset_ids() {
    let backend = MockBackend::with_items(sample_items());

    let result = backend
        .search("Standard", &sample_query())
        .expect("search should succeed");

    assert_eq!(result.ids, vec!["item-1".to_string(), "item-2".to_string()]);
}

#[test]
fn mock_fetch_returns_items_matching_requested_ids() {
    let backend = MockBackend::with_items(sample_items());

    let items = backend
        .fetch("Standard", &["item-2".to_string()])
        .expect("fetch should succeed");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "item-2");
    assert_eq!(items[0].name, "Goldrim");
    assert_eq!(items[0].price_chaos, 3.5);
}

#[test]
fn mock_fetch_preserves_requested_order() {
    let backend = MockBackend::with_items(sample_items());

    let items = backend
        .fetch("Standard", &["item-2".to_string(), "item-1".to_string()])
        .expect("fetch should succeed");

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "item-2");
    assert_eq!(items[1].id, "item-1");
}

#[test]
fn mock_fetch_skips_unknown_ids() {
    let backend = MockBackend::with_items(sample_items());

    let items = backend
        .fetch(
            "Standard",
            &["does-not-exist".to_string(), "item-1".to_string()],
        )
        .expect("fetch should succeed");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "item-1");
}

#[test]
fn mock_can_simulate_rate_limit_on_search() {
    let backend = MockBackend::with_items(sample_items()).failing(TradeError::RateLimit);

    let err = backend
        .search("Standard", &sample_query())
        .expect_err("should fail with rate limit");

    assert!(matches!(err, TradeError::RateLimit));
}

#[test]
fn mock_can_simulate_not_found_on_fetch() {
    let backend = MockBackend::with_items(sample_items()).failing(TradeError::NotFound);

    let err = backend
        .fetch("Standard", &["item-1".to_string()])
        .expect_err("should fail with not found");

    assert!(matches!(err, TradeError::NotFound));
}

#[test]
fn mock_can_simulate_http_error() {
    let backend =
        MockBackend::with_items(sample_items()).failing(TradeError::Http("503".to_string()));

    let err = backend
        .search("Standard", &sample_query())
        .expect_err("should fail with http error");

    match err {
        TradeError::Http(msg) => assert_eq!(msg, "503"),
        other => panic!("expected Http error, got {other:?}"),
    }
}

#[test]
fn mock_default_is_empty_and_succeeds() {
    let backend = MockBackend::default();

    let result = backend
        .search("Standard", &sample_query())
        .expect("empty search succeeds");
    assert!(result.ids.is_empty());

    let items = backend
        .fetch("Standard", &["item-1".to_string()])
        .expect("empty fetch succeeds");
    assert!(items.is_empty());
}

#[test]
fn trade_error_is_displayable() {
    let err = TradeError::Parse("bad json".to_string());
    let rendered = err.to_string();
    assert!(rendered.contains("bad json"));
}
