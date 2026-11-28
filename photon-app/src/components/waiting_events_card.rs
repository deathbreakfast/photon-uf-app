//! Waiting events card: the events a subscription still owes, oldest first.

use leptos::prelude::*;
use orbital::components::{
    Caption1, Card, CardContent, CardHeader, CardHeaderDescription, EmptyState, Subtitle2,
};
use orbital::primitives::{Flex, FlexGap};
use photon_backend::WaitingEvents;

use crate::components::{EventsTable, EventsTableColumns, EventsTableSkeleton};

/// Columns for waiting events: every row is on the subscription's topic.
const WAITING_COLUMNS: EventsTableColumns = EventsTableColumns {
    show_event_id: true,
    show_topic: false,
    show_key: true,
    show_seq: true,
    show_created: true,
};

/// "Showing the oldest 4 of 41." when the list is cut short.
#[must_use]
pub fn showing_caption(shown: usize, total: i64) -> Option<String> {
    let shown_i = i64::try_from(shown).unwrap_or(i64::MAX);
    (shown > 0 && total > shown_i).then(|| format!("Showing the oldest {shown} of {total}."))
}

fn card(body: AnyView) -> impl IntoView {
    view! {
        <div data-testid="photon-waiting-events">
            <Card>
                <CardHeader>
                    <Subtitle2>"Waiting events"</Subtitle2>
                    <CardHeaderDescription slot>
                        <Caption1>"Events this subscription hasn't handled yet, oldest first."</Caption1>
                    </CardHeaderDescription>
                </CardHeader>
                <CardContent>{body}</CardContent>
            </Card>
        </div>
    }
}

/// The waiting events table with "Showing the oldest N of M.", or "Nothing waiting".
#[component]
pub fn WaitingEventsCard(
    /// The oldest waiting events and the total.
    waiting: WaitingEvents,
) -> impl IntoView {
    let body = if waiting.events.is_empty() {
        view! {
            <div data-testid="photon-waiting-events-empty">
                <EmptyState message="Nothing waiting" description="It has handled every event on its topic." />
            </div>
        }
        .into_any()
    } else {
        let caption = showing_caption(waiting.events.len(), waiting.total);
        view! {
            <Flex vertical=true gap=FlexGap::Small>
                <EventsTable events=waiting.events columns=WAITING_COLUMNS />
                {caption.map(|c| view! { <Caption1 attr:data-testid="photon-waiting-events-caption">{c}</Caption1> })}
            </Flex>
        }
        .into_any()
    };
    card(body)
}

/// [`WaitingEventsCard`] while loading: heading and real column headers over placeholder rows.
#[component]
pub fn WaitingEventsCardSkeleton() -> impl IntoView {
    card(view! { <EventsTableSkeleton columns=WAITING_COLUMNS /> }.into_any())
}

/// The card's body for a failed load, under the same heading.
pub fn waiting_events_error(body: AnyView) -> impl IntoView {
    card(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_only_when_cut_short() {
        assert_eq!(
            showing_caption(4, 41).as_deref(),
            Some("Showing the oldest 4 of 41.")
        );
        assert_eq!(showing_caption(4, 4), None);
        assert_eq!(showing_caption(0, 0), None);
    }
}
