//! Replay dialog: pick where to start, see what will be sent again, then confirm.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::Caption1;
use orbital::primitives::{
    Button, ButtonAppearance, DatePicker, DatePickerAppearance, DatePickerBind, DatetimeFormat,
    DatetimeTimezone, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle,
    Field, Flex, FlexGap, Input, InputAppearance, InputBind, InputSuffix, InputType, MessageBar,
    MessageBarBody, MessageBarIntent, MessageBarTitle, OrbitalDateTime, Radio, RadioGroup,
    RadioGroupBind,
};
use photon_backend::{ReplayPreviewSummary, SubscriptionSummary};

use crate::components::{server_fn_error_bar, ActionNotice};
use crate::server::{replay_preview, replay_subscription};

/// The replay start the dialog sends: `(kind, value)` for the server fns, or `None` while
/// the matching field is empty or incomplete.
#[must_use]
pub fn replay_input(
    kind: &str,
    date: Option<chrono::NaiveDate>,
    time: &str,
    seq: &str,
    event: &str,
) -> Option<(String, String)> {
    match kind {
        "time" => {
            let date = date?;
            let time = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
            let at = date.and_time(time).and_utc();
            Some(("time".into(), at.to_rfc3339()))
        }
        "seq" => {
            let seq = seq.trim();
            seq.parse::<i64>()
                .ok()
                .map(|_| ("seq".into(), seq.to_string()))
        }
        "event" => {
            let event = event.trim();
            (!event.is_empty()).then(|| ("event".into(), event.to_string()))
        }
        _ => None,
    }
}

/// Where the replay starts, in words, for the preview and the notice.
#[must_use]
pub fn from_words(kind: &str, value: &str, preview: &ReplayPreviewSummary) -> String {
    match kind {
        "time" => chrono::DateTime::parse_from_rfc3339(value)
            .map_or_else(|_| value.to_string(), |t| t.format("%H:%M").to_string()),
        "seq" => format!("seq {}", preview.from_seq),
        _ => value.to_string(),
    }
}

/// "64 events from 07:30 to now: the 41 waiting and 23 it already handled."
#[must_use]
pub fn preview_text(from: &str, preview: &ReplayPreviewSummary) -> String {
    if preview.count == 0 {
        return format!("No events from {from} to now, so there is nothing to replay.");
    }
    let waiting = preview.count.saturating_sub(preview.already_handled);
    format!(
        "{} events from {from} to now: the {waiting} waiting and {} it already handled.",
        preview.count, preview.already_handled
    )
}

fn retention_line(sub: &SubscriptionSummary) -> String {
    sub.retention_secs
        .and_then(|r| i64::try_from(r).ok())
        .map_or_else(
            || "Events that have left the log can't be replayed.".to_string(),
            |r| {
                format!(
                    "Events older than {} are gone and can't be replayed.",
                    photon_backend::duration_words(r)
                )
            },
        )
}

