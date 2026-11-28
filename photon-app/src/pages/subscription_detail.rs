use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use orbital::components::{Caption1, ContentContainer, SpacingSize, Title3};
use orbital::primitives::{
    Breadcrumb, BreadcrumbButton, BreadcrumbDivider, BreadcrumbItem, Button, ButtonAppearance,
    Flex, FlexGap, FlexWrap, Link, Menu, MenuItem, MenuTrigger, MessageBar, MessageBarBody,
    MessageBarIntent, MessageBarTitle,
};
use photon_backend::{SubscriptionHealth, SubscriptionSummary};

use crate::components::{
    action_notice_view, server_fn_error_bar, server_fn_error_or_denied, waiting_events_error,
    ActionNotice, PauseDialog, ReplayDialog, ResourceTabs, SkipDialog, SubscriptionHealthBadge,
    SubscriptionHealthCard, SubscriptionHealthCardSkeleton, SubscriptionMetaCard,
    SubscriptionMetaCardSkeleton, WaitingEventsCard, WaitingEventsCardSkeleton, WhatToDoCard,
};
use crate::permissions::subscription_ref;
use crate::server::{
    get_subscription, get_waiting_events, retry_subscription_now, SubscriptionRow,
};

/// Waiting events listed on the page.
const WAITING_LIMIT: u32 = 10;

