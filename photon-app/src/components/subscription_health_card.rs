//! Health card on the subscription page: the health in words, four stat cards and the last
//! error.

use leptos::prelude::*;
use orbital::components::{
    AutoGrid, Caption1, Card, CardContent, CardHeader, Skeleton, SkeletonItem, StatCard,
    StatCardVariant, Subtitle2,
};
use orbital::primitives::{Flex, FlexGap, MessageBar, MessageBarIntent};
use photon_backend::{SubscriptionHealth, SubscriptionSummary};

use crate::components::SubscriptionHealthBadge;

/// Stat card labels in order; shared with [`SubscriptionHealthCardSkeleton`].
const STAT_LABELS: [&str; 4] = [
    "Events waiting",
    "Oldest has waited",
    "Last handled",
    "Failed in 24 h",
];

/// The "Oldest has waited" value.
fn oldest_value(sub: &SubscriptionSummary) -> String {
    match (sub.waiting, sub.oldest_wait_secs) {
        (0, _) => "Nothing waiting".to_string(),
        (_, Some(secs)) => photon_backend::duration_words(secs),
        (_, None) => "Unknown".to_string(),
    }
}

/// The last error's line: the text, then which event failed and when it runs again.
#[must_use]
pub fn last_error_text(
    sub: &SubscriptionSummary,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    let error = sub.last_error.as_ref()?;
    let tail = sub.retrying.as_ref().map_or_else(
        || {
            sub.last_error_at.as_deref().map_or_else(String::new, |at| {
                format!(" Last failed at {}.", photon_backend::clock_words(at, now))
            })
        },
        |r| {
            let failed = r.attempt.saturating_sub(1).max(1);
            let times = if failed == 1 {
                "once".to_string()
            } else {
                format!("{failed} times")
            };
            format!(
                " Event {} failed {times}; the next retry is at {}.",
                r.event_id,
                photon_backend::clock_words(&r.next_retry_at, now)
            )
        },
    );
    Some(format!("{error}{tail}"))
}

/// Badge and sentence, then Events waiting, Oldest has waited, Last handled and Failed in
/// 24 h, then the last error when there is one.
#[component]
pub fn SubscriptionHealthCard(
    /// The subscription.
    sub: SubscriptionSummary,
) -> impl IntoView {
    let now = chrono::Utc::now();
    let behind = matches!(
        sub.health,
        SubscriptionHealth::Stalled | SubscriptionHealth::Failing
    );
    let waiting_variant = behind.then_some(StatCardVariant::Warning);
    let failed_variant = (sub.failures_24h > 0).then_some(StatCardVariant::Danger);
    let last_handled = sub.last_handled_at.as_deref().map_or_else(
        || "Never".to_string(),
        |at| photon_backend::clock_words(at, now),
    );
    let error = last_error_text(&sub, now);

    view! {
        <div data-testid="photon-subscription-health">
            <Card>
                <CardHeader>
                    <Subtitle2>"Health"</Subtitle2>
                </CardHeader>
                <CardContent>
                    <Flex vertical=true gap=FlexGap::Medium>
                        <Flex gap=FlexGap::Small>
                            <SubscriptionHealthBadge health=sub.health />
                            <Caption1>{sub.health.sentence()}</Caption1>
                        </Flex>
                        <AutoGrid min="160px">
                            <StatCard label=STAT_LABELS[0] value=sub.waiting.to_string() variant=waiting_variant.unwrap_or_default() attr:data-testid="photon-stat-waiting" />
                            <StatCard label=STAT_LABELS[1] value=oldest_value(&sub) attr:data-testid="photon-stat-oldest" />
                            <StatCard label=STAT_LABELS[2] value=last_handled attr:data-testid="photon-stat-last-handled" />
                            <StatCard label=STAT_LABELS[3] value=sub.failures_24h.to_string() variant=failed_variant.unwrap_or_default() attr:data-testid="photon-stat-failed" />
                        </AutoGrid>
                        {error.map(|text| view! {
                            <MessageBar intent=MessageBarIntent::Error attr:data-testid="photon-subscription-last-error">
                                {text}
                            </MessageBar>
                        })}
                    </Flex>
                </CardContent>
            </Card>
        </div>
    }
}

/// [`SubscriptionHealthCard`] while loading: the heading in place over four placeholder stat
/// cards.
#[component]
pub fn SubscriptionHealthCardSkeleton() -> impl IntoView {
    view! {
        <Skeleton attr:data-testid="photon-subscription-health-skeleton">
            <Card>
                <CardHeader>
                    <Subtitle2>"Health"</Subtitle2>
                </CardHeader>
                <CardContent>
                    <AutoGrid min="160px">
                        {STAT_LABELS
                            .into_iter()
                            .map(|_| view! {
                                <Card>
                                    <CardContent>
                                        <Flex vertical=true gap=FlexGap::Small>
                                            <SkeletonItem width="90px" />
                                            <SkeletonItem width="50px" />
                                        </Flex>
                                    </CardContent>
                                </Card>
                            })
                            .collect_view()}
                    </AutoGrid>
                </CardContent>
            </Card>
        </Skeleton>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_backend::RetryingEvent;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-11-28T08:14:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn last_error_names_the_event_and_next_retry() {
        let sub = SubscriptionSummary {
            last_error: Some("SMTP relay refused the connection.".into()),
            retrying: Some(RetryingEvent {
                event_id: "ev-9".into(),
                attempt: 4,
                next_retry_at: "2026-11-28T08:16:00Z".into(),
            }),
            ..SubscriptionSummary::default()
        };
        assert_eq!(
            last_error_text(&sub, now()).as_deref(),
            Some("SMTP relay refused the connection. Event ev-9 failed 3 times; the next retry is at 08:16.")
        );
    }

    #[test]
    fn last_error_without_retry_says_when() {
        let sub = SubscriptionSummary {
            last_error: Some("boom".into()),
            last_error_at: Some("2026-11-28T08:10:00Z".into()),
            ..SubscriptionSummary::default()
        };
        assert_eq!(
            last_error_text(&sub, now()).as_deref(),
            Some("boom Last failed at 08:10.")
        );
        assert_eq!(
            last_error_text(&SubscriptionSummary::default(), now()),
            None
        );
    }

    #[test]
    fn oldest_value_reads_in_words() {
        let mut sub = SubscriptionSummary::default();
        assert_eq!(oldest_value(&sub), "Nothing waiting");
        sub.waiting = 4;
        sub.oldest_wait_secs = Some(38 * 60);
        assert_eq!(oldest_value(&sub), "38 min");
    }
}
