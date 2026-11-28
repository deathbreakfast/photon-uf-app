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

/// Summary of a Photon subscription for list/detail views: configuration
/// plus read-state visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSummary {
    /// Unique subscription identifier.
    pub subscription_id: String,
    /// Human-readable subscription name.
    pub subscription_name: String,
    /// Name of the topic this subscription is attached to.
    pub topic_name: String,
    /// Whether the subscription is currently enabled.
    pub enabled: bool,
    /// Dispatch mode (e.g. at-least-once, durable) as a display string.
    pub mode: String,
    /// Optional topic key filter restricting which keys this subscription receives.
    pub topic_key_filter: Option<String>,
    /// Number of events the subscription's checkpoint is behind the topic head.
    pub checkpoint_lag: i64,
    /// Sequence number of the last event this subscription has processed, if any.
    pub last_seq: Option<i64>,
    /// Timestamp of the last successfully processed event, if any.
    pub last_processed_at: Option<String>,
}

/// Summary of a single Photon event for list views and dashboards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Full detail record for a single event: metadata, masked payload and delivery rows.
///
/// Personal values never travel in this record. Fields the topic schema marks
/// `"x-personal": true` (and a person's id in [`PublishedBy`]) come back masked; the event
/// page asks for the real values with a recorded reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDetail {
    /// Unique event identifier.
    pub event_id: String,
    /// Name of the topic the event was published to.
    pub topic_name: String,
    /// Optional key the event was published under.
    pub topic_key: Option<String>,
    /// Sequence number of the event within its topic partition.
    pub seq: i64,
    /// RFC3339 timestamp of when the event was created.
    pub created_at: String,
    /// Delivery status recorded for the event (e.g. delivered, dead-lettered).
    pub delivery_status: String,
    /// Payload JSON with personal values replaced by [`MASKED_VALUE`]; `Null` when expired.
    pub payload_json: serde_json::Value,
    /// Top-level payload fields in schema order, personal ones masked.
    pub fields: Vec<PayloadField>,
    /// Who published the event, with a person's id masked.
    pub published_by: PublishedBy,
    /// Names of the payload fields the topic schema marks personal.
    pub personal_fields: Vec<String>,
    /// One row per handler on the topic: handled, waiting or failed.
    pub deliveries: Vec<DeliveryRow>,
    /// How long the store keeps payloads, in seconds; `None` when it isn't set.
    pub retention_secs: Option<u64>,
    /// True when the transport record is no longer available for this event id.
    pub transport_expired: bool,
}

impl EventDetail {
    /// Whether the page has anything to reveal: a personal payload field or a person's id.
    #[must_use]
    pub fn has_personal(&self) -> bool {
        !self.transport_expired
            && (self.fields.iter().any(|f| f.personal)
                || matches!(self.published_by, PublishedBy::Person))
    }
}

/// Dots shown in place of a personal value.
pub const MASKED_VALUE: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

/// One top-level payload field on the event page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadField {
    /// Field name as serialized in the payload.
    pub name: String,
    /// Display value (strings unquoted, everything else as JSON); `None` while masked.
    pub value: Option<String>,
    /// Whether the topic schema marks the field personal.
    pub personal: bool,
}

/// Who published an event, as the event page names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishedBy {
    /// A signed-in person; their id is personal and stays masked until a reveal.
    Person,
    /// A system job or a service user.
    System,
    /// Nobody signed in, or the actor wasn't recorded.
    Anonymous,
}

impl PublishedBy {
    /// Label for the facts grid.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Person => "A person",
            Self::System => "The system",
            Self::Anonymous => "Anonymous",
        }
    }
}

/// Where one event stands for one subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryState {
    /// The subscription's checkpoint is at or past the event's seq.
    Handled,
    /// The subscription hasn't reached the event yet.
    Waiting {
        /// 1 when the event is next in line for this subscription.
        position: i64,
    },
    /// The handler failed on the event and it went to the dead-letter queue, or is retrying.
    Failed {
        /// Delivery attempts so far.
        attempts: u32,
        /// Sanitized error text from the last attempt.
        error: String,
        /// RFC3339 time of the next retry while the handler is still retrying.
        next_retry_at: Option<String>,
    },
}

/// One subscription's row in the event page's Subscriptions card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRow {
    /// Subscription id (`{topic}:{handler}`), the registry key Redeliver takes.
    pub subscription_id: String,
    /// Display name (the durable name, or the id).
    pub subscription_name: String,
    /// Handled, waiting or failed.
    pub state: DeliveryState,
    /// RFC3339 time the outcome happened, where it's known.
    pub at: Option<String>,
    /// Whether the viewer holds Run on the subscription (filled in by the server fn).
    pub can_run: bool,
}

/// Rows on one page of the Events list.
pub const EVENTS_PAGE_SIZE: u32 = 25;

/// Deepest page the Events list serves ([`EVENTS_PAGE_SIZE`] rows each).
pub const MAX_EVENTS_PAGE: u32 = 40;

/// Time window an events search covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EventsRange {
    /// The last 60 minutes.
    LastHour,
    /// The last 24 hours.
    #[default]
    Last24h,
    /// The last 7 days.
    Last7d,
    /// Whole UTC days from [`EventsQuery::from`] through [`EventsQuery::to`].
    Custom,
}

