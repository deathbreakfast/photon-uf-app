//! Live Photon IO contracts for ops helpers (`photon-backend` feature `ops`).
//!
//! Validates happy/sad postconditions through mem Photon + `admin_snapshot` /
//! list/get APIs — not Leptos server fns (those are covered by photon-uf-app-e2e).
//!
//! Uses a process-wide Photon (same as production `configure`) so parallel tests
//! do not race on the global runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use photon::{
    configure, subscribe, topic, InProcStoragePort, JsonIdentityFactory, Photon, StoragePort,
    TransportCrypto,
};
use photon_backend::ops::{
    list_subscriptions, load_dashboard_stats, load_event, load_events, load_events_page,
    load_recent_events, load_subscription, load_topic, load_topics, redeliver_event,
    require_photon, require_photon_admin, require_session_user, reveal_event, OpsError,
};
use photon_backend::{DeliveryState, EventsQuery, EventsRange, PublishedBy, MASKED_VALUE};
use photon_core::Actor;
use tokio::sync::OnceCell;

static HANDLER_HITS: AtomicUsize = AtomicUsize::new(0);
static PHOTON: OnceCell<Arc<Photon>> = OnceCell::const_new();
static STORAGE: OnceCell<Arc<InProcStoragePort>> = OnceCell::const_new();
static ORDERS: OnceCell<OrderFixtures> = OnceCell::const_new();
static CONFIRMED: Mutex<Vec<String>> = Mutex::new(Vec::new());

const ORDERS_TOPIC: &str = "test.photon.ops.orders";

#[topic(name = "test.photon.ops.orders", keyed_by = "order_id")]
pub struct OpsOrder {
    pub order_id: String,
    #[photon(personal)]
    pub customer_email: String,
    pub total_cents: u32,
}

#[subscribe(topic = "test.photon.ops.orders", durable = "confirm")]
#[allow(clippy::unused_async)]
async fn on_order_confirm(actor: Box<dyn Actor>, event: OpsOrder) -> photon::Result<()> {
    let _ = actor;
    CONFIRMED.lock().unwrap().push(event.order_id);
    Ok(())
}

#[subscribe(topic = "test.photon.ops.orders", durable = "warehouse")]
#[allow(clippy::unused_async)]
async fn on_order_warehouse(actor: Box<dyn Actor>, event: OpsOrder) -> photon::Result<()> {
    let _ = (actor, event);
    Ok(())
}

#[subscribe(topic = "test.photon.ops.orders", durable = "fraud")]
#[allow(clippy::unused_async)]
async fn on_order_fraud(actor: Box<dyn Actor>, event: OpsOrder) -> photon::Result<()> {
    let _ = (actor, event);
    Err(photon::PhotonError::Internal("fraud service down".into()))
}

fn confirmations(order_id: &str) -> usize {
    CONFIRMED
        .lock()
        .unwrap()
        .iter()
        .filter(|id| *id == order_id)
        .count()
}

struct OrderFixtures {
    first_id: String,
    expired_id: String,
}

/// 27 single-key orders (the first published by a person), three under `ord-search`, and one
/// whose payload is trimmed after `fraud` dead-letters it. `warehouse` is paused throughout.
async fn order_fixtures() -> &'static OrderFixtures {
    ORDERS
        .get_or_init(|| async {
            let photon = shared_photon().await;
            let storage = STORAGE.get().expect("storage").clone();
            photon
                .pause(&format!("{ORDERS_TOPIC}:warehouse"), "test")
                .await
                .expect("pause warehouse");
            let system = serde_json::json!({"System": {"operation": "test"}});
            let person = serde_json::json!({"User": {"user_id": "ines"}});
            let order = |key: &str, n: u32| {
                serde_json::json!({
                    "order_id": key,
                    "customer_email": format!("customer{n}@example.com"),
                    "total_cents": 1000 + n,
                })
            };
            let mut first_id = String::new();
            for n in 1..=27u32 {
                let key = format!("ord-{n:04}");
                let actor = if n == 1 {
                    person.clone()
                } else {
                    system.clone()
                };
                let id = photon
                    .publish(ORDERS_TOPIC, Some(&key), actor, order(&key, n))
                    .await
                    .expect("publish order");
                if n == 1 {
                    first_id = id;
                }
            }
            for n in 0..3u32 {
                photon
                    .publish(
                        ORDERS_TOPIC,
                        Some("ord-search"),
                        system.clone(),
                        order("ord-search", 100 + n),
                    )
                    .await
                    .expect("publish search order");
            }
            let expired_id = photon
                .publish(
                    ORDERS_TOPIC,
                    Some("ord-expired"),
                    system.clone(),
                    order("ord-expired", 999),
                )
                .await
                .expect("publish expired order");
            for _ in 0..50 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if !photon.dlq_for_event(&expired_id).is_empty() && confirmations("ord-expired") > 0
                {
                    break;
                }
            }
            photon
                .runtime()
                .executor_services
                .checkpoint_coalescer
                .flush()
                .await
                .expect("flush checkpoints");
            let seq = photon
                .get_event(&expired_id)
                .await
                .expect("get")
                .expect("expired order before trim")
                .seq;
            storage
                .truncate_before(ORDERS_TOPIC, Some("ord-expired"), seq + 1)
                .await
                .expect("trim");
            OrderFixtures {
                first_id,
                expired_id,
            }
        })
        .await
}

