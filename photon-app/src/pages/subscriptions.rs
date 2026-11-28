use std::collections::HashMap;
use std::sync::Arc;

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::NavigateOptions;
use orbital::components::{Body1, Caption1, ContentContainer, EmptyState, SpacingSize, Title3};
use orbital::primitives::{
    use_breakpoint_down, Badge, BadgeAppearance, BadgeColor, Breakpoint, DataRecord, DataTable,
    DataTableColumnDef, DataTableEmptyView, DataTableEvents, DataTableFeatures,
    DataTableHeaderChromeConfig, DataTableLoadingStyle, DataTableNoResultsView, DataTableRowModel,
    DataTableToolbarConfig, DataTableToolbarEnd, Flex, Link, ListViewConfig, MessageBar,
    MessageBarIntent, PagingMode, Select, SelectBind,
};
use photon_backend::SubscriptionHealth;

use crate::components::{server_fn_error_bar, SubscriptionHealthBadge};
use crate::server::{get_subscriptions, SubscriptionRow};

/// `health` cell value for a row the reader holds nothing on.
const NO_ACCESS_KEY: &str = "no-access";

/// Rows a loading table draws.
const SKELETON_ROWS: usize = 5;

/// Keeps the rows that match the Health filter (`None` is Any health). No access rows have
/// no health, so a filter hides them.
#[must_use]
pub fn filter_by_health(
    rows: &[SubscriptionRow],
    health: Option<SubscriptionHealth>,
) -> Vec<SubscriptionRow> {
    rows.iter()
        .filter(|row| health.is_none_or(|h| row.has_access() && row.sub.health == h))
        .cloned()
        .collect()
}

/// The empty state's message: it names the Health filter when one is set.
#[must_use]
pub const fn empty_message(health: Option<SubscriptionHealth>) -> &'static str {
    match health {
        None => "No subscriptions",
        Some(SubscriptionHealth::Failing) => "No subscriptions are Failing",
        Some(SubscriptionHealth::Stalled) => "No subscriptions are Stalled",
        Some(SubscriptionHealth::Paused) => "No subscriptions are Paused",
        Some(SubscriptionHealth::CatchingUp) => "No subscriptions are Catching up",
        Some(SubscriptionHealth::NotStarted) => "No subscriptions are Not started",
        Some(SubscriptionHealth::Healthy) => "No subscriptions are Healthy",
    }
}

fn row_model(row: &SubscriptionRow, now: chrono::DateTime<chrono::Utc>) -> DataTableRowModel {
    let sub = &row.sub;
    let has_access = row.has_access();
    let mut cells = HashMap::new();
    cells.insert("name".into(), sub.subscription_name.clone());
    cells.insert("topic".into(), sub.topic_name.clone());
    cells.insert("owner".into(), row.owner.clone());
    cells.insert(
        "health".into(),
        if has_access {
            sub.health.query_key().to_string()
        } else {
            NO_ACCESS_KEY.to_string()
        },
    );
    cells.insert(
        "waiting".into(),
        if has_access {
            photon_backend::waiting_text(sub)
        } else {
            String::new()
        },
    );
    cells.insert(
        "handled".into(),
        if has_access {
            photon_backend::time_words(sub.last_handled_at.as_deref(), now)
        } else {
            String::new()
        },
    );
    DataTableRowModel::from_text_cells(sub.subscription_id.clone(), cells)
}

fn cell_text(record: &DataRecord, field: &str) -> String {
    record
        .get(field)
        .map_or_else(String::new, orbital::primitives::DataValue::display_string)
}

