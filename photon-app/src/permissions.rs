//! Permission manifest and resource kinds for the Photon operations app.
//!
//! Two layers of access:
//!
//! - [`PhotonPermission::PhotonAdmin`], the app-wide permission synced into the `photon`
//!   domain. The host's admin group holds it.
//! - A permission bundle per topic ([`PHOTON_TOPIC_KIND`]) and per subscription
//!   ([`PHOTON_SUBSCRIPTION_KIND`]). Server functions check the resource's action as the
//!   session actor through `uf_product::access::require`, and the pages show the shared
//!   Access card and denied states.
//!
//! | Kind | Resource id | Actions |
//! |------|-------------|---------|
//! | Topic | topic name from `#[topic]` | View (inspect events), Run (publish a test event), Maintain |
//! | Subscription | subscription id (`{topic}:{handler}`) | View, Run (replay), Edit (pause, resume, reset), Maintain |
//!
//! Topics and subscriptions are declared in code, so their bundles are made at host boot by
//! `ensure_photon_resource_bundles` (SSR), with the host's admin group as default owner.

use uf_product::access::ResourceRef;
use uf_product_macros::UfPermissionManifest;

/// App-wide admin permission for the Photon ops UI.
///
/// Synced into the `photon` domain. The host's admin group holds it, and that group is the
/// default owner of every topic and subscription bundle.
#[allow(clippy::expl_impl_clone_on_copy)]
#[derive(UfPermissionManifest)]
#[permission_manifest(
    domain_key = "photon",
    domain_name = "Photon",
    domain_description = "Photon event pipeline administration"
)]
pub enum PhotonPermission {
    /// Administer every topic and subscription.
    #[permission(description = "Administer Photon topics, subscriptions, and event inspection")]
    PhotonAdmin,
}

/// Resource kind prefix for one Photon topic.
pub const PHOTON_TOPIC_KIND: &str = "photon_topic";

/// Resource kind prefix for one Photon subscription.
pub const PHOTON_SUBSCRIPTION_KIND: &str = "photon_subscription";

/// Owner shown for a topic or subscription whose only owner is the default admin group.
pub const DEFAULT_OWNER_LABEL: &str = "Photon admins";

/// What the list pages show for a resource the reader holds nothing on.
pub const NO_ACCESS: &str = "No access";

/// A topic as the shared access views name it.
#[must_use]
pub fn topic_ref(topic_name: &str) -> ResourceRef {
    ResourceRef::new(PHOTON_TOPIC_KIND, topic_name).noun("topic")
}

/// A subscription as the shared access views name it, by its id (`{topic}:{handler}`).
#[must_use]
pub fn subscription_ref(subscription_id: &str) -> ResourceRef {
    ResourceRef::new(PHOTON_SUBSCRIPTION_KIND, subscription_id).noun("subscription")
}

/// The role label for the actions the reader holds on one resource, strongest first.
///
/// `Maintain` reads as Owner. A reader with nothing gets [`NO_ACCESS`].
#[must_use]
pub fn access_label(held: &[&str]) -> &'static str {
    [
        ("Maintain", "Owner"),
        ("Edit", "Can edit"),
        ("Run", "Can run"),
        ("View", "Can view"),
    ]
    .into_iter()
    .find(|(action, _)| held.contains(action))
    .map_or(NO_ACCESS, |(_, label)| label)
}

/// The resource named in a permission-denied server error, with the missing action.
///
/// `uf_product::access::require` fails with `permission_denied::{kind}.{id}.{action}`.
/// Photon ids contain dots, so the kind is matched as a known prefix and the action is
/// the last segment. Returns `None` for any other error.
#[must_use]
pub fn denied_resource(error: &str) -> Option<(ResourceRef, String)> {
    const PREFIX: &str = "permission_denied::";
    let rest = &error[error.find(PREFIX)? + PREFIX.len()..];
    let rest = rest.split_whitespace().next()?;
    let (name, action) = rest.rsplit_once('.')?;
    if let Some(id) = name.strip_prefix(&format!("{PHOTON_SUBSCRIPTION_KIND}.")) {
        return Some((subscription_ref(id), action.to_string()));
    }
    let id = name.strip_prefix(&format!("{PHOTON_TOPIC_KIND}."))?;
    Some((topic_ref(id), action.to_string()))
}