fn orders_query(q: Option<&str>, page: u32) -> EventsQuery {
    EventsQuery {
        topic: Some(ORDERS_TOPIC.into()),
        q: q.map(Into::into),
        page,
        ..EventsQuery::default()
    }
}

fn all_topics() -> Vec<String> {
    vec![TOPIC.into(), ORDERS_TOPIC.into()]
}

const TOPIC: &str = "test.photon.ops.contract";
const HANDLER: &str = "ops-contract-handler";

#[topic(name = "test.photon.ops.contract")]
pub struct OpsContractEvent {
    pub value: u32,
}

#[subscribe(topic = "test.photon.ops.contract", durable = "ops-contract-handler")]
#[allow(clippy::unused_async)]
async fn on_ops_contract(actor: Box<dyn Actor>, event: OpsContractEvent) -> photon::Result<()> {
    let _ = actor;
    assert_eq!(event.value, 42);
    HANDLER_HITS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn ensure_transport_key() {
    if std::env::var_os("PHOTON_TRANSPORT_KEY").is_none() {
        // SAFETY: test process boot only.
        unsafe {
            std::env::set_var(
                "PHOTON_TRANSPORT_KEY",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            );
            std::env::set_var("PHOTON_ALLOW_DEV_TRANSPORT_KEY", "1");
        }
    }
}

async fn shared_photon() -> Arc<Photon> {
    PHOTON
        .get_or_init(|| async {
            ensure_transport_key();
            HANDLER_HITS.store(0, Ordering::SeqCst);

            let storage = Arc::new(InProcStoragePort::new(
                TransportCrypto::from_env().expect("transport key"),
            ));
            STORAGE.set(Arc::clone(&storage)).ok();
            let photon = Photon::builder()
                .storage_port(storage as Arc<dyn StoragePort>)
                .auto_registry()
                .build()
                .expect("build photon");
            photon
                .start_executor(Arc::new(JsonIdentityFactory))
                .expect("start executor");
            configure(photon.clone());

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            OpsContractEvent { value: 42 }
                .publish()
                .await
                .expect("publish");
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            photon
                .runtime()
                .executor_services
                .checkpoint_coalescer
                .flush()
                .await
                .expect("flush checkpoints");

            assert!(
                HANDLER_HITS.load(Ordering::SeqCst) >= 1,
                "durable handler must run"
            );
            Arc::new(photon)
        })
        .await
        .clone()
}

#[tokio::test]
async fn integ_ops_auth_guards_sad() {
    assert_eq!(
        require_session_user(None).unwrap_err(),
        OpsError::AuthenticationRequired
    );
    assert!(require_session_user(Some("user:admin")).is_ok());

    assert_eq!(
        require_photon_admin(false).unwrap_err(),
        OpsError::PermissionDenied
    );
    assert!(require_photon_admin(true).is_ok());

    assert_eq!(
        require_photon(None).err(),
        Some(OpsError::PhotonNotInContext)
    );
}

#[tokio::test]
async fn integ_dashboard_seeded_happy() {
    let photon = shared_photon().await;
    let stats = load_dashboard_stats(&photon).await.expect("dashboard");
    assert!(stats.topic_count >= 1, "registry must include seeded topic");
    assert!(
        stats.subscription_count >= 1,
        "admin_snapshot must list durable handler"
    );
    assert!(
        stats.event_count_24h >= 1,
        "published event must count in 24h window, got {}",
        stats.event_count_24h
    );
}

#[tokio::test]
async fn integ_topics_list_and_unknown_sad() {
    let photon = shared_photon().await;
    let topics = load_topics(&photon).await.expect("topics");
    assert!(
        topics.iter().any(|t| t.topic_name == TOPIC),
        "seeded topic missing: {topics:?}"
    );
    let detail = load_topic(&photon, TOPIC).await.expect("topic detail");
    assert_eq!(detail.map(|t| t.topic_name).as_deref(), Some(TOPIC));

    let missing = load_topic(&photon, "test.photon.ops.missing")
        .await
        .expect("unknown topic");
    assert!(missing.is_none());

    let blank = load_topic(&photon, "").await.expect_err("blank topic");
    assert!(matches!(blank, OpsError::InvalidId(_)));
}

#[tokio::test]
async fn integ_subs_list_and_unknown_sad() {
    let photon = shared_photon().await;
    let subs = list_subscriptions(&photon).await.expect("subs");
    let expected_key = format!("{TOPIC}:{HANDLER}");
    assert!(
        subs.iter().any(|s| s.subscription_id == expected_key),
        "handler missing: {subs:?}"
    );
    let detail = load_subscription(&photon, &expected_key)
        .await
        .expect("sub detail");
    assert_eq!(
        detail.map(|s| s.subscription_id).as_deref(),
        Some(expected_key.as_str())
    );

    let missing = load_subscription(&photon, "no-such-handler")
        .await
        .expect("unknown sub");
    assert!(missing.is_none());
}

#[tokio::test]
async fn integ_events_list_detail_and_unknown_sad() {
    let photon = shared_photon().await;
    let recent = load_recent_events(&photon, 50).await.expect("recent");
    assert_ne!(recent.len(), 0);
    let event_id = recent[0].event_id.clone();

    let by_topic = load_events(&photon, Some(TOPIC), 50)
        .await
        .expect("by topic");
    assert!(by_topic.iter().any(|e| e.event_id == event_id));

    let detail = load_event(&photon, &event_id).await.expect("detail");
    let detail = detail.expect("event exists");
    assert_eq!(detail.event_id, event_id);
    assert_eq!(detail.topic_name, TOPIC);

    let missing = load_event(&photon, "no-such-event-id")
        .await
        .expect("unknown event");
    assert!(missing.is_none());

    let blank = load_event(&photon, "").await.expect_err("blank event id");
    assert!(matches!(blank, OpsError::InvalidId(_)));
}

#[tokio::test]
async fn integ_events_page_pages_with_a_total() {
    let photon = shared_photon().await;
    order_fixtures().await;
    let now = chrono::Utc::now();
    let page1 = load_events_page(&photon, &orders_query(None, 1), &all_topics(), now)
        .await
        .expect("page 1");
    assert_eq!(page1.total, Some(30));
    assert_eq!(page1.rows.len(), 25);
    assert_eq!((page1.prev, page1.next), (None, Some(2)));
    let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).expect("rfc3339");
    assert!(page1
        .rows
        .windows(2)
        .all(|w| at(&w[0].created_at) >= at(&w[1].created_at)));
    let page2 = load_events_page(&photon, &orders_query(None, 2), &all_topics(), now)
        .await
        .expect("page 2");
    assert_eq!(page2.rows.len(), 5);
    assert_eq!((page2.prev, page2.next), (Some(1), None));
    let ids: std::collections::HashSet<_> = page1
        .rows
        .iter()
        .chain(&page2.rows)
        .map(|e| &e.event_id)
        .collect();
    assert_eq!(ids.len(), 30, "pages must not overlap");
}

