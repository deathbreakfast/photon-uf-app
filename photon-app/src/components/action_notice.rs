//! Success message shown on the subscription page after an action.

use leptos::prelude::*;
use orbital::primitives::{MessageBar, MessageBarBody, MessageBarIntent, MessageBarTitle};
use photon_backend::SubscriptionSummary;

/// What the last action on the page did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionNotice {
    /// The subscription was paused.
    Paused,
    /// The subscription was resumed.
    Resumed,
    /// This many waiting events were skipped.
    Skipped(u64),
    /// A replay started.
    Replay {
        /// Events sent again.
        count: u64,
        /// Where it started, in words ("07:30", "seq 41", "ev-1").
        from: String,
    },
    /// The failing event is being retried now.
    Retry,
}

impl ActionNotice {
    /// Title and body for `sub` as it reads now (a replay's progress uses its waiting count).
    #[must_use]
    pub fn text(&self, sub: &SubscriptionSummary) -> (String, String) {
        let name = &sub.subscription_name;
        match self {
            Self::Paused => (
                format!("{name} paused"),
                "Events wait for it until someone resumes it.".to_string(),
            ),
            Self::Resumed => (
                format!("{name} resumed"),
                "It starts again from the last event it handled.".to_string(),
            ),
            Self::Skipped(n) => (
                format!("Skipped {n} events"),
                format!("{name} now starts from the newest event."),
            ),
            Self::Replay { count, from } => {
                let waiting = u64::try_from(sub.waiting.max(0)).unwrap_or(0);
                let handled = count.saturating_sub(waiting);
                (
                    "Replay started".to_string(),
                    format!("{name} gets {count} events again from {from}. It has handled {handled} so far."),
                )
            }
            Self::Retry => (
                "Retry started".to_string(),
                format!("{name} tries the failing event again now."),
            ),
        }
    }
}

/// A success `MessageBar` for `notice`.
pub fn action_notice_view(notice: &ActionNotice, sub: &SubscriptionSummary) -> AnyView {
    let (title, body) = notice.text(sub);
    view! {
        <MessageBar intent=MessageBarIntent::Success attr:data-testid="photon-action-notice">
            <MessageBarBody>
                <MessageBarTitle>{title}</MessageBarTitle>
                {body}
            </MessageBarBody>
        </MessageBar>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_reports_progress() {
        let sub = SubscriptionSummary {
            subscription_name: "warehouse".into(),
            waiting: 52,
            ..SubscriptionSummary::default()
        };
        let notice = ActionNotice::Replay {
            count: 64,
            from: "07:30".into(),
        };
        assert_eq!(
            notice.text(&sub),
            (
                "Replay started".to_string(),
                "warehouse gets 64 events again from 07:30. It has handled 12 so far.".to_string()
            )
        );
        assert_eq!(ActionNotice::Paused.text(&sub).0, "warehouse paused");
    }
}