#[cfg(feature = "ssr")]
pub use bundles::{
    ensure_photon_resource_bundles, BundleSetupError, EnsuredBundles, PHOTON_SUBSCRIPTION,
    PHOTON_TOPIC,
};

#[cfg(feature = "ssr")]
mod bundles {
    use gauge::resource_permissions::{
        ensure_resource_permission_bundle_for_group, KindDefaultGroups, ResourceAction,
        ResourceKindDescriptor, ResourcePermissionError, UmbrellaPolicy,
    };

    use super::{PHOTON_SUBSCRIPTION_KIND, PHOTON_TOPIC_KIND};

    /// Gauge kind for one topic: View, Run, Maintain.
    pub const PHOTON_TOPIC: ResourceKindDescriptor = ResourceKindDescriptor {
        prefix: PHOTON_TOPIC_KIND,
        display_label: "Photon topic",
        actions: &[
            ResourceAction::View,
            ResourceAction::Run,
            ResourceAction::Maintain,
        ],
        umbrella: UmbrellaPolicy::KindWide,
        groups: KindDefaultGroups {
            creators: "photon.topic.creators",
            viewers: "photon.topic.viewers",
            editors: "photon.topic.editors",
            operators: "photon.topic.operators",
        },
        create_permission: "CreatePhotonTopics",
    };

    /// Gauge kind for one subscription: View, Run, Edit, Maintain.
    pub const PHOTON_SUBSCRIPTION: ResourceKindDescriptor = ResourceKindDescriptor {
        prefix: PHOTON_SUBSCRIPTION_KIND,
        display_label: "Photon subscription",
        actions: &[
            ResourceAction::View,
            ResourceAction::Run,
            ResourceAction::Edit,
            ResourceAction::Maintain,
        ],
        umbrella: UmbrellaPolicy::KindWide,
        groups: KindDefaultGroups {
            creators: "photon.subscription.creators",
            viewers: "photon.subscription.viewers",
            editors: "photon.subscription.editors",
            operators: "photon.subscription.operators",
        },
        create_permission: "CreatePhotonSubscriptions",
    };

