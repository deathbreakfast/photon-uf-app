//! Reveal dialog: asks why the viewer needs an event's personal fields, records the answer
//! and hands the real values back to the page.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::Body1;
use orbital::primitives::{
    Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
    DialogTitle, Field, Flex, FlexGap, MessageBar, MessageBarIntent, Select, Textarea,
};

use crate::server::{reveal_event_fields, RevealReason, RevealedFields};

/// The sentence at the top of the dialog: which fields, and what gets recorded.
fn explain(fields: &[String], event_id: &str) -> String {
    let what = if fields.is_empty() {
        "The publisher's id".to_string()
    } else {
        fields.join(", ")
    };
    format!(
        "{what} on {event_id} are personal data. Say why you need them; the reason, your name \
         and the time are recorded and visible to the topic owners."
    )
}

/// A reason from a short list plus details. On success the dialog closes and
/// `on_revealed` gets the real values for this page view.
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
#[component]
pub fn RevealDialog(
    /// Whether the dialog is open.
    open: RwSignal<bool>,
    /// Event whose fields to reveal.
    event_id: String,
    /// Names of the personal payload fields (plus the publisher, when a person).
    personal: Vec<String>,
    /// Receives the revealed values.
    on_revealed: Callback<RevealedFields>,
) -> impl IntoView {
    let reason = RwSignal::new(RevealReason::Support.as_str().to_string());
    let details = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let sentence = explain(&personal, &event_id);
    let event_id = StoredValue::new(event_id);

    let reveal = Action::new(|input: &(String, RevealReason, String)| {
        let (id, reason, details) = input.clone();
        async move { reveal_event_fields(id, reason, details).await }
    });
    Effect::new(move |_| match reveal.value().get() {
        Some(Ok(fields)) => {
            error.set(None);
            open.set(false);
            on_revealed.run(fields);
        }
        Some(Err(e)) => error.set(Some(e.to_string())),
        None => {}
    });
    let pending = reveal.pending();
    let blocked = Signal::derive(move || pending.get() || details.with(|d| d.trim().is_empty()));
    let submit = Callback::new(move |_| {
        let reason = RevealReason::parse(&reason.get_untracked()).unwrap_or(RevealReason::Support);
        reveal.dispatch((event_id.get_value(), reason, details.get_untracked()));
    });

    view! {
        <Dialog open=open>
            <DialogSurface attr:data-testid="photon-reveal-dialog">
                <DialogBody>
                    <DialogTitle>"Reveal personal fields"</DialogTitle>
                    <DialogContent>
                        <Flex vertical=true gap=FlexGap::Medium>
                            <Body1 block=true>{sentence}</Body1>
                            <Field label="Reason">
                                <Select bind=reason attr:data-testid="photon-reveal-reason">
                                    {RevealReason::ALL.into_iter().map(|r| view! {
                                        <option value=r.as_str()>{r.label()}</option>
                                    }).collect_view()}
                                </Select>
                            </Field>
                            <Field label="Details">
                                <Textarea bind=details attr:data-testid="photon-reveal-details" attr:placeholder="Ticket number, what you're checking" />
                            </Field>
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
                            disabled=blocked
                            on_click=submit
                            attr:data-testid="photon-reveal-submit"
                        >
                            "Reveal and record"
                        </Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}
