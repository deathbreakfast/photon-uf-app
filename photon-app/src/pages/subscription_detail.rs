use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use orbital::components::{
    Caption1, Card, CardHeader, CardHeaderDescription, ContentContainer, SpacingSize, Subtitle2,
    Title3,
};
use orbital::primitives::{Flex, MessageBar, MessageBarIntent};

use crate::components::{
    server_fn_error_bar, EventsTable, EventsTableColumns, EventsTableSkeleton,
    SubscriptionMetaCard, SubscriptionMetaCardSkeleton,
};
use crate::server::{get_events, get_subscription};

/// Recent events columns: every row is on the subscription's topic, so no Topic or Key column.
const SUB_EVENT_COLUMNS: EventsTableColumns = EventsTableColumns {
    show_event_id: true,
    show_topic: false,
    show_key: false,
    show_seq: true,
    show_created: true,
};

/// Detail view for a single subscription: configuration, read-state, and recent events.
#[component]
pub fn PhotonSubscriptionDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();

    let sub_res = Resource::new(id, |sid| async move { get_subscription(sid).await });
    // The route carries only the id, so the topic (and the events call) waits on the subscription.
    let topic_for_events = move || {
        sub_res
            .get()
            .and_then(|r| r.ok().flatten())
            .map(|s| s.topic_name)
    };
    let events_res = Resource::new(topic_for_events, |topic_opt| async move {
        if let Some(topic) = topic_opt {
            get_events(Some(topic), 20).await
        } else {
            Ok(vec![])
        }
    });

    view! {
        <ContentContainer data_testid="photon-subscription-detail">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                // The name once it loads; the URL id while loading, missing, or failed.
                <Title3>
                    <Suspense fallback=move || id()>
                        {move || match sub_res.get() {
                            Some(Ok(Some(s))) => s.subscription_name,
                            _ => id(),
                        }}
                    </Suspense>
                </Title3>

                <Suspense fallback=move || view! { <SubscriptionMetaCardSkeleton /> }>
                    {move || match sub_res.get() {
                        Some(Ok(Some(s))) => view! {
                            <SubscriptionMetaCard sub=s />
                        }.into_any(),
                        Some(Ok(None)) => view! { <MessageBar intent=MessageBarIntent::Warning>"Subscription not found"</MessageBar> }.into_any(),
                        Some(Err(e)) => server_fn_error_bar(&e),
                        None => view! { <SubscriptionMetaCardSkeleton /> }.into_any(),
                    }}
                </Suspense>

                <div data-testid="photon-subscription-events">
                <Card>
                    <CardHeader>
                        <Subtitle2>"Recent events on this topic"</Subtitle2>
                        <CardHeaderDescription slot>
                            <Caption1>"The newest events on the topic, not only the ones this subscription handled."</Caption1>
                        </CardHeaderDescription>
                    </CardHeader>
                    <Suspense fallback=move || view! { <EventsTableSkeleton columns=SUB_EVENT_COLUMNS /> }>
                        {move || match (sub_res.get(), events_res.get()) {
                            (None, _) | (_, None) => view! { <EventsTableSkeleton columns=SUB_EVENT_COLUMNS /> }.into_any(),
                            (Some(_), Some(Ok(events))) => view! {
                                <EventsTable events=events columns=SUB_EVENT_COLUMNS />
                            }.into_any(),
                            (Some(_), Some(Err(e))) => server_fn_error_bar(&e),
                        }}
                    </Suspense>
                </Card>
                </div>
            </Flex>
        </ContentContainer>
    }
}
