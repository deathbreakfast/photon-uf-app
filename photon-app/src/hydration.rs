//! Keeps a page from reading resource data that the server sent for a different page.
//!
//! Leptos numbers every resource in creation order and, on the client, looks each number
//! up in `window.__RESOLVED_RESOURCES`, the values the server rendered into the first
//! page. The counter never resets. When the server render used more numbers than
//! hydration did, a resource built after client navigation can land on a number the
//! server used for another resource. If the value decodes as the new resource's type
//! (the dashboard's recent events and the events page's list are both
//! `Vec<EventSummary>`), the page shows that stale value and never calls its server
//! function.

/// Empties the server-rendered resource values once hydration has finished.
///
/// Call it at the top of a page component, before its resources are created. While
/// hydrating it does nothing, so the first page still reuses the server's data. After
/// hydration nothing can match those values by number again, so dropping them is safe,
/// and every resource created after client navigation fetches its own data.
#[cfg_attr(not(feature = "hydrate"), allow(clippy::missing_const_for_fn))]
pub fn forget_server_resources() {
    #[cfg(feature = "hydrate")]
    {
        use leptos::reactive::owner::Owner;
        use leptos::wasm_bindgen::JsCast;
        use leptos::web_sys::{js_sys, window};

        let hydrating = Owner::current_shared_context().is_none_or(|sc| sc.during_hydration());
        if hydrating {
            return;
        }
        let Some(window) = window() else { return };
        if let Ok(values) = js_sys::Reflect::get(&window, &"__RESOLVED_RESOURCES".into()) {
            if let Some(values) = values.dyn_ref::<js_sys::Array>() {
                values.set_length(0);
            }
        }
    }
}
