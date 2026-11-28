//! Redeliver dialog: send one event to one subscription again.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::Body1;
use orbital::primitives::{
    Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
    DialogTitle, Field, Flex, FlexGap, MessageBar, MessageBarIntent, Select,
};

use crate::server::{redeliver_event, DeliveryRow, DeliveryState};

/// What the subscription did with the event so far, for the dialog's sentence.
fn so_far(row: &DeliveryRow) -> String {
    let name = &row.subscription_name;
    match &row.state {
        DeliveryState::Handled => row.at.as_ref().map_or_else(
            || format!("{name} has handled this event."),
            |at| format!("{name} handled this event at {at}."),
        ),
        DeliveryState::Waiting { .. } => format!("{name} hasn't reached this event yet."),
        DeliveryState::Failed { .. } => format!("{name} failed on this event."),
    }
}

/// Pick a subscription (the one clicked is selected), read what happens, confirm. Only that
/// subscription gets the event; the dialog warns that its handler runs again.
#[component]
pub fn RedeliverDialog(
    /// Whether the dialog is open.
    open: RwSignal<bool>,
    /// Event to send again.
    event_id: String,
    /// Subscriptions the viewer can run.
    rows: Vec<DeliveryRow>,
    /// Selected subscription id.
    selected: RwSignal<String>,
    /// Called with the subscription id after a successful redelivery.
    on_done: Callback<String>,
) -> impl IntoView {
    let rows = StoredValue::new(rows);
    let error = RwSignal::new(Option::<String>::None);
    let event_id = StoredValue::new(event_id);
    let redeliver = Action::new(|input: &(String, String)| {
        let (event_id, sub) = input.clone();
        async move {
            let result = redeliver_event(event_id, sub.clone()).await;
            result.map(|()| sub)
        }
    });
    Effect::new(move |_| match redeliver.value().get() {
        Some(Ok(sub)) => {
            error.set(None);
            open.set(false);
            on_done.run(sub);
        }
        Some(Err(e)) => error.set(Some(e.to_string())),
        None => {}
    });
    let pending = redeliver.pending();
    let sentence = move || {
        let id = selected.get();
        rows.with_value(|rows| {
            rows.iter().find(|r| r.subscription_id == id).map(|row| {
                format!(
                    "{} It gets the event once more; no other subscription does.",
                    so_far(row)
                )
            })
        })
    };
    let submit = Callback::new(move |_| {
        redeliver.dispatch((event_id.get_value(), selected.get_untracked()));
    });

    view! {
        <Dialog open=open>
            <DialogSurface attr:data-testid="photon-redeliver-dialog">
                <DialogBody>
                    <DialogTitle>"Redeliver to one subscription"</DialogTitle>
                    <DialogContent>
                        <Flex vertical=true gap=FlexGap::Medium>
                            <Field label="Subscription">
                                <Select bind=selected attr:data-testid="photon-redeliver-subscription">
                                    {rows.get_value().into_iter().map(|r| view! {
                                        <option value=r.subscription_id>{r.subscription_name}</option>
                                    }).collect_view()}
                                </Select>
                            </Field>
                            <Body1 block=true>{sentence}</Body1>
                            <MessageBar intent=MessageBarIntent::Warning>
                                "If the handler doesn't check for duplicates, it acts on this event twice."
                            </MessageBar>
                            {move || error.get().map(|e| view! {
                                <MessageBar intent=MessageBarIntent::Error>{e}</MessageBar>
                            })}
                        </Flex>
                    </DialogContent>
                    <DialogActions>
                        <Button appearance=ButtonAppearance::Secondary on_click=Callback::new(move |_| open.set(false))>
                            "Cancel"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Primary
                            disabled=pending
                            on_click=submit
                            attr:data-testid="photon-redeliver-submit"
                        >
                            "Redeliver"
                        </Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}