/// Replay from a time, a sequence number or an event id, with the count it will send.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
#[allow(clippy::too_many_lines)] // one form: the start fields, preview and confirm share signals
pub fn ReplayDialog(
    /// Open state.
    open: RwSignal<bool>,
    /// The subscription.
    sub: SubscriptionSummary,
    /// Called after the replay starts.
    on_done: Callback<ActionNotice>,
) -> impl IntoView {
    let id = StoredValue::new(sub.subscription_id.clone());
    let name = sub.subscription_name.clone();
    let now = chrono::Utc::now();
    let kind = RwSignal::new(Some("time".to_string()));
    let date = RwSignal::new(OrbitalDateTime::from_naive_date(
        now.date_naive(),
        DatetimeTimezone::Utc,
    ));
    let time = RwSignal::new(
        (now - chrono::Duration::hours(1))
            .format("%H:%M")
            .to_string(),
    );
    let seq = RwSignal::new(sub.last_seq.unwrap_or(0).to_string());
    let event = RwSignal::new(String::new());

    let input = Memo::new(move |_| {
        replay_input(
            &kind.get().unwrap_or_default(),
            date.get().and_then(|d| d.wall_date()),
            &time.get(),
            &seq.get(),
            &event.get(),
        )
    });
    // The dialog's own preview, keyed on the start point; it loads only while open.
    let preview = LocalResource::new(move || {
        let input = open.get().then(|| input.get()).flatten();
        let id = id.get_value();
        async move {
            match input {
                Some((k, v)) => Some(replay_preview(id, k, v).await),
                None => None,
            }
        }
    });
    let ready = Signal::derive(move || matches!(preview.get(), Some(Some(Ok(p))) if p.count > 0));
    let action = Action::new(move |(): &()| {
        let id = id.get_value();
        let input = input.get_untracked();
        async move {
            let (k, v) = input.ok_or_else(|| ServerFnError::new("Pick where the replay starts"))?;
            let p = replay_subscription(id, k.clone(), v.clone()).await?;
            Ok::<_, ServerFnError>((from_words(&k, &v, &p), p))
        }
    });
    let error = RwSignal::new(None::<ServerFnError>);
    Effect::new(move |_| match action.value().get() {
        Some(Ok((from, p))) => {
            error.set(None);
            open.set(false);
            on_done.run(ActionNotice::Replay {
                count: p.count,
                from,
            });
        }
        Some(Err(e)) => error.set(Some(e)),
        None => {}
    });
    let count_label = move || match preview.get() {
        Some(Some(Ok(p))) => format!("Replay {} events", p.count),
        _ => "Replay".to_string(),
    };
    let warning_title = format!("{name} sees these events again");
    let warning_body = format!(
        "Its handler must cope with duplicates. {}",
        retention_line(&sub)
    );
    let date_appearance = DatePickerAppearance {
        format: Signal::from(DatetimeFormat::IsoDate),
        timezone: Signal::from(DatetimeTimezone::Utc),
        ..Default::default()
    };
    let number = InputAppearance {
        input_type: Signal::from(InputType::Number),
        ..Default::default()
    };

    view! {
        <Dialog open=open>
            <DialogSurface attr:data-testid="photon-replay-dialog">
                <DialogBody>
                    <DialogTitle>{format!("Replay {name}")}</DialogTitle>
                    <DialogContent>
                        <Flex vertical=true gap=FlexGap::Medium>
                            <div data-testid="photon-replay-from">
                                <RadioGroup bind=RadioGroupBind::new(kind)>
                                    <Radio value="time" label="From a time" />
                                    <Radio value="seq" label="From a sequence number" />
                                    <Radio value="event" label="From an event id" />
                                </RadioGroup>
                            </div>
                            <div hidden=move || kind.get().as_deref() != Some("time")>
                                <Field label="Start at">
                                    <Flex gap=FlexGap::Small>
                                        <div data-testid="photon-replay-date">
                                            <DatePicker bind=DatePickerBind::new(date) appearance=date_appearance />
                                        </div>
                                        <div data-testid="photon-replay-time">
                                            <Input bind=InputBind::new(time)>
                                                <InputSuffix slot>"UTC"</InputSuffix>
                                            </Input>
                                        </div>
                                    </Flex>
                                </Field>
                            </div>
                            <div hidden=move || kind.get().as_deref() != Some("seq")>
                                <Field label="Start after seq">
                                    <div data-testid="photon-replay-seq">
                                        <Input bind=InputBind::new(seq) appearance=number />
                                    </div>
                                </Field>
                            </div>
                            <div hidden=move || kind.get().as_deref() != Some("event")>
                                <Field label="Event id">
                                    <div data-testid="photon-replay-event">
                                        <Input bind=InputBind::new(event) />
                                    </div>
                                </Field>
                            </div>
                            {move || match preview.get() {
                                Some(Some(Ok(p))) => {
                                    let (k, v) = input.get().unwrap_or_default();
                                    view! {
                                        <MessageBar intent=MessageBarIntent::Info attr:data-testid="photon-replay-preview">
                                            {preview_text(&from_words(&k, &v, &p), &p)}
                                        </MessageBar>
                                    }
                                    .into_any()
                                }
                                Some(Some(Err(e))) => server_fn_error_bar(&e),
                                _ => ().into_any(),
                            }}
                            <MessageBar intent=MessageBarIntent::Warning>
                                <MessageBarBody>
                                    <MessageBarTitle>{warning_title}</MessageBarTitle>
                                    {warning_body}
                                </MessageBarBody>
                            </MessageBar>
                            <Caption1>"Recorded in the subscription history with your name."</Caption1>
                            {move || error.get().map(|e| server_fn_error_bar(&e))}
                        </Flex>
                    </DialogContent>
                    <DialogActions>
                        <Button appearance=ButtonAppearance::Secondary on_click=Callback::new(move |_| open.set(false))>
                            "Cancel"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Primary
                            disabled=Signal::derive(move || !ready.get())
                            loading=action.pending()
                            attr:data-testid="photon-replay-confirm"
                            on_click=Callback::new(move |_| { action.dispatch(()); })
                        >
                            {count_label}
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
    fn inputs_need_their_field() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 27);
        assert_eq!(
            replay_input("time", day, "07:30", "", ""),
            Some(("time".into(), "2026-10-27T07:30:00+00:00".into()))
        );
        assert_eq!(replay_input("time", day, "7.30", "", ""), None);
        assert_eq!(replay_input("time", None, "07:30", "", ""), None);
        assert_eq!(
            replay_input("seq", None, "", " 40 ", ""),
            Some(("seq".into(), "40".into()))
        );
        assert_eq!(replay_input("seq", None, "", "x", ""), None);
        assert_eq!(replay_input("event", None, "", "", " "), None);
    }

    #[test]
    fn preview_reads_as_drawn() {
        let p = ReplayPreviewSummary {
            from_seq: 41,
            count: 56,
            already_handled: 15,
        };
        assert_eq!(from_words("seq", "40", &p), "seq 41");
        assert_eq!(
            preview_text("seq 41", &p),
            "56 events from seq 41 to now: the 41 waiting and 15 it already handled."
        );
        assert_eq!(from_words("time", "2026-10-27T07:30:00+00:00", &p), "07:30");
    }
}
