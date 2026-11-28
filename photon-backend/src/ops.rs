//! Photon IO + mapping for ops UI reads (feature `ops`).
//!
//! Server functions in `photon-app` resolve session / Photon context, then call
//! these helpers so SSR and `IsolatedLab` tests can exercise the same path without
//! Leptos request plumbing.

use std::sync::Arc;

use photon::Photon;

use chrono::{DateTime, Utc};

use crate::{
    aggregate_checkpoints, clamp_event_list_limit, clamp_waiting_limit, count_since,
    dashboard_stats, event_detail_from_transport, event_summary_from_transport,
    find_subscription_by_id, find_topic_by_name, sort_topics_by_name,
    subscription_summary_from_snapshot, topic_summary, validate_event_id, validate_replay_point,
    validate_subscription_id, validate_topic_name, CheckpointAggregate, CheckpointState,
    DashboardStats, EventDetail, EventSummary, HandlerSnapshot, PhotonIdError, ReplayPoint,
    ReplayPointError, ReplayPreviewSummary, SubscriptionSummary, TopicSummary, WaitingEvents,
    DEFAULT_STALL_AFTER,
};

/// Operator-facing ops failure (maps to `ServerFnError` at the Leptos boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpsError {
    /// Missing authenticated session.
    AuthenticationRequired,
    /// Photon was not provided in request / lab context.
    PhotonNotInContext,
    /// Permission check failed (`PhotonAdmin`).
    PermissionDenied,
    /// Path/query id failed validation.
    InvalidId(PhotonIdError),
    /// Replay start point failed validation.
    InvalidReplayPoint(ReplayPointError),
    /// The subscription id names no handler.
    SubscriptionNotFound,
    /// The installed backend can't pause, replay or skip.
    ControlUnsupported,
    /// Photon IO or snapshot failure with operation context.
    PhotonIo {
        /// Stable operation name for logs and operator messages.
        operation: &'static str,
        /// Photon runtime error detail.
        detail: String,
    },
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationRequired => write!(f, "Authentication required"),
            Self::PhotonNotInContext => write!(f, "Photon not in request context"),
            Self::PermissionDenied => write!(f, "Permission denied"),
            Self::InvalidId(e) => write!(f, "{e}"),
            Self::InvalidReplayPoint(e) => write!(f, "{e}"),
            Self::SubscriptionNotFound => write!(f, "Photon subscription not found"),
            Self::ControlUnsupported => write!(
                f,
                "This Photon backend can't pause, replay or skip subscriptions"
            ),
            Self::PhotonIo { operation, detail } => write!(f, "Failed to {operation}: {detail}"),
        }
    }
}

fn photon_io_err(operation: &'static str, source: impl std::fmt::Display) -> OpsError {
    OpsError::PhotonIo {
        operation,
        detail: source.to_string(),
    }
}

fn len_as_u32(len: usize) -> Result<u32, OpsError> {
    u32::try_from(len).map_err(|_| OpsError::PhotonIo {
        operation: "convert count",
        detail: "count exceeds u32::MAX".into(),
    })
}

impl std::error::Error for OpsError {}

impl From<ReplayPointError> for OpsError {
    fn from(value: ReplayPointError) -> Self {
        Self::InvalidReplayPoint(value)
    }
}

/// Maps a Photon control error, keeping not-found and unsupported apart from IO failures.
fn control_err(operation: &'static str, e: photon::PhotonError) -> OpsError {
    match e {
        photon::PhotonError::SubscriptionNotFound(_) => OpsError::SubscriptionNotFound,
        photon::PhotonError::Unsupported(_) => OpsError::ControlUnsupported,
        other => photon_io_err(operation, other),
    }
}

impl From<PhotonIdError> for OpsError {
    fn from(value: PhotonIdError) -> Self {
        Self::InvalidId(value)
    }
}

/// Reject anonymous callers (session user id absent).
pub const fn require_session_user(user_id: Option<&str>) -> Result<(), OpsError> {
    if user_id.is_some() {
        Ok(())
    } else {
        Err(OpsError::AuthenticationRequired)
    }
}

