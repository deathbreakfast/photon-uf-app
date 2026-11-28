//! Pure backend contracts for the Photon ops UI server surface.
//!
//! DTO shapes and pure mapping/validation helpers that `photon-app` `#[server]` functions
//! call after resolving Higgs and Photon request context. Keeps topic, subscription, event,
//! and dashboard contracts unit-testable without a Leptos host or UI graph.
//!
//! ## Features
//!
//! - **Id validation** — Validates topic names, subscription ids, and event ids so blank,
//!   oversized, or path-unsafe values fail closed before registry or store lookups.
//!   [Get started](#validate-ids)
//! - **Topic/subscription/event mapping** — Builds UI DTOs from registry rows, admin snapshot
//!   handlers, and transport events without performing Photon IO.
//!   [Get started](#map-topic-subscription-event)
//! - **Dashboard aggregates** — Provides KPI counters for topics, subscriptions, and 24-hour
//!   event windows via [`dashboard_stats`] and [`count_since`]. [Get started](#dashboard-kpis)
//! - **Ops path encoding** — Builds percent-encoded path segments for `/photon` hrefs via
//!   [`encode_ops_path_segment`], [`photon_topic_path`], [`photon_subscription_path`], and
//!   [`photon_event_path`].
//! - **Event list limits** — Caps `get_events` / `get_recent_events` list size with
//!   [`clamp_event_list_limit`] and [`MAX_EVENT_LIST_LIMIT`].
//! - **Ops Photon IO** (`ops` feature) — Loads dashboard / topic / subscription / event
//!   DTOs from a live `photon::Photon` handle via the `ops` module.
//!
//! ## Validate ids
//!
//! Id validation checks path and query parameters before they reach Photon IO, so blank or
//! path-unsafe values fail closed instead of breaking routing. [`validate_topic_name`],
//! [`validate_subscription_id`], and [`validate_event_id`] run in `photon-app` server
//! functions ahead of registry or store lookups — call them in custom wrappers when you add
//! new read paths that accept path or query parameters.
//!
//! **Prerequisites:** None beyond importing this crate; validators are synchronous and
//! return [`Result<(), PhotonIdError>`].
//!
//! ```rust,ignore
//! use photon_backend::{
//!     validate_topic_name, validate_subscription_id, validate_event_id, PhotonIdError,
//! };
//!
//! validate_topic_name("orders").expect("valid topic");
//! assert_eq!(
//!     validate_topic_name("").unwrap_err(),
//!     PhotonIdError::EmptyTopicName
//! );
//! validate_subscription_id("reg.key").expect("valid subscription");
//! validate_event_id("ev-1").expect("valid event");
//! ```
//!
//! On success validators return `Ok(())` and the trimmed id is safe for lookup. Blank,
//! oversized, control-character, slash, backslash, or `.` / `..` names map to typed
//! [`PhotonIdError`] variants with operator-facing messages.
//!
//! ## Map topic subscription event
//!
//! Topic/subscription/event mapping turns Photon registry rows and transport events into
//! serde-friendly DTOs the UI can render without touching Photon internals.
//! [`topic_summary`] and [`find_topic_by_name`] back topic list/detail pages;
//! [`subscription_summary_from_handler`] and [`filter_subscriptions_by_topic`] shape
//! subscription tables; [`event_summary_from_transport`] builds list-row previews with
//! `[stored]` / delivery-status chips. Call these after you already hold registry or
//! transport rows in memory — typically inside `photon-app` `#[server]` handlers that
//! assemble list or detail responses.
//!
//! **Prerequisites:** Caller already loaded registry topics, admin snapshot handlers, or
//! transport events from Photon — these functions do not perform IO.
//!
//! ```rust,ignore
//! use photon_backend::{
//!     topic_summary, subscription_summary_from_handler, event_summary_from_transport,
//!     find_topic_by_name, TopicSummary,
//! };
//!
//! let topic = topic_summary("orders", None, "{}", 5, 2);
//! assert_eq!(topic.topic_name, "orders");
//!
//! let sub = subscription_summary_from_handler(
//!     "reg.key",
//!     Some("orders.sub".into()),
//!     "orders",
//!     "durable",
//!     Some(42),
//!     Some(50),
//! );
//! assert_eq!(sub.subscription_id, "reg.key");
//! assert_eq!(sub.checkpoint_lag, 8);
//!
//! let row = event_summary_from_transport("e1", "orders", None, 1, "2026-01-01T00:00:00Z");
//! assert_eq!(row.payload_preview, "[stored]");
//!
//! let topics = vec![topic];
//! assert_eq!(find_topic_by_name(&topics, "orders").map(|t| t.topic_name.as_str()), Some("orders"));
//! ```
//!
//! On success helpers return populated [`TopicSummary`], [`SubscriptionSummary`], or
//! [`EventSummary`] rows ready for JSON serialization. Lookup helpers return `None` when
//! the id or name is absent from the caller-provided slice.
//!
//! ## Dashboard KPIs
//!
//! Dashboard aggregates package registry size and recent event volume into a single
//! [`DashboardStats`] value for the ops landing page, without UI-specific formatting.
//! [`dashboard_stats`] takes topic, subscription, and 24-hour event counts;
//! [`count_since`] filters timestamp slices for per-topic 24h windows after the caller
//! loads event timestamps from Photon. Call this when a dashboard server function has
//! already counted those slices from Photon.
//!
//! **Prerequisites:** Caller supplies counts from registry and event store queries — these
//! helpers do not call Photon.
//!
//! ```rust,ignore
//! use photon_backend::{dashboard_stats, count_since, DashboardStats};
//! use chrono::{DateTime, Utc};
//!
//! let stats: DashboardStats = dashboard_stats(3, 5, 12);
//! assert_eq!(stats.topic_count, 3);
//! assert_eq!(stats.subscription_count, 5);
//! assert_eq!(stats.event_count_24h, 12);
//!
//! let since = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
//!     .unwrap()
//!     .with_timezone(&Utc);
//! assert_eq!(count_since(&[since], since), 1);
//! ```
//!
//! On success `stats` carries the three KPI fields consumed by `photon-app` dashboard
//! server functions and `count_since` returns how many timestamps fall on or after the cutoff.
//!
//! ## Examples
//!
//! Start with [Validate ids](#validate-ids). This crate's unit and integ suites are listed in
//! `docs/VERIFICATION.md`. Runnable host: `examples/protected-photon-host` (auth + dashboard KPIs).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod mapping;
#[cfg(feature = "ops")]
pub mod ops;
mod types;
mod validate;

