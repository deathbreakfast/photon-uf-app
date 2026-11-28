//! Pure topic / subscription / event / dashboard mapping helpers.

use chrono::{DateTime, Utc};

use crate::types::{
    DashboardStats, EventDetail, EventSummary, PausedBy, RetryingEvent, SubscriptionHealth,
    SubscriptionSummary, TopicSummary,
};

/// How long events may wait with nothing handled before a subscription reads as Stalled.
pub const DEFAULT_STALL_AFTER: chrono::Duration = chrono::Duration::minutes(5);

/// Formats the short payload-preview chip shown in event list rows.
#[must_use]
pub fn format_delivery_preview(delivery_status: &str) -> String {
    format!("[{delivery_status}]")
}

/// Locates a topic summary by exact name (used by `get_topic` detail lookups).
#[must_use]
pub fn find_topic_by_name<'a>(
    topics: &'a [TopicSummary],
    topic_name: &str,
) -> Option<&'a TopicSummary> {
    topics.iter().find(|t| t.topic_name == topic_name)
}

/// Locates a subscription summary by exact id (used by `get_subscription`).
#[must_use]
pub fn find_subscription_by_id<'a>(
    subs: &'a [SubscriptionSummary],
    id: &str,
) -> Option<&'a SubscriptionSummary> {
    subs.iter().find(|s| s.subscription_id == id)
}

/// Filters subscriptions attached to a topic (topic-detail page contract).
#[must_use]
pub fn filter_subscriptions_by_topic<'a>(
    subs: &'a [SubscriptionSummary],
    topic_name: &str,
) -> Vec<&'a SubscriptionSummary> {
    subs.iter().filter(|s| s.topic_name == topic_name).collect()
}

/// Counts timestamps at or after `since` (dashboard / topic 24h windows).
#[must_use]
pub fn count_since(timestamps: &[DateTime<Utc>], since: DateTime<Utc>) -> u64 {
    timestamps.iter().filter(|ts| **ts >= since).count() as u64
}

/// Builds dashboard KPI counters after registry and store queries resolve.
#[must_use]
pub const fn dashboard_stats(
    topic_count: u32,
    subscription_count: u32,
    event_count_24h: u64,
) -> DashboardStats {
    DashboardStats {
        topic_count,
        subscription_count,
        event_count_24h,
    }
}

/// Sorts topic summaries by name (stable list contract for `get_topics`).
pub fn sort_topics_by_name(topics: &mut [TopicSummary]) {
    topics.sort_by(|a, b| a.topic_name.cmp(&b.topic_name));
}

/// Builds an [`EventSummary`] list-row preview from projected metadata fields.
#[must_use]
pub fn event_summary_from_meta(
    event_id: impl Into<String>,
    topic_name: impl Into<String>,
    topic_key: Option<String>,
    seq: i64,
    created_at: impl Into<String>,
    delivery_status: &str,
) -> EventSummary {
    EventSummary {
        event_id: event_id.into(),
        topic_name: topic_name.into(),
        topic_key,
        seq,
        created_at: created_at.into(),
        payload_preview: format_delivery_preview(delivery_status),
    }
}

/// Builds an [`EventSummary`] from a transport-log event (ops browse via Photon list APIs).
#[must_use]
pub fn event_summary_from_transport(
    event_id: impl Into<String>,
    topic_name: impl Into<String>,
    topic_key: Option<String>,
    seq: i64,
    created_at: impl Into<String>,
) -> EventSummary {
    event_summary_from_meta(event_id, topic_name, topic_key, seq, created_at, "stored")
}

/// Builds [`EventDetail`] when Valence metadata exists but transport payload is gone.
#[must_use]
pub fn event_detail_transport_expired(
    event_id: impl Into<String>,
    topic_name: impl Into<String>,
    topic_key: Option<String>,
    seq: i64,
    created_at: impl Into<String>,
    delivery_status: impl Into<String>,
) -> EventDetail {
    EventDetail {
        event_id: event_id.into(),
        topic_name: topic_name.into(),
        topic_key,
        seq,
        created_at: created_at.into(),
        delivery_status: delivery_status.into(),
        payload_json: serde_json::Value::Null,
        actor_json: serde_json::Value::Null,
        transport_expired: true,
    }
}

