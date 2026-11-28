use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;
use orbital::components::{ContentContainer, EmptyState, SpacingSize, Title3};
use orbital::primitives::{
    Breadcrumb, BreadcrumbButton, BreadcrumbDivider, BreadcrumbItem, Button, ButtonAppearance,
    CopyButton, Flex, FlexAlign, FlexGap, FlexWrap, MessageBar, MessageBarBody, MessageBarIntent,
    MessageBarTitle,
};

use crate::components::server_fn_error_or_denied;
use crate::components::{
    EventDeliveryCard, EventDetailSkeleton, EventMetaCard, EventPayloadCard, RedeliverDialog,
    RevealDialog,
};
use crate::server::{get_event, EventDetail, RevealedFields};

/// "7 days", "12 hours", "30 minutes".
fn retention_phrase(secs: u64) -> String {
    let unit = |n: u64, one: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {one}s")
        }
    };
    match secs {
        s if s >= 86_400 && s % 86_400 == 0 => unit(s / 86_400, "day"),
        s if s >= 3_600 && s % 3_600 == 0 => unit(s / 3_600, "hour"),
        s => unit((s / 60).max(1), "minute"),
    }
}

/// "Sep 30, 2026" from an RFC 3339 time, or the input when it doesn't parse.
fn day(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339).map_or_else(
        |_| rfc3339.to_string(),
        |t| t.format("%b %-d, %Y").to_string(),
    )
}

/// "08:31 UTC" from an RFC 3339 time.
fn time_of_day(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339).map_or_else(
        |_| rfc3339.to_string(),
        |t| {
            t.with_timezone(&chrono::Utc)
                .format("%H:%M UTC")
                .to_string()
        },
    )
}

/// The expired notice: how long payloads are kept, when this one was published, who to ask.
fn expired_sentence(event: &EventDetail) -> String {
    let kept = event.retention_secs.map_or_else(
        || "This store's retention isn't set.".to_string(),
        |s| format!("Photon keeps event payloads for {}.", retention_phrase(s)),
    );
    format!(
        "{kept} This one was published on {}, so only its id, topic, key and time are left. \
         If you need what it carried, ask the owners of {}.",
        day(&event.created_at),
        event.topic_name
    )
}

