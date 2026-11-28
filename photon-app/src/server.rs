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
//! - Event payloads leave the server with personal fields (the topic schema's
//!   `"x-personal": true` marks) and a publishing person's id masked. [`reveal_event_fields`]
//!   returns the real values only after View on the topic and a recorded reason, and writes
//!   the reveal to the topic's reveal log ([`get_reveal_log`], topic owners only) as the
//!   session actor.
//! - [`redeliver_event`] needs Run on the subscription.
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
    DashboardStats, DeliveryRow, DeliveryState, EventDetail, EventSummary, EventsPage, EventsQuery,
    EventsRange, PayloadField, PublishedBy, RevealReason, RevealRecordRow, RevealedFields,
    SubscriptionSummary, TopicSummary, EVENTS_PAGE_SIZE, MASKED_VALUE, MAX_EVENTS_PAGE,
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
///
/// An unknown id returns `None` before the check, as for [`get_topic`].
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
    let Some(sub) = photon_backend::ops::load_subscription(&photon, &id)
        .await
        .map_err(|e| map_ops_err(&e))?
    else {
        return Ok(None);
    };
    require_subscription(&id, "View").await?;
    Ok(Some(sub))
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

/// Get one page of the Events list: search by event id or key, a topic and a time range.
///
/// Searches only topics the reader can View; asking for one topic needs View on it. An
/// exact event id on a viewable topic comes back as `exact_id` (whatever the range or
/// topic filter) so the page can open it.
#[uf_product_macros::server]
pub async fn get_events_page(
    /// Filters, time range and page.
    query: EventsQuery,
) -> Result<EventsPage, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    if let Some(topic) = query
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        photon_backend::validate_topic_name(topic)
            .map_err(|e| ServerFnError::new(format!("get events: {e}")))?;
        require_topic(topic, "View").await?;
    }
    let viewable: Vec<String> = viewable_topics(&photon).await.into_iter().collect();
    photon_backend::ops::load_events_page(&photon, &query, &viewable, chrono::Utc::now())
        .await
        .map_err(|e| map_ops_err(&e))
}

/// Get a single event by ID, personal values masked. Needs View on the event's topic.
///
/// Each delivery row says whether the reader holds Run on that subscription, so the page
/// offers Redeliver only where it would work.
#[uf_product_macros::server]
pub async fn get_event(
    /// Unique identifier of the event to look up.
    id: String,
) -> Result<Option<EventDetail>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let photon = photon_from_context()?;
    let mut event = photon_backend::ops::load_event(&photon, &id)
        .await
        .map_err(|e| map_ops_err(&e))?;
    if let Some(event) = &mut event {
        require_topic(&event.topic_name, "View").await?;
        let backend = uf_product::access::use_resource_access_backend();
        for row in &mut event.deliveries {
            row.can_run = reader_can(
                backend.as_ref(),
                &crate::permissions::subscription_ref(&row.subscription_id),
                "Run",
            )
            .await;
        }
    }
    Ok(event)
}

/// Longest details text a reveal accepts.
pub const MAX_REVEAL_DETAILS_CHARS: usize = 500;

/// Show an event's personal fields to the reader, for this page view only.
///
/// Needs View on the event's topic and a reason. Records who asked, when, the reason and the
/// details in the topic's reveal log before returning anything.
#[uf_product_macros::server]
pub async fn reveal_event_fields(
    /// Event whose personal fields to show.
    event_id: String,
    /// Why the reader needs them.
    reason: RevealReason,
    /// What the reader typed to explain the reason.
    details: String,
) -> Result<RevealedFields, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    let details = details.trim();
    if details.is_empty() {
        return Err(ServerFnError::new(
            "reveal personal fields: say why you need them",
        ));
    }
    if details.chars().count() > MAX_REVEAL_DETAILS_CHARS {
        return Err(ServerFnError::new(format!(
            "reveal personal fields: keep the details under {MAX_REVEAL_DETAILS_CHARS} characters"
        )));
    }
    let photon = photon_from_context()?;
    let topic = photon_backend::ops::event_topic(&photon, &event_id)
        .await
        .map_err(|e| map_ops_err(&e))?
        .ok_or_else(|| ServerFnError::new("reveal personal fields: no such event"))?;
    require_topic(&topic, "View").await?;
    let reveal = photon_backend::ops::reveal_event(&photon, &event_id)
        .await
        .map_err(|e| map_ops_err(&e))?
        .ok_or_else(|| {
            ServerFnError::new("reveal personal fields: this event's payload has expired")
        })?;
    let user_id = ctx
        .session_user_id()
        .cloned()
        .ok_or_else(|| ServerFnError::new("Authentication required"))?;
    let valence = ctx
        .valence()
        .map_err(|e| ServerFnError::new(format!("reveal personal fields: {e}")))?;
    let record = reveal_log::record(&valence, &topic, &event_id, &user_id, reason, details).await?;
    Ok(RevealedFields {
        fields: reveal.fields,
        payload_json: reveal.payload_json,
        published_by_id: reveal.published_by_id,
        record,
    })
}