/// Builds [`EventDetail`] from a live transport event.
#[must_use]
pub fn event_detail_from_transport(
    event_id: impl Into<String>,
    topic_name: impl Into<String>,
    topic_key: Option<String>,
    seq: i64,
    created_at: impl Into<String>,
    payload_json: serde_json::Value,
    actor_json: serde_json::Value,
) -> EventDetail {
    EventDetail {
        event_id: event_id.into(),
        topic_name: topic_name.into(),
        topic_key,
        seq,
        created_at: created_at.into(),
        delivery_status: "stored".into(),
        payload_json,
        actor_json,
        transport_expired: false,
    }
}

/// Builds a [`SubscriptionSummary`] from Photon `admin_snapshot` handler + checkpoint fields.
#[must_use]
pub fn subscription_summary_from_handler(
    registry_key: impl Into<String>,
    subscription_name: Option<String>,
    topic_name: impl Into<String>,
    mode: impl Into<String>,
    last_seq: Option<i64>,
    head_seq: Option<i64>,
) -> SubscriptionSummary {
    let topic_name = topic_name.into();
    let registry_key = registry_key.into();
    let display_name = subscription_name.unwrap_or_else(|| registry_key.clone());
    let waiting = compute_checkpoint_lag(head_seq, last_seq);
    let health = derive_health(
        &HealthInputs {
            has_checkpoint: last_seq.is_some(),
            has_events: head_seq.is_some_and(|h| h > 0),
            waiting,
            ..HealthInputs::default()
        },
        Utc::now(),
        DEFAULT_STALL_AFTER,
    );
    SubscriptionSummary {
        subscription_id: registry_key,
        subscription_name: display_name,
        topic_name,
        enabled: true,
        mode: mode.into(),
        checkpoint_lag: waiting,
        last_seq,
        head_seq,
        health,
        waiting,
        ..SubscriptionSummary::default()
    }
}

/// A checkpoint row: `(subscription_name, topic_name, topic_key, last_seq, head_seq)`.
pub type CheckpointRow = (String, String, Option<String>, Option<i64>, Option<i64>);

/// Matches a checkpoint `last_seq` for a handler subscription/topic pair.
#[must_use]
pub fn find_checkpoint_seq(
    checkpoints: &[CheckpointRow],
    subscription_name: Option<&str>,
    topic_name: &str,
) -> Option<i64> {
    let sub = subscription_name?;
    checkpoints
        .iter()
        .find(|(s, t, _, _, _)| s == sub && t == topic_name)
        .and_then(|(_, _, _, last_seq, _)| *last_seq)
}

/// Matches a checkpoint `head_seq` for a handler subscription/topic pair.
#[must_use]
pub fn find_checkpoint_head_seq(
    checkpoints: &[CheckpointRow],
    subscription_name: Option<&str>,
    topic_name: &str,
) -> Option<i64> {
    let sub = subscription_name?;
    checkpoints
        .iter()
        .find(|(s, t, _, _, _)| s == sub && t == topic_name)
        .and_then(|(_, _, _, _, head_seq)| *head_seq)
}

/// Builds a [`TopicSummary`] from registry metadata plus traffic counts.
#[must_use]
pub fn topic_summary(
    topic_name: impl Into<String>,
    keyed_by: Option<String>,
    schema_json: impl Into<String>,
    event_count_24h: u64,
    subscription_count: u32,
) -> TopicSummary {
    TopicSummary {
        topic_name: topic_name.into(),
        keyed_by,
        schema_json: schema_json.into(),
        event_count_24h,
        subscription_count,
    }
}

/// Lag between a topic's head sequence and a subscription's committed checkpoint.
///
/// A missing checkpoint is treated as "nothing processed yet" (full lag). A missing or
/// unsupported head seq is treated as zero head (no visible lag) rather than as an error.
#[must_use]
pub fn compute_checkpoint_lag(head_seq: Option<i64>, last_seq: Option<i64>) -> i64 {
    let head = head_seq.unwrap_or(0);
    let last = last_seq.unwrap_or(0);
    (head - last).max(0)
}

