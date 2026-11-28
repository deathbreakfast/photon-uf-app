//! Skip to latest confirm: type the subscription name, then a Danger button.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::Body1;
use orbital::primitives::{
    Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
    DialogTitle, Field, Flex, FlexGap, Input, InputAppearance, InputBind, MessageBar,
    MessageBarIntent,
};
use photon_backend::SubscriptionSummary;

use crate::components::{server_fn_error_bar, ActionNotice};
use crate::server::skip_subscription_to_latest;

/// What skipping drops.
#[must_use]
pub fn skip_text(sub: &SubscriptionSummary) -> String {
    let head = sub.head_seq.map_or_else(
        || "the newest event".to_string(),
        |h| format!("the newest event, seq {h}"),
    );
    format!(
        "{name} moves its checkpoint to {head}. The {n} waiting events are never delivered to it.",
        name = sub.subscription_name,
        n = sub.waiting,
    )
}

/// Confirm before moving a subscription's checkpoint to the head.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
pub fn SkipDialog(
    /// Open state.
    open: RwSignal<bool>,
    /// The subscription.
    sub: SubscriptionSummary,
    /// Called after the skip succeeds.
    on_done: Callback<ActionNotice>,
) -> impl IntoView {
    let name = StoredValue::new(sub.subscription_name.clone());
    let id = StoredValue::new(sub.subscription_id.clone());
    let typed = RwSignal::new(String::new());
    let matches = Signal::derive(move || {
        name.with_value(|n| photon_backend::confirm_name_matches(&typed.get(), n))
    });
    let action = Action::new(move |(): &()| {
        let id = id.get_value();
        async move { skip_subscription_to_latest(id).await }
    });
    let error = RwSignal::new(None::<ServerFnError>);
    Effect::new(move |_| match action.value().get() {
        Some(Ok(skipped)) => {
            error.set(None);
            typed.set(String::new());
            open.set(false);
            on_done.run(ActionNotice::Skipped(skipped));
        }
        Some(Err(e)) => error.set(Some(e)),
        None => {}
    });

    let title = format!("Skip {} to the latest event?", sub.subscription_name);
    let body = skip_text(&sub);
    let placeholder = sub.subscription_name.clone();
    let confirm_label = format!("Skip {} events", sub.waiting);

    view! {
        <Dialog open=open>
            <DialogSurface attr:data-testid="photon-skip-dialog">
                <DialogBody>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogContent>
                        <Flex vertical=true gap=FlexGap::Medium>
                            <Body1>{body}</Body1>
                            <MessageBar intent=MessageBarIntent::Warning>
                                "Use this only when the waiting events no longer matter, for example after the handler's team rebuilt its data another way."
                            </MessageBar>
                            <Field label="Type the subscription name to confirm">
                                <div data-testid="photon-skip-confirm-name">
                                    <Input
                                        bind=InputBind::new(typed)
                                        appearance=InputAppearance {
                                            placeholder: MaybeProp::from(placeholder),
                                            ..Default::default()
                                        }
                                    />
                                </div>
                            </Field>
                            {move || error.get().map(|e| server_fn_error_bar(&e))}
                        </Flex>
                    </DialogContent>
                    <DialogActions>
                        <Button appearance=ButtonAppearance::Secondary on_click=Callback::new(move |_| open.set(false))>
                            "Cancel"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Danger
                            disabled=Signal::derive(move || !matches.get())
                            loading=action.pending()
                            attr:data-testid="photon-skip-confirm"
                            on_click=Callback::new(move |_| { action.dispatch(()); })
                        >
                            {confirm_label}
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

    #[test]
    fn skip_says_what_is_dropped() {
        let sub = SubscriptionSummary {
            subscription_name: "warehouse".into(),
            head_seq: Some(96),
            waiting: 41,
            ..SubscriptionSummary::default()
        };
        assert_eq!(
            skip_text(&sub),
            "warehouse moves its checkpoint to the newest event, seq 96. The 41 waiting events are never delivered to it."
        );
    }
}
