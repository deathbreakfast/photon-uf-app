//! Harness-only seed endpoint for Playwright.

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::e2e_valence::{e2e_confirmations, e2e_fixtures};
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
            "orders_topic": fixtures.orders_topic,
            "order_event_id": fixtures.order_event_id,
            "order_key": fixtures.order_key,
            "search_key": fixtures.search_key,
            "orders_total": fixtures.orders_total,
            "expired_event_id": fixtures.expired_event_id,
            "order_confirmations": e2e_confirmations(&fixtures.order_key),
        }
    })))
}
