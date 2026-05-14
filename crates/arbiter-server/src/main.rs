//! Entry point for the Muni Town Arbiter Server.
//!
//! Sets up the Salvo HTTP server with all XRPC endpoints, auth middleware,
//! DID web identity, persistence, and background tasks.

use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;

use salvo::conn::tcp::TcpListener;
use salvo::prelude::*;
use salvo::Router;
use salvo::writing::Json;

use arbiter_core::futures::AsyncArbiterServer;

use atproto_identity::resolve::InnerIdentityResolver;

mod auth;
mod did;
mod handlers;
mod io;
mod persistence;

use auth::AuthConfig;
use io::HttpArbiterIo;
use persistence::Persister;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

struct AppConfig {
    listen: String,
    hostname: String,
    data_dir: PathBuf,
    plc_url: String,
    unsafe_auth_token: Option<String>,
    persist_interval: Duration,
    tick_interval: Duration,
}

impl AppConfig {
    fn from_env() -> Self {
        Self {
            listen: std::env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            hostname: std::env::var("HOSTNAME")
                .expect("HOSTNAME environment variable is required (e.g. localhost:3001)"),
            data_dir: PathBuf::from(
                std::env::var("DATA_DIR").unwrap_or_else(|_| "./data/arbiters".to_string()),
            ),
            plc_url: std::env::var("PLC_URL")
                .unwrap_or_else(|_| "http://localhost:3001".to_string()),
            unsafe_auth_token: std::env::var("UNSAFE_AUTH_TOKEN").ok(),
            persist_interval: Duration::from_secs(
                std::env::var("PERSIST_INTERVAL")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5),
            ),
            tick_interval: Duration::from_millis(
                std::env::var("TICK_INTERVAL_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
            ),
        }
    }

    fn server_did(&self) -> String {
        let encoded = self.hostname.replace(':', "%3A");
        format!("did:web:{encoded}")
    }
}

// ---------------------------------------------------------------------------
// Shared server data
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ServerData {
    server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
}

struct ServerDataMiddleware {
    data: ServerData,
}

#[async_trait]
impl salvo::Handler for ServerDataMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        depot.insert("server", self.data.server.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

// ---------------------------------------------------------------------------
// DID document handler
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DidDocumentHandler {
    document: serde_json::Value,
}

#[async_trait]
impl salvo::Handler for DidDocumentHandler {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        res.render(Json(self.document.clone()));
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = AppConfig::from_env();
    let server_did = config.server_did();

    tracing::info!(
        "Starting arbiter server on {} (DID: {})",
        config.listen,
        server_did
    );

    let client = reqwest::Client::new();

    // Set up the server's signing key / DID identity
    let key_path = config.data_dir.join("signing-key.hex");
    let identity = Arc::new(did::Identity::load_or_generate(
        server_did.clone(),
        &key_path,
    ));
    tracing::info!("Server identity: {} (key: {:?})", identity.did, key_path);

    // Create the atproto-identity resolver (handles did:plc + did:web)
    use atproto_identity::resolve::HickoryDnsResolver;
    let dns_resolver = Arc::new(HickoryDnsResolver::create_resolver(&[]));
    let resolver = Arc::new(InnerIdentityResolver {
        dns_resolver,
        http_client: client.clone(),
        plc_hostname: config.plc_url.clone(),
    });

    // Set up IO layer with the resolver and identity
    let io = HttpArbiterIo::new(client, resolver.clone(), identity.clone());

    // Create the async arbiter server
    let arbiter_server = AsyncArbiterServer::with_tick_interval(io, config.tick_interval);

    // Set up persistence
    let persister = Arc::new(Persister::new(config.data_dir.clone()));

    // Load existing arbiter states from disk
    let loaded = persister.load_all();
    if !loaded.is_empty() {
        tracing::info!("Loaded {} arbiter states from disk", loaded.len());
        arbiter_server.load_all(loaded).await;
    }

    // Spawn background tick task
    {
        let server = arbiter_server.clone();
        tokio::spawn(async move {
            server.background_task().await;
        });
        tracing::info!("Background tick task started");
    }

    // Spawn persistence task
    {
        let server = arbiter_server.clone();
        let persister = persister.clone();
        let interval = config.persist_interval;
        tokio::spawn(async move {
            persistence_loop(server, persister, interval).await;
        });
        tracing::info!("Persistence task started (interval: {:?})", interval);
    }

    // Build auth config with the atproto-identity resolver
    let auth_config = Arc::new(
        AuthConfig::new(resolver.clone())
            .with_unsafe_token_if(config.unsafe_auth_token.clone()),
    );

    // Build the DID document for `/.well-known/did.json`
    let did_doc_value = serde_json::to_value(
        identity.did_document()
            .expect("Failed to build DID document"),
    )
    .expect("Failed to serialize DID document");
    let did_doc_handler = DidDocumentHandler {
        document: did_doc_value,
    };

    // Build the router
    let router = build_router(arbiter_server.clone(), auth_config, did_doc_handler);

    // Start serving
    tracing::info!("Listening on {}", config.listen);
    let acceptor = TcpListener::new(&config.listen).bind().await;
    Server::new(acceptor).serve(router).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

fn build_router(
    server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
    auth_config: Arc<AuthConfig>,
    did_doc_handler: DidDocumentHandler,
) -> Router {
    let auth_middleware = auth::AuthMiddleware::new(auth_config);
    let server_data = ServerData { server };
    let server_middleware = ServerDataMiddleware { data: server_data };

    Router::new()
        .hoop(server_middleware)
        .push(Router::with_path("/.well-known/did.json").get(did_doc_handler))
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::create_arbiter),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_arbiter),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::get_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.resolveMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::resolve_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::create_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.configureSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::configure_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setMemberAccess")
                .hoop(auth_middleware.clone())
                .post(handlers::set_member_access),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.removeMember")
                .hoop(auth_middleware)
                .post(handlers::remove_member),
        )
        .push(Router::with_path("/").get(index))
}

#[handler]
async fn index(res: &mut Response) {
    res.render("Arbiter Server is running");
}

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

impl AuthConfig {
    fn with_unsafe_token_if(self, token: Option<String>) -> Self {
        match token {
            Some(t) => self.with_unsafe_token(t),
            None => self,
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence loop
// ---------------------------------------------------------------------------

async fn persistence_loop(
    arbiter_server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
    persister: Arc<Persister>,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;

        let dirty = arbiter_server.drain_dirty_arbiters().await;
        if dirty.is_empty() {
            continue;
        }

        let mut persisted = 0usize;
        let mut deleted = 0usize;

        for did in &dirty {
            if let Some(state) = arbiter_server.snapshot_arbiter(did).await {
                if let Err(e) = persister.persist(did, &state) {
                    tracing::error!(%did, %e, "Failed to persist arbiter state");
                } else {
                    persisted += 1;
                }
            } else if let Err(e) = persister.delete(did) {
                tracing::error!(%did, %e, "Failed to delete arbiter state file");
            } else {
                deleted += 1;
            }
        }

        if persisted > 0 || deleted > 0 {
            tracing::debug!("Persisted {} and deleted {} arbiter(s)", persisted, deleted);
        }
    }
}