/// Reject callers without `PhotonAdmin` (lab / integ seam; server macro also gates).
pub const fn require_photon_admin(has_admin: bool) -> Result<(), OpsError> {
    if has_admin {
        Ok(())
    } else {
        Err(OpsError::PermissionDenied)
    }
}

/// Require a Photon handle (mirrors Leptos context lookup).
pub fn require_photon(photon: Option<Arc<Photon>>) -> Result<Arc<Photon>, OpsError> {
    photon.ok_or(OpsError::PhotonNotInContext)
}

fn map_transport_event(ev: &photon::Event) -> EventSummary {
    event_summary_from_transport(
        ev.event_id.clone(),
        ev.topic_name.clone(),
        ev.topic_key.clone(),
        ev.seq,
        ev.created_at.to_rfc3339(),
    )
}

/// Handler inventory, checkpoints and health via `admin_snapshot`, in registry order.
///
/// Health uses [`DEFAULT_STALL_AFTER`]; see [`list_subscriptions_at`] to pick the threshold.
pub async fn list_subscriptions(photon: &Photon) -> Result<Vec<SubscriptionSummary>, OpsError> {
    list_subscriptions_at(photon, Utc::now(), DEFAULT_STALL_AFTER).await
}

/// [`list_subscriptions`] with an explicit clock and stall threshold.
pub async fn list_subscriptions_at(
    photon: &Photon,
    now: DateTime<Utc>,
    stall_after: chrono::Duration,
) -> Result<Vec<SubscriptionSummary>, OpsError> {
    let snap = photon
        .admin_snapshot()
        .await
        .map_err(|e| photon_io_err("load admin snapshot", e))?;
    let checkpoints: Vec<CheckpointState> = snap
        .checkpoints
        .iter()
        .map(|c| CheckpointState {
            subscription_name: c.subscription_name.clone(),
            topic_name: c.topic_name.clone(),
            topic_key: c.topic_key.clone(),
            last_seq: c.last_seq,
            head_seq: c.head_seq,
            oldest_waiting_at: c.oldest_waiting_at,
        })
        .collect();
    let supports_control = snap.backend.supports_subscription_control;
    let retention_secs = snap.backend.max_replay_window_secs;
    let mut list = Vec::with_capacity(snap.handlers.len());
    for h in snap.handlers {
        let name = h.subscription_name.or(h.consumer_group);
        let checkpoint = name
            .as_deref()
            .map_or_else(CheckpointAggregate::default, |n| {
                aggregate_checkpoints(&checkpoints, n, &h.topic_name)
            });
        let health = h.health;
        list.push(subscription_summary_from_snapshot(
            HandlerSnapshot {
                registry_key: h.registry_key,
                subscription_name: name,
                topic_name: h.topic_name,
                mode: h.mode,
                checkpoint,
                last_handled_at: health.last_handled_at,
                failures_24h: health.failures_24h,
                last_error: health.last_error,
                last_error_at: health.last_error_at,
                retrying: health
                    .retrying
                    .map(|r| (r.event_id, r.attempt, r.next_retry_at)),
                paused: health.paused.map(|p| (p.by, p.at)),
                supports_control,
                retention_secs,
            },
            now,
            stall_after,
        ));
    }
    Ok(list)
}

/// Dashboard KPIs from registry size, subscription count, and 24h events.
pub async fn load_dashboard_stats(photon: &Photon) -> Result<DashboardStats, OpsError> {
    let topic_count = len_as_u32(photon.registry().len())?;
    let subscription_count = len_as_u32(list_subscriptions(photon).await?.len())?;

    let events = photon
        .list_recent_events(clamp_event_list_limit(1000))
        .await
        .map_err(|e| photon_io_err("list recent events", e))?;
    let now = chrono::Utc::now();
    let day_ago = now - chrono::Duration::hours(24);
    let stamps: Vec<_> = events.iter().map(|e| e.created_at).collect();
    let event_count_24h = count_since(&stamps, day_ago);

    Ok(dashboard_stats(
        topic_count,
        subscription_count,
        event_count_24h,
    ))
}

