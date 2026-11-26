//! Leptos server functions for Photon UI.
//!
//! DTOs and pure mapping helpers live in [`photon_backend`] so contracts stay
//! unit/integration-testable without the host UI graph. Photon IO for ops reads
//! lives in [`photon_backend::ops`] (feature `ops`). Server functions run on
//! SSR only and use Photon request context for IO (Chronon/Boson-shaped: no
//! Valence ops projection).
//!
//! ## Security map
//!
//! - Every ops-UI server fn requires an authenticated session.
//! - Reads are gated per resource as the session actor through
//!   [`uf_product::access::require`]: View on the topic for its events and event details,
//!   View on the subscription for its read state. Cross-topic reads (dashboard, recent
//!   events, the unfiltered events list) keep only topics the reader can View.
//! - The topic and subscription lists show every resource with the reader's role
//!   ("No access" where nothing); a No access row carries only its name and topic.
//! - Run and Edit checks for actions go through [`require_topic`] and
//!   [`require_subscription`]. Hosts without an access backend fail closed.
//! - Catalog, subscription, and event reads come from Photon (`admin_snapshot`,
//!   registry, `list_*`, `get_event`).
//! - Path/query ids are rejected when blank, oversized, or containing `/` `\`,
//!   controls, or `.` / `..` (`photon_backend::validate_*`).
//!
//! ## Errors
//!
//! Fallible ops return [`ServerFnError`] (Leptos boundary). Blank path/query ids
//! are rejected by `photon_backend::validate_*` as [`photon_backend::PhotonIdError`]
//! and mapped with operation context. Missing session and missing Photon context
//! use [`photon_backend::ops::require_session_user`] and
//! [`photon_backend::ops::require_photon`]. Photon IO failures are also
//! `ServerFnError` strings at this boundary.

use leptos::prelude::*;
pub use photon_backend::{
    DashboardStats, EventDetail, EventSummary, SubscriptionSummary, TopicSummary,
};

/// App-wide admin permission name
/// (manifest: [`crate::permissions::PhotonPermission::PhotonAdmin`]). Its holders' group
/// is the default owner of every topic and subscription bundle.
pub const PHOTON_ADMIN_PERMISSION: &str = "PhotonAdmin";

/// A topic in the topics list, with what the reader can do on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TopicRow {
    /// The topic. Counts and schema are blank when `access` is No access.
    pub topic: TopicSummary,
    /// The reader's role: Owner, Can run, Can view, or No access.
    pub access: String,
}

/// A subscription in the subscriptions list, with what the reader can do on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionRow {
    /// The subscription. Read state is blank when `access` is No access.
    pub sub: SubscriptionSummary,
    /// The reader's role: Owner, Can edit, Can run, Can view, or No access.
    pub access: String,
}

impl TopicRow {
    /// Whether the reader holds anything on this topic.
    #[must_use]
    pub fn has_access(&self) -> bool {
        self.access != crate::permissions::NO_ACCESS
    }
}

impl SubscriptionRow {
    /// Whether the reader holds anything on this subscription.
    #[must_use]
    pub fn has_access(&self) -> bool {
        self.access != crate::permissions::NO_ACCESS
    }
}

/// A topic row for a reader with no access: the name only.
#[must_use]
pub fn redact_topic(topic: TopicSummary) -> TopicSummary {
    TopicSummary {
        topic_name: topic.topic_name,
        keyed_by: None,
        schema_json: String::new(),
        event_count_24h: 0,
        subscription_count: 0,
    }
}

/// A subscription row for a reader with no access: the id, name and topic only.
#[must_use]
pub fn redact_subscription(sub: SubscriptionSummary) -> SubscriptionSummary {
    SubscriptionSummary {
        subscription_id: sub.subscription_id,
        subscription_name: sub.subscription_name,
        topic_name: sub.topic_name,
        enabled: false,
        mode: String::new(),
        topic_key_filter: None,
        checkpoint_lag: 0,
        last_seq: None,
        last_processed_at: None,
    }
}

#[cfg(feature = "ssr")]
fn map_ops_err(e: &photon_backend::ops::OpsError) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[cfg(feature = "ssr")]
fn photon_from_context() -> Result<std::sync::Arc<photon::Photon>, ServerFnError> {
    photon_backend::ops::require_photon(leptos::context::use_context::<
        std::sync::Arc<photon::Photon>,
    >())
    .map_err(|e| map_ops_err(&e))
}

/// Require `action` on one topic as the session actor (fails closed without a backend).
///
/// # Errors
///
/// A `permission_denied::photon_topic.{name}.{action}` error when the reader lacks it.
#[cfg(feature = "ssr")]
pub async fn require_topic(topic_name: &str, action: &str) -> Result<(), ServerFnError> {
    uf_product::access::require(&crate::permissions::topic_ref(topic_name), action).await
}

/// Require `action` on one subscription as the session actor (fails closed without a backend).
///
/// # Errors
///
/// A `permission_denied::photon_subscription.{id}.{action}` error when the reader lacks it.
#[cfg(feature = "ssr")]
pub async fn require_subscription(
    subscription_id: &str,
    action: &str,
) -> Result<(), ServerFnError> {
    uf_product::access::require(
        &crate::permissions::subscription_ref(subscription_id),
        action,
    )
    .await
}

