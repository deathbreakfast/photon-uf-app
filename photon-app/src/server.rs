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
//!   [`require_subscription`]. Hosts without an access backend fail closed. Subscription
//!   actions check the subscription: Run for replay and retry now, Edit for pause, resume
//!   and skip to latest. Photon records the session user as `by` with each action.
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
    DashboardStats, EventDetail, EventSummary, ReplayPreviewSummary, SubscriptionHealth,
    SubscriptionSummary, TopicSummary, WaitingEvents,
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
    /// The team that owns it: its first owner other than the default admin group, else
    /// [`crate::permissions::DEFAULT_OWNER_LABEL`].
    pub owner: String,
    /// Every owner, for "ask an owner" lines.
    pub owners: Vec<String>,
}

/// How long events may wait with nothing handled before a subscription reads as Stalled.
///
/// Hosts may provide this as Leptos context; without it Photon uses five minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StallAfter(pub std::time::Duration);

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

    /// Whether the reader may replay it or retry now (Run).
    #[must_use]
    pub fn can_run(&self) -> bool {
        matches!(self.access.as_str(), "Owner" | "Can edit" | "Can run")
    }

    /// Whether the reader may pause, resume or skip it (Edit).
    #[must_use]
    pub fn can_edit(&self) -> bool {
        matches!(self.access.as_str(), "Owner" | "Can edit")
    }
}

/// Picks the owner shown on a subscription from its owner rows (`(label, default_owner)`).
#[must_use]
pub fn owner_label(owners: &[(String, bool)]) -> String {
    owners
        .iter()
        .find(|(_, default_owner)| !default_owner)
        .or_else(|| owners.first())
        .map_or_else(
            || crate::permissions::DEFAULT_OWNER_LABEL.to_string(),
            |(label, _)| label.clone(),
        )
}

/// Parses the Replay dialog's choice: `seq` (start after that seq), `time` (RFC 3339) or
/// `event` (an event id).
///
/// # Errors
///
/// An error naming the field when `kind` is unknown or the seq isn't a number.
pub fn replay_point(kind: &str, value: &str) -> Result<photon_backend::ReplayPoint, String> {
    let value = value.trim();
    match kind {
        "seq" => value
            .parse::<i64>()
            .map(photon_backend::ReplayPoint::AfterSeq)
            .map_err(|_| "Replay sequence number must be a whole number".to_string()),
        "time" => Ok(photon_backend::ReplayPoint::Time(value.to_string())),
        "event" => Ok(photon_backend::ReplayPoint::Event(value.to_string())),
        _ => Err(format!("Unknown replay start `{kind}`")),
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
        ..SubscriptionSummary::default()
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
    request: &RequestContexts,
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
        request.restore();
        if reader_can(backend, resource, action).await {
            return crate::permissions::access_label(&[action]);
        }
    }
    crate::permissions::NO_ACCESS
}

