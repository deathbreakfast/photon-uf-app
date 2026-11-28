//! Events tables: the searchable [`EventsBrowser`] behind the Events page, and the small
//! [`EventsTable`] the dashboard, topic and subscription pages show.
//!
//! [`EventsTable`] column visibility is controlled via [`EventsTableColumns`].

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use std::collections::HashMap;
use std::sync::Arc;

use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate, use_query_map};
use leptos_router::NavigateOptions;
use orbital::components::{Body1, Caption1, EmptyState, Link, Pagination, PaginationConfig};
use orbital::primitives::{
    Button, ButtonAppearance, DataTable, DataTableColumnDef, DataTableHeaderChromeConfig,
    DataTableRowModel, DataTableSource, DataTableToolbarConfig, DatetimeTimezone, Flex, FlexAlign,
    FlexGap, FlexJustify, FlexWrap, ListViewConfig, PagingMode,
};
use orbital_base_components::OrbitalDateTime;
use orbital_data::{DataRecord, DataValue};

use crate::components::event_filter_toolbar::{EventFilterToolbar, EventFilters};
use crate::components::server_fn_error_or_denied;
use crate::components::skeletons::{SkeletonColumn, TableSkeleton};
use crate::server::{
    get_events_page, get_topics, EventSummary, EventsPage, EventsQuery, EventsRange, TopicRow,
    EVENTS_PAGE_SIZE,
};
use orbital::primitives::{Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow};

/// [`EventsBrowser`] columns with skeleton bar widths.
const BROWSER_COLUMNS: [SkeletonColumn; 5] = [
    ("Event", "96px"),
    ("Topic", "120px"),
    ("Key", "96px"),
    ("Seq", "32px"),
    ("Published", "160px"),
];

fn day_param(day: Option<OrbitalDateTime>) -> Option<String> {
    day.and_then(|d| d.wall_date())
        .map(|d| d.format("%Y-%m-%d").to_string())
}

fn day_value(param: Option<&String>) -> Option<OrbitalDateTime> {
    let date = chrono::NaiveDate::parse_from_str(param?, "%Y-%m-%d").ok()?;
    OrbitalDateTime::from_naive_date(date, DatetimeTimezone::Utc)
}

/// "N events · newest first", or just the order when the store can't count.
fn caption(page: &EventsPage) -> String {
    match page.total {
        Some(1) => "1 event \u{b7} newest first".to_string(),
        Some(n) => format!("{n} events \u{b7} newest first"),
        None => "Newest first".to_string(),
    }
}

fn page_count(page: &EventsPage) -> usize {
    let size = u64::from(EVENTS_PAGE_SIZE);
    let by_total = page
        .total
        .map(|t| usize::try_from(t.div_ceil(size)).unwrap_or(usize::MAX));
    let by_links = page.next.unwrap_or(page.page) as usize;
    by_total.map_or(by_links, |n| {
        n.max(1).min(crate::server::MAX_EVENTS_PAGE as usize)
    })
}

const fn no_events_message(range: EventsRange) -> &'static str {
    match range {
        EventsRange::LastHour => "No events in the last hour",
        EventsRange::Last24h => "No events in the last 24 hours",
        EventsRange::Last7d => "No events in the last 7 days",
        EventsRange::Custom => "No events in this range",
    }
}