/// Whether the reader holds `action` on `resource`; no backend or a failed check reads as no.
#[cfg(feature = "ssr")]
async fn reader_can(
    backend: Option<&std::sync::Arc<dyn uf_product::access::ResourceAccessBackend>>,
    resource: &uf_product::access::ResourceRef,
    action: &str,
) -> bool {
    let Some(backend) = backend else {
        return false;
    };
    match backend.can(resource, action).await {
        Ok(can) => can,
        Err(e) => {
            leptos::logging::warn!(
                "photon access check failed kind={} id={} action={action}: {e}",
                resource.kind,
                resource.id
            );
            false
        }
    }
}

/// The reader's role label on one resource, checking the strongest action first.
#[cfg(feature = "ssr")]
async fn reader_access(
    backend: Option<&std::sync::Arc<dyn uf_product::access::ResourceAccessBackend>>,
    resource: &uf_product::access::ResourceRef,
    has_edit: bool,
) -> &'static str {
    let actions: &[&str] = if has_edit {
        &["Maintain", "Edit", "Run", "View"]
    } else {
        &["Maintain", "Run", "View"]
    };
    for action in actions {
        if reader_can(backend, resource, action).await {
            return crate::permissions::access_label(&[action]);
        }
    }
    crate::permissions::NO_ACCESS
}

/// Names of the topics the reader can View.
#[cfg(feature = "ssr")]
async fn viewable_topics(photon: &photon::Photon) -> std::collections::HashSet<String> {
    let backend = uf_product::access::use_resource_access_backend();
    let mut names = std::collections::HashSet::new();
    for desc in photon.registry().iter() {
        let name = desc.topic_name.to_string();
        if reader_can(
            backend.as_ref(),
            &crate::permissions::topic_ref(&name),
            "View",
        )
        .await
        {
            names.insert(name);
        }
    }
    names
}

#[cfg(feature = "ssr")]
fn require_session(ctx: &higgs::Higgs) -> Result<(), ServerFnError> {
    photon_backend::ops::require_session_user(ctx.session_user_id().map(String::as_str))
        .map_err(|e| map_ops_err(&e))
}

// ============================================================================
// Server functions (SSR only)
// ============================================================================

/// Get dashboard stats: topic and subscription counts (every resource, as the lists show
/// them) and events in the last 24 hours on topics the reader can View.
#[uf_product_macros::server]
pub async fn get_dashboard_stats() -> Result<DashboardStats, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let viewable = viewable_topics(&photon).await;
    let mut stats = photon_backend::ops::load_dashboard_stats(&photon)
        .await
        .map_err(|e| map_ops_err(&e))?;
    let topics = photon_backend::ops::load_topics(&photon)
        .await
        .map_err(|e| map_ops_err(&e))?;
    stats.event_count_24h = topics
        .iter()
        .filter(|t| viewable.contains(&t.topic_name))
        .map(|t| t.event_count_24h)
        .sum();
    Ok(stats)
}

/// Get recent events for the dashboard, on topics the reader can View.
#[uf_product_macros::server]
pub async fn get_recent_events(
    /// Maximum number of recent events to return.
    limit: u32,
) -> Result<Vec<EventSummary>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let viewable = viewable_topics(&photon).await;
    let events = photon_backend::ops::load_recent_events(&photon, limit)
        .await
        .map_err(|e| map_ops_err(&e))?;
    Ok(events
        .into_iter()
        .filter(|e| viewable.contains(&e.topic_name))
        .collect())
}

/// Get every topic (registry + Photon list counts) with the reader's role on each.
///
/// Topics the reader holds nothing on come back with only their name.
#[uf_product_macros::server]
pub async fn get_topics() -> Result<Vec<TopicRow>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let topics = photon_backend::ops::load_topics(&photon)
        .await
        .map_err(|e| map_ops_err(&e))?;
    let backend = uf_product::access::use_resource_access_backend();
    let mut rows = Vec::with_capacity(topics.len());
    for topic in topics {
        let resource = crate::permissions::topic_ref(&topic.topic_name);
        let access = reader_access(backend.as_ref(), &resource, false).await;
        let topic = if access == crate::permissions::NO_ACCESS {
            redact_topic(topic)
        } else {
            topic
        };
        rows.push(TopicRow {
            topic,
            access: access.to_string(),
        });
    }
    Ok(rows)
}

/// Get a single topic by name. Needs View on the topic.
#[uf_product_macros::server]
pub async fn get_topic(
    /// Name of the topic to look up.
    topic_name: String,
) -> Result<Option<TopicSummary>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    photon_backend::validate_topic_name(&topic_name)
        .map_err(|e| ServerFnError::new(format!("get topic: {e}")))?;
    require_topic(&topic_name, "View").await?;
    photon_backend::ops::load_topic(&photon, &topic_name)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Get every subscription (handler inventory + checkpoints via `admin_snapshot`) with the
