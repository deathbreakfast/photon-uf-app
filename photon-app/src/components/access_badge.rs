use leptos::prelude::*;
use orbital::primitives::{Badge, BadgeAppearance, BadgeColor};

use crate::permissions::NO_ACCESS;

/// Badge for the reader's role on one topic or subscription ("Owner", "Can view", "No access").
#[component]
pub fn AccessBadge(
    /// Role label from the list server fns.
    #[prop(into)]
    access: String,
) -> impl IntoView {
    let (appearance, color) = if access == NO_ACCESS {
        (BadgeAppearance::Outline, BadgeColor::Subtle)
    } else {
        (BadgeAppearance::Tint, BadgeColor::Informative)
    };

    view! {
        <Badge appearance=appearance color=color attr:data-testid="photon-your-access">{access}</Badge>
    }
}