/// What [`derive_health`] looks at for one subscription.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent facts about one subscription
pub struct HealthInputs {
    /// Someone paused it.
    pub paused: bool,
    /// A checkpoint exists.
    pub has_checkpoint: bool,
    /// The topic partition has events.
    pub has_events: bool,
    /// Events after the checkpoint.
    pub waiting: i64,
    /// Creation time of the oldest waiting event, when the backend knows it.
    pub oldest_waiting_at: Option<DateTime<Utc>>,
    /// When the handler last handled an event.
    pub last_handled_at: Option<DateTime<Utc>>,
    /// Dead-lettered deliveries in the last 24 hours.
    pub failures_24h: u32,
    /// When the last failure happened.
    pub last_error_at: Option<DateTime<Utc>>,
    /// An event is waiting on a retry.
    pub retrying: bool,
}

/// A subscription's health from what Photon reports about it.
///
/// Checked in order: Paused; Not started (no checkpoint and no events); Failing (failures in 24 h and the last outcome was a failure, or an event is
/// retrying); Stalled (events wait and nothing was handled for `stall_after` while they
/// waited); Catching up (events wait); otherwise Healthy.
#[must_use]
pub fn derive_health(
    inputs: &HealthInputs,
    now: DateTime<Utc>,
    stall_after: chrono::Duration,
) -> SubscriptionHealth {
    if inputs.paused {
        return SubscriptionHealth::Paused;
    }
    if !inputs.has_checkpoint && !inputs.has_events && inputs.waiting == 0 {
        return SubscriptionHealth::NotStarted;
    }
    let last_outcome_failed = match (inputs.last_error_at, inputs.last_handled_at) {
        (Some(err), Some(ok)) => err >= ok,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if inputs.failures_24h > 0 && (last_outcome_failed || inputs.retrying) {
        return SubscriptionHealth::Failing;
    }
    if inputs.waiting > 0 {
        // Nothing handled since the later of the last handled event and the oldest wait.
        let quiet_since = match (inputs.last_handled_at, inputs.oldest_waiting_at) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        if quiet_since.is_some_and(|since| now - since >= stall_after) {
            return SubscriptionHealth::Stalled;
        }
        return SubscriptionHealth::CatchingUp;
    }
    SubscriptionHealth::Healthy
}

/// One checkpoint partition from Photon's admin snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointState {
    /// Durable subscription name or consumer-group id.
    pub subscription_name: String,
    /// Topic name.
    pub topic_name: String,
    /// Partition or shard key.
    pub topic_key: Option<String>,
    /// Last committed sequence.
    pub last_seq: Option<i64>,
    /// Highest sequence in the partition.
    pub head_seq: Option<i64>,
    /// Creation time of the first event after `last_seq`.
    pub oldest_waiting_at: Option<DateTime<Utc>>,
}

/// A subscription's checkpoints folded into one read state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointAggregate {
    /// Lowest committed sequence across partitions (`None` when any has none).
    pub last_seq: Option<i64>,
    /// Highest head sequence across partitions.
    pub head_seq: Option<i64>,
    /// Events waiting, summed over partitions.
    pub waiting: i64,
    /// Earliest oldest-waiting time across partitions.
    pub oldest_waiting_at: Option<DateTime<Utc>>,
    /// The partition key when the subscription reads exactly one keyed partition.
    pub topic_key: Option<String>,
    /// Whether any checkpoint was committed.
    pub has_checkpoint: bool,
}

/// Folds every checkpoint of `subscription_name` on `topic_name` (one for a durable handler,
/// one per shard for a consumer group).
#[must_use]
pub fn aggregate_checkpoints(
    checkpoints: &[CheckpointState],
    subscription_name: &str,
    topic_name: &str,
) -> CheckpointAggregate {
    let rows: Vec<&CheckpointState> = checkpoints
        .iter()
        .filter(|c| c.subscription_name == subscription_name && c.topic_name == topic_name)
        .collect();
    let mut agg = CheckpointAggregate::default();
    if rows.is_empty() {
        return agg;
    }
    agg.has_checkpoint = rows.iter().any(|c| c.last_seq.is_some());
    agg.last_seq = if rows.iter().all(|c| c.last_seq.is_some()) {
        rows.iter().filter_map(|c| c.last_seq).min()
    } else {
        None
    };
    agg.head_seq = rows.iter().filter_map(|c| c.head_seq).max();
    agg.waiting = rows
        .iter()
        .map(|c| compute_checkpoint_lag(c.head_seq, c.last_seq))
        .sum();
    agg.oldest_waiting_at = rows.iter().filter_map(|c| c.oldest_waiting_at).min();
    if let [only] = rows.as_slice() {
        agg.topic_key.clone_from(&only.topic_key);
    }
    agg
}

