//! Async wrapper for the sans-IO arbiter server.
//!
//! Provides `ArbiterIo` trait (for external IO like remote resolution) and
//! `AsyncArbiterServer` which drives the server state machine with async glue.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, oneshot, mpsc};

use im::HashMap;

use crate::core::*;
use crate::server::*;

// ---------------------------------------------------------------------------
// ArbiterIo trait
// ---------------------------------------------------------------------------

/// IO operations needed by the arbiter server.
///
/// Implementations provide the actual network/HTTP calls to resolve remote
/// space member lists on other arbiters.
#[async_trait]
pub trait ArbiterIo: Send + Sync + 'static {
    /// Resolve the member list for a space on a remote arbiter.
    ///
    /// `arbiter_did` is the DID of the remote arbiter to query.
    /// `space_key` is the space to resolve.
    /// `resolver_depth` is the remaining resolution depth.
    ///
    /// Returns the resolved member list on success, or an error string.
    async fn resolve_remote_members(
        &self,
        arbiter_did: &str,
        space_key: &str,
        resolver_depth: i64,
    ) -> Result<ResolvedMemberList, String>;
}

// ---------------------------------------------------------------------------
// Internal message types
// ---------------------------------------------------------------------------

/// Messages sent on the effect processing channel.
enum InternalMsg {
    /// Process a batch of effects from the server.
    Effects(Vec<ServerEffect>),
}

// ---------------------------------------------------------------------------
// AsyncArbiterServer
// ---------------------------------------------------------------------------

/// Async wrapper around the sans-IO `Server`.
///
/// Drives the server state machine, manages pending requests via oneshot
/// channels, and processes effects (remote resolution, persistence triggers)
/// asynchronously.
pub struct AsyncArbiterServer<Io: ArbiterIo> {
    /// The sans-IO server state (locked for each mutation).
    server: Mutex<Server>,
    /// The IO implementation for remote resolution.
    io: Arc<Io>,
    /// Monotonically increasing request ID counter.
    next_req_id: AtomicU64,
    /// Pending requests — maps JobId to oneshot sender.
    pending_requests: Mutex<std::collections::HashMap<JobId, oneshot::Sender<Result<JobResult, ServerError>>>>,
    /// Channel sender for effect processing.
    effect_tx: mpsc::UnboundedSender<InternalMsg>,
    /// Track which arbiters have changed state and need persistence.
    dirty_arbiters: Mutex<HashSet<ArbiterDid>>,
    /// Tick interval for the background task.
    tick_interval: Duration,
}

impl<Io: ArbiterIo> AsyncArbiterServer<Io> {
    /// Create a new `AsyncArbiterServer` with the default tick interval (100ms).
    pub fn new(io: Io) -> Arc<Self> {
        Self::with_tick_interval(io, Duration::from_millis(2000))
    }