impl EventsRange {
    /// Every range, in menu order.
    pub const ALL: [Self; 4] = [Self::LastHour, Self::Last24h, Self::Last7d, Self::Custom];

    /// Value in the `range` query parameter.
    #[must_use]
    pub const fn as_param(self) -> &'static str {
        match self {
            Self::LastHour => "1h",
            Self::Last24h => "24h",
            Self::Last7d => "7d",
            Self::Custom => "custom",
        }
    }

    /// Parse a `range` query parameter; unknown values read as `None`.
    #[must_use]
    pub fn from_param(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_param() == raw)
    }

    /// Menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LastHour => "Last hour",
            Self::Last24h => "Last 24 hours",
            Self::Last7d => "Last 7 days",
            Self::Custom => "Custom range",
        }
    }

    /// Phrase for empty states ("No events in the last hour").
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        match self {
            Self::LastHour => "in the last hour",
            Self::Last24h => "in the last 24 hours",
            Self::Last7d => "in the last 7 days",
            Self::Custom => "in this range",
        }
    }
}

/// One Events list request: filters, time window and page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventsQuery {
    /// Only events on this topic.
    pub topic: Option<String>,
    /// An event id (exact) or a key (exact).
    pub q: Option<String>,
    /// Time window.
    pub range: EventsRange,
    /// First day of a [`EventsRange::Custom`] range, `YYYY-MM-DD` (UTC).
    pub from: Option<String>,
    /// Last day of a [`EventsRange::Custom`] range, `YYYY-MM-DD` (UTC), inclusive.
    pub to: Option<String>,
    /// 1-based page number.
    pub page: u32,
}

impl EventsQuery {
    /// Read the Events page's URL parameters: `q`, `topic`, `range`, `from`, `to`, `page`.
    ///
    /// Missing or unreadable values fall back to the defaults (no filters, the last 24 hours,
    /// page 1).
    pub fn from_params(get: impl Fn(&str) -> Option<String>) -> Self {
        let text = |name: &str| {
            get(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Self {
            topic: text("topic"),
            q: text("q"),
            range: text("range")
                .and_then(|r| EventsRange::from_param(&r))
                .unwrap_or_default(),
            from: text("from"),
            to: text("to"),
            page: text("page").and_then(|p| p.parse().ok()).unwrap_or(1),
        }
    }

    /// URL query string for this query (no leading `?`), leaving defaults out.
    #[must_use]
    pub fn to_query_string(&self) -> String {
        let mut parts = Vec::new();
        let mut push = |name: &str, value: &str| {
            parts.push(format!("{name}={}", crate::encode_ops_path_segment(value)));
        };
        if let Some(q) = &self.q {
            push("q", q);
        }
        if let Some(topic) = &self.topic {
            push("topic", topic);
        }
        if self.range != EventsRange::default() {
            push("range", self.range.as_param());
        }
        if self.range == EventsRange::Custom {
            if let Some(from) = &self.from {
                push("from", from);
            }
            if let Some(to) = &self.to {
                push("to", to);
            }
        }
        if self.page > 1 {
            push("page", &self.page.to_string());
        }
        parts.join("&")
    }
}

/// One page of events, newest first, with the total when the store can count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsPage {
    /// Up to [`EVENTS_PAGE_SIZE`] rows.
    pub rows: Vec<EventSummary>,
    /// Events matching the filters in the window; `None` when the store can't count.
    pub total: Option<u64>,
    /// Page served (1-based, clamped).
    pub page: u32,
    /// Previous page number, if any.
    pub prev: Option<u32>,
    /// Next page number, if any.
    pub next: Option<u32>,
    /// Set when `q` is an event id the reader can open; the page navigates there.
    pub exact_id: Option<String>,
}

/// One entry in a topic's reveal log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevealRecordRow {
    /// Event whose personal fields were shown.
    pub event_id: String,
    /// User id of the person who revealed them; `None` once that account is deleted.
    pub revealed_by: Option<String>,
    /// RFC3339 time of the reveal.
    pub revealed_at: String,
    /// Reason picked in the dialog.
    pub reason: RevealReason,
    /// Details typed in the dialog.
    pub details: String,
}

/// Why someone revealed an event's personal fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevealReason {
    /// Answering a support request.
    Support,
    /// Debugging a handler.
    Debugging,
    /// Handling a data subject request.
    DataSubjectRequest,
}

impl RevealReason {
    /// Every reason, in menu order.
    pub const ALL: [Self; 3] = [Self::Support, Self::Debugging, Self::DataSubjectRequest];

    /// Stored value and form value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Support => "support",
            Self::Debugging => "debugging",
            Self::DataSubjectRequest => "data_subject_request",
        }
    }

    /// Parse a stored or form value.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == raw)
    }

    /// Menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Support => "Support request",
            Self::Debugging => "Debugging a handler",
            Self::DataSubjectRequest => "Data subject request",
        }
    }
}

/// Unmasked values from a reveal, for this page view only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevealedFields {
    /// Personal payload fields with their real values.
    pub fields: Vec<PayloadField>,
    /// Raw payload JSON, unmasked.
    pub payload_json: serde_json::Value,
    /// The publishing person's user id, when a person published the event.
    pub published_by_id: Option<String>,
    /// The record as written to the reveal log.
    pub record: RevealRecordRow,
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