fn columns() -> Vec<DataTableColumnDef> {
    let name_view = Arc::new(|record: DataRecord| {
        let href = photon_backend::photon_subscription_path(&record.id);
        view! { <Link href=href>{cell_text(&record, "name")}</Link> }.into_any()
    });
    let health_view = Arc::new(|record: DataRecord| {
        let key = cell_text(&record, "health");
        SubscriptionHealth::from_query_key(&key).map_or_else(
            || {
                view! {
                    <Badge appearance=BadgeAppearance::Outline color=BadgeColor::Subtle>
                        {crate::permissions::NO_ACCESS}
                    </Badge>
                }
                .into_any()
            },
            |health| view! { <SubscriptionHealthBadge health=health /> }.into_any(),
        )
    });
    let caption_view = |field: &'static str| {
        Arc::new(move |record: DataRecord| {
            view! { <Caption1>{cell_text(&record, field)}</Caption1> }.into_any()
        })
    };
    vec![
        DataTableColumnDef::new("name", "Subscription").with_cell_view(name_view),
        DataTableColumnDef::new("topic", "Topic"),
        DataTableColumnDef::new("owner", "Owner"),
        DataTableColumnDef::new("health", "Health").with_cell_view(health_view),
        DataTableColumnDef::new("waiting", "Waiting").with_cell_view(caption_view("waiting")),
        DataTableColumnDef::new("handled", "Last handled").with_cell_view(caption_view("handled")),
    ]
}

