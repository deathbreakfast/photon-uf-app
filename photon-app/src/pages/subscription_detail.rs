use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use orbital::components::{
    Caption1, Card, CardHeader, CardHeaderDescription, ContentContainer, SpacingSize, Subtitle2,
    Title3,
};
use orbital::primitives::{Flex, MessageBar, MessageBarIntent};

use crate::components::{
    server_fn_error_or_denied, EventsTable, EventsTableColumns, EventsTableSkeleton, ResourceTabs,
    SubscriptionMetaCard, SubscriptionMetaCardSkeleton,
};
use crate::permissions::subscription_ref;
use crate::server::{get_events, get_subscription};

/// Recent events columns: every row is on the subscription's topic, so no Topic or Key column.
const SUB_EVENT_COLUMNS: EventsTableColumns = EventsTableColumns {
    show_event_id: true,
    show_topic: false,
    show_key: false,
    show_seq: true,
    show_created: true,
};

/// Detail view for a single subscription: configuration, read-state, and recent events, with
/// an Access tab.
///
/// Without View on the subscription, its read state gives way to the shared denied state with
/// Request access. The recent events need View on the subscription's topic.
#[component]
pub fn PhotonSubscriptionDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();

    let sub_res = Resource::new(id, |sid| async move { get_subscription(sid).await });
    // The route carries only the id, so this looks the subscription up again for its
    // topic instead of reading `sub_res`: a page-level read of `sub_res` in a resource
    // source would register with the layout's Suspense and blank the whole page.
    let events_res = Resource::new(id, |sid| async move {
        match get_subscription(sid).await {
            Ok(Some(sub)) => get_events(Some(sub.topic_name), 20).await,
            // Not found or failed: the meta card reports it; the table stays empty.
            Ok(None) | Err(_) => Ok(vec![]),
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

                {move || {
                    let resource = subscription_ref(&id());
                    view! {
                        <ResourceTabs resource=resource>
                            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                                <Suspense fallback=move || view! { <SubscriptionMetaCardSkeleton /> }>
                                    {move || match sub_res.get() {
                                        Some(Ok(Some(s))) => view! {
                                            <SubscriptionMetaCard sub=s />
                                        }.into_any(),
                                        Some(Ok(None)) => view! { <MessageBar intent=MessageBarIntent::Warning>"Subscription not found"</MessageBar> }.into_any(),
                                        Some(Err(e)) => server_fn_error_or_denied(&e, "see this subscription's read state"),
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
                                        {move || match events_res.get() {
                                            Some(Ok(events)) => view! {
                                                <EventsTable events=events columns=SUB_EVENT_COLUMNS />
                                            }.into_any(),
                                            Some(Err(e)) => server_fn_error_or_denied(&e, "see recent events on this topic"),
                                            None => view! { <EventsTableSkeleton columns=SUB_EVENT_COLUMNS /> }.into_any(),
                                        }}
                                    </Suspense>
                                </Card>
                                </div>
                            </Flex>
                        </ResourceTabs>
                    }
                }}
            </Flex>
        </ContentContainer>
    }
}
