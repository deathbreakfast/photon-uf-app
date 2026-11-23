//! Subscription metadata card for the subscription detail page.

use leptos::prelude::*;
use orbital::components::{
    Body1, Caption1Strong, Card, CardContent, CardHeader, CardHeaderAction, Grid, GridConfig,
    GridItem, Link, Skeleton, SkeletonItem, Subtitle2, Text, TextFont,
};
use orbital::primitives::{Flex, FlexGap};

use crate::components::SubscriptionStatusBadge;
use crate::server::SubscriptionSummary;

/// Field labels in grid order; shared with [`SubscriptionMetaCardSkeleton`].
const LABELS: [&str; 6] = [
    "Id",
    "Topic",
    "Mode",
    "Key filter",
    "Checkpoint lag",
    "Last seq",
];

/// Readable dispatch mode: `durable` and `consumer_group` get labels, anything
/// else shows as stored.
fn mode_label(mode: &str) -> String {
    match mode {
        "durable" => "Durable".to_string(),
        "consumer_group" => "Consumer group".to_string(),
        other => other.to_string(),
    }
}

fn key_filter_label(filter: Option<&str>) -> String {
    filter.map_or_else(|| "All keys".to_string(), str::to_string)
}

fn lag_label(lag: i64, last_seq: Option<i64>) -> String {
    match (last_seq, lag) {
        (None, _) => "Not started".to_string(),
        (Some(_), 0) => "Up to date".to_string(),
        (Some(_), 1) => "1 event behind".to_string(),
        (Some(_), n) => format!("{n} events behind"),
    }
}

fn last_seq_label(last_seq: Option<i64>) -> String {
    last_seq.map_or_else(|| "No checkpoint yet".to_string(), |s| s.to_string())
}

/// One label/value cell of the details grid.
fn field(label: &'static str, value: AnyView) -> impl IntoView {
    view! {
        <GridItem>
            <Flex vertical=true gap=FlexGap::Size(2)>
                <Caption1Strong block=true>{label}</Caption1Strong>
                {value}
            </Flex>
        </GridItem>
    }
}

/// Renders the subscription's details: a "Details" header with the status
/// badge, then a two-column grid of id, topic, mode, key filter, checkpoint
/// lag, and last seq.
#[component]
pub fn SubscriptionMetaCard(
    /// Subscription data to display.
    sub: SubscriptionSummary,
) -> impl IntoView {
    let values: [AnyView; 6] = [
        view! { <Text font=TextFont::Monospace block=true>{sub.subscription_id.clone()}</Text> }
            .into_any(),
        view! {
            <Body1 block=true>
                <Link href=photon_backend::photon_topic_path(&sub.topic_name)>{sub.topic_name.clone()}</Link>
            </Body1>
        }
        .into_any(),
        view! { <Body1 block=true>{mode_label(&sub.mode)}</Body1> }.into_any(),
        view! { <Body1 block=true>{key_filter_label(sub.topic_key_filter.as_deref())}</Body1> }
            .into_any(),
        view! { <Body1 block=true>{lag_label(sub.checkpoint_lag, sub.last_seq)}</Body1> }.into_any(),
        view! { <Body1 block=true>{last_seq_label(sub.last_seq)}</Body1> }.into_any(),
    ];
    let cells = LABELS
        .into_iter()
        .zip(values)
        .map(|(label, value)| field(label, value))
        .collect_view();

    view! {
        <Card attr:data-testid="photon-subscription-meta">
            <CardHeader>
                <Subtitle2>"Details"</Subtitle2>
                <CardHeaderAction slot>
                    <SubscriptionStatusBadge enabled=sub.enabled />
                </CardHeaderAction>
            </CardHeader>
            <CardContent>
                <Grid config=GridConfig::with_gaps(2, 16, 16)>{cells}</Grid>
            </CardContent>
        </Card>
    }
}

/// [`SubscriptionMetaCard`] while the subscription loads: the header and all
/// six labels render at once, and only the values are placeholders.
#[component]
pub fn SubscriptionMetaCardSkeleton() -> impl IntoView {
    let widths = ["220px", "120px", "90px", "60px", "80px", "40px"];
    let cells = LABELS
        .into_iter()
        .zip(widths)
        .map(|(label, width)| field(label, view! { <SkeletonItem width=width /> }.into_any()))
        .collect_view();

    view! {
        <Card attr:data-testid="photon-subscription-meta-skeleton">
            <CardHeader>
                <Subtitle2>"Details"</Subtitle2>
            </CardHeader>
            <CardContent>
                <Skeleton>
                    <Grid config=GridConfig::with_gaps(2, 16, 16)>{cells}</Grid>
                </Skeleton>
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_label_names_known_modes() {
        assert_eq!(mode_label("durable"), "Durable");
        assert_eq!(mode_label("consumer_group"), "Consumer group");
        assert_eq!(mode_label("at_least_once"), "at_least_once");
    }

    #[test]
    fn key_filter_label_defaults_to_all_keys() {
        assert_eq!(key_filter_label(None), "All keys");
        assert_eq!(key_filter_label(Some("tenant-1")), "tenant-1");
    }

    #[test]
    fn lag_label_covers_no_checkpoint_zero_and_behind() {
        assert_eq!(lag_label(0, None), "Not started");
        assert_eq!(lag_label(12, None), "Not started");
        assert_eq!(lag_label(0, Some(7)), "Up to date");
        assert_eq!(lag_label(1, Some(7)), "1 event behind");
        assert_eq!(lag_label(41, Some(7)), "41 events behind");
    }

    #[test]
    fn last_seq_label_says_when_there_is_no_checkpoint() {
        assert_eq!(last_seq_label(None), "No checkpoint yet");
        assert_eq!(last_seq_label(Some(42)), "42");
    }
}