/// Everything Photon reports about one handler, in the shape [`subscription_summary_from_snapshot`]
/// maps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandlerSnapshot {
    /// Stable registry key (the subscription id).
    pub registry_key: String,
    /// Durable name or consumer-group id.
    pub subscription_name: Option<String>,
    /// Topic it reads.
    pub topic_name: String,
    /// `durable` or `consumer_group`.
    pub mode: String,
    /// Folded checkpoint state.
    pub checkpoint: CheckpointAggregate,
    /// When it last handled an event.
    pub last_handled_at: Option<DateTime<Utc>>,
    /// Dead-lettered deliveries in 24 hours.
    pub failures_24h: u32,
    /// Last failure text.
    pub last_error: Option<String>,
    /// Last failure time.
    pub last_error_at: Option<DateTime<Utc>>,
    /// `(event_id, attempt, next_retry_at)` of the event on a retry backoff.
    pub retrying: Option<(String, u32, DateTime<Utc>)>,
    /// `(by, at)` while paused.
    pub paused: Option<(String, DateTime<Utc>)>,
    /// Backend supports pause, replay and skip.
    pub supports_control: bool,
    /// Backend retention window.
    pub retention_secs: Option<u64>,
}

/// Builds a [`SubscriptionSummary`] with its health from a handler snapshot.
#[must_use]
pub fn subscription_summary_from_snapshot(
    h: HandlerSnapshot,
    now: DateTime<Utc>,
    stall_after: chrono::Duration,
) -> SubscriptionSummary {
    let cp = &h.checkpoint;
    let health = derive_health(
        &HealthInputs {
            paused: h.paused.is_some(),
            has_checkpoint: cp.has_checkpoint,
            has_events: cp.head_seq.is_some_and(|s| s > 0),
            waiting: cp.waiting,
            oldest_waiting_at: cp.oldest_waiting_at,
            last_handled_at: h.last_handled_at,
            failures_24h: h.failures_24h,
            last_error_at: h.last_error_at,
            retrying: h.retrying.is_some(),
        },
        now,
        stall_after,
    );
    let oldest_wait_secs = if cp.waiting > 0 {
        cp.oldest_waiting_at.map(|t| (now - t).num_seconds().max(0))
    } else {
        None
    };
    let last_handled_at = h.last_handled_at.map(|t| t.to_rfc3339());
    let name = h
        .subscription_name
        .unwrap_or_else(|| h.registry_key.clone());
    let topic_key_filter = if h.mode == "consumer_group" {
        None
    } else {
        cp.topic_key.clone()
    };
    SubscriptionSummary {
        subscription_id: h.registry_key,
        subscription_name: name,
        topic_name: h.topic_name,
        enabled: h.paused.is_none(),
        mode: h.mode,
        topic_key_filter,
        checkpoint_lag: cp.waiting,
        last_seq: cp.last_seq,
        head_seq: cp.head_seq,
        last_processed_at: last_handled_at.clone(),
        health,
        waiting: cp.waiting,
        oldest_wait_secs,
        last_handled_at,
        failures_24h: h.failures_24h,
        last_error: h.last_error,
        last_error_at: h.last_error_at.map(|t| t.to_rfc3339()),
        retrying: h.retrying.map(|(event_id, attempt, at)| RetryingEvent {
            event_id,
            attempt,
            next_retry_at: at.to_rfc3339(),
        }),
        paused: h.paused.map(|(by, at)| PausedBy {
            by,
            at: at.to_rfc3339(),
        }),
        supports_control: h.supports_control,
        retention_secs: h.retention_secs,
    }
}

/// Sorts subscriptions worst health first, then by name.
pub fn sort_worst_first(subs: &mut [SubscriptionSummary]) {
    subs.sort_by(|a, b| {
        a.health
            .cmp(&b.health)
            .then_with(|| a.subscription_name.cmp(&b.subscription_name))
    });
}

