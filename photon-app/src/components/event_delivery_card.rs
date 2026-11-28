//! Subscriptions card for the event page: which handlers handled the event, are waiting on
//! it, or failed.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::{Body1, Card, CardContent, CardHeader, Link, Subtitle2};
use orbital::primitives::{
    Badge, BadgeAppearance, BadgeColor, Button, ButtonAppearance, ButtonSize, Table, TableBody,
    TableCell, TableHeader, TableHeaderCell, TableRow,
};

use crate::server::{DeliveryRow, DeliveryState};

/// "1st", "2nd", "3rd", "11th", "22nd".
fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Status badge text, color and the detail line for one row.
fn describe(state: &DeliveryState) -> (&'static str, BadgeColor, String) {
    match state {
        DeliveryState::Handled => ("Handled", BadgeColor::Success, String::new()),
        DeliveryState::Waiting { position } => (
            "Waiting",
            BadgeColor::Informative,
            format!("{} in line", ordinal(*position)),
        ),
        DeliveryState::Failed {
            attempts,
            error,
            next_retry_at,
        } => {
            let tries = if *attempts == 1 {
                "1 attempt".to_string()
            } else {
                format!("{attempts} attempts")
            };
            let detail = next_retry_at.as_ref().map_or_else(
                || format!("{tries}. {error}"),
                |at| format!("{tries}, next retry {at}. {error}"),
            );
            ("Failed", BadgeColor::Danger, detail)
        }
    }
}

/// One row per subscription on the topic: Handled, Waiting (and its place in line) or
/// Failed (attempts, next retry, error), when it happened where known, and "Redeliver…"
/// where the viewer holds Run.
#[component]
pub fn EventDeliveryCard(
    /// Delivery rows from the event.
    deliveries: Vec<DeliveryRow>,
    /// Open the Redeliver dialog for a subscription id.
    on_redeliver: Callback<String>,
) -> impl IntoView {
    let body = if deliveries.is_empty() {
        view! { <Body1 block=true>"No subscriptions listen to this topic."</Body1> }.into_any()
    } else {
        view! {
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHeaderCell>"Subscription"</TableHeaderCell>
                        <TableHeaderCell>"Status"</TableHeaderCell>
                        <TableHeaderCell>"Details"</TableHeaderCell>
                        <TableHeaderCell>"At"</TableHeaderCell>
                        <TableHeaderCell>""</TableHeaderCell>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {deliveries.into_iter().map(|row| {
                        let (label, color, detail) = describe(&row.state);
                        let href = photon_backend::photon_subscription_path(&row.subscription_id);
                        let testid = format!("photon-delivery-{}", row.subscription_name);
                        let sub_id = row.subscription_id.clone();
                        view! {
                            <TableRow attr:data-testid=testid attr:data-state=label>
                                <TableCell><Link href=href>{row.subscription_name.clone()}</Link></TableCell>
                                <TableCell>
                                    <Badge appearance=BadgeAppearance::Tint color=color>{label}</Badge>
                                </TableCell>
                                <TableCell>{detail}</TableCell>
                                <TableCell>{row.at.clone().unwrap_or_default()}</TableCell>
                                <TableCell>
                                    {row.can_run.then(|| view! {
                                        <Button
                                            appearance=ButtonAppearance::Subtle
                                            size=ButtonSize::Small
                                            on_click=Callback::new(move |_| on_redeliver.run(sub_id.clone()))
                                        >
                                            "Redeliver\u{2026}"
                                        </Button>
                                    })}
                                </TableCell>
                            </TableRow>
                        }
                    }).collect_view()}
                </TableBody>
            </Table>
        }
        .into_any()
    };
    view! {
        <div data-testid="photon-event-deliveries">
            <Card>
                <CardHeader>
                    <Subtitle2>"Subscriptions"</Subtitle2>
                </CardHeader>
                <CardContent>{body}</CardContent>
            </Card>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_read_naturally() {
        let got: Vec<_> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101].map(ordinal).into();
        assert_eq!(
            got,
            ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st"]
        );
    }

    #[test]
    fn failed_rows_say_attempts_and_retry() {
        let (label, _, detail) = describe(&DeliveryState::Failed {
            attempts: 3,
            error: "boom".into(),
            next_retry_at: Some("08:14".into()),
        });
        assert_eq!(label, "Failed");
        assert_eq!(detail, "3 attempts, next retry 08:14. boom");
        let (_, _, waiting) = describe(&DeliveryState::Waiting { position: 2 });
        assert_eq!(waiting, "2nd in line");
    }
}
