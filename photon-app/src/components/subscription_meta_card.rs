//! Subscription metadata card for the subscription detail page.

use leptos::prelude::*;
use orbital::components::{
    Body1, Caption1Strong, Card, CardContent, CardHeader, Grid, GridConfig, GridItem, Skeleton,
    SkeletonItem, Subtitle2, Text, TextFont,
};
use orbital::primitives::{Flex, FlexGap};

use crate::server::SubscriptionSummary;

/// Field labels in grid order; shared with [`SubscriptionMetaCardSkeleton`].
const LABELS: [&str; 5] = ["Mode", "Key filter", "Checkpoint", "Lag", "Id"];

/// Readable dispatch mode: `durable` and `consumer_group` get labels, anything
/// else shows as stored.
#[must_use]
pub fn mode_label(mode: &str) -> String {
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
    match last_seq {
        None if lag == 0 => "Not started".to_string(),
        _ => photon_backend::lag_words(lag),
    }
}

fn checkpoint_label(last_seq: Option<i64>) -> String {
    last_seq.map_or_else(|| "No checkpoint yet".to_string(), |s| format!("Seq {s}"))
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

/// Renders the subscription's details: a "Details" header, then a two-column grid of mode,
/// key filter, checkpoint, lag in words, and id.
#[component]
pub fn SubscriptionMetaCard(
    /// Subscription data to display.
    sub: SubscriptionSummary,
) -> impl IntoView {
    let values: [AnyView; 5] = [
        view! { <Body1 block=true>{mode_label(&sub.mode)}</Body1> }.into_any(),
        view! { <Body1 block=true>{key_filter_label(sub.topic_key_filter.as_deref())}</Body1> }
            .into_any(),
        view! { <Body1 block=true>{checkpoint_label(sub.last_seq)}</Body1> }.into_any(),
        view! { <Body1 block=true>{lag_label(sub.checkpoint_lag, sub.last_seq)}</Body1> }
            .into_any(),
        view! { <Text font=TextFont::Monospace block=true>{sub.subscription_id.clone()}</Text> }
            .into_any(),
    ];
    view! {
        <div data-testid="photon-subscription-meta">
            <Card>
                <CardHeader>
                    <Subtitle2>"Details"</Subtitle2>
                </CardHeader>
                <CardContent>
                    // Cells are built inside Grid: GridItem reads Grid's context.
                    <Grid config=GridConfig::with_gaps(2, 16, 16)>
                        {values
                            .into_iter()
                            .zip(LABELS)
                            .map(|(value, label)| field(label, value))
                            .collect_view()}
                    </Grid>
                </CardContent>
            </Card>
        </div>
    }
}

/// [`SubscriptionMetaCard`] while the subscription loads: the header and all
/// five labels render at once, and only the values are placeholders.
#[component]
pub fn SubscriptionMetaCardSkeleton() -> impl IntoView {
    let widths = ["90px", "60px", "60px", "100px", "220px"];

    view! {
        <Skeleton attr:data-testid="photon-subscription-meta-skeleton">
            <Card>
                <CardHeader>
                    <Subtitle2>"Details"</Subtitle2>
                </CardHeader>
                <CardContent>
                    <Grid config=GridConfig::with_gaps(2, 16, 16)>
                        {LABELS
                            .into_iter()
                            .zip(widths)
                            .map(|(label, width)| field(label, view! { <SkeletonItem width=width /> }.into_any()))
                            .collect_view()}
                    </Grid>
                </CardContent>
            </Card>
        </Skeleton>
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
        assert_eq!(lag_label(12, None), "12 events behind");
        assert_eq!(lag_label(0, Some(7)), "Up to date");
        assert_eq!(lag_label(1, Some(7)), "1 event behind");
        assert_eq!(lag_label(41, Some(7)), "41 events behind");
    }

    #[test]
    fn checkpoint_label_says_when_there_is_no_checkpoint() {
        assert_eq!(checkpoint_label(None), "No checkpoint yet");
        assert_eq!(checkpoint_label(Some(55)), "Seq 55");
    }
}
