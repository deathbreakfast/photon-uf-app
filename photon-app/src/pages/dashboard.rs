use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use orbital::components::{Card, ContentContainer, SpacingSize, Subtitle2, Title3};
use orbital::primitives::{Button, ButtonAppearance, Flex, Space};

use crate::components::{
    server_fn_error_bar, server_fn_error_bar_with_prefix, ActiveSubscriptionsTable,
    ActiveSubscriptionsTableSkeleton, EventsTable, EventsTableColumns, EventsTableSkeleton,
    PhotonStatsGrid, StatsGridSkeleton,
};
use crate::server::{get_dashboard_stats, get_recent_events, get_subscriptions, SubscriptionRow};

/// Recent Events columns: topic links to the event, so no Event ID column.
const RECENT_EVENT_COLUMNS: EventsTableColumns = EventsTableColumns {
    show_event_id: false,
    show_topic: true,
    show_key: true,
    show_seq: true,
    show_created: true,
};

/// Photon dashboard: aggregate stats, recent events, and active subscriptions at a glance.
#[component]
pub fn PhotonDashboardPage() -> impl IntoView {
    crate::hydration::forget_server_resources();
    let navigate = use_navigate();
    let nav_events = navigate.clone();
    let nav_subs = navigate.clone();
    let stats_res = Resource::new(|| (), |()| async move { get_dashboard_stats().await });
    let events_res = Resource::new(|| (), |()| async move { get_recent_events(10).await });
    let subs_res = Resource::new(|| (), |()| async move { get_subscriptions().await });

    view! {
        <ContentContainer data_testid="photon-dashboard">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Title3>"Photon Dashboard"</Title3>

                <Suspense fallback=move || view! { <StatsGridSkeleton /> }>
                    {move || match stats_res.get() {
                        Some(Ok(stats)) => view! {
                            <PhotonStatsGrid stats=stats />
                        }.into_any(),
                        Some(Err(e)) => server_fn_error_bar_with_prefix("Failed to load stats: ", &e),
                        None => view! { <StatsGridSkeleton /> }.into_any(),
                    }}
                </Suspense>

                <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
                    <Space>
                        <Subtitle2>"Recent Events"</Subtitle2>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            on_click=Callback::new(move |_| nav_events(crate::paths::EVENTS, NavigateOptions::default()))
                        >
                            "View All \u{2192}"
                        </Button>
                    </Space>
                    <Suspense fallback=move || view! { <Card><EventsTableSkeleton columns=RECENT_EVENT_COLUMNS /></Card> }>
                        {move || match events_res.get() {
                            Some(Ok(events)) => view! {
                                <Card>
                                    <EventsTable
                                        events=events
                                        columns=RECENT_EVENT_COLUMNS
                                    />
                                </Card>
                            }.into_any(),
                            Some(Err(e)) => server_fn_error_bar(&e),
                            None => view! { <Card><EventsTableSkeleton columns=RECENT_EVENT_COLUMNS /></Card> }.into_any(),
                        }}
                    </Suspense>
                </Flex>

                <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
                    <Space>
                        <Subtitle2>"Active Subscriptions"</Subtitle2>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            on_click=Callback::new(move |_| nav_subs(crate::paths::SUBSCRIPTIONS, NavigateOptions::default()))
                        >
                            "View All \u{2192}"
                        </Button>
                    </Space>
                    <Suspense fallback=move || view! { <Card><ActiveSubscriptionsTableSkeleton /></Card> }>
                        {move || match subs_res.get() {
                            Some(Ok(rows)) => {
                                // Only subscriptions the reader can see; the list page shows the rest.
                                let subs: Vec<_> = rows
                                    .into_iter()
                                    .filter(SubscriptionRow::has_access)
                                    .map(|row| row.sub)
                                    .collect();
                                view! {
                                    <Card>
                                        <ActiveSubscriptionsTable subs=subs />
                                    </Card>
                                }.into_any()
                            }
                            Some(Err(e)) => server_fn_error_bar(&e),
                            None => view! { <Card><ActiveSubscriptionsTableSkeleton /></Card> }.into_any(),
                        }}
                    </Suspense>
                </Flex>
            </Flex>
        </ContentContainer>
    }
}