/// "Paused by kenji at 08:20" and what waits while it is paused.
#[must_use]
pub fn paused_text(
    sub: &SubscriptionSummary,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(String, String)> {
    let paused = sub.paused.as_ref()?;
    let title = format!(
        "Paused by {} at {}",
        paused.by,
        photon_backend::clock_words(&paused.at, now)
    );
    let waiting = match sub.waiting {
        0 => "New events wait for it".to_string(),
        1 => "1 event is waiting and new ones join it".to_string(),
        n => format!("{n} events are waiting and new ones join them"),
    };
    let retention = sub
        .retention_secs
        .and_then(|r| i64::try_from(r).ok())
        .map_or_else(String::new, |r| {
            format!(
                " while they stay within the {} retention",
                photon_backend::duration_words(r)
            )
        });
    Some((title, format!("{waiting}. Nothing is lost{retention}.")))
}

/// The caption under the name: "Reads <topic> · Owner: … · Consumer group".
fn caption_line(row: &SubscriptionRow) -> impl IntoView {
    let topic = row.sub.topic_name.clone();
    let mode = crate::components::mode_label(&row.sub.mode);
    let rest = if row.has_access() {
        format!(" · Owner: {} · {mode}", row.owner)
    } else {
        format!(" · Owner: {}", row.owner)
    };
    view! {
        <Caption1 attr:data-testid="photon-subscription-caption">
            "Reads " <Link href=photon_backend::photon_topic_path(&topic)>{topic.clone()}</Link> {rest}
        </Caption1>
    }
}

/// Detail view for a single subscription: health first, What to do when it's Stalled or
/// Failing, the events it still owes, and its details, with an Access tab. Pause, Resume,
/// Replay, Retry now and Skip to latest show for the viewer's role, each behind a confirm.
///
/// Without View on the subscription, its read state gives way to the shared denied state with
/// Request access.
#[component]
#[allow(clippy::too_many_lines)] // one page: header, tabs and overview share the resources and dialog signals
pub fn PhotonSubscriptionDetailPage() -> impl IntoView {
    crate::hydration::forget_server_resources();
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();
    // Bumped after each action so the page reads the new state. Dialogs, tabs and notices
    // never feed these keys.
    let refresh = RwSignal::new(0_u32);

    let sub_res = Resource::new(
        move || (id(), refresh.get()),
        |(sid, _)| async move { get_subscription(sid).await },
    );
    let waiting_res = Resource::new(
        move || (id(), refresh.get()),
        |(sid, _)| async move { get_waiting_events(sid, WAITING_LIMIT).await },
    );

    let notice = RwSignal::new(None::<ActionNotice>);
    let replay_open = RwSignal::new(false);
    let pause_open = RwSignal::new(false);
    let skip_open = RwSignal::new(false);
    let on_done = Callback::new(move |n: ActionNotice| {
        notice.set(Some(n));
        refresh.update(|r| *r += 1);
    });
    let retry = Action::new(move |sid: &String| {
        let sid = sid.clone();
        async move { retry_subscription_now(sid).await }
    });
    let retry_error = RwSignal::new(None::<ServerFnError>);
    Effect::new(move |_| match retry.value().get() {
        Some(Ok(())) => {
            retry_error.set(None);
            on_done.run(ActionNotice::Retry);
        }
        Some(Err(e)) => retry_error.set(Some(e)),
        None => {}
    });
    let on_pause = Callback::new(move |()| pause_open.set(true));
    let on_replay = Callback::new(move |()| replay_open.set(true));
    let on_retry = Callback::new(move |()| {
        retry.dispatch(id());
    });

    view! {
        <ContentContainer data_testid="photon-subscription-detail">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Breadcrumb>
                    <BreadcrumbItem>
                        <BreadcrumbButton href="/photon/subscriptions">"Subscriptions"</BreadcrumbButton>
                    </BreadcrumbItem>
                    <BreadcrumbDivider />
                    <BreadcrumbItem>
                        <BreadcrumbButton current=true>
                            <Transition fallback=move || id()>
                                {move || match sub_res.get() {
                                    Some(Ok(Some(row))) => row.sub.subscription_name,
                                    _ => id(),
                                }}
                            </Transition>
                        </BreadcrumbButton>
                    </BreadcrumbItem>
                </Breadcrumb>

                // Name, health, actions for the viewer's role, and the caption. The dialogs
                // live here with the row they act on.
                <Transition fallback=move || view! { <Title3>{id()}</Title3> }>
                    {move || match sub_res.get() {
                        Some(Ok(Some(row))) => {
                            let sub = row.sub.clone();
                            let has_access = row.has_access();
                            let control = sub.supports_control;
                            let paused = sub.health == SubscriptionHealth::Paused;
                            let can_run = row.can_run() && control;
                            let can_edit = row.can_edit() && control;
                            let name = sub.subscription_name.clone();
                            view! {
                                <Flex vertical=true gap=FlexGap::Small>
                                    <Flex gap=FlexGap::Medium wrap=FlexWrap::Wrap>
                                        <Flex gap=FlexGap::Small>
                                            <Title3>{name}</Title3>
                                            {has_access.then(|| view! { <SubscriptionHealthBadge health=sub.health /> })}
                                        </Flex>
                                        <Flex gap=FlexGap::Small wrap=FlexWrap::Wrap>
                                            {can_run.then(|| view! {
                                                <Button appearance=ButtonAppearance::Secondary attr:data-testid="photon-sub-action-replay" on_click=Callback::new(move |_| replay_open.set(true))>
                                                    "Replay from…"
                                                </Button>
                                            })}
                                            {(can_edit && paused).then(|| view! {
                                                <Button appearance=ButtonAppearance::Primary attr:data-testid="photon-sub-action-resume" on_click=Callback::new(move |_| pause_open.set(true))>
                                                    "Resume"
                                                </Button>
                                            })}
                                            {(can_edit && !paused).then(|| view! {
                                                <Button appearance=ButtonAppearance::Secondary attr:data-testid="photon-sub-action-pause" on_click=Callback::new(move |_| pause_open.set(true))>
                                                    "Pause"
                                                </Button>
                                            })}
                                            {can_edit.then(|| view! {
                                                <Menu on_select=move |value: String| if value == "skip" { skip_open.set(true) }>
                                                    <MenuTrigger slot>
                                                        <Button appearance=ButtonAppearance::Subtle attr:aria-label="More actions" attr:data-testid="photon-sub-action-more">
                                                            "⋯"
                                                        </Button>
                                                    </MenuTrigger>
                                                    <MenuItem value="skip".to_string() attr:data-testid="photon-sub-action-skip">
                                                        "Skip to latest"
                                                    </MenuItem>
                                                </Menu>
                                            })}
                                        </Flex>
                                    </Flex>
                                    {caption_line(&row)}
                                    {can_run.then(|| view! { <ReplayDialog open=replay_open sub=sub.clone() on_done=on_done /> })}
                                    {can_edit.then(|| view! {
                                        <PauseDialog open=pause_open sub=sub.clone() on_done=on_done />
                                        <SkipDialog open=skip_open sub=sub.clone() on_done=on_done />
                                    })}
                                </Flex>
                            }
                            .into_any()
                        }
                        _ => view! { <Title3>{id()}</Title3> }.into_any(),
                    }}
                </Transition>

                {move || {
                    let resource = subscription_ref(&id());
                    view! {
                        <ResourceTabs resource=resource>
                            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                                <Transition fallback=move || view! {
                                    <SubscriptionHealthCardSkeleton />
                                    <WaitingEventsCardSkeleton />
                                    <SubscriptionMetaCardSkeleton />
                                }>
                                    {move || match sub_res.get() {
                                        Some(Ok(Some(row))) => {
                                            let sub = row.sub.clone();
                                            let now = chrono::Utc::now();
                                            let behind = matches!(sub.health, SubscriptionHealth::Stalled | SubscriptionHealth::Failing);
                                            let acts = row.can_run() || row.can_edit();
                                            let view_only = row.has_access() && !acts;
                                            let owners = row.owners.join(" or ");
                                            let notice_sub = sub.clone();
                                            view! {
                                                {move || notice.get().map(|n| action_notice_view(&n, &notice_sub))}
                                                {move || retry_error.get().map(|e| server_fn_error_bar(&e))}
                                                {paused_text(&sub, now).map(|(title, body)| view! {
                                                    <MessageBar intent=MessageBarIntent::Warning attr:data-testid="photon-subscription-paused">
                                                        <MessageBarBody>
                                                            <MessageBarTitle>{title}</MessageBarTitle>
                                                            {body}
                                                        </MessageBarBody>
                                                    </MessageBar>
                                                })}
                                                {view_only.then(|| view! {
                                                    <MessageBar intent=MessageBarIntent::Info attr:data-testid="photon-subscription-view-only">
                                                        {format!("You can view this subscription. To pause or replay it, ask an owner: {owners}.")}
                                                    </MessageBar>
                                                })}
                                                {(behind && acts).then(|| view! {
                                                    <WhatToDoCard
                                                        sub=sub.clone()
                                                        owner=row.owner.clone()
                                                        can_run=row.can_run()
                                                        can_edit=row.can_edit()
                                                        on_pause=on_pause
                                                        on_replay=on_replay
                                                        on_retry=on_retry
                                                    />
                                                })}
                                                <SubscriptionHealthCard sub=sub.clone() />
                                                <Transition fallback=move || view! { <WaitingEventsCardSkeleton /> }>
                                                    {move || match waiting_res.get() {
                                                        Some(Ok(waiting)) => view! { <WaitingEventsCard waiting=waiting /> }.into_any(),
                                                        Some(Err(e)) => waiting_events_error(server_fn_error_or_denied(&e, "see the events waiting for this subscription")).into_any(),
                                                        None => view! { <WaitingEventsCardSkeleton /> }.into_any(),
                                                    }}
                                                </Transition>
                                                <SubscriptionMetaCard sub=sub />
                                            }
                                            .into_any()
                                        }
                                        Some(Ok(None)) => view! { <MessageBar intent=MessageBarIntent::Warning>"Subscription not found"</MessageBar> }.into_any(),
                                        Some(Err(e)) => server_fn_error_or_denied(&e, "see this subscription's health"),
                                        None => view! {
                                            <SubscriptionHealthCardSkeleton />
                                            <WaitingEventsCardSkeleton />
                                            <SubscriptionMetaCardSkeleton />
                                        }
                                        .into_any(),
                                    }}
                                </Transition>
                            </Flex>
                        </ResourceTabs>
                    }
                }}
            </Flex>
        </ContentContainer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_backend::PausedBy;

    #[test]
    fn paused_banner_says_who_and_what_waits() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-28T09:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let sub = SubscriptionSummary {
            paused: Some(PausedBy {
                by: "kenji".into(),
                at: "2026-11-28T08:20:00Z".into(),
            }),
            waiting: 41,
            retention_secs: Some(7 * 86_400),
            ..SubscriptionSummary::default()
        };
        assert_eq!(
            paused_text(&sub, now),
            Some((
                "Paused by kenji at 08:20".to_string(),
                "41 events are waiting and new ones join them. Nothing is lost while they stay within the 7 days retention.".to_string()
            ))
        );
        assert_eq!(paused_text(&SubscriptionSummary::default(), now), None);
    }
}