/// The subscriptions DataTable: worst first as the server sorted them, the Health Select on
/// the search row, and skeleton rows under the real headers while `loading`.
///
/// The Health filter swaps the table's rows in place, so filtering never rebuilds the
/// table or clears its search.
#[component]
fn SubscriptionsTable(
    /// Every row, worst first.
    rows: Vec<SubscriptionRow>,
    /// The Health Select's value (a `query_key`, or empty for Any health).
    health: RwSignal<String>,
    /// Still waiting for `get_subscriptions`.
    #[prop(optional)]
    loading: bool,
) -> impl IntoView {
    let navigate = use_navigate();
    let is_narrow = use_breakpoint_down(Breakpoint::Md);
    let rows = StoredValue::new(rows);
    let selected = move || SubscriptionHealth::from_query_key(&health.get());
    let models = move |h: Option<SubscriptionHealth>| {
        let now = chrono::Utc::now();
        rows.with_value(|rows| {
            filter_by_health(rows, h)
                .iter()
                .map(|r| row_model(r, now))
                .collect::<Vec<_>>()
        })
    };
    let items = RwSignal::new(models(SubscriptionHealth::from_query_key(
        &health.get_untracked(),
    )));
    Effect::new(move |_| items.set(models(selected())));
    let loading = RwSignal::new(loading);
    let on_row_click = Callback::new(move |(id,): (String,)| {
        navigate(
            &photon_backend::photon_subscription_path(&id),
            NavigateOptions::default(),
        );
    });

    view! {
        <DataTable
            items=items
            paging=PagingMode::None
            columns=columns()
            sortable=false
            loading=loading
            loading_style=DataTableLoadingStyle::SkeletonRows(SKELETON_ROWS)
            features=if is_narrow.get_untracked() { DataTableFeatures::LIST_VIEW } else { DataTableFeatures::empty() }
            list_view=ListViewConfig::new("name")
                .with_secondary_fields(vec!["health".into(), "waiting".into()])
            toolbar_config=DataTableToolbarConfig {
                quick_search: true,
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
            events=DataTableEvents {
                on_row_click: Some(on_row_click),
                ..Default::default()
            }
        >
            <DataTableToolbarEnd slot>
                <div data-testid="photon-health-filter">
                    <Select bind=SelectBind::new(health)>
                        <option value="">"Any health"</option>
                        {SubscriptionHealth::ALL
                            .into_iter()
                            .map(|h| view! { <option value=h.query_key()>{h.label()}</option> })
                            .collect_view()}
                    </Select>
                </div>
            </DataTableToolbarEnd>
            <DataTableEmptyView slot>
                <div data-testid="photon-subscriptions-empty">
                    {move || view! { <EmptyState message=empty_message(selected()) /> }}
                </div>
            </DataTableEmptyView>
            <DataTableNoResultsView slot>
                <MessageBar intent=MessageBarIntent::Info>
                    "No subscriptions match your search."
                </MessageBar>
            </DataTableNoResultsView>
        </DataTable>
    }
}

/// Subscription index: every subscription with its health, worst first, with search and a
/// Health filter kept in `?health=`. "No access" rows show only the name, topic and owner,
/// so people can ask.
#[component]
pub fn PhotonSubscriptionsIndexPage() -> impl IntoView {
    crate::hydration::forget_server_resources();
    let navigate = use_navigate();
    let query = use_query_map();
    let url_health = Memo::new(move |_| query.read().get("health").unwrap_or_default());
    let health = RwSignal::new(url_health.get_untracked());
    // Select → URL (replace, no scroll), and URL (back/forward) → Select.
    Effect::new(move |_| {
        let value = health.get();
        if value != url_health.get_untracked() {
            let href = if value.is_empty() {
                "/photon/subscriptions".to_string()
            } else {
                format!("/photon/subscriptions?health={value}")
            };
            navigate(
                &href,
                NavigateOptions {
                    replace: true,
                    scroll: false,
                    ..NavigateOptions::default()
                },
            );
        }
    });
    Effect::new(move |_| {
        let value = url_health.get();
        if value != health.get_untracked() {
            health.set(value);
        }
    });

    let subs_res = Resource::new(|| (), |()| async move { get_subscriptions().await });

    view! {
        <ContentContainer data_testid="photon-subscriptions">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Flex vertical=true gap=SpacingSize::Size40.flex_gap()>
                    <Title3>"Subscriptions"</Title3>
                    <Body1>"Handlers that read a topic, worst first."</Body1>
                </Flex>

                <Suspense fallback=move || view! {
                    <div data-testid="photon-subscriptions-skeleton">
                        <SubscriptionsTable rows=vec![] health=health loading=true />
                    </div>
                }>
                    {move || match subs_res.get() {
                        Some(Ok(rows)) => {
                            let some_hidden = rows.iter().any(|r| !r.has_access());
                            view! {
                                <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
                                    {some_hidden.then(|| view! {
                                        <MessageBar intent=MessageBarIntent::Info attr:data-testid="photon-subscriptions-no-access">
                                            "You can see every subscription's name and owner. Health and events show on the ones your teams can view."
                                        </MessageBar>
                                    })}
                                    <div data-testid="photon-subscriptions-table">
                                        <SubscriptionsTable rows=rows health=health />
                                    </div>
                                </Flex>
                            }
                            .into_any()
                        }
                        Some(Err(e)) => server_fn_error_bar(&e),
                        None => view! {
                            <div data-testid="photon-subscriptions-skeleton">
                                <SubscriptionsTable rows=vec![] health=health loading=true />
                            </div>
                        }
                        .into_any(),
                    }}
                </Suspense>
            </Flex>
        </ContentContainer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_backend::SubscriptionSummary;

    fn row(name: &str, health: SubscriptionHealth, access: &str) -> SubscriptionRow {
        SubscriptionRow {
            sub: SubscriptionSummary {
                subscription_id: format!("t:{name}"),
                subscription_name: name.into(),
                health,
                ..SubscriptionSummary::default()
            },
            access: access.into(),
            owner: "Warehouse".into(),
            owners: vec!["Warehouse".into()],
        }
    }

    #[test]
    fn health_filter_keeps_matching_rows_in_order() {
        let rows = vec![
            row("a", SubscriptionHealth::Failing, "Owner"),
            row("b", SubscriptionHealth::Stalled, "Can view"),
            row(
                "c",
                SubscriptionHealth::Stalled,
                crate::permissions::NO_ACCESS,
            ),
            row("d", SubscriptionHealth::Healthy, "Owner"),
        ];
        assert_eq!(filter_by_health(&rows, None).len(), 4);
        let stalled = filter_by_health(&rows, Some(SubscriptionHealth::Stalled));
        let names: Vec<_> = stalled
            .iter()
            .map(|r| r.sub.subscription_name.as_str())
            .collect();
        assert_eq!(names, ["b"]);
        assert_eq!(
            filter_by_health(&rows, Some(SubscriptionHealth::Paused)),
            vec![]
        );
    }

    #[test]
    fn empty_state_names_the_filter() {
        assert_eq!(empty_message(None), "No subscriptions");
        assert_eq!(
            empty_message(Some(SubscriptionHealth::Stalled)),
            "No subscriptions are Stalled"
        );
    }
}