/// A duration in one unit: "6 s", "38 min", "2 h", "7 days".
#[must_use]
pub fn duration_words(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        3600..=86_399 => format!("{} h", secs / 3600),
        _ => plural(secs / 86_400, "day", "days"),
    }
}

/// A longer duration in two units: "6 days 23 hours", "5 hours 10 minutes", "12 minutes".
#[must_use]
pub fn long_duration_words(secs: i64) -> String {
    let secs = secs.max(0);
    let (days, hours, mins) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if days > 0 {
        if hours > 0 {
            format!(
                "{} {}",
                plural(days, "day", "days"),
                plural(hours, "hour", "hours")
            )
        } else {
            plural(days, "day", "days")
        }
    } else if hours > 0 {
        if mins > 0 {
            format!(
                "{} {}",
                plural(hours, "hour", "hours"),
                plural(mins, "minute", "minutes")
            )
        } else {
            plural(hours, "hour", "hours")
        }
    } else {
        plural(mins.max(1), "minute", "minutes")
    }
}

fn plural(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The Waiting cell: "41 waiting, oldest 38 min · 5 failed in 24 h", "Up to date",
/// "No events yet".
#[must_use]
pub fn waiting_text(sub: &SubscriptionSummary) -> String {
    let mut text = if sub.waiting > 0 {
        sub.oldest_wait_secs.map_or_else(
            || format!("{} waiting", sub.waiting),
            |secs| format!("{} waiting, oldest {}", sub.waiting, duration_words(secs)),
        )
    } else if sub.health == SubscriptionHealth::NotStarted {
        "No events yet".to_string()
    } else {
        "Up to date".to_string()
    };
    if sub.failures_24h > 0 {
        text = format!("{text} · {} failed in 24 h", sub.failures_24h);
    }
    text
}

/// The Lag line on the Details card: "41 events behind", "1 event behind", "Up to date".
#[must_use]
pub fn lag_words(waiting: i64) -> String {
    match waiting {
        i64::MIN..=0 => "Up to date".to_string(),
        1 => "1 event behind".to_string(),
        n => format!("{n} events behind"),
    }
}

/// A time relative to `now`: "Just now", "Today, 07:36", "Nov 27, 07:36"; "Never" for none.
///
/// Times are shown in UTC.
#[must_use]
pub fn time_words(at: Option<&str>, now: DateTime<Utc>) -> String {
    let Some(at) = at.and_then(|s| DateTime::parse_from_rfc3339(s).ok()) else {
        return "Never".to_string();
    };
    let at = at.with_timezone(&Utc);
    if (now - at).num_seconds().abs() < 60 {
        "Just now".to_string()
    } else if at.date_naive() == now.date_naive() {
        format!("Today, {}", at.format("%H:%M"))
    } else {
        at.format("%b %-d, %H:%M").to_string()
    }
}

/// A clock time in UTC ("08:16") for times within the day, else "Nov 27, 08:16".
#[must_use]
pub fn clock_words(at: &str, now: DateTime<Utc>) -> String {
    let Ok(at) = DateTime::parse_from_rfc3339(at) else {
        return at.to_string();
    };
    let at = at.with_timezone(&Utc);
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%b %-d, %H:%M").to_string()
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-28T08:20:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[allow(clippy::unnecessary_wraps)] // reads as the Option fields it fills
    fn mins_ago(m: i64) -> Option<DateTime<Utc>> {
        Some(now() - chrono::Duration::minutes(m))
    }

    fn base() -> HealthInputs {
        HealthInputs {
            has_checkpoint: true,
            has_events: true,
            ..HealthInputs::default()
        }
    }

    fn health(i: &HealthInputs) -> SubscriptionHealth {
        derive_health(i, now(), DEFAULT_STALL_AFTER)
    }

    #[test]
    fn paused_wins_over_everything() {
        let i = HealthInputs {
            paused: true,
            waiting: 40,
            failures_24h: 3,
            last_error_at: mins_ago(1),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Paused);
    }

    #[test]
    fn no_checkpoint_and_no_events_is_not_started() {
        assert_eq!(
            health(&HealthInputs::default()),
            SubscriptionHealth::NotStarted
        );
    }

    #[test]
    fn events_without_checkpoint_are_waiting() {
        let i = HealthInputs {
            has_events: true,
            waiting: 3,
            oldest_waiting_at: mins_ago(0),
            ..HealthInputs::default()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn failure_after_last_success_is_failing() {
        let i = HealthInputs {
            failures_24h: 5,
            last_handled_at: mins_ago(10),
            last_error_at: mins_ago(2),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Failing);
    }

    #[test]
    fn success_after_old_failure_is_not_failing() {
        let i = HealthInputs {
            failures_24h: 5,
            last_handled_at: mins_ago(1),
            last_error_at: mins_ago(30),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Healthy);
    }

    #[test]
    fn retrying_with_failures_is_failing() {
        let i = HealthInputs {
            failures_24h: 1,
            last_handled_at: mins_ago(1),
            last_error_at: mins_ago(30),
            retrying: true,
            waiting: 2,
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Failing);
    }

    #[test]
    fn retrying_without_failures_is_not_failing() {
        let i = HealthInputs {
            retrying: true,
            waiting: 2,
            oldest_waiting_at: mins_ago(0),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn waiting_with_nothing_handled_for_five_minutes_is_stalled() {
        let i = HealthInputs {
            waiting: 41,
            oldest_waiting_at: mins_ago(38),
            last_handled_at: mins_ago(44),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Stalled);
    }

    #[test]
    fn recent_handling_keeps_it_catching_up() {
        let i = HealthInputs {
            waiting: 41,
            oldest_waiting_at: mins_ago(38),
            last_handled_at: mins_ago(1),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn fresh_wait_after_old_handling_is_catching_up() {
        let i = HealthInputs {
            waiting: 1,
            oldest_waiting_at: mins_ago(0),
            last_handled_at: mins_ago(600),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn stall_threshold_is_inclusive() {
        let i = HealthInputs {
            waiting: 1,
            oldest_waiting_at: mins_ago(5),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::Stalled);
        let i = HealthInputs {
            waiting: 1,
            oldest_waiting_at: mins_ago(4),
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn unknown_wait_time_is_catching_up() {
        let i = HealthInputs {
            waiting: 4,
            ..base()
        };
        assert_eq!(health(&i), SubscriptionHealth::CatchingUp);
    }

    #[test]
    fn nothing_waiting_is_healthy() {
        assert_eq!(health(&base()), SubscriptionHealth::Healthy);
    }

    #[test]
    fn sort_puts_worst_first_then_name() {
        let mk = |name: &str, health| SubscriptionSummary {
            subscription_name: name.into(),
            health,
            ..SubscriptionSummary::default()
        };
        let mut subs = vec![
            mk("b", SubscriptionHealth::Healthy),
            mk("a", SubscriptionHealth::Healthy),
            mk("z", SubscriptionHealth::Failing),
            mk("y", SubscriptionHealth::NotStarted),
            mk("x", SubscriptionHealth::CatchingUp),
            mk("w", SubscriptionHealth::Paused),
            mk("v", SubscriptionHealth::Stalled),
        ];
        sort_worst_first(&mut subs);
        let names: Vec<_> = subs.iter().map(|s| s.subscription_name.as_str()).collect();
        assert_eq!(names, ["z", "v", "w", "x", "y", "a", "b"]);
    }

    #[test]
    fn waiting_text_matches_the_list() {
        let sub = SubscriptionSummary {
            waiting: 12,
            oldest_wait_secs: Some(21 * 60),
            failures_24h: 5,
            health: SubscriptionHealth::Failing,
            ..SubscriptionSummary::default()
        };
        assert_eq!(
            waiting_text(&sub),
            "12 waiting, oldest 21 min · 5 failed in 24 h"
        );
        let sub = SubscriptionSummary {
            waiting: 3,
            oldest_wait_secs: Some(6),
            health: SubscriptionHealth::CatchingUp,
            ..SubscriptionSummary::default()
        };
        assert_eq!(waiting_text(&sub), "3 waiting, oldest 6 s");
        let healthy = SubscriptionSummary {
            health: SubscriptionHealth::Healthy,
            ..SubscriptionSummary::default()
        };
        assert_eq!(waiting_text(&healthy), "Up to date");
        assert_eq!(
            waiting_text(&SubscriptionSummary::default()),
            "No events yet"
        );
    }

    #[test]
    fn durations_read_in_words() {
        assert_eq!(duration_words(6), "6 s");
        assert_eq!(duration_words(38 * 60 + 5), "38 min");
        assert_eq!(duration_words(2 * 3600), "2 h");
        assert_eq!(duration_words(7 * 86_400), "7 days");
        assert_eq!(duration_words(86_400), "1 day");
        assert_eq!(
            long_duration_words(6 * 86_400 + 23 * 3600),
            "6 days 23 hours"
        );
        assert_eq!(long_duration_words(3600 + 60), "1 hour 1 minute");
        assert_eq!(long_duration_words(5), "1 minute");
    }

    #[test]
    fn lag_reads_in_words() {
        assert_eq!(lag_words(0), "Up to date");
        assert_eq!(lag_words(1), "1 event behind");
        assert_eq!(lag_words(41), "41 events behind");
    }

    #[test]
    fn times_read_relative_to_now() {
        let n = now();
        assert_eq!(time_words(None, n), "Never");
        assert_eq!(time_words(Some("2026-11-28T08:19:30Z"), n), "Just now");
        assert_eq!(time_words(Some("2026-11-28T07:36:00Z"), n), "Today, 07:36");
        assert_eq!(time_words(Some("2026-11-27T07:36:00Z"), n), "Nov 27, 07:36");
        assert_eq!(clock_words("2026-11-28T08:16:00Z", n), "08:16");
    }

    #[test]
    fn group_checkpoints_fold_across_shards() {
        let cp = |key: &str, last, head, oldest: Option<DateTime<Utc>>| CheckpointState {
            subscription_name: "grp".into(),
            topic_name: "orders".into(),
            topic_key: Some(key.into()),
            last_seq: last,
            head_seq: head,
            oldest_waiting_at: oldest,
        };
        let rows = vec![
            cp("0", Some(5), Some(9), mins_ago(3)),
            cp("1", Some(2), Some(2), None),
            cp("2", Some(7), Some(10), mins_ago(8)),
            CheckpointState {
                subscription_name: "other".into(),
                topic_name: "orders".into(),
                ..CheckpointState::default()
            },
        ];
        let agg = aggregate_checkpoints(&rows, "grp", "orders");
        assert_eq!(agg.waiting, 7);
        assert_eq!(agg.last_seq, Some(2));
        assert_eq!(agg.head_seq, Some(10));
        assert_eq!(agg.oldest_waiting_at, mins_ago(8));
        assert_eq!(agg.topic_key, None);
        assert!(agg.has_checkpoint);
        assert_eq!(
            aggregate_checkpoints(&rows, "missing", "orders"),
            CheckpointAggregate::default()
        );
    }

    #[test]
    fn snapshot_maps_pause_and_errors() {
        let sub = subscription_summary_from_snapshot(
            HandlerSnapshot {
                registry_key: "orders:warehouse".into(),
                subscription_name: Some("warehouse".into()),
                topic_name: "orders".into(),
                mode: "durable".into(),
                checkpoint: CheckpointAggregate {
                    last_seq: Some(55),
                    head_seq: Some(96),
                    waiting: 41,
                    oldest_waiting_at: mins_ago(38),
                    topic_key: Some("eu".into()),
                    has_checkpoint: true,
                },
                last_handled_at: mins_ago(44),
                paused: Some(("Kenji".into(), now())),
                supports_control: true,
                retention_secs: Some(7 * 86_400),
                ..HandlerSnapshot::default()
            },
            now(),
            DEFAULT_STALL_AFTER,
        );
        assert_eq!(sub.health, SubscriptionHealth::Paused);
        assert!(!sub.enabled);
        assert_eq!(sub.waiting, 41);
        assert_eq!(sub.oldest_wait_secs, Some(38 * 60));
        assert_eq!(sub.topic_key_filter.as_deref(), Some("eu"));
        assert_eq!(sub.last_processed_at, sub.last_handled_at);
        assert_eq!(sub.paused.map(|p| p.by).as_deref(), Some("Kenji"));
    }
}
