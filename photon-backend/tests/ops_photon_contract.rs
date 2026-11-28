//! Live Photon IO contracts for ops helpers (`photon-backend` feature `ops`).
//!
//! Validates happy/sad postconditions through mem Photon + `admin_snapshot` /
//! list/get APIs — not Leptos server fns (those are covered by photon-uf-app-e2e).
//!
//! Uses a process-wide Photon (same as production `configure`) so parallel tests
//! do not race on the global runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use photon::{configure, subscribe, topic, JsonIdentityFactory, Photon};
use photon_backend::ops::{
    list_subscriptions, load_dashboard_stats, load_event, load_events, load_recent_events,
    load_subscription, load_topic, load_topics, load_waiting_events, pause_subscription,
    replay_preview, replay_subscription, require_photon, require_photon_admin,
    require_session_user, resume_subscription, skip_subscription_to_latest, OpsError,
};
use photon_backend::{ReplayPoint, SubscriptionHealth};
use photon_core::Actor;
use tokio::sync::OnceCell;

static HANDLER_HITS: AtomicUsize = AtomicUsize::new(0);
static PHOTON: OnceCell<Arc<Photon>> = OnceCell::const_new();

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

const CONTROL_TOPIC: &str = "test.photon.ops.control";

#[topic(name = "test.photon.ops.control")]
pub struct OpsControlEvent {
    pub n: u32,
}

#[subscribe(topic = "test.photon.ops.control", durable = "ops-control-handler")]
#[allow(clippy::unused_async)]
async fn on_ops_control(actor: Box<dyn Actor>, event: OpsControlEvent) -> photon::Result<()> {
    let _ = (actor, event);
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

            let photon = Photon::builder()
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
async fn integ_subscription_control_round_trip() {
    let photon = shared_photon().await;
    let id = format!("{CONTROL_TOPIC}:ops-control-handler");

    pause_subscription(&photon, &id, "contract")
        .await
        .expect("pause");
    for n in 0..3 {
        OpsControlEvent { n }.publish().await.expect("publish");
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let sub = load_subscription(&photon, &id)
        .await
        .expect("load")
        .expect("sub");
    assert_eq!(sub.health, SubscriptionHealth::Paused);
    assert!(!sub.enabled);
    assert_eq!(sub.paused.as_ref().map(|p| p.by.as_str()), Some("contract"));
    assert!(sub.supports_control);
    assert_eq!(sub.waiting, 3);

    let waiting = load_waiting_events(&photon, &id, 2).await.expect("waiting");
    assert_eq!(waiting.total, 3);
    assert_eq!(waiting.events.len(), 2);
    assert!(
        waiting.events[0].seq < waiting.events[1].seq,
        "oldest first"
    );

    let preview = replay_preview(&photon, &id, &ReplayPoint::AfterSeq(0))
        .await
        .expect("preview");
    assert_eq!(preview.count, 3);
    let bad = replay_preview(&photon, &id, &ReplayPoint::AfterSeq(-1)).await;
    assert!(matches!(bad, Err(OpsError::InvalidReplayPoint(_))));

    let skipped = skip_subscription_to_latest(&photon, &id, "contract")
        .await
        .expect("skip");
    assert_eq!(skipped, 3);
    let sub = load_subscription(&photon, &id)
        .await
        .expect("load")
        .expect("sub");
    assert_eq!(sub.waiting, 0);
    let empty = load_waiting_events(&photon, &id, 10)
        .await
        .expect("waiting");
    assert_eq!((empty.total, empty.events.len()), (0, 0));

    let replayed = replay_subscription(&photon, &id, &ReplayPoint::AfterSeq(0), "contract")
        .await
        .expect("replay");
    assert_eq!(replayed.count, 3);

    resume_subscription(&photon, &id, "contract")
        .await
        .expect("resume");
    let sub = load_subscription(&photon, &id)
        .await
        .expect("load")
        .expect("sub");
    assert!(sub.paused.is_none());
    assert_ne!(sub.health, SubscriptionHealth::Paused);

    let missing = pause_subscription(&photon, "no-such-handler", "contract").await;
    assert_eq!(missing, Err(OpsError::SubscriptionNotFound));
}
