//! One page of events as a DataTable, and what the Events list says when nothing matches.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use std::collections::HashMap;
use std::sync::Arc;

use leptos::prelude::*;
use orbital::components::{Body1, EmptyState, Link};
use orbital::primitives::{
    Button, ButtonAppearance, DataTable, DataTableColumnDef, DataTableHeaderChromeConfig,
    DataTableRowModel, DataTableSource, DataTableToolbarConfig, Flex, FlexAlign, FlexGap,
    ListViewConfig, PagingMode,
};
use orbital_data::{DataRecord, DataValue};

use crate::server::{EventSummary, EventsQuery, EventsRange};

const fn no_events_message(range: EventsRange) -> &'static str {
    match range {
        EventsRange::LastHour => "No events in the last hour",
        EventsRange::Last24h => "No events in the last 24 hours",
        EventsRange::Last7d => "No events in the last 7 days",
        EventsRange::Custom => "No events in this range",
    }
}

/// What the list says when nothing matches: names the search and the range, and offers the
/// last 7 days when a search found nothing in a shorter preset.
pub fn events_empty_state(query: &EventsQuery, widen: Callback<()>) -> AnyView {
    let Some(q) = query.q.clone() else {
        let message = no_events_message(query.range);
        return view! {
            <div data-testid="photon-events-empty">
                <EmptyState message=message />
            </div>
        }
        .into_any();
    };
    let sentence = format!("No event or key matches {q} {}", query.range.phrase());
    let offer_week = matches!(query.range, EventsRange::LastHour | EventsRange::Last24h);
    view! {
        // EmptyState's call-to-action slot crashes rustc here (ICE), and its message
        // must be static, so the sentence and the button sit below it.
        <Flex vertical=true gap=FlexGap::Medium align=FlexAlign::Center attr:data-testid="photon-events-empty">
            <EmptyState message="Nothing found" />
            <Body1 block=true>{sentence}</Body1>
            {offer_week.then(|| view! {
                <Button
                    appearance=ButtonAppearance::Secondary
                    on_click=Callback::new(move |_| widen.run(()))
                >
                    "Search the last 7 days"
                </Button>
            })}
        </Flex>
    }
    .into_any()
}

fn event_record(event: EventSummary) -> DataTableRowModel {
    DataTableRowModel::new(DataRecord::new(
        event.event_id.clone(),
        HashMap::from([
            ("event_id".into(), DataValue::Text(event.event_id)),
            ("topic".into(), DataValue::Text(event.topic_name)),
            (
                "key".into(),
                DataValue::Text(event.topic_key.unwrap_or_default()),
            ),
            ("seq".into(), DataValue::Text(event.seq.to_string())),
            ("published".into(), DataValue::Text(event.created_at)),
        ]),
    ))
}

/// One page of events as a DataTable: the id links to the event page. Below 600px it shows
/// as a list.
#[component]
pub fn EventsDataTable(
    /// Rows on this page.
    rows: Vec<EventSummary>,
    /// Show the Topic column.
    show_topic: bool,
) -> impl IntoView {
    let items = RwSignal::new(rows.into_iter().map(event_record).collect::<Vec<_>>());
    let link_view = Arc::new(|record: DataRecord| {
        let id = record
            .get("event_id")
            .map_or_else(String::new, DataValue::display_string);
        let href = photon_backend::photon_event_path(&id);
        view! { <Link href=href>{id}</Link> }.into_any()
    });
    let mut columns = vec![DataTableColumnDef::new("event_id", "Event")
        .with_sortable(false)
        .with_cell_view(link_view)];
    if show_topic {
        columns.push(DataTableColumnDef::new("topic", "Topic").with_sortable(false));
    }
    columns.extend([
        DataTableColumnDef::new("key", "Key").with_sortable(false),
        DataTableColumnDef::new("seq", "Seq").with_sortable(false),
        DataTableColumnDef::new("published", "Published").with_sortable(false),
    ]);
    let mut list = ListViewConfig::new("event_id");
    list.secondary_fields = vec!["topic".into(), "key".into(), "published".into()];
    list.breakpoint_px = Some(600);

    view! {
        <div data-testid="photon-events-table">
            <DataTable
                data_source=DataTableSource::Client(items)
                paging=PagingMode::None
                sortable=false
                columns=columns
                toolbar_config=DataTableToolbarConfig {
                    quick_search: false,
                    filter_panel: false,
                    column_picker: false,
                    pivot: false,
                    export_menu: false,
                }
                header_chrome=DataTableHeaderChromeConfig {
                    column_menu: false,
                    column_filter_button: false,
                    column_hide: false,
                    show_table_grid: true,
                }
                list_view=list
            />
        </div>
    }
}