/// A topic's reveal log, newest first. Needs Maintain on the topic (its owners).
#[uf_product_macros::server]
#[server(prefix = "/api", endpoint = "photon_reveal_log")]
pub async fn get_reveal_log(
    /// Topic whose reveal log to read.
    topic_name: String,
) -> Result<Vec<RevealRecordRow>, ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    photon_backend::validate_topic_name(&topic_name)
        .map_err(|e| ServerFnError::new(format!("get reveal log: {e}")))?;
    require_topic(&topic_name, "Maintain").await?;
    let valence = ctx
        .valence()
        .map_err(|e| ServerFnError::new(format!("get reveal log: {e}")))?;
    reveal_log::list(&valence, &topic_name).await
}

/// Send one event to one subscription again. Needs Run on the subscription.
///
/// Only that subscription gets the event; its handler runs once more, so it may act twice.
#[uf_product_macros::server]
pub async fn redeliver_event(
    /// Event to send again.
    event_id: String,
    /// Subscription (`{topic}:{handler}`) to send it to.
    subscription_id: String,
) -> Result<(), ServerFnError> {
    let ctx = higgs::Higgs::from_request().await?;
    require_session(&ctx)?;
    photon_backend::validate_subscription_id(&subscription_id)
        .map_err(|e| ServerFnError::new(format!("redeliver: {e}")))?;
    require_subscription(&subscription_id, "Run").await?;
    let photon = photon_from_context()?;
    let by = ctx
        .session_user_id()
        .map(|id| format!("user:{id}"))
        .ok_or_else(|| ServerFnError::new("Authentication required"))?;
    photon_backend::ops::redeliver_event(&photon, &event_id, &subscription_id, &by)
        .await
        .map_err(|e| map_ops_err(&e))
}

#[cfg(feature = "ssr")]
pub use reveal_log::{TopicOwnerRead, PHOTON_REVEALED_BY_VIEWER, PHOTON_TOPIC_OWNER_READ};

/// The per-topic reveal log: the `PhotonEventReveal` Valence model's privacy rules, writes
/// and reads.
#[cfg(feature = "ssr")]
mod reveal_log {
    use std::any::Any;

    use gauge::resource_permissions::{permission_name, ResourceAction};
    use leptos::prelude::ServerFnError;
    use valence::{
        Actor, ActorContext, Error, PolicyEvaluator, PrivacyOperation, PrivacyRule, Valence,
    };

    use super::{RevealReason, RevealRecordRow};
    use crate::generated::{PhotonEventReveal, PhotonEventRevealReason};
    use valence::{Model, RecordId, SortDirection, StringPredicate};

    fn revealed_by_viewer(record: &serde_json::Value, viewer: &Actor) -> bool {
        valence::privacy_policies::owner::check_owner_by_field(record, viewer, "revealed_by")
    }

    /// The person a reveal record names: they write it as themselves and can read it.
    pub const PHOTON_REVEALED_BY_VIEWER: PrivacyRule = PrivacyRule {
        name: "photon::REVEALED_BY_VIEWER",
        description: Some("The person who was shown the personal fields"),
        check: revealed_by_viewer,
    };

    /// Lets people with Maintain on the record's topic read it (System always passes).
    #[derive(Debug, Clone)]
    pub struct TopicOwnerRead {
        /// Valence privacy rule name.
        pub rule_name: &'static str,
    }

