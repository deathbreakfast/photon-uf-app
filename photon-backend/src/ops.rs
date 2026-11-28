//! Photon IO + mapping for ops UI reads (feature `ops`).
//!
//! Server functions in `photon-app` resolve session / Photon context, then call
//! these helpers so SSR and `IsolatedLab` tests can exercise the same path without
//! Leptos request plumbing.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use photon::{ListEventsQuery, Photon};

use crate::{
    checkpoint_delivery_state, clamp_event_list_limit, clamp_events_page, count_since,
    dashboard_stats, event_detail_from_transport, event_detail_transport_expired,
    event_summary_from_transport, events_window, find_checkpoint_head_seq, find_checkpoint_seq,
    find_subscription_by_id, find_topic_by_name, normalize_events_search, payload_fields,
    published_by, sort_topics_by_name, subscription_summary_from_handler, topic_summary,
    validate_event_id, validate_subscription_id, validate_topic_name, CheckpointRow,
    DashboardStats, DeliveryRow, DeliveryState, EventDetail, EventSummary, EventsPage, EventsQuery,
    EventsQueryError, PayloadField, PhotonIdError, SubscriptionSummary, TopicSummary,
    EVENTS_PAGE_SIZE, MAX_EVENTS_PAGE,
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
    /// Events search filters failed validation.
    InvalidQuery(EventsQueryError),
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
            Self::InvalidQuery(e) => write!(f, "{e}"),
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

/// Handler inventory + checkpoints via `admin_snapshot`.
pub async fn list_subscriptions(photon: &Photon) -> Result<Vec<SubscriptionSummary>, OpsError> {
    let snap = photon
        .admin_snapshot()
        .await
        .map_err(|e| photon_io_err("load admin snapshot", e))?;
    let checkpoints: Vec<CheckpointRow> = snap
        .checkpoints
        .iter()
        .map(|c| {
            (
                c.subscription_name.clone(),
                c.topic_name.clone(),
                c.topic_key.clone(),
                c.last_seq,
                c.head_seq,
            )
        })
        .collect();

    let mut list = Vec::with_capacity(snap.handlers.len());
    for h in snap.handlers {
        let last_seq =
            find_checkpoint_seq(&checkpoints, h.subscription_name.as_deref(), &h.topic_name);
        let head_seq =
            find_checkpoint_head_seq(&checkpoints, h.subscription_name.as_deref(), &h.topic_name);
        list.push(subscription_summary_from_handler(
            h.registry_key,
            h.subscription_name.or(h.consumer_group),
            h.topic_name,
            h.mode,
            last_seq,
            head_seq,
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
    validate_subscription_id(id)?;
    let subs = list_subscriptions(photon).await?;
    Ok(find_subscription_by_id(&subs, id).cloned())
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

/// Most events [`load_events_page`] reads for one page: a topic's whole window, or enough
/// rows across topics after dropping those the reader can't View.
const MAX_EVENTS_SCAN: usize = 10_000;

/// One page of the Events list for `query`, searching only `topics`.
///
/// `topics` is what the reader may see: the one topic they asked for (already checked), or
/// every topic they can View. When `q` is an event id on one of those topics (live, or still
/// known to the dead-letter queue), the page comes back empty with `exact_id` set so the
/// caller can open the event; otherwise `q` matches keys exactly. Rows are newest first and
/// the total is exact when the store can count.
///
/// # Errors
///
/// [`OpsError::InvalidQuery`] for a bad range or search text, [`OpsError::InvalidId`] for a
/// bad topic name, and [`OpsError::PhotonIo`] when Photon fails.
pub async fn load_events_page(
    photon: &Photon,
    query: &EventsQuery,
    topics: &[String],
    now: DateTime<Utc>,
) -> Result<EventsPage, OpsError> {
    let page = clamp_events_page(query.page);
    let (since, until) = events_window(query, now).map_err(OpsError::InvalidQuery)?;
    let q = normalize_events_search(query.q.as_deref()).map_err(OpsError::InvalidQuery)?;
    let topic = query
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(topic) = topic {
        validate_topic_name(topic)?;
    }
    let visible =
        |name: &str| topics.iter().any(|t| t == name) && topic.is_none_or(|wanted| wanted == name);
    let empty = |exact_id| EventsPage {
        rows: Vec::new(),
        total: Some(0),
        page,
        prev: None,
        next: None,
        exact_id,
    };

    if let Some(q) = q.filter(|q| validate_event_id(q).is_ok()) {
        let live_topic = photon
            .get_event(q)
            .await
            .map_err(|e| photon_io_err("get transport event", e))?
            .map(|ev| ev.topic_name);
        let known_topic = live_topic.or_else(|| {
            photon
                .dlq_for_event(q)
                .first()
                .map(|r| r.topic_name.clone())
        });
        if known_topic.as_deref().is_some_and(visible) {
            return Ok(empty(Some(q.to_string())));
        }
    }
    if topic.is_some_and(|t| !visible(t)) {
        return Ok(empty(None));
    }

    let page_size = EVENTS_PAGE_SIZE as usize;
    let skip = (page as usize - 1) * page_size;
    let want = skip + page_size + 1;
    let filter_topics = topic.is_none() && photon.registry().iter().any(|d| !visible(d.topic_name));
    // Photon orders a topic's events by seq, which counts per key, so the newest `want` by
    // seq aren't the newest by time. Read the topic's window (up to the scan cap) and sort.
    let mut limit = if topic.is_some() {
        MAX_EVENTS_SCAN
    } else {
        want
    };
    let mut kept = loop {
        let events = photon
            .list_events(&ListEventsQuery {
                topic: topic.map(str::to_string),
                key: q.map(str::to_string),
                since: Some(since),
                until,
                newest_first: true,
                limit,
                ..ListEventsQuery::default()
            })
            .await
            .map_err(|e| photon_io_err("list events", e))?;
        let fetched = events.len();
        let kept: Vec<_> = events
            .into_iter()
            .filter(|e| visible(&e.topic_name))
            .collect();
        if !filter_topics || kept.len() >= want || fetched < limit || limit >= MAX_EVENTS_SCAN {
            break kept;
        }
        limit = (limit * 4).min(MAX_EVENTS_SCAN);
    };
    kept.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.seq.cmp(&a.seq)));
    let has_more = kept.len() > skip + page_size;
    let rows = kept
        .iter()
        .skip(skip)
        .take(page_size)
        .map(map_transport_event)
        .collect();

    let count_topics: Vec<&str> =
        topic.map_or_else(|| topics.iter().map(String::as_str).collect(), |t| vec![t]);
    let total = count_window(photon, &count_topics, q, since, until).await?;

    Ok(EventsPage {
        rows,
        total,
        page,
        prev: (page > 1).then(|| page - 1),
        next: (has_more && page < MAX_EVENTS_PAGE).then_some(page + 1),
        exact_id: None,
    })
}

/// Events on `topics` with `key` in `[since, until)`, or `None` when the store can't count.
async fn count_window(
    photon: &Photon,
    topics: &[&str],
    key: Option<&str>,
    since: DateTime<Utc>,
    until: Option<DateTime<Utc>>,
) -> Result<Option<u64>, OpsError> {
    let count = |topic, at| async move {
        photon
            .count_events_by_topic(topic, key, Some(at))
            .await
            .map_err(|e| photon_io_err("count events", e))
    };
    let mut total = 0u64;
    for topic in topics {
        let Some(from_since) = count(topic, since).await? else {
            return Ok(None);
        };
        let from_until = match until {
            Some(until) => count(topic, until).await?.unwrap_or(0),
            None => 0,
        };
        total += from_since.saturating_sub(from_until);
    }
    Ok(Some(total))
}

/// Payload fields the topic's `#[topic]` declaration marks `#[photon(personal)]`.
#[must_use]
pub fn personal_fields_for(photon: &Photon, topic_name: &str) -> Vec<&'static str> {
    photon
        .registry()
        .iter()
        .find(|d| d.topic_name == topic_name)
        .map_or_else(Vec::new, photon::TopicDescriptor::personal_fields)
}

/// Single event detail by validated id, with personal values masked.
///
/// A live event comes back with its masked payload and one delivery row per handler on its
/// topic. An event whose payload has expired but which the dead-letter queue still knows
/// comes back with `transport_expired` and only its metadata (its time is when it was
/// dead-lettered, which is within a retry of when it was published). Anything else is
/// `None`.
///
/// # Errors
///
/// [`OpsError::InvalidId`] for a bad id and [`OpsError::PhotonIo`] when Photon fails.
pub async fn load_event(photon: &Photon, id: &str) -> Result<Option<EventDetail>, OpsError> {
    validate_event_id(id)?;
    let snap = photon
        .admin_snapshot()
        .await
        .map_err(|e| photon_io_err("load admin snapshot", e))?;
    let transport = photon
        .get_event(id)
        .await
        .map_err(|e| photon_io_err("get transport event", e))?;
    let dlq = photon.dlq_for_event(id);

    let mut detail = if let Some(t) = transport {
        let personal = personal_fields_for(photon, &t.topic_name);
        event_detail_from_transport(
            t.event_id,
            t.topic_name,
            t.topic_key,
            t.seq,
            t.created_at.to_rfc3339(),
            &t.payload_json,
            &t.actor_json,
            &personal,
        )
    } else if let Some(record) = dlq.first() {
        event_detail_transport_expired(
            id,
            record.topic_name.clone(),
            record.topic_key.clone(),
            record.seq,
            record.recorded_at.to_rfc3339(),
            "dead-lettered",
        )
    } else {
        return Ok(None);
    };
    detail.retention_secs = snap.backend.max_replay_window_secs;
    detail.deliveries = delivery_rows(photon, &snap, &detail, &dlq).await?;
    Ok(Some(detail))
}

/// One row per handler on the event's topic: Failed when the handler is retrying the event
/// or dead-lettered it, otherwise Handled or Waiting from its checkpoint.
async fn delivery_rows(
    photon: &Photon,
    snap: &photon::AdminSnapshot,
    event: &EventDetail,
    dlq: &[photon::DlqRecord],
) -> Result<Vec<DeliveryRow>, OpsError> {
    let mut rows = Vec::new();
    for h in snap
        .handlers
        .iter()
        .filter(|h| h.topic_name == event.topic_name)
    {
        let sub_name = h
            .subscription_name
            .as_deref()
            .or(h.consumer_group.as_deref());
        let retrying = h
            .health
            .retrying
            .as_ref()
            .filter(|r| r.event_id == event.event_id);
        let dead = dlq
            .iter()
            .find(|r| sub_name.is_some() && r.subscription_name.as_deref() == sub_name);
        let (state, at) = if let Some(r) = retrying {
            (
                DeliveryState::Failed {
                    attempts: r.attempt,
                    error: h.health.last_error.clone().unwrap_or_default(),
                    next_retry_at: Some(r.next_retry_at.to_rfc3339()),
                },
                h.health.last_error_at.map(|t| t.to_rfc3339()),
            )
        } else if let Some(d) = dead {
            (
                DeliveryState::Failed {
                    attempts: d.attempt,
                    error: d.error.clone(),
                    next_retry_at: None,
                },
                Some(d.recorded_at.to_rfc3339()),
            )
        } else {
            let checkpoint = match sub_name {
                Some(sub) => photon
                    .get_checkpoint_seq(sub, &event.topic_name, event.topic_key.as_deref())
                    .await
                    .map_err(|e| photon_io_err("load checkpoint", e))?,
                None => None,
            };
            let state = checkpoint_delivery_state(event.seq, checkpoint);
            let at = (state == DeliveryState::Handled
                && h.health.last_handled_seq == Some(event.seq))
            .then(|| h.health.last_handled_at.map(|t| t.to_rfc3339()))
            .flatten();
            (state, at)
        };
        rows.push(DeliveryRow {
            subscription_id: h.registry_key.clone(),
            subscription_name: sub_name.unwrap_or(&h.registry_key).to_string(),
            state,
            at,
            can_run: false,
        });
    }
    rows.sort_by(|a, b| a.subscription_name.cmp(&b.subscription_name));
    Ok(rows)
}

/// An event's personal values, unmasked, for a recorded reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventReveal {
    /// Topic the event is on; the caller checks View on it before using anything else.
    pub topic_name: String,
    /// Personal payload fields with their real values.
    pub fields: Vec<PayloadField>,
    /// The whole payload, unmasked.
    pub payload_json: serde_json::Value,
    /// The publishing person's user id, when a person published it.
    pub published_by_id: Option<String>,
}

/// Unmasked personal values of a live event, or `None` when its payload is gone.
///
/// # Errors
///
/// [`OpsError::InvalidId`] for a bad id and [`OpsError::PhotonIo`] when Photon fails.
pub async fn reveal_event(photon: &Photon, id: &str) -> Result<Option<EventReveal>, OpsError> {
    validate_event_id(id)?;
    let Some(t) = photon
        .get_event(id)
        .await
        .map_err(|e| photon_io_err("get transport event", e))?
    else {
        return Ok(None);
    };
    let personal = personal_fields_for(photon, &t.topic_name);
    let fields = payload_fields(&t.payload_json, &personal, true)
        .into_iter()
        .filter(|f| f.personal)
        .collect();
    Ok(Some(EventReveal {
        topic_name: t.topic_name,
        fields,
        published_by_id: published_by(&t.actor_json).1,
        payload_json: t.payload_json,
    }))
}

/// Topic of a live or dead-lettered event, for permission checks before an action.
///
/// # Errors
///
/// [`OpsError::InvalidId`] for a bad id and [`OpsError::PhotonIo`] when Photon fails.
pub async fn event_topic(photon: &Photon, id: &str) -> Result<Option<String>, OpsError> {
    validate_event_id(id)?;
    let live = photon
        .get_event(id)
        .await
        .map_err(|e| photon_io_err("get transport event", e))?
        .map(|t| t.topic_name);
    Ok(live.or_else(|| {
        photon
            .dlq_for_event(id)
            .first()
            .map(|r| r.topic_name.clone())
    }))
}

/// Send one event to one subscription again, recorded as done `by` a user id.
///
/// # Errors
///
/// [`OpsError::InvalidId`] for a bad id, and [`OpsError::PhotonIo`] when the subscription or
/// event is unknown, the event is on another topic, or the handler fails again.
pub async fn redeliver_event(
    photon: &Photon,
    event_id: &str,
    subscription_id: &str,
    by: &str,
) -> Result<(), OpsError> {
    validate_event_id(event_id)?;
    validate_subscription_id(subscription_id)?;
    photon
        .redeliver(event_id, subscription_id, by)
        .await
        .map_err(|e| photon_io_err("redeliver event", e))
}
