//! Process-wide Valence + Higgs + mem Photon for Playwright.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use gauge::manifest_sync::{
    sync_permission_manifests, PermissionDomainInput, PermissionInput, PermissionManifestInput,
};
use gauge::service;
use gauge::super_user::SUPER_USER_GROUP_NAME;
use higgs::actor_policy::external_actor_json_policy;
use higgs::{HiggsConfig, HiggsValenceFactory};
use photon::{configure, subscribe, topic, Actor, InProcStoragePort, JsonIdentityFactory, Photon};
use tokio::sync::OnceCell;
use valence::{
    register_backend_logical_names, router_key, Actor as ValenceActor, DatabaseBackend,
    DatabaseRouter, InMemoryBackend, Model, RegisterBackendLogicalNamesOptions,
    RouterValenceFactory, RouterValenceFactoryConfig, Valence, ValenceFactory, MEM_ENGINE_ID,
    SQLITE_ENGINE_ID,
};

struct E2eState {
    router: Arc<DatabaseRouter>,
    #[allow(dead_code)]
    storage: Arc<InProcStoragePort>,
    higgs: Arc<HiggsConfig>,
    photon: Arc<Photon>,
    default_backend_key: String,
    fixtures: Mutex<FixtureIds>,
}

/// Stable fixture ids exposed to seed JSON / Playwright.
#[derive(Clone, Debug, Default)]
pub struct FixtureIds {
    pub topic_name: String,
    pub subscription_id: String,
    pub event_id: String,
    /// Orders topic: a personal field, three handlers (handled, waiting, failed).
    pub orders_topic: String,
    /// An order published by a person; every handler state shows on it.
    pub order_event_id: String,
    /// That order's key.
    pub order_key: String,
    /// A key on three orders.
    pub search_key: String,
    /// Live events on the orders topic.
    pub orders_total: u64,
    /// An order whose payload was trimmed after its handler dead-lettered it.
    pub expired_event_id: String,
}

static E2E_STATE: OnceCell<Arc<E2eState>> = OnceCell::const_new();
static HANDLER_HITS: AtomicUsize = AtomicUsize::new(0);
/// `send-confirmation` runs per event id, so a redeliver shows up as one more.
static CONFIRMATIONS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Lab topic registered into the in-process Photon registry.
pub const E2E_TOPIC_NAME: &str = "test.photon.e2e.ops";
/// Durable handler name (registry key suffix).
pub const E2E_HANDLER_NAME: &str = "e2e-ops-handler";
/// Group that holds `PhotonAdmin`; default owner of every topic and subscription bundle.
pub const PHOTON_ADMINS_GROUP_ID: &str = "photon.admins";

#[topic(name = "test.photon.e2e.ops")]
pub struct E2eOpsEvent {
    pub value: u32,
}

/// Orders topic for the Events and event page specs.
pub const E2E_ORDERS_TOPIC: &str = "test.photon.e2e.orders";
/// Orders published as one each under these keys (`ord-0001` ..).
const E2E_SINGLE_ORDERS: usize = 27;
/// Key on [`E2E_SEARCH_ORDERS`] orders.
const E2E_SEARCH_KEY: &str = "ord-search";
const E2E_SEARCH_ORDERS: usize = 3;
/// Key of the order whose payload is trimmed.
const E2E_EXPIRED_KEY: &str = "ord-expired";

#[topic(name = "test.photon.e2e.orders", keyed_by = "order_id")]
pub struct E2eOrderPlaced {
    pub order_id: String,
    #[photon(personal)]
    pub customer_email: String,
    pub total_cents: u32,
}

#[subscribe(topic = "test.photon.e2e.orders", durable = "send-confirmation")]
async fn on_order_confirm(_actor: Box<dyn Actor>, event: E2eOrderPlaced) -> photon::Result<()> {
    if let Ok(mut seen) = CONFIRMATIONS.lock() {
        seen.push(event.order_id);
    }
    Ok(())
}

/// Paused at boot, so every order waits for it.
#[subscribe(topic = "test.photon.e2e.orders", durable = "warehouse")]
async fn on_order_warehouse(_actor: Box<dyn Actor>, _event: E2eOrderPlaced) -> photon::Result<()> {
    Ok(())
}

