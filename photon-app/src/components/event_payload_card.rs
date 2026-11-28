//! Payload card for the event page: a field table with personal values masked, and the raw
//! JSON masked the same way.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use std::collections::HashSet;

use leptos::prelude::*;
use orbital::components::{
    Body1, Card, CardContent, CardHeader, CardHeaderAction, Subtitle2, Text, TextFont, TextSize,
    TextTag,
};
use orbital::primitives::{
    Accordion, AccordionHeader, AccordionItem, Button, ButtonAppearance, ButtonSize, Flex, FlexGap,
    Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow,
};

use crate::components::event_meta_card::MaskedValue;
use crate::server::{PayloadField, RevealedFields};

/// The payload as Field/Value rows, personal values masked until `revealed` has them, with a
/// "Reveal personal fields" button and the raw JSON in an accordion.
#[allow(clippy::needless_pass_by_value)] // Leptos props are owned
#[component]
pub fn EventPayloadCard(
    /// Top-level fields from the masked event.
    fields: Vec<PayloadField>,
    /// Masked payload JSON.
    payload_json: serde_json::Value,
    /// Values from a reveal on this page view.
    #[prop(into)]
    revealed: Signal<Option<RevealedFields>>,
    /// Whether the event has anything to reveal.
    can_reveal: bool,
    /// Open the reveal dialog.
    on_reveal: Callback<()>,
) -> impl IntoView {
    let fields = StoredValue::new(fields);
    let masked_json = serde_json::to_string_pretty(&payload_json).unwrap_or_default();
    let open_items = RwSignal::new(HashSet::new());

    let rows = move || {
        let revealed = revealed.get();
        fields
            .get_value()
            .into_iter()
            .map(|field| {
                let real = revealed.as_ref().and_then(|r| {
                    r.fields
                        .iter()
                        .find(|f| f.name == field.name)
                        .and_then(|f| f.value.clone())
                });
                let value = match (field.value.clone(), real) {
                    (_, Some(real)) => view! { <Text attr:data-testid="photon-revealed-value">{real}</Text> }.into_any(),
                    (Some(value), None) => view! { <Text>{value}</Text> }.into_any(),
                    (None, None) => view! { <MaskedValue /> }.into_any(),
                };
                let testid = format!("photon-payload-field-{}", field.name);
                view! {
                    <TableRow attr:data-testid=testid>
                        <TableCell><Text font=TextFont::Monospace>{field.name.clone()}</Text></TableCell>
                        <TableCell>{value}</TableCell>
                    </TableRow>
                }
            })
            .collect_view()
    };
    let raw = move || {
        revealed.get().map_or_else(
            || masked_json.clone(),
            |r| serde_json::to_string_pretty(&r.payload_json).unwrap_or_default(),
        )
    };
    let show_reveal = move || can_reveal && revealed.with(Option::is_none);

    view! {
        <div data-testid="photon-event-payload">
            <Card>
                <CardHeader>
                    <Subtitle2>"Payload"</Subtitle2>
                    <CardHeaderAction slot>
                        <Show when=show_reveal>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                size=ButtonSize::Small
                                on_click=Callback::new(move |_| on_reveal.run(()))
                                attr:data-testid="photon-reveal-open"
                            >
                                "Reveal personal fields"
                            </Button>
                        </Show>
                    </CardHeaderAction>
                </CardHeader>
                <CardContent>
                    <Flex vertical=true gap=FlexGap::Medium>
                        {if fields.with_value(Vec::is_empty) {
                            view! { <Body1 block=true>"This event has no payload fields."</Body1> }.into_any()
                        } else {
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHeaderCell>"Field"</TableHeaderCell>
                                            <TableHeaderCell>"Value"</TableHeaderCell>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>{rows}</TableBody>
                                </Table>
                            }.into_any()
                        }}
                        <Accordion open_items=open_items collapsible=true>
                            <AccordionItem value="raw-json">
                                <AccordionHeader slot>"Raw JSON"</AccordionHeader>
                                <Text tag=TextTag::Pre font=TextFont::Monospace size=TextSize::S200 block=true attr:data-testid="photon-raw-json">
                                    {raw}
                                </Text>
                            </AccordionItem>
                        </Accordion>
                    </Flex>
                </CardContent>
            </Card>
        </div>
    }
}
