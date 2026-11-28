//! DTO shapes and typed blank-id errors for Photon ops contracts.

use serde::{Deserialize, Serialize};

/// Summary of a Photon topic for list/detail views: registry metadata plus
/// recent traffic counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicSummary {
    /// Topic name as registered with Photon.
    pub topic_name: String,
    /// Optional key field the topic is partitioned/keyed by.
    pub keyed_by: Option<String>,
    /// JSON-encoded schema description for the topic's payload.
    pub schema_json: String,
    /// Number of events published to this topic in the last 24 hours.
    pub event_count_24h: u64,
    /// Number of subscriptions currently registered against this topic.
    pub subscription_count: u32,
}

/// How a subscription is doing, in the words the subscriptions list and page show.
///
/// Variants are declared worst first, so sorting by this type puts the subscriptions that
/// need someone at the top of the list.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SubscriptionHealth {
    /// Its handler failed in the last 24 hours and is still failing.
    Failing,
    /// Events wait and nothing has been handled for a while.
    Stalled,
    /// Someone paused it.
    Paused,
    /// A few events wait and it is handling them.
    CatchingUp,
    /// No checkpoint and no events yet.
    #[default]
    NotStarted,
    /// Nothing waits.
    Healthy,
}

impl SubscriptionHealth {
    /// Every health, worst first (the order of the Health filter).
    pub const ALL: [Self; 6] = [
        Self::Failing,
        Self::Stalled,
        Self::Paused,
        Self::CatchingUp,
        Self::NotStarted,
        Self::Healthy,
    ];

    /// The word shown on badges: "Catching up", "Not started"…
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Failing => "Failing",
            Self::Stalled => "Stalled",
            Self::Paused => "Paused",
            Self::CatchingUp => "Catching up",
            Self::NotStarted => "Not started",
            Self::Healthy => "Healthy",
        }
    }

    /// The sentence beside the badge on the subscription's Health card.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Failing => "The handler is taking events and failing on them. Photon retries each one with backoff.",
            Self::Stalled => "Events are waiting and none has been handled for a while. Nothing has failed: the handler isn't taking them.",
            Self::Paused => "Paused by a person. Events wait until someone resumes it.",
            Self::CatchingUp => "A few events waiting, being handled now.",
            Self::NotStarted => "No events have reached it yet.",
            Self::Healthy => "Up to date. Every event has been handled.",
        }
    }

    /// Stable key for the `?health=` query parameter.
    #[must_use]
    pub const fn query_key(self) -> &'static str {
        match self {
            Self::Failing => "failing",
            Self::Stalled => "stalled",
            Self::Paused => "paused",
            Self::CatchingUp => "catching-up",
            Self::NotStarted => "not-started",
            Self::Healthy => "healthy",
        }
    }

    /// Parses a `?health=` value; anything else means "Any health".
    #[must_use]
    pub fn from_query_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|h| h.query_key() == key)
    }
}

/// An event waiting for its next delivery attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryingEvent {
    /// Event being retried.
    pub event_id: String,
    /// Number of the next attempt (the first delivery is attempt 1).
    pub attempt: u32,
    /// RFC 3339 time of the next attempt unless retried sooner.
    pub next_retry_at: String,
}

/// Who paused a subscription, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PausedBy {
    /// Display label of the person who paused it.
    pub by: String,
    /// RFC 3339 time it was paused.
    pub at: String,
}

/// Summary of a Photon subscription for list/detail views: configuration,
/// read state and health.
///
/// Delivery figures (`last_handled_at`, `failures_24h`, the last error, `retrying`) are kept in
/// memory by the node that answered; other nodes keep their own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSummary {
    /// Unique subscription identifier.
    pub subscription_id: String,
    /// Human-readable subscription name.
    pub subscription_name: String,
    /// Name of the topic this subscription is attached to.
    pub topic_name: String,
    /// Whether the subscription is taking events (false while paused).
    pub enabled: bool,
    /// Dispatch mode (`durable` or `consumer_group`) as reported by Photon.
    pub mode: String,
    /// Optional topic key filter restricting which keys this subscription receives.
    pub topic_key_filter: Option<String>,
    /// Number of events the subscription's checkpoint is behind the topic head.
    pub checkpoint_lag: i64,
    /// Sequence number of the last event this subscription has processed, if any.
    pub last_seq: Option<i64>,
    /// Highest sequence on the topic partition, if known.
    pub head_seq: Option<i64>,
    /// RFC 3339 time of the last successfully processed event, if any.
    pub last_processed_at: Option<String>,
    /// How the subscription is doing.
    pub health: SubscriptionHealth,
    /// Events after the checkpoint (head minus checkpoint).
    pub waiting: i64,
    /// Seconds the oldest waiting event has waited, when one waits and the backend knows.
    pub oldest_wait_secs: Option<i64>,
    /// RFC 3339 time the handler last handled an event successfully.
    pub last_handled_at: Option<String>,
    /// Deliveries that ended in the dead-letter queue over the last 24 hours.
    pub failures_24h: u32,
    /// Text of the last failure.
    pub last_error: Option<String>,
    /// RFC 3339 time of the last failure.
    pub last_error_at: Option<String>,
    /// The event waiting on a retry backoff, if any.
    pub retrying: Option<RetryingEvent>,
    /// Set while the subscription is paused.
    pub paused: Option<PausedBy>,
    /// Whether the backend supports pause, resume, replay and skip.
    pub supports_control: bool,
    /// How long the event log keeps events, when the backend bounds it.
    pub retention_secs: Option<u64>,
}