#[tokio::test]
async fn integ_events_page_key_exact_id_and_scope() {
    let photon = shared_photon().await;
    let fx = order_fixtures().await;
    let now = chrono::Utc::now();
    let by_key = load_events_page(
        &photon,
        &orders_query(Some("ord-search"), 1),
        &all_topics(),
        now,
    )
    .await
    .expect("key search");
    assert_eq!(by_key.total, Some(3));
    assert_eq!(by_key.rows.len(), 3);
    assert!(by_key
        .rows
        .iter()
        .all(|e| e.topic_key.as_deref() == Some("ord-search")));

    // An exact id opens the event whatever the topic filter or range says.
    let mut exact = orders_query(Some(&fx.first_id), 1);
    exact.topic = Some(TOPIC.into());
    exact.range = EventsRange::LastHour;
    let page = load_events_page(&photon, &exact, &all_topics(), now)
        .await
        .expect("exact id");
    assert_eq!(page.exact_id.as_deref(), Some(fx.first_id.as_str()));
    // The dead-letter queue still knows the expired id.
    let expired = load_events_page(
        &photon,
        &orders_query(Some(&fx.expired_id), 1),
        &all_topics(),
        now,
    )
    .await
    .expect("expired id");
    assert_eq!(expired.exact_id.as_deref(), Some(fx.expired_id.as_str()));

    // Topics the reader can't View stay hidden, ids included.
    let hidden = load_events_page(
        &photon,
        &orders_query(Some(&fx.first_id), 1),
        &[TOPIC.into()],
        now,
    )
    .await
    .expect("hidden");
    assert!(hidden.exact_id.is_none() && hidden.rows.is_empty());
    let mut everything = orders_query(None, 1);
    everything.topic = None;
    let only_ops = load_events_page(&photon, &everything, &[TOPIC.into()], now)
        .await
        .expect("viewable only");
    assert!(only_ops.rows.iter().all(|e| e.topic_name == TOPIC));

    let mut yesterday = orders_query(None, 1);
    yesterday.range = EventsRange::Custom;
    let day = (now - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    yesterday.from = Some(day.clone());
    yesterday.to = Some(day);
    let none = load_events_page(&photon, &yesterday, &all_topics(), now)
        .await
        .expect("yesterday");
    assert_eq!((none.rows.len(), none.total), (0, Some(0)));
    let mut bad = yesterday.clone();
    bad.to = None;
    assert!(matches!(
        load_events_page(&photon, &bad, &all_topics(), now).await,
        Err(OpsError::InvalidQuery(_))
    ));
}

#[tokio::test]
async fn integ_event_detail_says_who_handled_it_and_masks_personal_fields() {
    let photon = shared_photon().await;
    let fx = order_fixtures().await;
    let detail = load_event(&photon, &fx.first_id)
        .await
        .expect("detail")
        .expect("order");
    let wire = serde_json::to_string(&detail).unwrap();
    assert!(
        !wire.contains("customer1@example.com"),
        "personal value leaked: {wire}"
    );
    assert!(!wire.contains("\"ines\""), "publisher id leaked: {wire}");
    assert_eq!(detail.payload_json["customer_email"], MASKED_VALUE);
    assert_eq!(detail.published_by, PublishedBy::Person);
    assert_eq!(detail.personal_fields, vec!["customer_email".to_string()]);
    let state = |name: &str| {
        detail
            .deliveries
            .iter()
            .find(|d| d.subscription_name == name)
            .map_or_else(
                || panic!("no {name} row in {:?}", detail.deliveries),
                |d| d.state.clone(),
            )
    };
    assert_eq!(state("confirm"), DeliveryState::Handled);
    assert_eq!(state("warehouse"), DeliveryState::Waiting { position: 1 });
    assert!(matches!(
        state("fraud"),
        DeliveryState::Failed { attempts: 1.., .. }
    ));

    let reveal = reveal_event(&photon, &fx.first_id)
        .await
        .expect("reveal")
        .expect("live order");
    assert_eq!(reveal.topic_name, ORDERS_TOPIC);
    assert_eq!(reveal.published_by_id.as_deref(), Some("ines"));
    assert_eq!(
        reveal
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.as_deref()))
            .collect::<Vec<_>>(),
        vec![("customer_email", Some("customer1@example.com"))]
    );
}