/// Always fails, so every order is in its dead-letter queue.
#[subscribe(topic = "test.photon.e2e.orders", durable = "fraud-check")]
async fn on_order_fraud(_actor: Box<dyn Actor>, _event: E2eOrderPlaced) -> photon::Result<()> {
    Err(photon::PhotonError::Internal(
        "fraud service unavailable".into(),
    ))
}

/// How many times `send-confirmation` handled orders with `order_id`.
pub fn e2e_confirmations(order_id: &str) -> usize {
    CONFIRMATIONS
        .lock()
        .map(|seen| seen.iter().filter(|id| *id == order_id).count())
        .unwrap_or(0)
}

#[subscribe(topic = "test.photon.e2e.ops", durable = "e2e-ops-handler")]
async fn on_e2e_ops(_actor: Box<dyn Actor>, event: E2eOpsEvent) -> photon::Result<()> {
    assert_eq!(event.value, 42);
    HANDLER_HITS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

struct HiggsFactory(RouterValenceFactory);

impl HiggsValenceFactory for HiggsFactory {
    fn build(&self, actor_json: &serde_json::Value) -> anyhow::Result<Valence> {
        self.0.build(actor_json).map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn prepare_env() {
    valence::deletion::register_noop_deletion_dispatcher_for_tests();
    valence::clear_for_test();
    // SAFETY: host boot only.
    unsafe {
        if std::env::var_os("VALENCE_OWNERSHIP_UNIFIED_FETCH").is_none() {
            std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");
        }
    }
}

fn ensure_transport_key() {
    if std::env::var_os("PHOTON_TRANSPORT_KEY").is_none() {
        // SAFETY: lab process boot only.
        unsafe {
            std::env::set_var(
                "PHOTON_TRANSPORT_KEY",
                // base64 of 32 zero bytes — accepted when ALLOW_DEV is set
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            );
            std::env::set_var("PHOTON_ALLOW_DEV_TRANSPORT_KEY", "1");
        }
    }
}

async fn seed_user(id: &str, email_verified: bool, valence: &Valence) {
    let now = Utc::now();
    let confirmed_at = email_verified.then_some(now);
    let user = lepton::generated::User::new(
        Some(lepton::generated::UserUserType::Person),
        Some("e2e-password-hash".to_string()),
        Some(lepton::generated::UserStatus::Active),
        None,
        None,
        confirmed_at,
        None,
        None,
        now,
        now,
    )
    .expect("build user");
    lepton::generated::User::upsert(id, user, valence, valence::use_!(r#"**Test:** Fixture **User** save for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#))
        .await
        .expect("upsert user");
}

async fn seed_super_user_with_member(system: &Valence, member_user_id: &str) {
    let super_group = gauge::generated::PermissionGroup::new(
        None,
        None,
        None,
        SUPER_USER_GROUP_NAME.to_string(),
        Some("super users".to_string()),
        Utc::now(),
        Utc::now(),
    )
    .expect("build super user group");
    let created =
        gauge::generated::PermissionGroup::upsert("super_user_group", super_group, system, valence::use_!(r#"**Test:** Fixture **Permission Group** save for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#))
            .await
            .expect("upsert super user group");

    let member = lepton::generated::User::get(member_user_id, system, valence::use_!(r#"**Test:** Fixture **User** load for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#))
        .await
        .expect("query member")
        .expect("member exists");
    let principal = gauge::generated::PermissionUserPrincipal::upsert(
        &format!("user:{member_user_id}"),
        gauge::generated::PermissionUserPrincipal::new(
            member.id().expect("member id").clone(),
            member_user_id.to_string(),
        )
        .expect("new principal"),
        system,
        valence::use_!(r#"**Test:** Fixture **Permission User Principal** save for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#),
    )
    .await
    .expect("upsert principal");
    created
        .relate_to_owner_record(
            principal.id().expect("principal id"),
            system,
            valence::use_!(r#"**Test:** Fixture **owner edge** write in `e2e_valence` so the suite can arrange Gauge super-user membership. CI and developers running the suite only."#),
        )
        .await
        .expect("relate super owner");
    created
        .relate_to_member_record(
            principal.id().expect("principal id"),
            system,
            valence::use_!(r#"**Test:** Fixture **member edge** write in `e2e_valence` so the suite can arrange Gauge super-user membership. CI and developers running the suite only."#),
        )
        .await
        .expect("relate super member");
}

async fn demote_admin_from_super_user(system: &Valence) {
    let Some(super_group) = gauge::generated::PermissionGroup::get("super_user_group", system, valence::use_!(r#"**Test:** Fixture **Permission Group** load for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#))
        .await
        .expect("get super user group")
    else {
        return;
    };
    let Some(principal) = gauge::generated::PermissionUserPrincipal::get("user:admin", system, valence::use_!(r#"**Test:** Fixture **Permission User Principal** load for `e2e_valence` so the suite can arrange and assert persistence behavior. CI and developers running the suite only."#))
        .await
        .expect("get admin principal")
    else {
        return;
    };
    let pid = principal.id().expect("principal id").clone();
    let _ = super_group
        .unrelate_from_member_record(
            &pid,
            system,
            valence::use_!(r#"**Test:** Fixture **member edge** remove in `e2e_valence` so the suite can demote a user from super-user after grants. CI and developers running the suite only."#),
        )
        .await;
    let _ = super_group
        .unrelate_from_owner_record(
            &pid,
            system,
            valence::use_!(r#"**Test:** Fixture **owner edge** remove in `e2e_valence` so the suite can demote a user from super-user after grants. CI and developers running the suite only."#),
        )
        .await;
}

fn photon_admin_manifest() -> PermissionManifestInput {
    PermissionManifestInput {
        app_id: "photon".into(),
        domains: vec![PermissionDomainInput {
            key: "photon".into(),
            name: "Photon".into(),
            description: "Photon event pipeline administration".into(),
            permissions: vec![PermissionInput {
                name: "PhotonAdmin".into(),
                description: "Administer Photon topics, subscriptions, and event inspection".into(),
            }],
        }],
    }
}

async fn grant_photon_admin(admin_ctx: &Valence, user_id: &str) {
    let perms = service::list_permissions(admin_ctx, None)
        .await
        .expect("list permissions");
    let photon_admin = perms
        .into_iter()
        .find(|p| p.name == "PhotonAdmin")
        .expect("PhotonAdmin after sync");
    service::grant_permission_to_user(&photon_admin.id, user_id, admin_ctx)
        .await
        .expect("grant PhotonAdmin");
}

/// The host's admin group: holds `PhotonAdmin`, with `admin` as member and owner.
async fn seed_photon_admins_group(system: &Valence) {
    gauge::resource_permissions::ensure_group_granted_permission(
        system,
        PHOTON_ADMINS_GROUP_ID,
        "Photon admins",
        "People who administer Photon",
        "PhotonAdmin",
        &[],
    )
    .await
    .expect("seed Photon admins group");
    service::add_group_member_user(PHOTON_ADMINS_GROUP_ID, "admin", system)
        .await
        .expect("add admin to Photon admins");
    // Gauge lets owners (not members) of the admin group manage each bundle's access.
    service::add_group_owner_user(PHOTON_ADMINS_GROUP_ID, "admin", system)
        .await
        .expect("make admin an owner of Photon admins");
}

/// Bundles for every topic and subscription (as host boot does, as System), then Can view for
/// `viewer` on the fixture topic and subscription, granted by `owner` (a member of the admin
/// group) the way an owner does it from the Access card.
async fn seed_resource_access(
    system: &Valence,
    owner: &Valence,
    photon: &Photon,
    fixtures: &FixtureIds,
) {
    use gauge::resource_permissions::{roles::Role, set_principal_role};
    use gauge::types::PrincipalKind;
    use photon_app::permissions::{
        ensure_photon_resource_bundles, PHOTON_SUBSCRIPTION, PHOTON_TOPIC,
    };

    ensure_photon_resource_bundles(system, photon, PHOTON_ADMINS_GROUP_ID)
        .await
        .expect("ensure Photon resource bundles");
    set_principal_role(
        owner,
        PHOTON_TOPIC,
        &fixtures.topic_name,
        (PrincipalKind::User, "viewer"),
        Role::View,
    )
    .await
    .expect("viewer can view the fixture topic");
    set_principal_role(
        owner,
        PHOTON_SUBSCRIPTION,
        &fixtures.subscription_id,
        (PrincipalKind::User, "viewer"),
        Role::View,
    )
    .await
    .expect("viewer can view the fixture subscription");
    set_principal_role(
        owner,
        PHOTON_TOPIC,
        &fixtures.orders_topic,
        (PrincipalKind::User, "viewer"),
        Role::View,
    )
    .await
    .expect("viewer can view the orders topic");
}

async fn bootstrap_photon_fixtures(photon: &Photon) -> anyhow::Result<FixtureIds> {
    HANDLER_HITS.store(0, Ordering::SeqCst);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    E2eOpsEvent { value: 42 }.publish().await?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    photon
        .runtime()
        .executor_services
        .checkpoint_coalescer
        .flush()
        .await?;

    if HANDLER_HITS.load(Ordering::SeqCst) < 1 {
        anyhow::bail!("durable e2e handler must run");
    }

    let recent = photon_backend::ops::load_recent_events(photon, 50).await?;
    let event_id = recent
        .first()
        .map(|e| e.event_id.clone())
        .ok_or_else(|| anyhow::anyhow!("published event missing from recent list"))?;

    Ok(FixtureIds {
        topic_name: E2E_TOPIC_NAME.into(),
        subscription_id: format!("{E2E_TOPIC_NAME}:{E2E_HANDLER_NAME}"),
        event_id,
        ..FixtureIds::default()
    })
}

/// Orders: 27 single-key orders (one published by a person), three under one search key,
/// and one whose payload is trimmed after `fraud-check` dead-letters it.
async fn bootstrap_order_fixtures(
    photon: &Photon,
    storage: &InProcStoragePort,
    fixtures: &mut FixtureIds,
) -> anyhow::Result<()> {
    use photon::StoragePort;

    photon
        .pause(&format!("{E2E_ORDERS_TOPIC}:warehouse"), "e2e")
        .await?;
    let system = serde_json::json!({"System": {"operation": "e2e_orders"}});
    let person = serde_json::json!({"User": {"user_id": "ines"}});
    let order = |key: &str, n: usize| {
        serde_json::json!({
            "order_id": key,
            "customer_email": format!("customer{n}@example.com"),
            "total_cents": 1000 + n,
        })
    };
    let mut first_order = None;
    for n in 1..=E2E_SINGLE_ORDERS {
        let key = format!("ord-{n:04}");
        let actor = if n == 1 {
            person.clone()
        } else {
            system.clone()
        };
        let id = photon
            .publish(E2E_ORDERS_TOPIC, Some(&key), actor, order(&key, n))
            .await?;
        if n == 1 {
            first_order = Some((id, key));
        }
    }
    for n in 0..E2E_SEARCH_ORDERS {
        photon
            .publish(
                E2E_ORDERS_TOPIC,
                Some(E2E_SEARCH_KEY),
                system.clone(),
                order(E2E_SEARCH_KEY, 100 + n),
            )
            .await?;
    }
    let expired_id = photon
        .publish(
            E2E_ORDERS_TOPIC,
            Some(E2E_EXPIRED_KEY),
            system.clone(),
            order(E2E_EXPIRED_KEY, 999),
        )
        .await?;

    // Let every handler run (fraud-check dead-letters each order), then commit checkpoints.
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if !photon.dlq_for_event(&expired_id).is_empty() && e2e_confirmations(E2E_EXPIRED_KEY) > 0 {
            break;
        }
    }
    photon
        .runtime()
        .executor_services
        .checkpoint_coalescer
        .flush()
        .await?;
    let expired = photon
        .get_event(&expired_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expired fixture missing before trim"))?;
    storage
        .truncate_before(E2E_ORDERS_TOPIC, Some(E2E_EXPIRED_KEY), expired.seq + 1)
        .await?;
    if photon.dlq_for_event(&expired_id).is_empty() {
        anyhow::bail!("fraud-check must dead-letter the expired fixture");
    }

    let (order_event_id, order_key) =
        first_order.ok_or_else(|| anyhow::anyhow!("no orders published"))?;
    fixtures.orders_topic = E2E_ORDERS_TOPIC.into();
    fixtures.order_event_id = order_event_id;
    fixtures.order_key = order_key;
    fixtures.search_key = E2E_SEARCH_KEY.into();
    fixtures.orders_total = (E2E_SINGLE_ORDERS + E2E_SEARCH_ORDERS) as u64;
    fixtures.expired_event_id = expired_id;
    Ok(())
}

/// Build shared Valence/Higgs/Photon once and seed baseline fixtures.
pub async fn init_e2e_valence() {
    E2E_STATE
        .get_or_init(|| async {
            prepare_env();
            ensure_transport_key();

            let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
            let mut router = DatabaseRouter::new();
            register_backend_logical_names(
                &mut router,
                Arc::clone(&backend),
                gauge::embedded_surreal::EMBEDDED_SURREAL_LOGICAL_NAMES,
                RegisterBackendLogicalNamesOptions {
                    register_alias_engine_id: Some(SQLITE_ENGINE_ID),
                },
            );
            router.register(
                router_key(gauge::embedded_surreal::LOGICAL_NAME, SQLITE_ENGINE_ID),
                Arc::clone(&backend),
            );
            let router = Arc::new(router);
            let default_key = router_key(gauge::embedded_surreal::LOGICAL_NAME, MEM_ENGINE_ID);

            let system = Valence::builder()
                .database_router(Arc::clone(&router))
                .default_backend_key(default_key.clone())
                .with_actor(ValenceActor::System {
                    operation: "e2e_photon_host".into(),
                })
                .build()
                .expect("e2e Valence");

            seed_user("admin", true, &system).await;
            seed_user("outsider", true, &system).await;
            seed_user("viewer", true, &system).await;
            seed_user("unverified", false, &system).await;
            seed_super_user_with_member(&system, "admin").await;

            sync_permission_manifests(&system, &[photon_admin_manifest()])
                .await
                .expect("sync PhotonAdmin manifest");

            let admin_ctx = system.with_actor(ValenceActor::User {
                user_id: "admin".to_string(),
            });
            seed_photon_admins_group(&system).await;
            grant_photon_admin(&admin_ctx, "unverified").await;
            demote_admin_from_super_user(&system).await;

            let storage = Arc::new(InProcStoragePort::new(
                photon::TransportCrypto::from_env().expect("e2e transport key"),
            ));
            let photon = Photon::builder()
                .storage_port(Arc::clone(&storage) as Arc<dyn photon::StoragePort>)
                .auto_registry()
                .build()
                .expect("e2e Photon");
            photon
                .start_executor(Arc::new(JsonIdentityFactory))
                .expect("start executor");
            configure(photon.clone());

            let mut fixtures = bootstrap_photon_fixtures(&photon)
                .await
                .expect("bootstrap photon fixtures");
            bootstrap_order_fixtures(&photon, &storage, &mut fixtures)
                .await
                .expect("bootstrap order fixtures");
            seed_resource_access(&system, &admin_ctx, &photon, &fixtures).await;
            let photon = Arc::new(photon);

            let factory: Arc<dyn HiggsValenceFactory> =
                Arc::new(HiggsFactory(RouterValenceFactory::new(
                    Arc::clone(&router),
                    RouterValenceFactoryConfig::new(default_key.clone())
                        .actor_json_policy(external_actor_json_policy()),
                )));
            let higgs = Arc::new(
                HiggsConfig::builder()
                    .valence_factory_arc(factory)
                    .build()
                    .expect("e2e HiggsConfig"),
            );

            Arc::new(E2eState {
                router,
                storage,
                higgs,
                photon,
                default_backend_key: default_key,
                fixtures: Mutex::new(fixtures),
            })
        })
        .await;
}

fn state() -> Arc<E2eState> {
    E2E_STATE
        .get()
        .expect("init_e2e_valence must run first")
        .clone()
}

pub fn e2e_router() -> Arc<DatabaseRouter> {
    Arc::clone(&state().router)
}

pub fn e2e_higgs_config() -> Arc<HiggsConfig> {
    Arc::clone(&state().higgs)
}

pub fn e2e_photon() -> Arc<Photon> {
    Arc::clone(&state().photon)
}

pub fn e2e_fixtures() -> FixtureIds {
    state().fixtures.lock().expect("fixtures").clone()
}

pub fn e2e_system_valence() -> Valence {
    Valence::builder()
        .database_router(e2e_router())
        .default_backend_key(state().default_backend_key.clone())
        .with_actor(ValenceActor::System {
            operation: "e2e_seed".into(),
        })
        .build()
        .expect("system valence")
}
