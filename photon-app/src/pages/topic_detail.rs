use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use orbital::components::{
    Card, ContentContainer, SkeletonItemSize, SpacingSize, Subtitle2, Title3,
};
use orbital::primitives::{Flex, MessageBar, MessageBarIntent};

use crate::components::{
    server_fn_error_bar, server_fn_error_or_denied, EventsTable, EventsTableColumns,
    EventsTableSkeleton, LinesCardSkeleton, ResourceTabs, TopicMetaCard, TopicSubscriptionsTable,
    TopicSubscriptionsTableSkeleton,
};
use crate::permissions::{denied_resource, topic_ref};
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

/// Detail view for a single topic: schema, subscriptions, and recent events, with an Access tab.
///
/// Without View on the topic, the schema and events give way to the shared denied state with
/// Request access; the title and the subscriptions list stay.
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

                {move || {
                    let resource = topic_ref(&topic_name());
                    view! {
                        <ResourceTabs resource=resource>
                            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                                <Suspense fallback=move || view! { <TopicMetaSkeleton /> }>
                                    {move || match topic_res.get() {
                                        Some(Ok(Some(t))) => view! {
                                            <TopicMetaCard topic=t />
                                        }.into_any(),
                                        Some(Ok(None)) => view! { <MessageBar intent=MessageBarIntent::Warning>"Topic not found"</MessageBar> }.into_any(),
                                        Some(Err(e)) => server_fn_error_or_denied(&e, "see this topic's schema and events"),
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
                                                    let filtered: Vec<_> = subs
                                                        .iter()
                                                        .filter(|row| row.sub.topic_name == name)
                                                        .map(|row| row.sub.clone())
                                                        .collect();
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
                                                // The schema card above already shows the denied state.
                                                Some(Err(e)) if denied_resource(&e.to_string()).is_some() => ().into_any(),
                                                Some(Err(e)) => server_fn_error_bar(&e),
                                                None => view! { <Card><EventsTableSkeleton columns=TOPIC_EVENT_COLUMNS /></Card> }.into_any(),
                                            }
                                        }}
                                    </Suspense>
                                </Flex>
                            </Flex>
                        </ResourceTabs>
                    }
                }}
            </Flex>
        </ContentContainer>
    }
}