impl SubscriptionSummary {
    /// Whether this is a consumer-group (load-balanced) subscription.
    #[must_use]
    pub fn is_consumer_group(&self) -> bool {
        self.mode == "consumer_group"
    }
}

/// The events a subscription still owes, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingEvents {
    /// The oldest waiting events, up to the requested limit.
    pub events: Vec<EventSummary>,
    /// Every event waiting, including those past the limit.
    pub total: i64,
}

/// Where a replay starts, as the Replay dialog asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayPoint {
    /// Start after this sequence number (the replay delivers `seq + 1` onwards).
    AfterSeq(i64),
    /// The first event created at or after this RFC 3339 time.
    Time(String),
    /// This event, by id.
    Event(String),
}

/// What a replay will resend, before it runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayPreviewSummary {
    /// First sequence the replay delivers again.
    pub from_seq: i64,
    /// Events the subscription will receive after the replay.
    pub count: u64,
    /// How many of those it has already handled once.
    pub already_handled: u64,
}

/// Summary of a single Photon event for list views and dashboards.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSummary {
    /// Unique event identifier.
    pub event_id: String,
    /// Name of the topic the event was published to.
    pub topic_name: String,
    /// Optional key the event was published under.
    pub topic_key: Option<String>,
    /// Sequence number of the event within its topic.
    pub seq: i64,
    /// RFC3339 timestamp of when the event was created.
    pub created_at: String,
    /// Short, human-readable preview of the event's delivery status/payload.
    pub payload_preview: String,
}

/// Aggregate counters shown on the Photon dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardStats {
    /// Total number of registered topics.
    pub topic_count: u32,
    /// Total number of registered subscriptions.
    pub subscription_count: u32,
    /// Number of events published across all topics in the last 24 hours.
    pub event_count_24h: u64,
}

/// Full detail record for a single event, including payload and actor context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDetail {
    /// Unique event identifier.
    pub event_id: String,
    /// Name of the topic the event was published to.
    pub topic_name: String,
    /// Optional key the event was published under.
    pub topic_key: Option<String>,
    /// Sequence number of the event within its topic.
    pub seq: i64,
    /// RFC3339 timestamp of when the event was created.
    pub created_at: String,
    /// Delivery status recorded for the event (e.g. delivered, dead-lettered).
    pub delivery_status: String,
    /// Raw event payload JSON, if the transport record is still available.
    pub payload_json: serde_json::Value,
    /// Raw actor JSON captured at publish time, if the transport record is still available.
    pub actor_json: serde_json::Value,
    /// True when the transport record is no longer available for this event id.
    pub transport_expired: bool,
}

/// Invalid topic name, subscription id, or event id rejected before Photon lookups.
///
/// Callers map this into Leptos `ServerFnError` (or equivalent) at the `#[server]`
/// boundary; the Display text stays stable for UI and contract tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PhotonIdError {
    /// Topic name was empty or whitespace-only.
    EmptyTopicName,
    /// Subscription id was empty or whitespace-only.
    EmptySubscriptionId,
    /// Event id was empty or whitespace-only.
    EmptyEventId,
    /// Topic name exceeded [`crate::MAX_PHOTON_ID_CHARS`].
    TopicNameTooLong,
    /// Subscription id exceeded [`crate::MAX_PHOTON_ID_CHARS`].
    SubscriptionIdTooLong,
    /// Event id exceeded [`crate::MAX_PHOTON_ID_CHARS`].
    EventIdTooLong,
    /// Topic name contained `/`, `\`, controls, or was `.` / `..`.
    UnsafeTopicName,
    /// Subscription id contained `/`, `\`, controls, or was `.` / `..`.
    UnsafeSubscriptionId,
    /// Event id contained `/`, `\`, controls, or was `.` / `..`.
    UnsafeEventId,
}

impl std::fmt::Display for PhotonIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTopicName => write!(f, "Photon topic name is required"),
            Self::EmptySubscriptionId => write!(f, "Photon subscription id is required"),
            Self::EmptyEventId => write!(f, "Photon event id is required"),
            Self::TopicNameTooLong => write!(f, "Photon topic name is too long"),
            Self::SubscriptionIdTooLong => write!(f, "Photon subscription id is too long"),
            Self::EventIdTooLong => write!(f, "Photon event id is too long"),
            Self::UnsafeTopicName => {
                write!(f, "Photon topic name contains unsafe path characters")
            }
            Self::UnsafeSubscriptionId => {
                write!(f, "Photon subscription id contains unsafe path characters")
            }
            Self::UnsafeEventId => {
                write!(f, "Photon event id contains unsafe path characters")
            }
        }
    }
}

impl std::error::Error for PhotonIdError {}