/// Recent events for the dashboard preview table.
pub async fn load_recent_events(
    photon: &Photon,
    limit: u32,
) -> Result<Vec<EventSummary>, OpsError> {
    let limit = clamp_event_list_limit(limit);
    let events = photon
        .list_recent_events(limit)
        .await
        .map_err(|e| photon_io_err("list recent events", e))?;
    Ok(events.iter().map(map_transport_event).collect())
}

/// All registry topics with per-topic subscription and 24h event counts.
pub async fn load_topics(photon: &Photon) -> Result<Vec<TopicSummary>, OpsError> {
    let subs = list_subscriptions(photon).await?;
    let registry = photon.registry();
    let mut topics = Vec::new();
    for desc in registry.iter() {
        let topic_name = desc.topic_name.to_string();
        let subscription_count =
            len_as_u32(subs.iter().filter(|s| s.topic_name == topic_name).count())?;

        let events = photon
            .list_events_by_topic(&topic_name, None, None, clamp_event_list_limit(1000))
            .await
            .map_err(|e| photon_io_err("list events", e))?;
        let now = chrono::Utc::now();
        let day_ago = now - chrono::Duration::hours(24);
        let stamps: Vec<_> = events.iter().map(|e| e.created_at).collect();
        let event_count_24h = count_since(&stamps, day_ago);

        topics.push(topic_summary(
            topic_name,
            desc.keyed_by.map(std::string::ToString::to_string),
            desc.schema_json.to_string(),
            event_count_24h,
            subscription_count,
        ));
    }
    sort_topics_by_name(&mut topics);
    Ok(topics)
}

/// Single topic by validated name.
pub async fn load_topic(
    photon: &Photon,
    topic_name: &str,
) -> Result<Option<TopicSummary>, OpsError> {
    validate_topic_name(topic_name)?;
    let topics = load_topics(photon).await?;
    Ok(find_topic_by_name(&topics, topic_name).cloned())
}

/// Single subscription by validated id.
pub async fn load_subscription(
    photon: &Photon,
    id: &str,
) -> Result<Option<SubscriptionSummary>, OpsError> {
    load_subscription_at(photon, id, Utc::now(), DEFAULT_STALL_AFTER).await
}

/// [`load_subscription`] with an explicit clock and stall threshold.
pub async fn load_subscription_at(
    photon: &Photon,
    id: &str,
    now: DateTime<Utc>,
    stall_after: chrono::Duration,
) -> Result<Option<SubscriptionSummary>, OpsError> {
    validate_subscription_id(id)?;
    let subs = list_subscriptions_at(photon, now, stall_after).await?;
    Ok(find_subscription_by_id(&subs, id).cloned())
}

async fn require_subscription_summary(
    photon: &Photon,
    id: &str,
) -> Result<SubscriptionSummary, OpsError> {
    load_subscription(photon, id)
        .await?
        .ok_or(OpsError::SubscriptionNotFound)
}

/// The events a subscription still owes, oldest first, up to `limit`.
///
/// Reads the topic after the subscription's checkpoint. For a consumer group that is the
/// lowest shard checkpoint, so the list can include events another shard already handled.
pub async fn load_waiting_events(
    photon: &Photon,
    id: &str,
    limit: u32,
) -> Result<WaitingEvents, OpsError> {
    let sub = require_subscription_summary(photon, id).await?;
    if sub.waiting == 0 {
        return Ok(WaitingEvents::default());
    }
    let query = photon::ListEventsQuery {
        topic: Some(sub.topic_name.clone()),
        after_seq: Some(sub.last_seq.unwrap_or(0)),
        newest_first: false,
        limit: clamp_waiting_limit(limit),
        ..photon::ListEventsQuery::default()
    };
    let events = photon
        .list_events(&query)
        .await
        .map_err(|e| photon_io_err("list waiting events", e))?;
    Ok(WaitingEvents {
        events: events.iter().map(map_transport_event).collect(),
        total: sub.waiting,
    })
}

/// Stops dispatch to a subscription; `by` is the display label recorded with it.
pub async fn pause_subscription(photon: &Photon, id: &str, by: &str) -> Result<(), OpsError> {
    validate_subscription_id(id)?;
    photon
        .pause(id, by)
        .await
        .map_err(|e| control_err("pause subscription", e))
}

