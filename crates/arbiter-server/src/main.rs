//! Muni Town Arbiter Server v2 — AT Protocol XRPC HTTP server.
//!
//! All XRPC requests are handled by a single catch-all handler that uses
//! the `atproto-proxy` header to identify the target arbiter DID and
//! routes through its [`StateMachine`](arbiter_core::StateMachine).

use std::sync::Arc;

use atproto_identity::resolve::{
    HickoryDnsResolver, IdentityResolver, InnerIdentityResolver, SharedIdentityResolver,
};
use clap::Parser;
use salvo::conn::tcp::TcpListener;
use salvo::prelude::*;
use salvo::writing::Json;

mod auth;
mod handlers;
mod persistence;
mod state;

use auth::AuthConfig;
use persistence::Persister;
use state::ArbiterCollection;

// ---------------------------------------------------------------------------
// CLI / env configuration
// ---------------------------------------------------------------------------

/// Muni Town Arbiter Server v2 — AT Protocol XRPC HTTP server.
#[derive(Parser, Debug)]
#[command(name = "arbiter-server", version, about)]
struct AppConfig {
    #[arg(short, long = "listen", env = "LISTEN", default_value = "0.0.0.0:8203")]
    listen: String,
    #[arg(short = 'H', long = "hostname", env = "HOSTNAME", default_value = "localhost:8080")]
    hostname: String,
    #[arg(short, long = "data-dir", env = "DATA_DIR", default_value = "./data/arbiters")]
    data_dir: std::path::PathBuf,
    #[arg(long = "unsafe-auth-token", env = "UNSAFE_AUTH_TOKEN")]
    unsafe_auth_token: Option<String>,
    #[arg(long = "persist-interval", env = "PERSIST_INTERVAL", default_value = "5")]
    persist_interval_secs: u64,
    #[arg(long = "plc-directory-url", env = "PLC_DIRECTORY_URL", default_value = "http://localhost:3001")]
    plc_directory_url: String,
}

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

pub struct ServerState {
    pub arbiters: tokio::sync::Mutex<ArbiterCollection>,
    pub default_policy: &'static str,
    pub server_did: String,
    pub client: reqwest::Client,
    pub auth: Arc<AuthConfig>,
    pub identity_resolver: Arc<dyn IdentityResolver>,
    pub plc_directory_url: String,
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ServerDataMiddleware {
    state: Arc<ServerState>,
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
        depot.insert("state", self.state.clone());
        ctrl.call_next(req, depot, res).await;
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

    let config = AppConfig::parse();
    let default_policy = include_str!("../../../policies/arbiter/access-levels.rego");
    let server_did = format!("did:web:{}", config.hostname.replace(':', "%3A"));

    tracing::info!(
        "Starting arbiter server v2 on {} (DID: {})",
        config.listen,
        server_did
    );

    // Identity resolver
    let resolver_client = reqwest::Client::new();
    let dns_resolver = HickoryDnsResolver::create_resolver(&[]);
    let identity_resolver = SharedIdentityResolver(Arc::new(InnerIdentityResolver {
        dns_resolver: Arc::new(dns_resolver),
        http_client: resolver_client,
        plc_hostname: config.plc_directory_url.clone(),
    }));
    let identity_resolver = Arc::new(identity_resolver) as Arc<dyn IdentityResolver>;
    let auth = Arc::new(
        AuthConfig::new(identity_resolver.clone())
            .with_unsafe_token_if(config.unsafe_auth_token.clone()),
    );

    // Load persisted state
    let persister = Persister::new(config.data_dir.clone());
    let mut collection = ArbiterCollection::new();
    if let Ok(snapshot) = persister.load_all() {
        tracing::info!("Loaded {} arbiters from disk", snapshot.arbiters.len());
        collection.load_snapshot(snapshot);
    } else {
        tracing::info!("No existing state found, starting fresh");
    }

    // Load app-password arbiters (PDS accounts) from separate file
    if let Ok(pds_snapshot) = persister.load_pds_arbiters() {
        tracing::info!(
            "Loaded {} app-password arbiters from disk",
            pds_snapshot.arbiters.len()
        );
        collection.load_pds_snapshot(pds_snapshot);
    } else {
        tracing::info!("No existing app-password arbiters found");
    }

    let state = Arc::new(ServerState {
        arbiters: tokio::sync::Mutex::new(collection),
        default_policy,
        server_did,
        client: reqwest::Client::new(),
        auth: auth.clone(),
        identity_resolver: identity_resolver.clone(),
        plc_directory_url: config.plc_directory_url.clone(),
    });

    // Persistence loop
    let state_clone = state.clone();
    let persister_clone = persister.clone();
    let interval = config.persist_interval_secs;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            {
                let coll = state_clone.arbiters.lock().await;
                let snapshot = coll.snapshot();
                if let Err(e) = persister_clone.save_all(&snapshot) {
                    tracing::error!("Failed to persist state: {e}");
                }
                let pds_snapshot = coll.pds_snapshot();
                if let Err(e) = persister_clone.save_pds_arbiters(&pds_snapshot) {
                    tracing::error!("Failed to persist PDS arbiters: {e}");
                }
            }
        }
    });

    let router = build_router(state.clone(), auth);
    tracing::info!("Listening on {}", config.listen);
    let acceptor = TcpListener::new(&config.listen).bind().await;
    Server::new(acceptor).serve(router).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

fn build_router(state: Arc<ServerState>, auth_config: Arc<AuthConfig>) -> Router {
    let auth_middleware = auth::AuthMiddleware::new(auth_config);

    Router::new()
        .hoop(ServerDataMiddleware {
            state: state.clone(),
        })
        .push(
            Router::with_path("/xrpc/{**rest}")
                .hoop(auth_middleware)
                .post(handlers::handle_xrpc)
                .get(handlers::handle_xrpc),
        )
        .get(index)
}

#[handler]
async fn index(res: &mut Response) {
    res.render(Json(serde_json::json!({
        "service": "muni-town-arbiter",
        "version": "2"
    })));
}
