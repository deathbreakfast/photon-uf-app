//! Pause and Resume confirms.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::{Body1, Caption1};
use orbital::primitives::{
    Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
    DialogTitle, Flex, FlexGap,
};
use photon_backend::SubscriptionSummary;

use crate::components::{server_fn_error_bar, ActionNotice};
use crate::server::{pause_subscription, resume_subscription};

/// "within the 7 days retention" when the backend bounds retention.
fn retention_clause(sub: &SubscriptionSummary) -> String {
    sub.retention_secs
        .and_then(|r| i64::try_from(r).ok())
        .map_or_else(String::new, |r| {
            format!(
                " while they stay within the {} retention",
                photon_backend::duration_words(r)
            )
        })
}

/// What pausing stops and what keeps running.
#[must_use]
pub fn pause_text(sub: &SubscriptionSummary) -> String {
    format!(
        "{name} stops taking events. New events wait for it, and nothing is lost{retention}. Other subscriptions on {topic} keep running.",
        name = sub.subscription_name,
        retention = retention_clause(sub),
        topic = sub.topic_name,
    )
}

/// Where resuming restarts.
#[must_use]
pub fn resume_text(sub: &SubscriptionSummary) -> String {
    let from = sub.last_seq.map_or_else(
        || "the first event".to_string(),
        |s| format!("seq {}", s + 1),
    );
    match sub.waiting {
        0 => format!("{} starts again from {from}.", sub.subscription_name),
        n => format!(
            "{} starts again from {from} and works through the {n} waiting events.",
            sub.subscription_name
        ),
    }
}

/// Confirm before pausing a running subscription or resuming a paused one.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
pub fn PauseDialog(
    /// Open state.
    open: RwSignal<bool>,
    /// The subscription.
    sub: SubscriptionSummary,
    /// Called after the pause or resume succeeds.
    on_done: Callback<ActionNotice>,
) -> impl IntoView {
    let resuming = sub.paused.is_some();
    let id = StoredValue::new(sub.subscription_id.clone());
    let action = Action::new(move |(): &()| {
        let id = id.get_value();
        async move {
            if resuming {
                resume_subscription(id).await
            } else {
                pause_subscription(id).await
            }
        }
    });
    let error = RwSignal::new(None::<ServerFnError>);
    Effect::new(move |_| match action.value().get() {
        Some(Ok(())) => {
            error.set(None);
            open.set(false);
            on_done.run(if resuming {
                ActionNotice::Resumed
            } else {
                ActionNotice::Paused
            });
        }
        Some(Err(e)) => error.set(Some(e)),
        None => {}
    });
    let (title, body, label) = if resuming {
        (
            format!("Resume {}?", sub.subscription_name),
            resume_text(&sub),
            "Resume",
        )
    } else {
        (
            format!("Pause {}?", sub.subscription_name),
            pause_text(&sub),
            "Pause",
        )
    };

    view! {
        <Dialog open=open>
            <DialogSurface attr:data-testid="photon-pause-dialog">
                <DialogBody>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogContent>
                        <Flex vertical=true gap=FlexGap::Medium>
                            <Body1>{body}</Body1>
                            <Caption1>"Recorded with your name."</Caption1>
                            {move || error.get().map(|e| server_fn_error_bar(&e))}
                        </Flex>
                    </DialogContent>
                    <DialogActions>
                        <Button appearance=ButtonAppearance::Secondary on_click=Callback::new(move |_| open.set(false))>
                            "Cancel"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Primary
                            loading=action.pending()
                            attr:data-testid="photon-pause-confirm"
                            on_click=Callback::new(move |_| { action.dispatch(()); })
                        >
                            {label}
                        </Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub() -> SubscriptionSummary {
        SubscriptionSummary {
            subscription_name: "warehouse".into(),
            topic_name: "orders.created".into(),
            last_seq: Some(55),
            waiting: 41,
            ..SubscriptionSummary::default()
        }
    }

    #[test]
    fn pause_says_what_stops_and_keeps_running() {
        let mut s = sub();
        s.retention_secs = Some(7 * 86_400);
        assert_eq!(
            pause_text(&s),
            "warehouse stops taking events. New events wait for it, and nothing is lost while they stay within the 7 days retention. Other subscriptions on orders.created keep running."
        );
        assert!(pause_text(&sub()).contains("nothing is lost. Other"));
    }

    #[test]
    fn resume_says_where_it_restarts() {
        assert_eq!(
            resume_text(&sub()),
            "warehouse starts again from seq 56 and works through the 41 waiting events."
        );
    }
}
