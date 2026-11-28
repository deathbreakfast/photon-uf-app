//! Tinted badge with a subscription's health word.

use leptos::prelude::*;
use orbital::primitives::{Badge, BadgeAppearance, BadgeColor};
use photon_backend::SubscriptionHealth;

/// The badge colour for each health: Healthy success, Catching up informative, Stalled
/// warning, Failing danger, Paused and Not started subtle.
#[must_use]
pub const fn health_color(health: SubscriptionHealth) -> BadgeColor {
    match health {
        SubscriptionHealth::Healthy => BadgeColor::Success,
        SubscriptionHealth::CatchingUp => BadgeColor::Informative,
        SubscriptionHealth::Stalled => BadgeColor::Warning,
        SubscriptionHealth::Failing => BadgeColor::Danger,
        SubscriptionHealth::Paused | SubscriptionHealth::NotStarted => BadgeColor::Subtle,
    }
}

/// A subscription's health as a tint badge ("Stalled", "Catching up"…).
#[component]
pub fn SubscriptionHealthBadge(
    /// The health to show.
    health: SubscriptionHealth,
) -> impl IntoView {
    view! {
        <Badge
            appearance=BadgeAppearance::Tint
            color=health_color(health)
            attr:data-testid="photon-health-badge"
            attr:data-health=health.query_key()
        >
            {health.label()}
        </Badge>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_match_the_design() {
        assert!(matches!(
            health_color(SubscriptionHealth::Failing),
            BadgeColor::Danger
        ));
        assert!(matches!(
            health_color(SubscriptionHealth::Stalled),
            BadgeColor::Warning
        ));
        assert!(matches!(
            health_color(SubscriptionHealth::CatchingUp),
            BadgeColor::Informative
        ));
        assert!(matches!(
            health_color(SubscriptionHealth::Healthy),
            BadgeColor::Success
        ));
        assert!(matches!(
            health_color(SubscriptionHealth::Paused),
            BadgeColor::Subtle
        ));
        assert!(matches!(
            health_color(SubscriptionHealth::NotStarted),
            BadgeColor::Subtle
        ));
    }
}