#[tokio::test]
async fn integ_expired_event_keeps_metadata() {
    let photon = shared_photon().await;
    let fx = order_fixtures().await;
    let detail = load_event(&photon, &fx.expired_id)
        .await
        .expect("detail")
        .expect("expired order is not 'not found'");
    assert!(detail.transport_expired);
    assert_eq!(detail.topic_name, ORDERS_TOPIC);
    assert_eq!(detail.topic_key.as_deref(), Some("ord-expired"));
    assert!(detail.payload_json.is_null() && detail.fields.is_empty());
    assert!(reveal_event(&photon, &fx.expired_id)
        .await
        .expect("reveal")
        .is_none());
    assert!(load_event(&photon, "no-such-order")
        .await
        .expect("lookup")
        .is_none());
}

#[tokio::test]
async fn integ_redeliver_runs_one_handler_once() {
    let photon = shared_photon().await;
    let fx = order_fixtures().await;
    let before = confirmations("ord-0001");
    redeliver_event(
        &photon,
        &fx.first_id,
        &format!("{ORDERS_TOPIC}:confirm"),
        "user:admin",
    )
    .await
    .expect("redeliver");
    assert_eq!(confirmations("ord-0001"), before + 1);
    let wrong_topic = redeliver_event(
        &photon,
        &fx.first_id,
        &format!("{TOPIC}:{HANDLER}"),
        "user:admin",
    )
    .await;
    assert!(matches!(wrong_topic, Err(OpsError::PhotonIo { .. })));
}
