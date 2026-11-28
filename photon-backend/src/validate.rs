//! Blank-id rejection, unsafe-id rejection, event-list limit capping, and
//! path-segment encoding for ops UI hrefs.

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::types::{EventsQuery, EventsRange, PhotonIdError, MAX_EVENTS_PAGE};

/// Maximum events returned by a single `get_events` / `get_recent_events` call.
pub const MAX_EVENT_LIST_LIMIT: u32 = 100;

/// Maximum Unicode scalar count for topic names, subscription ids, and event ids
/// accepted by ops detail lookups.
pub const MAX_PHOTON_ID_CHARS: usize = 256;

/// ASCII controls (C0 + DEL) plus path separators. Avoids `char::is_control`,
/// which is not `const` on the leptos-lints pinned nightly.
const fn is_unsafe_ops_id_char(c: char) -> bool {
    c <= '\u{1f}' || c == '\u{7f}' || c == '/' || c == '\\'
}

fn check_ops_id(raw: &str) -> Result<&str, PhotonIdErrorKind> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PhotonIdErrorKind::Empty);
    }
    if trimmed.chars().count() > MAX_PHOTON_ID_CHARS {
        return Err(PhotonIdErrorKind::TooLong);
    }
    if trimmed == "." || trimmed == ".." {
        return Err(PhotonIdErrorKind::Unsafe);
    }
    if trimmed.chars().any(is_unsafe_ops_id_char) {
        return Err(PhotonIdErrorKind::Unsafe);
    }
    Ok(trimmed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhotonIdErrorKind {
    Empty,
    TooLong,
    Unsafe,
}

/// Rejects blank, oversized, path-separating, control, or `.` / `..` topic names
/// before registry / detail lookups.
///
/// # Errors
///
/// Returns a [`PhotonIdError`] variant when the name is empty/whitespace-only,
/// longer than [`MAX_PHOTON_ID_CHARS`], contains `/` `\` or ASCII controls, or is
/// exactly `.` / `..`.
pub fn validate_topic_name(topic_name: &str) -> Result<(), PhotonIdError> {
    match check_ops_id(topic_name) {
        Ok(_) => Ok(()),
        Err(PhotonIdErrorKind::Empty) => Err(PhotonIdError::EmptyTopicName),
        Err(PhotonIdErrorKind::TooLong) => Err(PhotonIdError::TopicNameTooLong),
        Err(PhotonIdErrorKind::Unsafe) => Err(PhotonIdError::UnsafeTopicName),
    }
}

/// Rejects blank, oversized, path-separating, control, or `.` / `..` subscription
/// ids before detail lookups.
///
/// # Errors
///
/// Returns a [`PhotonIdError`] variant when the id fails the same rules as
/// [`validate_topic_name`].
pub fn validate_subscription_id(id: &str) -> Result<(), PhotonIdError> {
    match check_ops_id(id) {
        Ok(_) => Ok(()),
        Err(PhotonIdErrorKind::Empty) => Err(PhotonIdError::EmptySubscriptionId),
        Err(PhotonIdErrorKind::TooLong) => Err(PhotonIdError::SubscriptionIdTooLong),
        Err(PhotonIdErrorKind::Unsafe) => Err(PhotonIdError::UnsafeSubscriptionId),
    }
}

/// Rejects blank, oversized, path-separating, control, or `.` / `..` event ids
/// before detail lookups.
///
/// # Errors
///
/// Returns a [`PhotonIdError`] variant when the id fails the same rules as
/// [`validate_topic_name`].
pub fn validate_event_id(id: &str) -> Result<(), PhotonIdError> {
    match check_ops_id(id) {
        Ok(_) => Ok(()),
        Err(PhotonIdErrorKind::Empty) => Err(PhotonIdError::EmptyEventId),
        Err(PhotonIdErrorKind::TooLong) => Err(PhotonIdError::EventIdTooLong),
        Err(PhotonIdErrorKind::Unsafe) => Err(PhotonIdError::UnsafeEventId),
    }
}

/// Cap requested event list limits to [`MAX_EVENT_LIST_LIMIT`].
#[must_use]
pub fn clamp_event_list_limit(limit: u32) -> usize {
    limit.min(MAX_EVENT_LIST_LIMIT) as usize
}

/// Longest custom range the Events list accepts, in days.
pub const MAX_EVENTS_RANGE_DAYS: i64 = 366;

/// Clamp a requested Events page to `1..=MAX_EVENTS_PAGE`.
#[must_use]
pub fn clamp_events_page(page: u32) -> u32 {
    page.clamp(1, MAX_EVENTS_PAGE)
}

/// Trimmed search text, or `None` when blank. Longer than [`MAX_PHOTON_ID_CHARS`] is an error.
///
/// # Errors
///
/// [`EventsQueryError::SearchTooLong`] for oversized input.
pub fn normalize_events_search(q: Option<&str>) -> Result<Option<&str>, EventsQueryError> {
    let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) else {
        return Ok(None);
    };
    if q.chars().count() > MAX_PHOTON_ID_CHARS {
        return Err(EventsQueryError::SearchTooLong);
    }
    Ok(Some(q))
}

