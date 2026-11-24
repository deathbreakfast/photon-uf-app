//! Reusable events table with configurable columns.
//!
//! Used across the events index, topic detail, subscription detail, and
//! dashboard pages. Column visibility is controlled via [`EventsTableColumns`].

use leptos::prelude::*;

use crate::components::skeletons::TableSkeleton;
use crate::server::EventSummary;
use orbital::primitives::{Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow};

/// Which columns [`EventsTable`] renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // column visibility flags; clearer than bitflags for Leptos props
pub struct EventsTableColumns {
    /// Show the Event ID column (linked to detail page).
    pub show_event_id: bool,
    /// Show the Topic column.
    pub show_topic: bool,
    /// Show the Key column.
    pub show_key: bool,
    /// Show the Seq column.
    pub show_seq: bool,
    /// Show the Created column.
    pub show_created: bool,
}

impl EventsTableColumns {
    /// Header label and skeleton bar width for each visible column, in table order.
    fn visible(self) -> Vec<(&'static str, &'static str)> {
        [
            (self.show_event_id, "Event ID", "96px"),
            (self.show_topic, "Topic", "120px"),
            (self.show_key, "Key", "96px"),
            (self.show_seq, "Seq", "32px"),
            (self.show_created, "Created", "160px"),
        ]
        .into_iter()
        .filter_map(|(on, label, width)| on.then_some((label, width)))
        .collect()
    }
}

impl Default for EventsTableColumns {
    fn default() -> Self {
        Self {
            show_event_id: true,
            show_topic: true,
            show_key: true,
            show_seq: true,
            show_created: true,
        }
    }
}

/// Reusable events table.
///
/// Renders a `Table` of events with an optional Event ID link column.
#[component]
pub fn EventsTable(
    /// Event rows to display.
    events: Vec<EventSummary>,
    /// Column visibility. Defaults to all columns shown.
    #[prop(optional)]
    columns: Option<EventsTableColumns>,
) -> impl IntoView {
    let columns = columns.unwrap_or_default();
    let events = StoredValue::new(events);
    let show_event_id = columns.show_event_id;
    let show_topic = columns.show_topic;
    let show_key = columns.show_key;
    let show_seq = columns.show_seq;
    let show_created = columns.show_created;

    // Row hover + brand link color; Orbital Table has no row-hover / cell-link props yet.
    let (style_sheet, class_names) = turf::inline_style_sheet_values! {
        .Table { width: 100%; }
        .Row { cursor: pointer; }
        .Row:hover { background: var(--orb-color-surface-canvas-hover); }
        .Link {
            color: var(--colorBrandForeground1);
            text-decoration: none;
        }
        .Link:hover { text-decoration: underline; }
    };

    view! {
        <style>{style_sheet}</style>
        <Table class=class_names.table>
            <EventsTableHeader columns=columns />
            <TableBody>
                <For
                    each=move || events.get_value()
                    key=|e| e.event_id.clone()
                    let:e
                >
                    {
                        let id = e.event_id.clone();
                        let href = photon_backend::photon_event_path(&id);
                        let href2 = href.clone();
                        let topic = e.topic_name.clone();
                        let key = e.topic_key.clone().unwrap_or_default();
                        let created = e.created_at.clone();
                        view! {
                            <TableRow class=class_names.row>
                                {show_event_id.then(|| view! { <TableCell><a href=href class=class_names.link>{id}</a></TableCell> })}
                                {show_topic.then(|| {
                                    if show_event_id {
                                        view! { <TableCell>{topic}</TableCell> }
                                    } else {
                                        view! { <TableCell><a href=href2.clone() class=class_names.link>{topic}</a></TableCell> }
                                    }
                                })}
                                {show_key.then(|| view! { <TableCell>{key}</TableCell> })}
                                {show_seq.then(|| view! { <TableCell>{e.seq}</TableCell> })}
                                {show_created.then(|| view! { <TableCell>{created}</TableCell> })}
                            </TableRow>
                        }
                    }
                </For>
            </TableBody>
        </Table>
    }
}

/// Header row shared by [`EventsTable`] and [`EventsTableSkeleton`].
#[component]
fn EventsTableHeader(columns: EventsTableColumns) -> impl IntoView {
    view! {
        <TableHeader>
            <TableRow>
                {columns
                    .visible()
                    .into_iter()
                    .map(|(label, _)| view! { <TableHeaderCell>{label}</TableHeaderCell> })
                    .collect_view()}
            </TableRow>
        </TableHeader>
    }
}

/// Loading state for [`EventsTable`]: the real header row over rows of
/// `SkeletonItem` cells, so the columns are visible before the events arrive.
#[component]
pub fn EventsTableSkeleton(
    /// Column visibility; pass the same flags as the [`EventsTable`] it stands in for.
    #[prop(optional)]
    columns: Option<EventsTableColumns>,
) -> impl IntoView {
    view! { <TableSkeleton columns=columns.unwrap_or_default().visible() /> }
}