    /// The topic's owners read its reveal log.
    pub const PHOTON_TOPIC_OWNER_READ: TopicOwnerRead = TopicOwnerRead {
        rule_name: "photon::TOPIC_OWNER_READ",
    };

    #[async_trait::async_trait]
    impl PolicyEvaluator for TopicOwnerRead {
        fn name(&self) -> &'static str {
            self.rule_name
        }

        fn description(&self) -> Option<&'static str> {
            Some("Maintain on the record's Photon topic")
        }

        async fn evaluate(
            &self,
            _op: PrivacyOperation,
            record: &serde_json::Value,
            actor: &dyn ActorContext,
            v: &Valence,
        ) -> valence::Result<bool> {
            let viewer: Actor = serde_json::from_value(actor.actor_json().clone())
                .map_err(|e| Error::Internal(format!("invalid actor context: {e}")))?;
            if viewer.is_system() {
                return Ok(true);
            }
            let Some(topic) = record.get("topic_name").and_then(serde_json::Value::as_str) else {
                return Ok(false);
            };
            let name = permission_name(
                crate::permissions::PHOTON_TOPIC,
                topic,
                ResourceAction::Maintain,
            );
            gauge::service::actor_can(v, &name)
                .await
                .map_err(|e| Error::Privacy(format!("topic owner check for '{name}': {e}")))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn reveal_err(e: impl std::fmt::Display) -> ServerFnError {
        ServerFnError::new(format!("reveal log: {e}"))
    }

    const fn stored_reason(reason: RevealReason) -> PhotonEventRevealReason {
        match reason {
            RevealReason::Support => PhotonEventRevealReason::Support,
            RevealReason::Debugging => PhotonEventRevealReason::Debugging,
            RevealReason::DataSubjectRequest => PhotonEventRevealReason::DataSubjectRequest,
        }
    }

    fn row(model: &PhotonEventReveal) -> RevealRecordRow {
        RevealRecordRow {
            event_id: model.event_id().clone(),
            revealed_by: model.revealed_by().map(|r| r.id().to_string()),
            revealed_at: model.revealed_at().to_rfc3339(),
            reason: RevealReason::parse(model.reason().as_str()).unwrap_or(RevealReason::Support),
            details: model.details().clone(),
        }
    }

    /// Write one reveal as the session actor and return it as the log shows it.
    pub async fn record(
        valence: &Valence,
        topic: &str,
        event_id: &str,
        user_id: &str,
        reason: RevealReason,
        details: &str,
    ) -> Result<RevealRecordRow, ServerFnError> {
        let model = PhotonEventReveal::new(
            topic.to_string(),
            event_id.to_string(),
            Some(RecordId::new(
                "user",
                user_id.strip_prefix("user:").unwrap_or(user_id),
            )),
            chrono::Utc::now(),
            stored_reason(reason),
            details.to_string(),
        )
        .map_err(reveal_err)?;
        let saved = PhotonEventReveal::create(model, valence, valence::use_!(r"When someone on the Photon admin app asks to see an event's **personal fields**, we first **save a reveal record**: the event and its topic, **who asked**, the time, the **reason** they picked and the **details** they typed. Only after the record is saved are the fields shown to that person, on that page. The person who asked and the **owners of the topic** can read the record in the topic's reveal log, and nobody can change or delete it."))
            .await
            .map_err(reveal_err)?;
        Ok(row(&saved))
    }

    /// Every reveal on `topic` the actor may read, newest first.
    pub async fn list(
        valence: &Valence,
        topic: &str,
    ) -> Result<Vec<RevealRecordRow>, ServerFnError> {
        let rows = PhotonEventReveal::query(valence, valence::use_!(r"To show a topic's **reveal log** to the **owners of the topic** on the Photon admin app, we **list its reveal records**: which event, **who was shown its personal fields**, when, and the **reason and details** they gave. The records are returned newest first to the owner who asked for the log."))
            .where_topic_name(StringPredicate::Equals(topic.to_string()))
            .order_by_revealed_at(SortDirection::Desc)
            .await
            .map_err(reveal_err)?;
        Ok(rows.iter().map(row).collect())
    }
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