/// Half-open time window `[since, until)` for an Events query; `until` is `None` for "now".
///
/// Custom ranges cover whole UTC days, `from` through `to` inclusive.
///
/// # Errors
///
/// An [`EventsQueryError`] when a custom range is missing a day, a day doesn't parse as
/// `YYYY-MM-DD`, `from` is after `to`, or the range is longer than
/// [`MAX_EVENTS_RANGE_DAYS`].
pub fn events_window(
    query: &EventsQuery,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, Option<DateTime<Utc>>), EventsQueryError> {
    match query.range {
        EventsRange::LastHour => Ok((now - Duration::hours(1), None)),
        EventsRange::Last24h => Ok((now - Duration::hours(24), None)),
        EventsRange::Last7d => Ok((now - Duration::days(7), None)),
        EventsRange::Custom => {
            let day = |raw: Option<&String>| -> Result<NaiveDate, EventsQueryError> {
                let raw = raw
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .ok_or(EventsQueryError::MissingDates)?;
                NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| EventsQueryError::BadDate)
            };
            let from = day(query.from.as_ref())?;
            let to = day(query.to.as_ref())?;
            if from > to {
                return Err(EventsQueryError::FromAfterTo);
            }
            if (to - from).num_days() >= MAX_EVENTS_RANGE_DAYS {
                return Err(EventsQueryError::RangeTooLong);
            }
            let until = to.succ_opt().ok_or(EventsQueryError::BadDate)?;
            Ok((
                from.and_time(chrono::NaiveTime::MIN).and_utc(),
                Some(until.and_time(chrono::NaiveTime::MIN).and_utc()),
            ))
        }
    }
}

/// An Events query the server refuses before reading Photon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventsQueryError {
    /// A custom range without both days.
    MissingDates,
    /// A day that isn't `YYYY-MM-DD`.
    BadDate,
    /// The first day is after the last.
    FromAfterTo,
    /// The range is longer than [`MAX_EVENTS_RANGE_DAYS`].
    RangeTooLong,
    /// The search text is longer than [`MAX_PHOTON_ID_CHARS`].
    SearchTooLong,
}

impl std::fmt::Display for EventsQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDates => write!(f, "Pick both days for a custom range"),
            Self::BadDate => write!(f, "Dates must look like 2026-11-24"),
            Self::FromAfterTo => write!(f, "The first day must come before the last"),
            Self::RangeTooLong => write!(
                f,
                "A custom range can cover at most {MAX_EVENTS_RANGE_DAYS} days"
            ),
            Self::SearchTooLong => write!(f, "Search text is too long"),
        }
    }
}

impl std::error::Error for EventsQueryError {}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Percent-encode a single path segment for `/photon/...` hrefs.
///
/// Leaves RFC 3986 unreserved characters alone (`ALPHA` / `DIGIT` / `-` `.` `_`
/// `~`). Encodes `/`, `\`, controls, spaces, and other bytes so Orbital
/// `paths::*` format strings cannot smuggle extra path segments.
#[must_use]
pub fn encode_ops_path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for &b in raw.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
    out
}

/// `/photon/topics/{encoded}` detail href.
#[must_use]
pub fn photon_topic_path(topic_name: &str) -> String {
    format!("/photon/topics/{}", encode_ops_path_segment(topic_name))
}

/// `/photon/subscriptions/{encoded}` detail href.
#[must_use]
pub fn photon_subscription_path(subscription_id: &str) -> String {
    format!(
        "/photon/subscriptions/{}",
        encode_ops_path_segment(subscription_id)
    )
}

/// `/photon/events/{encoded}` detail href.
#[must_use]
pub fn photon_event_path(event_id: &str) -> String {
    format!("/photon/events/{}", encode_ops_path_segment(event_id))
}