    /// Create a new `AsyncArbiterServer` with a custom tick interval.
    pub fn with_tick_interval(io: Io, tick_interval: Duration) -> Arc<Self> {
        let (effect_tx, effect_rx) = mpsc::unbounded_channel();

        let this = Arc::new(Self {
            server: Mutex::new(Server::default()),
            io: Arc::new(io),
            next_req_id: AtomicU64::new(1),
            pending_requests: Mutex::new(std::collections::HashMap::new()),
            effect_tx,
            dirty_arbiters: Mutex::new(HashSet::new()),
            tick_interval,
        });

        // Start the effect processor with an Arc clone
        let this_clone = this.clone();
        tokio::spawn(async move {
            this_clone.effect_processor_loop(effect_rx).await;
        });

        this
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Submit a request to the arbiter server and wait for the result.
    ///
    /// This allocates a unique JobId, sends the message to the server state
    /// machine, and waits for the job to complete. If the job requires remote
    /// resolution, this future will not resolve until all remote resolutions
    /// have been fed back into the server.
    pub async fn handle_request(
        self: &Arc<Self>,
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        resolver_depth: i64,
        kind: MessageKind,
    ) -> Result<JobResult, ServerError> {
        let req_id = self.next_req_id.fetch_add(1, Ordering::SeqCst) as JobId;

        // Create the oneshot channel
        let (tx, rx) = oneshot::channel();

        // Store the sender
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(req_id, tx);
        }

        // Build the message
        let msg = Message {
            user_did: user_did.to_string(),
            arbiter_did: arbiter_did.to_string(),
            space_key: space_key.to_string(),
            src_job_id: req_id,
            resolver_depth,
            kind,
        };

        // Process the message through the server state machine
        let effects = {
            let mut server = self.server.lock().await;
            let (new_server, effects) = server.handle_message(&msg);
            *server = new_server;
            effects
        };

        // Submit effects for processing
        if !effects.is_empty() {
            let _ = self.effect_tx.send(InternalMsg::Effects(effects));
        }

        // Wait for the response
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(ServerError::ArbiterErr(ArbiterError::JobNotExists)),
        }
    }

    /// Background task that periodically ticks the server.
    ///
    /// The caller should spawn this on their async executor:
    ///
    /// ```ignore
    /// tokio::spawn(arbiter_server.clone().background_task());
    /// ```
    pub async fn background_task(self: Arc<Self>) {
        loop {
            tokio::time::sleep(self.tick_interval).await;

            let effects = {
                let mut server = self.server.lock().await;
                let (new_server, effects) = server.tick();
                *server = new_server;
                effects
            };

            if !effects.is_empty() {
                let _ = self.effect_tx.send(InternalMsg::Effects(effects));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Persistence helpers
    // -----------------------------------------------------------------------

    /// Drain the set of arbiters whose state has changed since the last call.
    pub async fn drain_dirty_arbiters(&self) -> HashSet<ArbiterDid> {
        let mut dirty = self.dirty_arbiters.lock().await;
        std::mem::take(&mut *dirty)
    }

    /// Get a `PersistentArbiter` snapshot of a specific arbiter.
    pub async fn snapshot_arbiter(&self, arbiter_did: &str) -> Option<PersistentArbiter> {
        let server = self.server.lock().await;
        server.persistent_arbiter(&arbiter_did.to_string())
    }

    /// Get snapshots of all arbiters (for initial persistence / reload).
    pub async fn get_all_arbiter_states(&self) -> HashMap<ArbiterDid, PersistentArbiter> {
        let server = self.server.lock().await;
        server.all_persistent_arbiters()
    }

    /// Load a persistent arbiter state into the server.
    pub async fn load_persistent_arbiter(&self, persistent: PersistentArbiter) {
        let mut server = self.server.lock().await;
        *server = server.load_persistent_arbiter(persistent);
    }

    /// Load all persistent arbiter states at once.
    pub async fn load_all(&self, arbiters: std::collections::HashMap<ArbiterDid, PersistentArbiter>) {
        let mut server = self.server.lock().await;
        for (_, pa) in arbiters {
            *server = server.load_persistent_arbiter(pa);
        }
    }

    // -----------------------------------------------------------------------
    // Internal: effect processor
    // -----------------------------------------------------------------------

    /// The effect processor loop. Runs as a single tokio task, receiving
    /// effect batches from the channel and processing them one by one.
    /// For `SendMessage` effects, it spawns sub-tasks for the IO.
    async fn effect_processor_loop(
        self: Arc<Self>,
        mut effect_rx: mpsc::UnboundedReceiver<InternalMsg>,
    ) {
        while let Some(msg) = effect_rx.recv().await {
            match msg {
                InternalMsg::Effects(effects) => {
                    self.handle_effects_batch(effects).await;
                }
            }
        }
    }

    /// Process a batch of effects from the server. This is called from the
    /// effect processor loop.
    async fn handle_effects_batch(self: &Arc<Self>, effects: Vec<ServerEffect>) {
        for effect in effects {
            match effect {
                ServerEffect::Respond { req_id, result } => {
                    let mut pending = self.pending_requests.lock().await;
                    if let Some(sender) = pending.remove(&req_id) {
                        let _ = sender.send(result);
                    }
                }

                ServerEffect::SendMessage { to_did, msg } => {
                    let this = self.clone();
                    let io = self.io.clone();
                    tokio::spawn(async move {
                        let result = io
                            .resolve_remote_members(
                                &to_did,
                                &msg.space_key,
                                msg.resolver_depth,
                            )
                            .await;

                        let resolved = match result {
                            Ok(r) => r,
                            Err(_) => {
                                // On error, return an empty member list with the
                                // unresolved space as missing
                                let mut missing = HashMap::new();
                                missing.insert(
                                    SpaceId {
                                        arbiter_did: to_did.clone(),
                                        space_key: msg.space_key.clone(),
                                    },
                                    Access::ReadMemberList,
                                );
                                ResolvedMemberList {
                                    member_list: HashMap::new(),
                                    missing_spaces: missing,
                                }
                            }
                        };

                        let reply = Message {
                            user_did: msg.user_did.clone(),
                            arbiter_did: to_did.clone(),
                            space_key: msg.space_key.clone(),
                            src_job_id: msg.src_job_id,
                            resolver_depth: msg.resolver_depth - 1,
                            kind: MessageKind::ReplyResolvedMembers { members: resolved },
                        };

                        let effects = {
                            let mut server = this.server.lock().await;
                            let (new_server, effects) = server.handle_message(&reply);
                            *server = new_server;
                            effects
                        };

                        if !effects.is_empty() {
                            let _ = this.effect_tx.send(InternalMsg::Effects(effects));
                        }
                    });
                }

                ServerEffect::ArbiterChanged { arbiter_did } => {
                    let mut dirty = self.dirty_arbiters.lock().await;
                    dirty.insert(arbiter_did);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock IO implementation that returns empty member lists.
    struct MockIo;

    #[async_trait]
    impl ArbiterIo for MockIo {
        async fn resolve_remote_members(
            &self,
            _arbiter_did: &str,
            _space_key: &str,
            _resolver_depth: i64,
        ) -> Result<ResolvedMemberList, String> {
            Ok(ResolvedMemberList {
                member_list: HashMap::new(),
                missing_spaces: HashMap::new(),
            })
        }
    }

    /// A mock IO that fails resolution.
    #[allow(dead_code)]
    struct FailingIo;

    #[async_trait]
    impl ArbiterIo for FailingIo {
        async fn resolve_remote_members(
            &self,
            _arbiter_did: &str,
            _space_key: &str,
            _resolver_depth: i64,
        ) -> Result<ResolvedMemberList, String> {
            Err("Network error".to_string())
        }
    }

    #[tokio::test]
    async fn test_handle_request_fetch_members_on_new_arbiter() {
        let server = AsyncArbiterServer::new(MockIo);

        // Create an arbiter first
        let result = server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await;
        assert!(result.is_ok(), "CreateArbiter should succeed");

        // Now fetch members — should return immediately with alice as owner
        let result = server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::FetchMembers)
            .await;
        assert!(result.is_ok(), "FetchMembers should succeed");
        if let Ok(JobResult::ResolvedMembersList(list)) = result {
            assert!(list.member_list.contains_key("alice"));
            assert_eq!(list.member_list["alice"], Access::Owner);
        } else {
            panic!("Expected ResolvedMembersList");
        }
    }

    #[tokio::test]
    async fn test_handle_request_nonexistent_arbiter() {
        let server = AsyncArbiterServer::new(MockIo);

        let result = server
            .handle_request(
                "alice",
                "did:example:nonexistent",
                "$admin",
                3,
                MessageKind::FetchMembers,
            )
            .await;
        assert!(result.is_err(), "Should return error for nonexistent arbiter");
        assert!(matches!(result, Err(ServerError::ArbiterNotExists)));
    }

    #[tokio::test]
    async fn test_create_duplicate_arbiter() {
        let server = AsyncArbiterServer::new(MockIo);

        let r1 = server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await;
        assert!(r1.is_ok());

        let r2 = server
            .handle_request("bob", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await;
        assert!(r2.is_err());
        assert!(matches!(r2, Err(ServerError::ArbiterAlreadyExists)));
    }

    #[tokio::test]
    async fn test_create_space() {
        let server = AsyncArbiterServer::new(MockIo);

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        let result = server
            .handle_request("alice", "did:example:arb1", "myspace", 3, MessageKind::CreateSpace)
            .await;
        assert!(result.is_ok());
        assert!(matches!(result, Ok(JobResult::Ok)));
    }

    #[tokio::test]
    async fn test_delete_arbiter() {
        let server = AsyncArbiterServer::new(MockIo);

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        let result = server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::DeleteArbiter)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dirty_arbiters_tracking() {
        let server = AsyncArbiterServer::new(MockIo);

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        // Small delay for effect processing
        tokio::time::sleep(Duration::from_millis(50)).await;

        let dirty = server.drain_dirty_arbiters().await;
        assert!(
            dirty.contains("did:example:arb1"),
            "CreateArbiter should mark arbiter dirty"
        );
    }

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        let server = AsyncArbiterServer::new(MockIo);

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        let snapshot = server.snapshot_arbiter("did:example:arb1").await;
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.did, "did:example:arb1");

        let server2 = AsyncArbiterServer::new(MockIo);
        server2.load_persistent_arbiter(snapshot).await;

        let members = server2
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::FetchMembers)
            .await;
        assert!(members.is_ok());
    }

    #[tokio::test]
    async fn test_remote_resolution_queued() {
        let server = AsyncArbiterServer::new(MockIo);

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        server
            .handle_request("alice", "did:example:arb1", "myspace", 3, MessageKind::CreateSpace)
            .await
            .unwrap();

        let remote = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        server
            .handle_request(
                "alice",
                "did:example:arb1",
                "myspace",
                3,
                MessageKind::SetMemberAccess {
                    member: Member::RemoteSpace(remote),
                    access: Access::ReadMemberList,
                },
            )
            .await
            .unwrap();

        let result = server
            .handle_request("alice", "did:example:arb1", "myspace", 3, MessageKind::FetchMembers)
            .await;
        assert!(result.is_ok(), "FetchMembers with remote resolution should succeed");
        if let Ok(JobResult::ResolvedMembersList(list)) = result {
            assert!(list.member_list.contains_key("alice"), "Alice should be in member list");
        }
    }

    #[tokio::test]
    async fn test_background_task_timeout() {
        let server =
            AsyncArbiterServer::with_tick_interval(MockIo, Duration::from_millis(10));

        let bg = server.clone();
        tokio::spawn(async move {
            bg.background_task().await;
        });

        server
            .handle_request("alice", "did:example:arb1", "$admin", 3, MessageKind::CreateArbiter)
            .await
            .unwrap();

        server
            .handle_request("alice", "did:example:arb1", "myspace", 3, MessageKind::CreateSpace)
            .await
            .unwrap();

        let remote = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        server
            .handle_request(
                "alice",
                "did:example:arb1",
                "myspace",
                3,
                MessageKind::SetMemberAccess {
                    member: Member::RemoteSpace(remote),
                    access: Access::ReadMemberList,
                },
            )
            .await
            .unwrap();

        // Fetch with depth 0 so it immediately times out (no remote resolution needed)
        let result = server
            .handle_request("alice", "did:example:arb1", "myspace", 0, MessageKind::FetchMembers)
            .await;
        // Should resolve (timeout produces a result with missing spaces)
        assert!(result.is_ok(), "Fetch with depth 0 should resolve via immediate timeout");
    }
}