/// Events search and list: search box, Topic and range filters, the table, the total and
/// paging. Filters live in the URL query string.
///
/// Mount it with `topic` to fix the topic (the topic page's Events tab); the Topic select is
/// then hidden. The toolbar sits outside the results' Suspense, so it keeps focus and typed
/// text while a new page loads.
#[allow(clippy::too_many_lines)] // one component: URL state, toolbar wiring and the results
#[component]
pub fn EventsBrowser(
    /// Fix the list to one topic.
    #[prop(optional, into)]
    topic: Option<String>,
) -> impl IntoView {
    let query_map = use_query_map();
    let location = use_location();
    let navigate = use_navigate();
    let fixed_topic = StoredValue::new(topic);

    // The URL is the state. Resources key on this memo only.
    let query = Memo::new(move |_| {
        let map = query_map.read();
        let mut query = EventsQuery::from_params(|k| map.get(k));
        if let Some(topic) = fixed_topic.get_value() {
            query.topic = Some(topic);
        }
        query
    });

    let initial = query.get_untracked();
    let filters = EventFilters {
        search: RwSignal::new(initial.q.clone().unwrap_or_default()),
        topic: RwSignal::new(initial.topic.clone().unwrap_or_default()),
        range: RwSignal::new(initial.range.as_param().to_string()),
        from: RwSignal::new(day_value(initial.from.as_ref())),
        to: RwSignal::new(day_value(initial.to.as_ref())),
    };
    let page_sig = RwSignal::new(initial.page.max(1) as usize);

    let commit = {
        move |update: &dyn Fn(&mut EventsQuery)| {
            let current = query.get_untracked();
            let mut next = current.clone();
            update(&mut next);
            if fixed_topic.get_value().is_some() {
                next.topic = None;
            }
            let mut current_url = current;
            if fixed_topic.get_value().is_some() {
                current_url.topic = None;
            }
            let qs = next.to_query_string();
            if qs == current_url.to_query_string() {
                return;
            }
            let path = location.pathname.get_untracked();
            let href = if qs.is_empty() {
                path
            } else {
                format!("{path}?{qs}")
            };
            navigate(
                &href,
                NavigateOptions {
                    scroll: false,
                    ..NavigateOptions::default()
                },
            );
        }
    };
    let commit = StoredValue::new(commit);
    let run_commit = move |update: &dyn Fn(&mut EventsQuery)| commit.with_value(|c| c(update));

    // URL -> controls (back button, "Search the last 7 days").
    Effect::new(move |_| {
        let q = query.get();
        let set = |sig: RwSignal<String>, value: String| {
            if sig.get_untracked() != value {
                sig.set(value);
            }
        };
        set(filters.topic, q.topic.clone().unwrap_or_default());
        set(filters.range, q.range.as_param().to_string());
        if page_sig.get_untracked() != q.page.max(1) as usize {
            page_sig.set(q.page.max(1) as usize);
        }
    });

    // Controls -> URL. Each skips its first run, which only reads the initial value.
    Effect::new(move |prev: Option<()>| {
        let topic = filters.topic.get();
        if prev.is_some() {
            run_commit(&|q| {
                q.topic = (!topic.is_empty()).then(|| topic.clone());
                q.page = 1;
            });
        }
    });
    Effect::new(move |prev: Option<()>| {
        let range = EventsRange::from_param(&filters.range.get()).unwrap_or_default();
        let from = day_param(filters.from.get());
        let to = day_param(filters.to.get());
        if prev.is_none() {
            return;
        }
        // A custom range waits for both days before it searches.
        if range == EventsRange::Custom && (from.is_none() || to.is_none()) {
            return;
        }
        run_commit(&|q| {
            q.range = range;
            q.from.clone_from(&from);
            q.to.clone_from(&to);
            q.page = 1;
        });
    });
    Effect::new(move |prev: Option<()>| {
        let page = page_sig.get();
        if prev.is_some() {
            run_commit(&|q| q.page = u32::try_from(page).unwrap_or(1));
        }
    });

    let on_search = Callback::new(move |()| {
        let text = filters.search.get_untracked().trim().to_string();
        run_commit(&|q| {
            q.q = (!text.is_empty()).then(|| text.clone());
            q.page = 1;
        });
    });

    // Topic names for the select load in the browser through an action, not a resource, so
    // no Suspense (the page's included) waits on them and the toolbar never remounts. The
    // current topic is always listed.
    let load_topics = Action::new(|(): &()| async move { get_topics().await });
    Effect::new(move |_| {
        load_topics.dispatch(());
    });
    let topics = Signal::derive(move || {
        let mut names: Vec<String> = load_topics
            .value()
            .get()
            .and_then(Result::ok)
            .unwrap_or_default()
            .into_iter()
            .filter(TopicRow::has_access)
            .map(|row| row.topic.topic_name)
            .collect();
        if let Some(current) = query.with(|q| q.topic.clone()) {
            if !names.contains(&current) {
                names.insert(0, current);
            }
        }
        names
    });

    let events_res = Resource::new(
        move || query.get(),
        |q| async move { get_events_page(q).await },
    );

    let show_topic = fixed_topic.get_value().is_none();
    let skeleton = move || view! { <TableSkeleton columns=BROWSER_COLUMNS.to_vec() /> };

    view! {
        <Flex vertical=true gap=FlexGap::Large attr:data-testid="photon-events-browser">
            <EventFilterToolbar filters=filters topics=topics show_topic=show_topic on_search=on_search />
            <Suspense fallback=skeleton>
                {move || events_res.get().map(|res| match res {
                    Ok(EventsPage { exact_id: Some(id), .. }) => view! { <OpenEvent id=id /> }.into_any(),
                    Ok(page) if page.rows.is_empty() => {
                        let q = query.get_untracked();
                        events_empty_state(&q, Callback::new(move |()| run_commit(&|q| {
                            q.range = EventsRange::Last7d;
                            q.page = 1;
                        })))
                    }
                    Ok(page) => {
                        let count = page_count(&page);
                        let caption = caption(&page);
                        view! {
                            <Flex vertical=true gap=FlexGap::Medium>
                                <EventsDataTable rows=page.rows show_topic=show_topic />
                                <Flex wrap=FlexWrap::Wrap justify=FlexJustify::SpaceBetween align=FlexAlign::Center>
                                    <Caption1 attr:data-testid="photon-events-caption">{caption}</Caption1>
                                    <div data-testid="photon-events-pagination">
                                        <Pagination config=PaginationConfig::new(page_sig.into(), Signal::derive(move || count)) />
                                    </div>
                                </Flex>
                            </Flex>
                        }.into_any()
                    }
                    Err(e) => server_fn_error_or_denied(&e, "see this topic's events"),
                })}
            </Suspense>
        </Flex>
    }
}

/// An exact event id: open the event. Replace the history entry so Back returns to the
/// search before it.
#[component]
fn OpenEvent(
    /// Event to open.
    id: String,
) -> impl IntoView {
    let navigate = use_navigate();
    let href = photon_backend::photon_event_path(&id);
    let target = href.clone();
    Effect::new(move |_| {
        navigate(
            &target,
            NavigateOptions {
                replace: true,
                ..NavigateOptions::default()
            },
        );
    });
    view! {
        <Body1 block=true>"Opening " <Link href=href>{id}</Link> "\u{2026}"</Body1>
    }
}

/// What the list says when nothing matches: names the search and the range, and offers the
/// last 7 days when a search found nothing in a shorter preset.
fn events_empty_state(query: &EventsQuery, widen: Callback<()>) -> AnyView {
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
fn EventsDataTable(
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