/// Clears a pause; the subscription restarts from its checkpoint.
pub async fn resume_subscription(photon: &Photon, id: &str, by: &str) -> Result<(), OpsError> {
    validate_subscription_id(id)?;
    photon
        .resume(id, by)
        .await
        .map_err(|e| control_err("resume subscription", e))
}

/// Cuts the current retry backoff short so the failing event runs again now.
pub fn retry_subscription_now(photon: &Photon, id: &str, by: &str) -> Result<(), OpsError> {
    validate_subscription_id(id)?;
    photon
        .retry_now(id, by)
        .map_err(|e| control_err("retry subscription", e))
}

/// Moves the checkpoint to the head; returns how many waiting events were dropped.
pub async fn skip_subscription_to_latest(
    photon: &Photon,
    id: &str,
    by: &str,
) -> Result<u64, OpsError> {
    validate_subscription_id(id)?;
    photon
        .skip_to_latest(id, by)
        .await
        .map_err(|e| control_err("skip subscription", e))
}

fn replay_from(point: &ReplayPoint, now: DateTime<Utc>) -> Result<photon::ReplayFrom, OpsError> {
    let at = validate_replay_point(point, now)?;
    Ok(match point {
        ReplayPoint::AfterSeq(seq) => photon::ReplayFrom::Seq(seq.saturating_add(1)),
        ReplayPoint::Time(_) => photon::ReplayFrom::Time(at.unwrap_or(now)),
        ReplayPoint::Event(id) => photon::ReplayFrom::Event(id.trim().to_string()),
    })
}

const fn preview_summary(p: &photon::ReplayPreview) -> ReplayPreviewSummary {
    ReplayPreviewSummary {
        from_seq: p.from_seq,
        count: p.count,
        already_handled: p.already_handled,
    }
}

/// What [`replay_subscription`] would resend from `point`, without moving the checkpoint.
pub async fn replay_preview(
    photon: &Photon,
    id: &str,
    point: &ReplayPoint,
) -> Result<ReplayPreviewSummary, OpsError> {
    validate_subscription_id(id)?;
    let from = replay_from(point, Utc::now())?;
    photon
        .replay_preview(id, &from)
        .await
        .map(|p| preview_summary(&p))
        .map_err(|e| control_err("preview replay", e))
}

/// Moves the checkpoint back to `point` so the subscription handles those events again.
pub async fn replay_subscription(
    photon: &Photon,
    id: &str,
    point: &ReplayPoint,
    by: &str,
) -> Result<ReplayPreviewSummary, OpsError> {
    validate_subscription_id(id)?;
    let from = replay_from(point, Utc::now())?;
    photon
        .replay(id, &from, by)
        .await
        .map(|p| preview_summary(&p))
        .map_err(|e| control_err("replay subscription", e))
}

/// Events across all topics or filtered by topic name.
pub async fn load_events(
    photon: &Photon,
    topic_name: Option<&str>,
    limit: u32,
) -> Result<Vec<EventSummary>, OpsError> {
    let limit = clamp_event_list_limit(limit);
    let events = if let Some(topic) = topic_name {
        validate_topic_name(topic)?;
        photon
            .list_events_by_topic(topic, None, None, limit)
            .await
            .map_err(|e| photon_io_err("list events", e))?
    } else {
        photon
            .list_recent_events(limit)
            .await
            .map_err(|e| photon_io_err("list recent events", e))?
    };
    Ok(events.iter().map(map_transport_event).collect())
}

/// Single event detail by validated id.
pub async fn load_event(photon: &Photon, id: &str) -> Result<Option<EventDetail>, OpsError> {
    validate_event_id(id)?;
    let transport = photon
        .get_event(id)
        .await
        .map_err(|e| photon_io_err("get transport event", e))?;

    Ok(transport.map(|t| {
        event_detail_from_transport(
            t.event_id,
            t.topic_name,
            t.topic_key,
            t.seq,
            t.created_at.to_rfc3339(),
            t.payload_json,
            t.actor_json,
        )
    }))
}