/// reader's role on each.
///
/// Subscriptions the reader holds nothing on come back with only their id, name and topic.
#[uf_product_macros::server]
pub async fn get_subscriptions() -> Result<Vec<SubscriptionRow>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let subs = photon_backend::ops::list_subscriptions(&photon)
        .await
        .map_err(|e| map_ops_err(&e))?;
    let backend = uf_product::access::use_resource_access_backend();
    let mut rows = Vec::with_capacity(subs.len());
    for sub in subs {
        let resource = crate::permissions::subscription_ref(&sub.subscription_id);
        let access = reader_access(backend.as_ref(), &resource, true).await;
        let sub = if access == crate::permissions::NO_ACCESS {
            redact_subscription(sub)
        } else {
            sub
        };
        rows.push(SubscriptionRow {
            sub,
            access: access.to_string(),
        });
    }
    Ok(rows)
}

/// Get a single subscription by ID. Needs View on the subscription.
#[uf_product_macros::server]
pub async fn get_subscription(
    /// Unique identifier of the subscription to look up.
    id: String,
) -> Result<Option<SubscriptionSummary>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    photon_backend::validate_subscription_id(&id)
        .map_err(|e| ServerFnError::new(format!("get subscription: {e}")))?;
    require_subscription(&id, "View").await?;
    photon_backend::ops::load_subscription(&photon, &id)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Get events (recent across all topics, or optionally filter by topic).
///
/// Results are capped at [`photon_backend::MAX_EVENT_LIST_LIMIT`]. When `topic_name` is
/// set, the query is topic-scoped via Photon `list_events_by_topic` and needs View on the
/// topic; otherwise only events on topics the reader can View are returned.
#[uf_product_macros::server]
pub async fn get_events(
    /// Optional topic name to restrict results to; when omitted, events from all
    /// topics are considered (still capped).
    topic_name: Option<String>,
    /// Maximum number of events to return.
    limit: u32,
) -> Result<Vec<EventSummary>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    if let Some(topic) = topic_name.as_deref() {
        photon_backend::validate_topic_name(topic)
            .map_err(|e| ServerFnError::new(format!("get events: {e}")))?;
        require_topic(topic, "View").await?;
        return photon_backend::ops::load_events(&photon, Some(topic), limit)
            .await
            .map_err(|e| map_ops_err(&e));
    }
    let viewable = viewable_topics(&photon).await;
    let events = photon_backend::ops::load_events(&photon, None, limit)
        .await
        .map_err(|e| map_ops_err(&e))?;
    Ok(events
        .into_iter()
        .filter(|e| viewable.contains(&e.topic_name))
        .collect())
}

/// Get a single event by ID. Needs View on the event's topic.
#[uf_product_macros::server]
pub async fn get_event(
    /// Unique identifier of the event to look up.
    id: String,
) -> Result<Option<EventDetail>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let event = photon_backend::ops::load_event(&photon, &id)
        .await
        .map_err(|e| map_ops_err(&e))?;
    if let Some(event) = &event {
        require_topic(&event.topic_name, "View").await?;
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_topic_keeps_only_the_name() {
        let topic = TopicSummary {
            topic_name: "orders.created".into(),
            keyed_by: Some("order_id".into()),
            schema_json: "{}".into(),
            event_count_24h: 12,
            subscription_count: 3,
        };
        let redacted = redact_topic(topic);
        assert_eq!(redacted.topic_name, "orders.created");
        assert_eq!(redacted.keyed_by, None);
        assert_eq!(redacted.schema_json, "");
        assert_eq!(
            (redacted.event_count_24h, redacted.subscription_count),
            (0, 0)
        );
    }

    #[test]
    fn redacted_subscription_keeps_id_name_and_topic() {
        let sub = SubscriptionSummary {
            subscription_id: "orders.created:billing".into(),
            subscription_name: "billing".into(),
            topic_name: "orders.created".into(),
            enabled: true,
            mode: "durable".into(),
            topic_key_filter: Some("eu".into()),
            checkpoint_lag: 7,
            last_seq: Some(40),
            last_processed_at: Some("2026-11-25T00:00:00Z".into()),
        };
        let redacted = redact_subscription(sub);
        assert_eq!(redacted.subscription_id, "orders.created:billing");
        assert_eq!(redacted.subscription_name, "billing");
        assert_eq!(redacted.topic_name, "orders.created");
        assert!(!redacted.enabled);
        assert_eq!(redacted.mode, "");
        assert_eq!(redacted.topic_key_filter, None);
        assert_eq!((redacted.checkpoint_lag, redacted.last_seq), (0, None));
        assert_eq!(redacted.last_processed_at, None);
    }

    #[test]
    fn rows_report_access() {
        let row = TopicRow {
            topic: redact_topic(TopicSummary {
                topic_name: "t".into(),
                keyed_by: None,
                schema_json: String::new(),
                event_count_24h: 0,
                subscription_count: 0,
            }),
            access: crate::permissions::NO_ACCESS.into(),
        };
        assert!(!row.has_access());
        assert!(TopicRow {
            access: "Can view".into(),
            ..row
        }
        .has_access());
    }
}
