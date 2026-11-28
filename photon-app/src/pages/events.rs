use leptos::prelude::*;
use orbital::components::{Body1, ContentContainer, SpacingSize, Title3};
use orbital::primitives::{Flex, FlexGap};

use crate::components::EventsBrowser;

/// Events page: search every event by id or key, filter by topic and time, page through
/// the results. [`EventsBrowser`] loads them with `get_events_page` (topic names from
/// `get_topics`).
#[component]
pub fn PhotonEventsIndexPage() -> impl IntoView {
    crate::hydration::forget_server_resources();
    view! {
        <ContentContainer data_testid="photon-events">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Flex vertical=true gap=FlexGap::Small>
                    <Title3>"Events"</Title3>
                    <Body1 block=true>"Everything published, newest first. Search by event id or key."</Body1>
                </Flex>
                <EventsBrowser />
            </Flex>
        </ContentContainer>
    }
}
