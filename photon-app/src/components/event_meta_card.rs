//! Facts grid for the event page: topic, key, seq, when and who published it.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::{
    Body1, Caption1Strong, Card, CardContent, Grid, GridConfig, GridItem, Link, Text, TextFont,
};
use orbital::primitives::{Badge, BadgeAppearance, BadgeColor, Flex, FlexAlign, FlexGap};

use crate::server::{EventDetail, EventsQuery, EventsRange, PublishedBy, MASKED_VALUE};

/// One label/value cell of the facts grid.
fn fact(label: &'static str, value: AnyView) -> impl IntoView {
    view! {
        <GridItem>
            <Flex vertical=true gap=FlexGap::Size(2)>
                <Caption1Strong block=true>{label}</Caption1Strong>
                {value}
            </Flex>
        </GridItem>
    }
}

/// Dots and a warning-tint "Personal" badge where a personal value would be.
#[component]
pub fn MaskedValue() -> impl IntoView {
    view! {
        <Flex gap=FlexGap::Small align=FlexAlign::Center attr:data-testid="photon-masked-value">
            <Text font=TextFont::Monospace>{MASKED_VALUE}</Text>
            <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Warning>"Personal"</Badge>
        </Flex>
    }
}

/// Events search for one key on one topic, over the last 7 days.
fn key_search_href(topic: &str, key: &str) -> String {
    let query = EventsQuery {
        topic: Some(topic.to_string()),
        q: Some(key.to_string()),
        range: EventsRange::Last7d,
        ..EventsQuery::default()
    };
    format!("/photon/events?{}", query.to_query_string())
}

/// The event's facts: Topic (link), Key (searches the key), Seq, Published, Published by.
///
/// A person's id stays masked until `published_by_id` carries it from a reveal.
#[component]
pub fn EventMetaCard(
    /// Event to describe.
    event: EventDetail,
    /// The publishing person's id once revealed.
    #[prop(into)]
    published_by_id: Signal<Option<String>>,
) -> impl IntoView {
    let topic_href = photon_backend::photon_topic_path(&event.topic_name);
    let key = event.topic_key.clone().map_or_else(
        || view! { <Body1 block=true>"No key"</Body1> }.into_any(),
        |key| {
            let href = key_search_href(&event.topic_name, &key);
            view! { <Body1 block=true><Link href=href>{key}</Link></Body1> }.into_any()
        },
    );
    let by = event.published_by;
    let published_by = view! {
        <Flex vertical=true gap=FlexGap::Size(2)>
            <Body1 block=true>{by.label()}</Body1>
            {(by == PublishedBy::Person).then(|| view! {
                {move || published_by_id.get().map_or_else(
                    || view! { <MaskedValue /> }.into_any(),
                    |id| view! {
                        <Text font=TextFont::Monospace attr:data-testid="photon-revealed-publisher">{id}</Text>
                    }.into_any(),
                )}
            })}
        </Flex>
    }
    .into_any();
    let facts: [(&'static str, AnyView); 5] = [
        (
            "Topic",
            view! { <Body1 block=true><Link href=topic_href>{event.topic_name.clone()}</Link></Body1> }
                .into_any(),
        ),
        ("Key", key),
        ("Seq", view! { <Body1 block=true>{event.seq}</Body1> }.into_any()),
        (
            "Published",
            view! { <Body1 block=true>{event.created_at.clone()}</Body1> }.into_any(),
        ),
        ("Published by", published_by),
    ];

    view! {
        <div data-testid="photon-event-meta">
            <Card>
                <CardContent>
                    // Cells are built inside Grid: GridItem reads Grid's context.
                    <Grid config=GridConfig::with_gaps(3, 16, 16)>
                        {facts.into_iter().map(|(label, value)| fact(label, value)).collect_view()}
                    </Grid>
                </CardContent>
            </Card>
        </div>
    }
}
