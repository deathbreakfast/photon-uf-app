//! Shared Err path for Photon page Resources: toast permission denials, soft `MessageBar`.

use leptos::prelude::*;
use orbital::primitives::{MessageBar, MessageBarIntent};
use uf_product::access::AccessDenied;
use uf_product::services::report_server_fn_error;

use crate::permissions::denied_resource;

/// Report permission failures to the toast bus and render a `MessageBar`.
///
/// When the toast handles the error, the bar uses soft copy instead of the raw
/// server-fn string.
pub fn server_fn_error_bar(error: &ServerFnError) -> AnyView {
    let soft = report_server_fn_error(error);
    let message = if soft {
        "Couldn't load this section.".to_string()
    } else {
        error.to_string()
    };
    view! {
        <MessageBar intent=MessageBarIntent::Error>{message}</MessageBar>
    }
    .into_any()
}

/// Like [`server_fn_error_bar`], with a soft prefix when the toast does not handle it.
pub fn server_fn_error_bar_with_prefix(prefix: &str, error: &ServerFnError) -> AnyView {
    let soft = report_server_fn_error(error);
    let message = if soft {
        "Couldn't load this section.".to_string()
    } else {
        format!("{prefix}{error}")
    };
    view! {
        <MessageBar intent=MessageBarIntent::Error>{message}</MessageBar>
    }
    .into_any()
}

/// [`AccessDenied`] when the error is a missing action on a topic or subscription, otherwise
/// [`server_fn_error_bar`].
///
/// `purpose` says what the action would let the reader do here ("see this topic's events").
pub fn server_fn_error_or_denied(error: &ServerFnError, purpose: &str) -> AnyView {
    match denied_resource(&error.to_string()) {
        Some((resource, action)) => view! {
            <AccessDenied resource=resource action=action purpose=purpose.to_string() />
        }
        .into_any(),
        None => server_fn_error_bar(error),
    }
}
