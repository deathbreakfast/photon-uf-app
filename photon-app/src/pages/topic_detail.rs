use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use orbital::components::{
    Card, ContentContainer, SkeletonItemSize, SpacingSize, Subtitle2, Title3,
};
use orbital::primitives::{Flex, MessageBar, MessageBarIntent};

use crate::components::{
    server_fn_error_bar, EventsTable, EventsTableColumns, EventsTableSkeleton, LinesCardSkeleton,
    TopicMetaCard, TopicSubscriptionsTable, TopicSubscriptionsTableSkeleton,
};
use crate::server::{get_events, get_subscriptions, get_topic};

/// Recent events columns: the topic is the page's, so no Topic or Key column.
const TOPIC_EVENT_COLUMNS: EventsTableColumns = EventsTableColumns {
    show_event_id: true,
    show_topic: false,
    show_key: false,
    show_seq: true,
    show_created: true,
};

/// Placeholder for `TopicMetaCard`: keyed-by, schema, and counts lines.
#[component]
fn TopicMetaSkeleton() -> impl IntoView {
    view! {
        <LinesCardSkeleton lines=vec![
            ("140px", SkeletonItemSize::S16),
            ("80%", SkeletonItemSize::S16),
            ("200px", SkeletonItemSize::S16),
        ] />
    }
}

/// Detail view for a single topic: schema, subscriptions, and recent events.
#[component]
pub fn PhotonTopicDetailPage() -> impl IntoView {
    let params = use_params_map();
    let topic_name = move || params.get().get("topic_name").unwrap_or_default();

    let topic_res = Resource::new(topic_name, |name| async move { get_topic(name).await });
    let events_res = Resource::new(topic_name, |name| async move {
        get_events(Some(name), 20).await
    });
    let subs_res = Resource::new(|| (), |()| async move { get_subscriptions().await });

    view! {
        <ContentContainer data_testid="photon-topic-detail">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Title3>"Topic: " {move || topic_name()}</Title3>

                <Suspense fallback=move || view! { <TopicMetaSkeleton /> }>
                    {move || match topic_res.get() {
                        Some(Ok(Some(t))) => view! {
                            <TopicMetaCard topic=t />
                        }.into_any(),
                        Some(Ok(None)) => view! { <MessageBar intent=MessageBarIntent::Warning>"Topic not found"</MessageBar> }.into_any(),
                        Some(Err(e)) => server_fn_error_bar(&e),
                        None => view! { <TopicMetaSkeleton /> }.into_any(),
                    }}
                </Suspense>

                <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
                    <Subtitle2>"Subscriptions for this topic"</Subtitle2>
                    <Suspense fallback=move || view! { <TopicSubscriptionsTableSkeleton /> }>
                        {move || {
                            let name = topic_name();
                            match subs_res.get() {
                                Some(Ok(subs)) => {
                                    let filtered: Vec<_> = subs.iter().filter(|s| s.topic_name == name).cloned().collect();
                                    view! {
                                        <TopicSubscriptionsTable subs=filtered />
                                    }.into_any()
                                }
                                _ => view! { <TopicSubscriptionsTableSkeleton /> }.into_any(),
                            }
                        }}
                    </Suspense>
                </Flex>

                <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
                    <Subtitle2>"Recent events"</Subtitle2>
                    <Suspense fallback=move || view! { <Card><EventsTableSkeleton columns=TOPIC_EVENT_COLUMNS /></Card> }>
                        {move || {
                            match events_res.get() {
                                Some(Ok(events)) => view! {
                                    <Card>
                                        <EventsTable
                                            events=events
                                            columns=TOPIC_EVENT_COLUMNS
                                        />
                                    </Card>
                                }.into_any(),
                                Some(Err(e)) => server_fn_error_bar(&e),
                                None => view! { <Card><EventsTableSkeleton columns=TOPIC_EVENT_COLUMNS /></Card> }.into_any(),
                            }
                        }}
                    </Suspense>
                </Flex>
            </Flex>
        </ContentContainer>
    }
}