/// Names of the topics the reader can View.
#[cfg(feature = "ssr")]
async fn viewable_topics(photon: &photon::Photon) -> std::collections::HashSet<String> {
    let request = RequestContexts::capture();
    let backend = uf_product::access::use_resource_access_backend();
    let mut names = std::collections::HashSet::new();
    for desc in photon.registry().iter() {
        let name = desc.topic_name.to_string();
        let resource = crate::permissions::topic_ref(&name);
        request.restore();
        if reader_can(backend.as_ref(), &resource, "View").await {
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

/// The label Photon records as `by` for an action: the session user.
#[cfg(feature = "ssr")]
fn actor_label(ctx: &higgs::Higgs) -> Result<String, ServerFnError> {
    require_session(ctx)?;
    Ok(ctx.session_user_id().map_or_else(String::new, |id| {
        id.strip_prefix("user:").unwrap_or(id).to_string()
    }))
}

/// The stall threshold from [`StallAfter`] context, else five minutes.
#[cfg(feature = "ssr")]
fn stall_after() -> chrono::Duration {
    leptos::context::use_context::<StallAfter>()
        .and_then(|s| chrono::Duration::from_std(s.0).ok())
        .unwrap_or(photon_backend::DEFAULT_STALL_AFTER)
}

/// Owner labels of one resource as `(label, default_owner)`; empty when the backend can't
/// say (none wired, or the reader may not read its access).
#[cfg(feature = "ssr")]
async fn resource_owners(
    backend: Option<&std::sync::Arc<dyn uf_product::access::ResourceAccessBackend>>,
    resource: &uf_product::access::ResourceRef,
) -> Vec<(String, bool)> {
    let Some(backend) = backend else {
        return Vec::new();
    };
    match backend.access(resource).await {
        Ok(view) => view
            .entries
            .into_iter()
            .filter(|e| e.role == uf_product::access::Role::Owner)
            .map(|e| (e.principal.label, e.default_owner))
            .collect(),
        Err(e) => {
            leptos::logging::warn!(
                "photon owner lookup failed kind={} id={}: {e}",
                resource.kind,
                resource.id
            );
            Vec::new()
        }
    }
}

/// The request contexts an access backend reads the session from, captured while the
/// server fn still has them.
///
/// During SSR a long server fn can lose its owner's contexts partway through (each access
/// check reads `HiggsConfig` and the request parts again). [`Self::restore`] provides them
/// again on the current owner before the next backend call, so a list of many rows checks
/// every row as the reader instead of failing closed.
#[cfg(feature = "ssr")]
struct RequestContexts {
    higgs: Option<std::sync::Arc<higgs::HiggsConfig>>,
    parts: Option<http::request::Parts>,
}

#[cfg(feature = "ssr")]
impl RequestContexts {
    fn capture() -> Self {
        Self {
            higgs: leptos::context::use_context::<std::sync::Arc<higgs::HiggsConfig>>(),
            parts: leptos::context::use_context::<http::request::Parts>(),
        }
    }

    fn restore(&self) {
        if leptos::context::use_context::<std::sync::Arc<higgs::HiggsConfig>>().is_none() {
            if let Some(higgs) = &self.higgs {
                provide_context(std::sync::Arc::clone(higgs));
            }
        }
        if leptos::context::use_context::<http::request::Parts>().is_none() {
            if let Some(parts) = &self.parts {
                provide_context(parts.clone());
            }
        }
    }
}

/// A subscription as a row: the reader's role and its owners (`(label, default_owner)`).
#[cfg(feature = "ssr")]
fn subscription_row(
    sub: SubscriptionSummary,
    access: &str,
    owner_rows: &[(String, bool)],
) -> SubscriptionRow {
    let mut owners: Vec<String> = owner_rows.iter().map(|(l, _)| l.clone()).collect();
    if owners.is_empty() {
        owners.push(crate::permissions::DEFAULT_OWNER_LABEL.to_string());
    }
    SubscriptionRow {
        sub,
        access: access.to_string(),
        owner: owner_label(owner_rows),
        owners,
    }
}

/// Loads a subscription for an action, failing when the id names none.
#[cfg(feature = "ssr")]
async fn action_target(
    photon: &photon::Photon,
    id: &str,
    action: &str,
) -> Result<SubscriptionSummary, ServerFnError> {
    photon_backend::validate_subscription_id(id)
        .map_err(|e| ServerFnError::new(format!("{action} subscription: {e}")))?;
    let sub =
        photon_backend::ops::load_subscription_at(photon, id, chrono::Utc::now(), stall_after())
            .await
            .map_err(|e| map_ops_err(&e))?
            .ok_or_else(|| map_ops_err(&photon_backend::ops::OpsError::SubscriptionNotFound))?;
    Ok(sub)
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
    let request = RequestContexts::capture();
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
        let access = reader_access(&request, backend.as_ref(), &resource, false).await;
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
///
/// An unknown name returns `None` before the check: it has no bundle to check against, and the
/// topics list already shows every name.
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
    let Some(topic) = photon_backend::ops::load_topic(&photon, &topic_name)
        .await
        .map_err(|e| map_ops_err(&e))?
    else {
        return Ok(None);
    };
    require_topic(&topic_name, "View").await?;
    Ok(Some(topic))
}

/// Get every subscription (handler inventory + checkpoints via `admin_snapshot`) with the
/// reader's role on each.
///
/// Subscriptions the reader holds nothing on come back with only their id, name and topic.
#[uf_product_macros::server]
pub async fn get_subscriptions() -> Result<Vec<SubscriptionRow>, ServerFnError> {
    let request = RequestContexts::capture();
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let mut subs =
        photon_backend::ops::list_subscriptions_at(&photon, chrono::Utc::now(), stall_after())
            .await
            .map_err(|e| map_ops_err(&e))?;
    photon_backend::sort_worst_first(&mut subs);
    let backend = uf_product::access::use_resource_access_backend();
    let mut checked = Vec::with_capacity(subs.len());
    for sub in subs {
        let resource = crate::permissions::subscription_ref(&sub.subscription_id);
        request.restore();
        let access = reader_access(&request, backend.as_ref(), &resource, true).await;
        let sub = if access == crate::permissions::NO_ACCESS {
            redact_subscription(sub)
        } else {
            sub
        };
        checked.push((sub, access, resource));
    }
    // Owner lookups start together, so each reads the session while the restored contexts
    // are still there.
    request.restore();
    let owners = futures::future::join_all(
        checked
            .iter()
            .map(|(_, _, resource)| resource_owners(backend.as_ref(), resource)),
    )
    .await;
    Ok(checked
        .into_iter()
        .zip(owners)
        .map(|((sub, access, _), owner_rows)| subscription_row(sub, access, &owner_rows))
        .collect())
}

/// Get a single subscription by ID, with the reader's role and its owners. Needs View on
/// the subscription.
///
/// An unknown id returns `None` before the check, as for [`get_topic`].
#[uf_product_macros::server]
pub async fn get_subscription(
    /// Unique identifier of the subscription to look up.
    id: String,
) -> Result<Option<SubscriptionRow>, ServerFnError> {
    let request = RequestContexts::capture();
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    photon_backend::validate_subscription_id(&id)
        .map_err(|e| ServerFnError::new(format!("get subscription: {e}")))?;
    let Some(sub) =
        photon_backend::ops::load_subscription_at(&photon, &id, chrono::Utc::now(), stall_after())
            .await
            .map_err(|e| map_ops_err(&e))?
    else {
        return Ok(None);
    };
    require_subscription(&id, "View").await?;
    let backend = uf_product::access::use_resource_access_backend();
    let resource = crate::permissions::subscription_ref(&id);
    let access = reader_access(&request, backend.as_ref(), &resource, true).await;
    request.restore();
    let owner_rows = resource_owners(backend.as_ref(), &resource).await;
    Ok(Some(subscription_row(sub, access, &owner_rows)))
}

/// The events a subscription still owes, oldest first, with the total waiting. Needs View
/// on the subscription.
#[uf_product_macros::server]
pub async fn get_waiting_events(
    /// Subscription id.
    subscription_id: String,
    /// Most events to return (capped at [`photon_backend::MAX_EVENT_LIST_LIMIT`]).
    limit: u32,
) -> Result<WaitingEvents, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    photon_backend::validate_subscription_id(&subscription_id)
        .map_err(|e| ServerFnError::new(format!("get waiting events: {e}")))?;
    require_subscription(&subscription_id, "View").await?;
    photon_backend::ops::load_waiting_events(&photon, &subscription_id, limit)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Pause a subscription: Photon stops dispatching to it until someone resumes it. Needs
/// Edit on the subscription.
#[uf_product_macros::server]
pub async fn pause_subscription(
    /// Subscription id.
    subscription_id: String,
) -> Result<(), ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    let by = actor_label(&ctx)?;
    let photon = photon_from_context()?;
    action_target(&photon, &subscription_id, "pause").await?;
    require_subscription(&subscription_id, "Edit").await?;
    photon_backend::ops::pause_subscription(&photon, &subscription_id, &by)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Resume a paused subscription from its checkpoint. Needs Edit on the subscription.
#[uf_product_macros::server]
pub async fn resume_subscription(
    /// Subscription id.
    subscription_id: String,
) -> Result<(), ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    let by = actor_label(&ctx)?;
    let photon = photon_from_context()?;
    action_target(&photon, &subscription_id, "resume").await?;
    require_subscription(&subscription_id, "Edit").await?;
    photon_backend::ops::resume_subscription(&photon, &subscription_id, &by)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Move a subscription's checkpoint to the newest event, dropping what waits; returns how
/// many events were skipped. Needs Edit on the subscription.
#[uf_product_macros::server]
pub async fn skip_subscription_to_latest(
    /// Subscription id.
    subscription_id: String,
) -> Result<u64, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    let by = actor_label(&ctx)?;
    let photon = photon_from_context()?;
    action_target(&photon, &subscription_id, "skip").await?;
    require_subscription(&subscription_id, "Edit").await?;
    photon_backend::ops::skip_subscription_to_latest(&photon, &subscription_id, &by)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// What a replay from `from_kind`/`from_value` would resend, without moving anything.
/// Needs Run on the subscription.
///
/// `from_kind` is `seq` (start after that sequence number), `time` (RFC 3339) or `event`.
#[uf_product_macros::server]
pub async fn replay_preview(
    /// Subscription id.
    subscription_id: String,
    /// `seq`, `time` or `event`.
    from_kind: String,
    /// The sequence number, RFC 3339 time or event id.
    from_value: String,
) -> Result<ReplayPreviewSummary, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let point = replay_point(&from_kind, &from_value).map_err(ServerFnError::new)?;
    action_target(&photon, &subscription_id, "replay").await?;
    require_subscription(&subscription_id, "Run").await?;
    photon_backend::ops::replay_preview(&photon, &subscription_id, &point)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Replay a subscription from a point: its checkpoint moves back and it handles those
/// events again. Needs Run on the subscription.
#[uf_product_macros::server]
pub async fn replay_subscription(
    /// Subscription id.
    subscription_id: String,
    /// `seq`, `time` or `event`.
    from_kind: String,
    /// The sequence number, RFC 3339 time or event id.
    from_value: String,
) -> Result<ReplayPreviewSummary, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    let by = actor_label(&ctx)?;
    let photon = photon_from_context()?;
    let point = replay_point(&from_kind, &from_value).map_err(ServerFnError::new)?;
    action_target(&photon, &subscription_id, "replay").await?;
    require_subscription(&subscription_id, "Run").await?;
    photon_backend::ops::replay_subscription(&photon, &subscription_id, &point, &by)
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Retry the failing event now instead of waiting for its backoff. Needs Run on the
/// subscription.
#[uf_product_macros::server]
pub async fn retry_subscription_now(
    /// Subscription id.
    subscription_id: String,
) -> Result<(), ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    let by = actor_label(&ctx)?;
    let photon = photon_from_context()?;
    action_target(&photon, &subscription_id, "retry").await?;
    require_subscription(&subscription_id, "Run").await?;
    photon_backend::ops::retry_subscription_now(&photon, &subscription_id, &by)
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
            health: SubscriptionHealth::Failing,
            waiting: 7,
            failures_24h: 3,
            last_error: Some("smtp refused".into()),
            ..SubscriptionSummary::default()
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
        assert_eq!(redacted.health, SubscriptionHealth::NotStarted);
        assert_eq!((redacted.waiting, redacted.failures_24h), (0, 0));
        assert_eq!(redacted.last_error, None);
    }

    #[test]
    fn owner_prefers_a_team_over_the_admin_group() {
        assert_eq!(owner_label(&[]), crate::permissions::DEFAULT_OWNER_LABEL);
        assert_eq!(
            owner_label(&[("Photon admins".into(), true), ("Warehouse".into(), false)]),
            "Warehouse"
        );
        assert_eq!(owner_label(&[("Admins".into(), true)]), "Admins");
    }

    #[test]
    fn roles_decide_actions() {
        let row = |access: &str| SubscriptionRow {
            sub: SubscriptionSummary::default(),
            access: access.into(),
            owner: String::new(),
            owners: vec![],
        };
        assert!(row("Owner").can_edit() && row("Owner").can_run());
        assert!(row("Can edit").can_edit() && row("Can edit").can_run());
        assert!(!row("Can run").can_edit() && row("Can run").can_run());
        assert!(!row("Can view").can_edit() && !row("Can view").can_run());
    }

    #[test]
    fn replay_points_parse() {
        assert_eq!(
            replay_point("seq", " 40 "),
            Ok(photon_backend::ReplayPoint::AfterSeq(40))
        );
        assert!(replay_point("seq", "forty").is_err());
        assert_eq!(
            replay_point("event", "ev-1"),
            Ok(photon_backend::ReplayPoint::Event("ev-1".into()))
        );
        assert!(replay_point("bogus", "1").is_err());
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
