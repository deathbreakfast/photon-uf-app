//! Search, topic and time range filters for the Events list.

// `#[component]` writes each prop name into its generated builder docs, where
// clippy's `doc_markdown` flags snake_case names. Backticks can't be added there.
#![allow(clippy::doc_markdown)]

use leptos::prelude::*;
use orbital::components::Body1;
use orbital::primitives::{
    DatePicker, Field, Flex, FlexAlign, FlexGap, FlexWrap, SearchBox, SearchBoxAppearance, Select,
};
use orbital_base_components::OrbitalDateTime;

use crate::server::EventsRange;

/// What the toolbar edits. The browser turns it into the URL when a value is committed.
#[derive(Clone, Copy)]
pub struct EventFilters {
    /// Text in the search box (committed with Enter).
    pub search: RwSignal<String>,
    /// Selected topic name; empty for all topics.
    pub topic: RwSignal<String>,
    /// Selected range, as its URL value (`1h`, `24h`, `7d`, `custom`).
    pub range: RwSignal<String>,
    /// First day of a custom range.
    pub from: RwSignal<Option<OrbitalDateTime>>,
    /// Last day of a custom range.
    pub to: RwSignal<Option<OrbitalDateTime>>,
}

/// Search box, Topic and range selects, and two date pickers for a custom range.
///
/// Sits outside the results' Suspense, so typing and focus survive while results load.
/// Enter in the search box calls `on_search`; the selects and pickers commit through the
/// signals in [`EventFilters`].
#[component]
pub fn EventFilterToolbar(
    /// Signals the toolbar edits.
    filters: EventFilters,
    /// Topics for the Topic select (topics the reader can View).
    topics: Signal<Vec<String>>,
    /// Hide the Topic select (the topic page's Events tab fixes the topic).
    #[prop(default = true)]
    show_topic: bool,
    /// Commit the search box text.
    on_search: Callback<()>,
) -> impl IntoView {
    let custom = move || filters.range.get() == EventsRange::Custom.as_param();
    view! {
        <Flex wrap=FlexWrap::Wrap gap=FlexGap::Medium align=FlexAlign::End attr:data-testid="photon-event-filters">
            // A form so Enter submits the search box; SearchBox has no key events.
            <form
                role="search"
                data-testid="photon-event-search"
                on:submit=move |ev| {
                    ev.prevent_default();
                    on_search.run(());
                }
            >
                <Field label="Event id or key" label_hidden=true>
                    <SearchBox bind=filters.search appearance=SearchBoxAppearance::with_placeholder("Event id or key") />
                </Field>
            </form>
            {show_topic.then(|| view! {
                <Field label="Topic" label_hidden=true>
                    <Select bind=filters.topic attr:aria-label="Topic" attr:data-testid="photon-event-topic">
                        <option value="">"All topics"</option>
                        {move || topics.get().into_iter().map(|name| {
                            let label = name.clone();
                            view! { <option value=name>{label}</option> }
                        }).collect_view()}
                    </Select>
                </Field>
            })}
            <Field label="Time range" label_hidden=true>
                <Select bind=filters.range attr:aria-label="Time range" attr:data-testid="photon-event-range">
                    {EventsRange::ALL.into_iter().map(|range| view! {
                        <option value=range.as_param()>{range.label()}</option>
                    }).collect_view()}
                </Select>
            </Field>
            <Show when=custom>
                <Flex gap=FlexGap::Small align=FlexAlign::Center attr:data-testid="photon-event-custom-range">
                    <Field label="From" label_hidden=true>
                        <DatePicker bind=filters.from attr:aria-label="From" />
                    </Field>
                    <Body1>"to"</Body1>
                    <Field label="To" label_hidden=true>
                        <DatePicker bind=filters.to attr:aria-label="To" />
                    </Field>
                </Flex>
            </Show>
        </Flex>
    }
}
