use leptos::prelude::*;
use orbital::components::SpacingSize;
use orbital::primitives::{Flex, Tab, TabList};
use uf_product::access::{AccessCard, ResourceRef};

const OVERVIEW: &str = "overview";
const ACCESS: &str = "access";

/// Overview and Access tabs for a topic or subscription page.
///
/// Both panels stay mounted and the inactive one is hidden, so switching tabs keeps the
/// overview's loaded data and the Access card's state instead of rebuilding either.
#[component]
pub fn ResourceTabs(
    /// The topic or subscription the Access tab shows.
    resource: ResourceRef,
    /// Overview panel content.
    children: Children,
) -> impl IntoView {
    let selected = RwSignal::new(OVERVIEW.to_string());

    view! {
        <Flex vertical=true gap=SpacingSize::Size160.flex_gap()>
            <div data-testid="photon-resource-tabs">
                <TabList selected_value=selected>
                    <Tab value=OVERVIEW>"Overview"</Tab>
                    <Tab value=ACCESS>"Access"</Tab>
                </TabList>
            </div>
            <div data-testid="photon-overview-panel" hidden=move || selected.get() != OVERVIEW>
                {children()}
            </div>
            <div data-testid="photon-access-panel" hidden=move || selected.get() != ACCESS>
                <AccessCard resource=resource />
            </div>
        </Flex>
    }
}
