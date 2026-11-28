//! Harness-only seed endpoint for Playwright.

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::e2e_valence::{e2e_fixtures, publish_fixture_events};
use crate::gate_demos::{write_e2e_auth_kind, E2eAuthKind};

#[derive(Debug, Deserialize)]
pub struct SeedRequest {
    /// `anonymous` | `admin` | `outsider` | `unverified` | `viewer`
    #[serde(default = "default_auth")]
    pub auth: String,
}

fn default_auth() -> String {
    E2eAuthKind::Anonymous.as_str().to_string()
}

pub async fn seed_data(
    session: tower_sessions::Session,
    Json(body): Json<SeedRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let kind = E2eAuthKind::parse(&body.auth);
    write_e2e_auth_kind(&session, kind)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let fixtures = e2e_fixtures();
    Ok(Json(serde_json::json!({
        "ok": true,
        "auth": kind.as_str(),
        "fixtures": {
            "topic_name": fixtures.topic_name,
            "subscription_id": fixtures.subscription_id,
            "event_id": fixtures.event_id,
            "stalled_subscription_id": fixtures.stalled_subscription_id,
            "failing_subscription_id": fixtures.failing_subscription_id,
            "backlog_subscription_id": fixtures.backlog_subscription_id,
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    /// `ops` | `stall` | `fail` | `backlog`
    pub topic: String,
    /// Events to publish.
    #[serde(default = "default_count")]
    pub count: u32,
}

const fn default_count() -> u32 {
    1
}

/// Publish fixture events so a test can arrange its own waiting counts.
pub async fn publish_events(
    Json(body): Json<PublishRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    publish_fixture_events(&body.topic, body.count.min(50))
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
