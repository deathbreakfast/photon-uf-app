//! What to do card: shown on a Stalled or Failing subscription to someone who can act on it.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::{Caption1, Card, CardContent, CardHeader, Subtitle2};
use orbital::primitives::{Button, ButtonAppearance, Flex, FlexGap, FlexWrap, List, ListItem};
use photon_backend::{SubscriptionHealth, SubscriptionSummary};

/// The advice lines for a Stalled or Failing subscription; empty for any other health.
#[must_use]
pub fn what_to_do_lines(sub: &SubscriptionSummary, owner: &str) -> Vec<String> {
    match sub.health {
        SubscriptionHealth::Stalled => {
            let mut lines = vec![if sub.is_consumer_group() {
                format!("Check that the {owner} team's service is running. A consumer group with no worker connected stalls like this.")
            } else {
                format!("Check that the {owner} team's service is running and its handler isn't stuck on one event.")
            }];
            lines.push(
                "When it comes back it starts from the last event it handled. Nothing is lost."
                    .to_string(),
            );
            if let Some(retention) = sub.retention_secs.and_then(|r| i64::try_from(r).ok()) {
                let kept = photon_backend::duration_words(retention);
                let left = retention - sub.oldest_wait_secs.unwrap_or(0);
                lines.push(format!(
                    "The oldest waiting event stays in the log for {kept}, so there is time: it ages out in {}.",
                    photon_backend::long_duration_words(left)
                ));
            }
            lines.push(
                "Use Replay only if the handler processed events wrongly and has to see them again."
                    .to_string(),
            );
            lines
        }
        SubscriptionHealth::Failing => vec![
            "Fix what the last error names. Photon keeps retrying each failing event with backoff."
                .to_string(),
            "Press Retry now once it's fixed, instead of waiting for the next retry.".to_string(),
            "Pause stops the retries if they cause trouble downstream. Events wait while it's paused."
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

/// What the state means, what to check, and the actions the viewer may take: Pause and
/// Replay from… when Stalled, Retry now and Pause when Failing.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
pub fn WhatToDoCard(
    /// The subscription (Stalled or Failing).
    sub: SubscriptionSummary,
    /// The owning team.
    owner: String,
    /// The viewer holds Run (Replay, Retry now).
    can_run: bool,
    /// The viewer holds Edit (Pause).
    can_edit: bool,
    /// Pause pressed.
    on_pause: Callback<()>,
    /// Replay from… pressed.
    on_replay: Callback<()>,
    /// Retry now pressed.
    on_retry: Callback<()>,
) -> impl IntoView {
    let lines = what_to_do_lines(&sub, &owner);
    let failing = sub.health == SubscriptionHealth::Failing;
    let control = sub.supports_control;
    let pause = (can_edit && control).then(|| {
        view! {
            <Button appearance=ButtonAppearance::Secondary attr:data-testid="photon-what-to-do-pause" on_click=Callback::new(move |_| on_pause.run(()))>
                "Pause"
            </Button>
        }
    });
    let second = if failing {
        can_run.then(|| view! {
            <Button appearance=ButtonAppearance::Primary attr:data-testid="photon-what-to-do-retry" on_click=Callback::new(move |_| on_retry.run(()))>
                "Retry now"
            </Button>
        }.into_any())
    } else {
        (can_run && control).then(|| view! {
            <Button appearance=ButtonAppearance::Secondary attr:data-testid="photon-what-to-do-replay" on_click=Callback::new(move |_| on_replay.run(()))>
                "Replay from…"
            </Button>
        }.into_any())
    };
    let (first, last) = if failing {
        (second, pause.map(IntoAny::into_any))
    } else {
        (pause.map(IntoAny::into_any), second)
    };

    view! {
        <div data-testid="photon-what-to-do">
            <Card>
                <CardHeader>
                    <Subtitle2>"What to do"</Subtitle2>
                </CardHeader>
                <CardContent>
                    <Flex vertical=true gap=FlexGap::Medium>
                        <List>
                            {lines.into_iter().map(|line| view! { <ListItem>{line}</ListItem> }).collect_view()}
                        </List>
                        <Flex gap=FlexGap::Small wrap=FlexWrap::Wrap>
                            {first}
                            {last}
                        </Flex>
                        <Caption1>{format!("Owner: {owner}. Ask them first if the handler is theirs.")}</Caption1>
                    </Flex>
                </CardContent>
            </Card>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_group_lines_mention_workers_and_retention() {
        let sub = SubscriptionSummary {
            health: SubscriptionHealth::Stalled,
            mode: "consumer_group".into(),
            retention_secs: Some(7 * 86_400),
            oldest_wait_secs: Some(3600),
            ..SubscriptionSummary::default()
        };
        let lines = what_to_do_lines(&sub, "Warehouse");
        assert!(lines[0].contains("Warehouse team's service"));
        assert!(lines[0].contains("consumer group"));
        assert_eq!(
            lines[2],
            "The oldest waiting event stays in the log for 7 days, so there is time: it ages out in 6 days 23 hours."
        );
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn stalled_without_retention_skips_that_line() {
        let sub = SubscriptionSummary {
            health: SubscriptionHealth::Stalled,
            mode: "durable".into(),
            ..SubscriptionSummary::default()
        };
        assert_eq!(what_to_do_lines(&sub, "Orders").len(), 3);
    }

    #[test]
    fn failing_lines_point_at_retry_now() {
        let sub = SubscriptionSummary {
            health: SubscriptionHealth::Failing,
            ..SubscriptionSummary::default()
        };
        let lines = what_to_do_lines(&sub, "Messaging");
        assert!(lines[1].starts_with("Press Retry now"));
        assert_eq!(
            what_to_do_lines(&SubscriptionSummary::default(), "x"),
            Vec::<String>::new()
        );
    }
}
