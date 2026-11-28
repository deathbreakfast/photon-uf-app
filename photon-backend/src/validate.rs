//! Blank-id rejection, unsafe-id rejection, event-list limit capping, and
//! path-segment encoding for ops UI hrefs.

use crate::types::PhotonIdError;

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

/// Cap a waiting-events request to [`MAX_EVENT_LIST_LIMIT`], with at least one row.
#[must_use]
pub fn clamp_waiting_limit(limit: u32) -> usize {
    limit.clamp(1, MAX_EVENT_LIST_LIMIT) as usize
}

/// A replay start point the Replay dialog can't use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayPointError {
    /// The sequence number was negative.
    NegativeSeq,
    /// The time was not an RFC 3339 timestamp.
    InvalidTime,
    /// The time is in the future, so nothing would replay.
    FutureTime,
    /// The event id failed validation.
    Event(PhotonIdError),
}

impl std::fmt::Display for ReplayPointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NegativeSeq => write!(f, "Replay sequence number can't be negative"),
            Self::InvalidTime => write!(f, "Replay time must be a date and a UTC time"),
            Self::FutureTime => write!(f, "Replay time is in the future"),
            Self::Event(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReplayPointError {}

/// Checks a replay start point and returns the time as UTC when it is one.
///
/// # Errors
///
/// [`ReplayPointError`] for a negative seq, an unparsable or future time, or a bad event id.
pub fn validate_replay_point(
    point: &crate::ReplayPoint,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, ReplayPointError> {
    match point {
        crate::ReplayPoint::AfterSeq(seq) if *seq < 0 => Err(ReplayPointError::NegativeSeq),
        crate::ReplayPoint::AfterSeq(_) => Ok(None),
        crate::ReplayPoint::Time(raw) => {
            let at = chrono::DateTime::parse_from_rfc3339(raw.trim())
                .map_err(|_| ReplayPointError::InvalidTime)?
                .with_timezone(&chrono::Utc);
            if at > now {
                return Err(ReplayPointError::FutureTime);
            }
            Ok(Some(at))
        }
        crate::ReplayPoint::Event(id) => validate_event_id(id)
            .map(|()| None)
            .map_err(ReplayPointError::Event),
    }
}

/// Whether the name typed into the Skip dialog matches the subscription's name exactly
/// (surrounding spaces ignored).
#[must_use]
pub fn confirm_name_matches(typed: &str, subscription_name: &str) -> bool {
    !subscription_name.is_empty() && typed.trim() == subscription_name
}

#[cfg(test)]
mod action_input_tests {
    use super::*;
    use crate::ReplayPoint;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-11-28T08:20:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn waiting_limit_is_clamped() {
        assert_eq!(clamp_waiting_limit(0), 1);
        assert_eq!(clamp_waiting_limit(20), 20);
        assert_eq!(clamp_waiting_limit(10_000), MAX_EVENT_LIST_LIMIT as usize);
    }

    #[test]
    fn replay_points_validate() {
        assert_eq!(
            validate_replay_point(&ReplayPoint::AfterSeq(40), now()),
            Ok(None)
        );
        assert_eq!(
            validate_replay_point(&ReplayPoint::AfterSeq(-1), now()),
            Err(ReplayPointError::NegativeSeq)
        );
        assert!(matches!(
            validate_replay_point(&ReplayPoint::Time("2026-11-28T07:30:00Z".into()), now()),
            Ok(Some(_))
        ));
        assert_eq!(
            validate_replay_point(&ReplayPoint::Time("07:30".into()), now()),
            Err(ReplayPointError::InvalidTime)
        );
        assert_eq!(
            validate_replay_point(&ReplayPoint::Time("2026-11-29T07:30:00Z".into()), now()),
            Err(ReplayPointError::FutureTime)
        );
        assert_eq!(
            validate_replay_point(&ReplayPoint::Event("  ".into()), now()),
            Err(ReplayPointError::Event(PhotonIdError::EmptyEventId))
        );
    }

    #[test]
    fn skip_confirm_needs_the_exact_name() {
        assert!(confirm_name_matches(" warehouse ", "warehouse"));
        assert!(!confirm_name_matches("Warehouse", "warehouse"));
        assert!(!confirm_name_matches("", ""));
    }
}