pub use mapping::{
    checkpoint_delivery_state, compute_checkpoint_lag, count_since, dashboard_stats,
    event_detail_from_transport, event_detail_transport_expired, event_summary_from_meta,
    event_summary_from_transport, filter_subscriptions_by_topic, find_checkpoint_head_seq,
    find_checkpoint_seq, find_subscription_by_id, find_topic_by_name, format_delivery_preview,
    mask_payload, mask_values, payload_fields, personal_values, published_by, sort_topics_by_name,
    subscription_summary_from_handler, topic_summary, CheckpointRow,
};
pub use types::{
    DashboardStats, DeliveryRow, DeliveryState, EventDetail, EventSummary, EventsPage, EventsQuery,
    EventsRange, PayloadField, PhotonIdError, PublishedBy, RevealReason, RevealRecordRow,
    RevealedFields, SubscriptionSummary, TopicSummary, EVENTS_PAGE_SIZE, MASKED_VALUE,
    MAX_EVENTS_PAGE,
};
pub use validate::{
    clamp_event_list_limit, clamp_events_page, encode_ops_path_segment, events_window,
    normalize_events_search, photon_event_path, photon_subscription_path, photon_topic_path,
    validate_event_id, validate_subscription_id, validate_topic_name, EventsQueryError,
    MAX_EVENTS_RANGE_DAYS, MAX_EVENT_LIST_LIMIT, MAX_PHOTON_ID_CHARS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn sample_topic(name: &str) -> TopicSummary {
        TopicSummary {
            topic_name: name.into(),
            keyed_by: None,
            schema_json: "{}".into(),
            event_count_24h: 0,
            subscription_count: 0,
        }
    }

    fn sample_sub(id: &str, topic: &str) -> SubscriptionSummary {
        SubscriptionSummary {
            subscription_id: id.into(),
            subscription_name: "sub".into(),
            topic_name: topic.into(),
            enabled: true,
            mode: "at_least_once".into(),
            topic_key_filter: None,
            checkpoint_lag: 0,
            last_seq: None,
            last_processed_at: None,
        }
    }

    #[test]
    fn format_delivery_preview_wraps_status_happy_path() {
        assert_eq!(format_delivery_preview("delivered"), "[delivered]");
    }

    #[test]
    fn find_topic_by_name_resolves_exact_happy_path() {
        let topics = vec![sample_topic("alpha"), sample_topic("beta")];
        assert_eq!(
            find_topic_by_name(&topics, "beta").map(|t| t.topic_name.as_str()),
            Some("beta")
        );
    }

    #[test]
    fn find_topic_by_name_unknown_is_none_sad() {
        let topics = vec![sample_topic("alpha")];
        assert!(find_topic_by_name(&topics, "missing").is_none());
    }

    #[test]
    fn find_subscription_by_id_resolves_exact_happy_path() {
        let subs = vec![sample_sub("s1", "t"), sample_sub("s2", "t")];
        assert_eq!(
            find_subscription_by_id(&subs, "s1").map(|s| s.subscription_id.as_str()),
            Some("s1")
        );
    }

    #[test]
    fn find_subscription_by_id_unknown_is_none_sad() {
        let subs = vec![sample_sub("s1", "t")];
        assert!(find_subscription_by_id(&subs, "nope").is_none());
    }

    #[test]
    fn filter_subscriptions_by_topic_happy_path() {
        let subs = vec![
            sample_sub("a", "orders"),
            sample_sub("b", "payments"),
            sample_sub("c", "orders"),
        ];
        let filtered = filter_subscriptions_by_topic(&subs, "orders");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|s| s.topic_name == "orders"));
    }

    #[test]
    fn filter_subscriptions_by_topic_unknown_empty_sad() {
        let subs = vec![sample_sub("a", "orders")];
        assert_eq!(filter_subscriptions_by_topic(&subs, "missing").len(), 0);
    }

    #[test]
    fn count_since_includes_boundary_excludes_older_happy_path() {
        let since = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let older = since - chrono::Duration::hours(1);
        let on_boundary = since;
        let newer = since + chrono::Duration::hours(1);
        assert_eq!(count_since(&[older, on_boundary, newer], since), 2);
    }

    #[test]
    fn validate_topic_name_accepts_non_empty_happy_path() {
        validate_topic_name("orders").expect("non-empty");
        validate_topic_name("  payments  ").expect("trimmed non-empty");
        validate_topic_name("orders.v1").expect("dot in name");
    }

    #[test]
    fn validate_topic_name_rejects_blank_sad() {
        assert_eq!(
            validate_topic_name("").expect_err("blank"),
            PhotonIdError::EmptyTopicName
        );
        assert_eq!(
            validate_topic_name("   ").expect_err("whitespace"),
            PhotonIdError::EmptyTopicName
        );
        assert!(PhotonIdError::EmptyTopicName
            .to_string()
            .contains("required"));
    }

    #[test]
    fn validate_topic_name_rejects_slash_control_dotdot_sad() {
        assert_eq!(
            validate_topic_name("a/b").expect_err("slash"),
            PhotonIdError::UnsafeTopicName
        );
        assert_eq!(
            validate_topic_name("a\\b").expect_err("backslash"),
            PhotonIdError::UnsafeTopicName
        );
        assert_eq!(
            validate_topic_name("a\tb").expect_err("control"),
            PhotonIdError::UnsafeTopicName
        );
        assert_eq!(
            validate_topic_name("..").expect_err("dotdot"),
            PhotonIdError::UnsafeTopicName
        );
        assert_eq!(
            validate_topic_name(".").expect_err("dot"),
            PhotonIdError::UnsafeTopicName
        );
    }

    #[test]
    fn validate_topic_name_rejects_oversized_sad() {
        let oversized: String = "t".repeat(MAX_PHOTON_ID_CHARS + 1);
        assert_eq!(
            validate_topic_name(&oversized).expect_err("too long"),
            PhotonIdError::TopicNameTooLong
        );
    }

    #[test]
    fn validate_subscription_id_rejects_blank_sad() {
        assert_eq!(
            validate_subscription_id("").expect_err("blank"),
            PhotonIdError::EmptySubscriptionId
        );
    }

    #[test]
    fn validate_subscription_id_rejects_slash_sad() {
        assert_eq!(
            validate_subscription_id("reg/key").expect_err("slash"),
            PhotonIdError::UnsafeSubscriptionId
        );
    }

    #[test]
    fn validate_event_id_rejects_blank_sad() {
        assert_eq!(
            validate_event_id("").expect_err("blank"),
            PhotonIdError::EmptyEventId
        );
    }

    #[test]
    fn validate_event_id_rejects_control_sad() {
        assert_eq!(
            validate_event_id("ev\nid").expect_err("control"),
            PhotonIdError::UnsafeEventId
        );
    }

    #[test]
    fn encode_ops_path_segment_encodes_slash_and_space_happy_path() {
        assert_eq!(encode_ops_path_segment("orders"), "orders");
        assert_eq!(encode_ops_path_segment("a/b"), "a%2Fb");
        assert_eq!(encode_ops_path_segment("a b"), "a%20b");
        assert_eq!(encode_ops_path_segment("a\\b"), "a%5Cb");
    }

    #[test]
    fn photon_ops_paths_encode_segments_happy_path() {
        assert_eq!(photon_topic_path("a/b"), "/photon/topics/a%2Fb");
        assert_eq!(
            photon_subscription_path("reg/key"),
            "/photon/subscriptions/reg%2Fkey"
        );
        assert_eq!(photon_event_path("e 1"), "/photon/events/e%201");
    }

    #[test]
    fn dashboard_stats_shape_happy_path() {
        let stats = dashboard_stats(3, 5, 12);
        assert_eq!(stats.topic_count, 3);
        assert_eq!(stats.subscription_count, 5);
        assert_eq!(stats.event_count_24h, 12);
    }

    #[test]
    fn sort_topics_by_name_orders_lexicographically_happy_path() {
        let mut topics = vec![sample_topic("zeta"), sample_topic("alpha")];
        sort_topics_by_name(&mut topics);
        assert_eq!(topics[0].topic_name, "alpha");
        assert_eq!(topics[1].topic_name, "zeta");
    }

    #[test]
    fn event_summary_from_meta_preview_happy_path() {
        let row = event_summary_from_meta("e1", "t", None, 1, "2026-01-01T00:00:00Z", "delivered");
        assert_eq!(row.payload_preview, "[delivered]");
        assert_eq!(row.event_id, "e1");
    }

    #[test]
    fn event_detail_transport_expired_marks_null_payload_happy_path() {
        let detail =
            event_detail_transport_expired("e1", "t", None, 1, "2026-01-01T00:00:00Z", "delivered");
        assert!(detail.transport_expired);
        assert!(detail.payload_json.is_null());
        assert_eq!(detail.published_by, PublishedBy::Anonymous);
        assert!(!detail.has_personal());
    }

    #[test]
    fn event_detail_masks_personal_fields_and_actor_happy_path() {
        let payload = serde_json::json!({
            "order_id": "ord-1",
            "customer_email": "ines@example.com",
            "total_cents": 4200,
        });
        let actor = serde_json::json!({"User": {"user_id": "ines"}});
        let detail = event_detail_from_transport(
            "e1",
            "orders",
            Some("ord-1".into()),
            3,
            "2026-01-01T00:00:00Z",
            &payload,
            &actor,
            &["customer_email"],
        );
        let wire = serde_json::to_string(&detail).expect("serialize");
        assert!(
            !wire.contains("ines@example.com"),
            "personal value leaked: {wire}"
        );
        assert!(!wire.contains("\"ines\""), "publisher id leaked: {wire}");
        assert_eq!(detail.payload_json["customer_email"], MASKED_VALUE);
        assert_eq!(detail.payload_json["total_cents"], 4200);
        assert_eq!(detail.published_by, PublishedBy::Person);
        assert!(detail.has_personal());
        let email = detail
            .fields
            .iter()
            .find(|f| f.name == "customer_email")
            .expect("email row");
        assert!(email.personal && email.value.is_none());
        let total = detail
            .fields
            .iter()
            .find(|f| f.name == "total_cents")
            .expect("total");
        assert_eq!(total.value.as_deref(), Some("4200"));
    }

    #[test]
    fn payload_fields_reveal_shows_personal_values() {
        let payload = serde_json::json!({"customer_email": "a@b.c", "n": 1});
        let rows = payload_fields(&payload, &["customer_email"], true);
        assert_eq!(rows[0].value.as_deref(), Some("a@b.c"));
        assert!(rows[0].personal);
        assert_eq!(payload_fields(&serde_json::Value::Null, &[], false), vec![]);
        let scalar = payload_fields(&serde_json::json!(7), &[], false);
        assert_eq!(scalar[0].name, "value");
    }

    #[test]
    fn handler_errors_lose_personal_values() {
        let payload = serde_json::json!({"customer_email": "a@b.c", "n": 7});
        let actor = serde_json::json!({"User": {"user_id": "ines"}});
        let values = personal_values(&payload, &actor, &["customer_email"]);
        assert_eq!(values, vec!["a@b.c".to_string(), "ines".to_string()]);
        let masked = mask_values("invalid email a@b.c for ines", &values);
        assert_eq!(
            masked,
            format!("invalid email {MASKED_VALUE} for {MASKED_VALUE}")
        );
        assert_eq!(mask_values("no match", &values), "no match");
    }

    #[test]
    fn published_by_reads_valence_actor_shapes() {
        let person = published_by(&serde_json::json!({"User": {"user_id": "u1"}}));
        assert_eq!(person, (PublishedBy::Person, Some("u1".into())));
        let system = published_by(&serde_json::json!({"System": {"operation": "job"}}));
        assert_eq!(system.0, PublishedBy::System);
        let bot = published_by(&serde_json::json!({"ServiceUser": {"service_name": "svc"}}));
        assert_eq!(bot.0, PublishedBy::System);
        assert_eq!(
            published_by(&serde_json::json!("Anonymous")).0,
            PublishedBy::Anonymous
        );
        assert_eq!(
            published_by(&serde_json::Value::Null).0,
            PublishedBy::Anonymous
        );
    }

    #[test]
    fn checkpoint_delivery_state_handled_and_waiting() {
        assert_eq!(
            checkpoint_delivery_state(3, Some(5)),
            DeliveryState::Handled
        );
        assert_eq!(
            checkpoint_delivery_state(5, Some(5)),
            DeliveryState::Handled
        );
        assert_eq!(
            checkpoint_delivery_state(7, Some(5)),
            DeliveryState::Waiting { position: 2 }
        );
        assert_eq!(
            checkpoint_delivery_state(1, None),
            DeliveryState::Waiting { position: 1 }
        );
    }

    fn query(range: EventsRange, from: Option<&str>, to: Option<&str>) -> EventsQuery {
        EventsQuery {
            range,
            from: from.map(Into::into),
            to: to.map(Into::into),
            ..EventsQuery::default()
        }
    }

    #[test]
    fn events_window_presets_and_custom_days() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .expect("now")
            .with_timezone(&chrono::Utc);
        let (since, until) =
            events_window(&query(EventsRange::LastHour, None, None), now).expect("last hour");
        assert_eq!(now - since, chrono::Duration::hours(1));
        assert!(until.is_none());
        let (since, until) = events_window(
            &query(EventsRange::Custom, Some("2026-11-20"), Some("2026-11-21")),
            now,
        )
        .expect("custom");
        assert_eq!(since.to_rfc3339(), "2026-11-20T00:00:00+00:00");
        assert_eq!(
            until.expect("until").to_rfc3339(),
            "2026-11-22T00:00:00+00:00"
        );
    }

    #[test]
    fn events_window_rejects_bad_custom_ranges() {
        let now = chrono::Utc::now();
        let err = |from, to| events_window(&query(EventsRange::Custom, from, to), now);
        assert_eq!(
            err(None, Some("2026-11-21")),
            Err(EventsQueryError::MissingDates)
        );
        assert_eq!(
            err(Some("21/11/2026"), Some("2026-11-21")),
            Err(EventsQueryError::BadDate)
        );
        assert_eq!(
            err(Some("2026-11-22"), Some("2026-11-21")),
            Err(EventsQueryError::FromAfterTo)
        );
        assert_eq!(
            err(Some("2024-01-01"), Some("2026-11-21")),
            Err(EventsQueryError::RangeTooLong)
        );
    }

    #[test]
    fn events_page_and_search_limits() {
        assert_eq!(clamp_events_page(0), 1);
        assert_eq!(clamp_events_page(3), 3);
        assert_eq!(clamp_events_page(10_000), MAX_EVENTS_PAGE);
        assert_eq!(normalize_events_search(Some("  ")), Ok(None));
        assert_eq!(normalize_events_search(Some(" ord-1 ")), Ok(Some("ord-1")));
        let long = "x".repeat(MAX_PHOTON_ID_CHARS + 1);
        assert_eq!(
            normalize_events_search(Some(&long)),
            Err(EventsQueryError::SearchTooLong)
        );
    }

    #[test]
    fn events_query_url_round_trip() {
        let query = EventsQuery {
            topic: Some("orders.created".into()),
            q: Some("ord 1/2".into()),
            range: EventsRange::Custom,
            from: Some("2026-11-20".into()),
            to: Some("2026-11-21".into()),
            page: 2,
        };
        let qs = query.to_query_string();
        assert_eq!(
            qs,
            "q=ord%201%2F2&topic=orders.created&range=custom&from=2026-11-20&to=2026-11-21&page=2"
        );
        let decoded: std::collections::HashMap<&str, String> = [
            ("q", "ord 1/2"),
            ("topic", "orders.created"),
            ("range", "custom"),
            ("from", "2026-11-20"),
            ("to", "2026-11-21"),
            ("page", "2"),
        ]
        .into_iter()
        .map(|(k, v)| (k, v.to_string()))
        .collect();
        assert_eq!(EventsQuery::from_params(|k| decoded.get(k).cloned()), query);
        assert_eq!(EventsQuery::from_params(|_| None).to_query_string(), "");
        let junk = EventsQuery::from_params(|k| (k == "page" || k == "range").then(|| "x".into()));
        assert_eq!((junk.page, junk.range), (1, EventsRange::Last24h));
    }

    #[test]
    fn events_range_params_round_trip() {
        for range in EventsRange::ALL {
            assert_eq!(EventsRange::from_param(range.as_param()), Some(range));
        }
        assert_eq!(EventsRange::from_param("forever"), None);
        for reason in RevealReason::ALL {
            assert_eq!(RevealReason::parse(reason.as_str()), Some(reason));
        }
    }

    #[test]
    fn topic_summary_serde_roundtrip_happy_path() {
        let topic = sample_topic("orders");
        let json = serde_json::to_string(&topic).expect("serialize");
        let back: TopicSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, topic);
    }

    #[test]
    fn compute_checkpoint_lag_no_checkpoint_reports_full_head_happy_path() {
        assert_eq!(compute_checkpoint_lag(Some(9), None), 9);
    }

    #[test]
    fn compute_checkpoint_lag_unknown_head_falls_back_to_zero_happy_path() {
        assert_eq!(compute_checkpoint_lag(None, Some(5)), 0);
        assert_eq!(compute_checkpoint_lag(None, None), 0);
    }

    #[test]
    fn compute_checkpoint_lag_reports_gap_between_head_and_last_seq_happy_path() {
        assert_eq!(compute_checkpoint_lag(Some(9), Some(3)), 6);
    }

    #[test]
    fn compute_checkpoint_lag_clamps_negative_gap_to_zero_sad_path() {
        assert_eq!(compute_checkpoint_lag(Some(3), Some(9)), 0);
    }

    #[test]
    fn subscription_summary_from_handler_uses_registry_key_happy_path() {
        let sub = subscription_summary_from_handler(
            "reg.key",
            Some("orders.sub".into()),
            "orders",
            "durable",
            Some(42),
            Some(50),
        );
        assert_eq!(sub.subscription_id, "reg.key");
        assert_eq!(sub.subscription_name, "orders.sub");
        assert_eq!(sub.last_seq, Some(42));
        assert_eq!(sub.checkpoint_lag, 8);
        assert!(sub.enabled);
    }

    #[test]
    fn find_checkpoint_seq_matches_subscription_topic_happy_path() {
        let cps = vec![
            ("sub-a".into(), "orders".into(), None, Some(9), Some(20)),
            ("sub-b".into(), "orders".into(), None, Some(3), Some(20)),
        ];
        assert_eq!(find_checkpoint_seq(&cps, Some("sub-a"), "orders"), Some(9));
        assert_eq!(find_checkpoint_seq(&cps, None, "orders"), None);
        assert_eq!(find_checkpoint_seq(&cps, Some("missing"), "orders"), None);
    }

    #[test]
    fn find_checkpoint_head_seq_matches_subscription_topic_happy_path() {
        let cps = vec![
            ("sub-a".into(), "orders".into(), None, Some(9), Some(20)),
            ("sub-b".into(), "orders".into(), None, Some(3), None),
        ];
        assert_eq!(
            find_checkpoint_head_seq(&cps, Some("sub-a"), "orders"),
            Some(20)
        );
        assert_eq!(
            find_checkpoint_head_seq(&cps, Some("sub-b"), "orders"),
            None
        );
        assert_eq!(find_checkpoint_head_seq(&cps, None, "orders"), None);
    }

    #[test]
    fn event_summary_from_transport_marks_stored_happy_path() {
        let row = event_summary_from_transport("e1", "t", None, 1, "2026-01-01T00:00:00Z");
        assert_eq!(row.payload_preview, "[stored]");
    }

    #[test]
    fn clamp_event_list_limit_caps_oversized_sad() {
        assert_eq!(
            clamp_event_list_limit(10_000),
            MAX_EVENT_LIST_LIMIT as usize
        );
        assert_eq!(
            clamp_event_list_limit(MAX_EVENT_LIST_LIMIT),
            MAX_EVENT_LIST_LIMIT as usize
        );
    }

    #[test]
    fn clamp_event_list_limit_preserves_small_happy_path() {
        assert_eq!(clamp_event_list_limit(20), 20);
        assert_eq!(clamp_event_list_limit(0), 0);
    }
}