/// The event page: breadcrumb, title with copy, facts, who handled it, and the payload with
/// personal fields masked until a recorded reveal.
///
/// Needs View on the event's topic; without it the page shows the shared denied state. An
/// expired event keeps its metadata; an unknown id shows a way back to search.
#[allow(clippy::too_many_lines)] // one page view: header, notices, cards and dialogs
#[component]
pub fn PhotonEventDetailPage() -> impl IntoView {
    crate::hydration::forget_server_resources();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());

    // Bumped after a redelivery so the Subscriptions card shows the new state.
    let refresh = RwSignal::new(0u32);
    let event_res = Resource::new(
        move || (id.get(), refresh.get()),
        |(eid, _)| async move { get_event(eid).await },
    );

    // Reveals and notices last for this page view only.
    let revealed = RwSignal::new(Option::<RevealedFields>::None);
    let redelivered = RwSignal::new(Option::<String>::None);
    Effect::new(move |_| {
        id.track();
        revealed.set(None);
        redelivered.set(None);
    });

    let reveal_open = RwSignal::new(false);
    let redeliver_open = RwSignal::new(false);
    let redeliver_to = RwSignal::new(String::new());
    let navigate = use_navigate();
    let search = Callback::new(move |_| navigate("/photon/events", NavigateOptions::default()));

    view! {
        <ContentContainer data_testid="photon-event-detail">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Breadcrumb>
                    <BreadcrumbItem>
                        <BreadcrumbButton href="/photon/events">"Events"</BreadcrumbButton>
                    </BreadcrumbItem>
                    <BreadcrumbDivider />
                    <Transition fallback=|| ()>
                        {move || event_res.get().and_then(Result::ok).flatten().map(|ev| {
                            let href = photon_backend::photon_topic_path(&ev.topic_name);
                            view! {
                                <BreadcrumbItem>
                                    <BreadcrumbButton href=href>{ev.topic_name}</BreadcrumbButton>
                                </BreadcrumbItem>
                                <BreadcrumbDivider />
                            }
                        })}
                    </Transition>
                    <BreadcrumbItem>
                        <BreadcrumbButton current=true>{move || id.get()}</BreadcrumbButton>
                    </BreadcrumbItem>
                </Breadcrumb>

                <Flex wrap=FlexWrap::Wrap gap=FlexGap::Medium align=FlexAlign::Center>
                    <Title3>"Event " {move || id.get()}</Title3>
                    <CopyButton text=Signal::derive(move || id.get()) label="Copy" />
                </Flex>

                {move || revealed.get().map(|r| {
                    let who = r.record.revealed_by.clone().unwrap_or_else(|| "you".into());
                    let line = format!(
                        "Recorded: {who}, {}, reason {}: \"{}\". The topic's owners can see this in its reveal log.",
                        time_of_day(&r.record.revealed_at),
                        r.record.reason.label(),
                        r.record.details,
                    );
                    view! {
                        <div data-testid="photon-reveal-banner">
                            <MessageBar intent=MessageBarIntent::Info>
                                <MessageBarBody>
                                    <MessageBarTitle>"Personal fields shown to you until you leave this page"</MessageBarTitle>
                                    {line}
                                </MessageBarBody>
                            </MessageBar>
                        </div>
                    }
                })}
                {move || redelivered.get().map(|sub| view! {
                    <div data-testid="photon-redeliver-notice">
                        <MessageBar intent=MessageBarIntent::Success>
                            {format!("Sent the event to {sub} again.")}
                        </MessageBar>
                    </div>
                })}

                <Suspense fallback=move || view! { <EventDetailSkeleton /> }>
                    {move || event_res.get().map(|res| match res {
                        Ok(Some(ev)) if ev.transport_expired => {
                            let sentence = expired_sentence(&ev);
                            view! {
                                <div data-testid="photon-event-expired">
                                    <MessageBar intent=MessageBarIntent::Warning>
                                        <MessageBarBody>
                                            <MessageBarTitle>"The payload has expired"</MessageBarTitle>
                                            {sentence}
                                        </MessageBarBody>
                                    </MessageBar>
                                </div>
                                <EventMetaCard event=ev published_by_id=Signal::derive(|| Option::<String>::None) />
                            }.into_any()
                        }
                        Ok(Some(ev)) => {
                            let mut personal = ev.personal_fields.clone();
                            if ev.published_by == crate::server::PublishedBy::Person {
                                personal.push("the publisher's id".into());
                            }
                            let runnable: Vec<_> = ev.deliveries.iter().filter(|d| d.can_run).cloned().collect();
                            let can_reveal = ev.has_personal();
                            let event_id = ev.event_id.clone();
                            view! {
                                <EventMetaCard
                                    event=ev.clone()
                                    published_by_id=Signal::derive(move || revealed.get().and_then(|r| r.published_by_id))
                                />
                                <EventDeliveryCard
                                    deliveries=ev.deliveries.clone()
                                    on_redeliver=Callback::new(move |sub: String| {
                                        redeliver_to.set(sub);
                                        redeliver_open.set(true);
                                    })
                                />
                                <EventPayloadCard
                                    fields=ev.fields.clone()
                                    payload_json=ev.payload_json
                                    revealed=revealed
                                    can_reveal=can_reveal
                                    on_reveal=Callback::new(move |()| reveal_open.set(true))
                                />
                                <RevealDialog
                                    open=reveal_open
                                    event_id=event_id.clone()
                                    personal=personal
                                    on_revealed=Callback::new(move |r| revealed.set(Some(r)))
                                />
                                <RedeliverDialog
                                    open=redeliver_open
                                    event_id=event_id
                                    rows=runnable
                                    selected=redeliver_to
                                    on_done=Callback::new(move |sub: String| {
                                        let name = sub.rsplit(':').next().unwrap_or(&sub).to_string();
                                        redelivered.set(Some(name));
                                        refresh.update(|n| *n += 1);
                                    })
                                />
                            }.into_any()
                        }
                        Ok(None) => view! {
                            <Flex vertical=true gap=FlexGap::Medium align=FlexAlign::Center attr:data-testid="photon-event-not-found">
                                <EmptyState
                                    message="No event with this id"
                                    description="Check the id for a typo, or search events by id or key."
                                />
                                <Button appearance=ButtonAppearance::Primary on_click=search>
                                    "Search events"
                                </Button>
                            </Flex>
                        }.into_any(),
                        Err(e) => server_fn_error_or_denied(&e, "see this event's payload"),
                    })}
                </Suspense>
            </Flex>
        </ContentContainer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_reads_in_whole_units() {
        assert_eq!(retention_phrase(7 * 86_400), "7 days");
        assert_eq!(retention_phrase(86_400), "1 day");
        assert_eq!(retention_phrase(12 * 3_600), "12 hours");
        assert_eq!(retention_phrase(90), "1 minute");
    }

    #[test]
    fn expired_sentence_names_retention_or_says_it_is_unset() {
        let mut ev = photon_backend::event_detail_transport_expired(
            "e1",
            "orders",
            None,
            1,
            "2026-09-30T08:00:00+00:00",
            "dead-lettered",
        );
        assert!(expired_sentence(&ev).starts_with("This store's retention isn't set."));
        ev.retention_secs = Some(7 * 86_400);
        let s = expired_sentence(&ev);
        assert!(s.contains("keeps event payloads for 7 days"), "{s}");
        assert!(s.contains("published on Sep 30, 2026"), "{s}");
    }
}