    /// How many bundles [`ensure_photon_resource_bundles`] ensured.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EnsuredBundles {
        /// Topic bundles.
        pub topics: usize,
        /// Subscription bundles.
        pub subscriptions: usize,
    }

    /// Why the boot ensure stopped.
    #[derive(Debug)]
    pub enum BundleSetupError {
        /// Reading the subscription inventory from Photon failed.
        Inventory(photon_backend::ops::OpsError),
        /// Gauge refused or failed one bundle (including a non-System `Valence`).
        Bundle {
            /// Kind prefix of the resource.
            kind: &'static str,
            /// Resource id.
            resource_id: String,
            /// Gauge's error.
            source: ResourcePermissionError,
        },
    }

    impl std::fmt::Display for BundleSetupError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Inventory(e) => write!(f, "photon access setup: read subscriptions: {e}"),
                Self::Bundle {
                    kind,
                    resource_id,
                    source,
                } => write!(
                    f,
                    "photon access setup: ensure {kind} bundle for {resource_id}: {source}"
                ),
            }
        }
    }

    impl std::error::Error for BundleSetupError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::Inventory(e) => Some(e),
                Self::Bundle { source, .. } => Some(source),
            }
        }
    }

    /// Ensure a permission bundle for every registered topic and subscription.
    ///
    /// Call at host boot after manifest sync, with a System `Valence` (boot starts as
    /// System; never call this from a request). `admin_group_id` is the group that holds
    /// `PhotonAdmin`: it becomes each bundle's default owner and holds every action, so
    /// people with Photon admin today keep their access. Idempotent; a resource that leaves
    /// the code keeps its bundle until someone removes it in Gauge.
    ///
    /// # Errors
    ///
    /// [`BundleSetupError::Inventory`] when Photon's admin snapshot fails, and
    /// [`BundleSetupError::Bundle`] when Gauge refuses a bundle: `system` is not System,
    /// `admin_group_id` doesn't exist, or a write fails.
    pub async fn ensure_photon_resource_bundles(
        system: &valence::Valence,
        photon: &photon::Photon,
        admin_group_id: &str,
    ) -> Result<EnsuredBundles, BundleSetupError> {
        let mut ensured = EnsuredBundles {
            topics: 0,
            subscriptions: 0,
        };
        let topic_names: Vec<String> = photon
            .registry()
            .iter()
            .map(|desc| desc.topic_name.to_string())
            .collect();
        for name in topic_names {
            ensure_resource_permission_bundle_for_group(
                system,
                PHOTON_TOPIC,
                &name,
                &format!("Photon topic {name}"),
                &[],
                admin_group_id,
            )
            .await
            .map_err(|source| BundleSetupError::Bundle {
                kind: PHOTON_TOPIC_KIND,
                resource_id: name.clone(),
                source,
            })?;
            ensured.topics += 1;
        }

        let subs = photon_backend::ops::list_subscriptions(photon)
            .await
            .map_err(BundleSetupError::Inventory)?;
        for sub in subs {
            ensure_resource_permission_bundle_for_group(
                system,
                PHOTON_SUBSCRIPTION,
                &sub.subscription_id,
                &format!("Photon subscription {}", sub.subscription_name),
                &[],
                admin_group_id,
            )
            .await
            .map_err(|source| BundleSetupError::Bundle {
                kind: PHOTON_SUBSCRIPTION_KIND,
                resource_id: sub.subscription_id.clone(),
                source,
            })?;
            ensured.subscriptions += 1;
        }
        Ok(ensured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_label_picks_the_strongest_role() {
        assert_eq!(access_label(&["View", "Run", "Maintain"]), "Owner");
        assert_eq!(access_label(&["View", "Run", "Edit"]), "Can edit");
        assert_eq!(access_label(&["Run", "View"]), "Can run");
        assert_eq!(access_label(&["View"]), "Can view");
        assert_eq!(access_label(&[]), NO_ACCESS);
    }

    #[test]
    fn denied_resource_reads_dotted_topic_ids() {
        let err = "error running server function: permission_denied::photon_topic.test.photon.e2e.ops.View";
        let (resource, action) = denied_resource(err).expect("denied topic");
        assert_eq!(resource.kind, PHOTON_TOPIC_KIND);
        assert_eq!(resource.id, "test.photon.e2e.ops");
        assert_eq!(action, "View");
    }

    #[test]
    fn denied_resource_reads_subscription_ids() {
        let err = "permission_denied::photon_subscription.test.photon.e2e.ops:e2e-ops-handler.View";
        let (resource, action) = denied_resource(err).expect("denied subscription");
        assert_eq!(resource.kind, PHOTON_SUBSCRIPTION_KIND);
        assert_eq!(resource.id, "test.photon.e2e.ops:e2e-ops-handler");
        assert_eq!(action, "View");
    }

    #[test]
    fn denied_resource_ignores_other_errors() {
        assert!(denied_resource("permission_denied::PhotonAdmin").is_none());
        assert!(denied_resource("Photon IO failed").is_none());
        assert!(denied_resource("permission_denied::gauge_domain.x.View").is_none());
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn descriptors_name_permissions_per_resource() {
        use gauge::resource_permissions::{permission_name, ResourceAction};
        let name = permission_name(PHOTON_TOPIC, "orders.created", ResourceAction::Run);
        assert!(name.starts_with("photon_topic.orders_created_"), "{name}");
        assert!(name.ends_with(".Run"), "{name}");
        assert!(!PHOTON_TOPIC.actions.contains(&ResourceAction::Edit));
        assert!(PHOTON_SUBSCRIPTION.actions.contains(&ResourceAction::Edit));
    }
}
