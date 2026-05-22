//! WebAssembly bindings for arbiter-core2.
//!
//! Provides a `SimulationEngine` that wraps the sans-IO core state machine
//! for use in the browser-based arbiter simulator. All complex types cross
//! the WASM boundary as JSON strings.
//!
//! The simulator auto-resolves remote spaces internally because all arbiters
//! are in-memory (no actual network IO).

use arbiter_core2::core::*;
use arbiter_core2::policy_vm::PolicyVmPool;
use serde_json::json;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// SimulationEngine
// ---------------------------------------------------------------------------

/// The core simulation engine running in the browser.
///
/// Manages multiple arbiters with their policies and provides methods
/// for the Svelte UI to drive the simulation.
#[wasm_bindgen]
pub struct SimulationEngine {
    state: ServerState,
    vm_pool: PolicyVmPool,
}

#[wasm_bindgen]
impl SimulationEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            state: ServerState::default(),
            vm_pool: PolicyVmPool::new(),
        }
    }

    /// Create a new arbiter.
    /// `config_json` must be valid JSON with `$type` and `policy` fields.
    pub fn create_arbiter(
        &mut self,
        arbiter_did: &str,
        owner_did: &str,
        config_json: &str,
    ) -> Result<(), JsValue> {
        let config: serde_json::Value =
            serde_json::from_str(config_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let arbiter = Arbiter::new(
            arbiter_did.to_string(),
            owner_did.to_string(),
            config,
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.state.arbiters = self
            .state
            .arbiters
            .update(arbiter_did.to_string(), arbiter);
        Ok(())
    }

    /// Process an operation on an arbiter and returns JSON result.
    ///
    /// If the operation needs remote resolution, this method auto-resolves
    /// all remotes (all arbiters are in-memory) and returns the final result.
    pub fn process_operation(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args_json: &str,
    ) -> String {
        let args: JobArgs = match serde_json::from_str(args_json) {
            Ok(a) => a,
            Err(e) => {
                return serde_json::json!({"status": "error", "error": e.to_string()}).to_string()
            }
        };

        let mut arbiter = match self.state.arbiters.get(arbiter_did) {
            Some(a) => a.clone(),
            None => {
                return serde_json::json!({"status": "error", "error": "ArbiterNotExists"})
                    .to_string()
            }
        };

        // Start the operation
        arbiter.process_operation(user_did, space_key, args, &mut self.vm_pool);

        // Auto-resolve loop: for each suspension, resolve the remote and resume
        loop {
            match arbiter.result.clone() {
                ArbiterResult::Suspended { job_id, request } => {
                    let resolved = match request.kind {
                        SuspensionKind::ResolveRemote => {
                            let remote_did = request
                                .remote_arbiter_did
                                .as_deref()
                                .unwrap_or("");
                            let remote_key = request
                                .space_key
                                .as_deref()
                                .unwrap_or("");
                            // Resolve as the calling arbiter's DID (server-to-server)
                            self.resolve_remote_members(remote_did, arbiter_did, remote_key)
                        }
                    };

                    arbiter.resume_operation(job_id, &resolved, &mut self.vm_pool);
                }
                _ => {
                    // Completed, error, or deleted
                    break;
                }
            }
        }

        let result = operation_result_json(&arbiter.result);
        apply_arbiter_result(&mut self.state, arbiter_did, arbiter);
        result
    }

    /// Validate a Rego policy string. Returns empty string if valid, error message if invalid.
    pub fn validate_policy(&self, policy: &str) -> String {
        match arbiter_core2::policy::validate_policy(policy) {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        }
    }

    /// Update ALL existing arbiters' configs with a new policy.
    /// This bypasses normal authorization — it's a super-admin operation
    /// for the simulator UI.
    pub fn update_all_policies(&mut self, policy: &str) -> Result<(), JsValue> {
        let new_config = serde_json::json!({
            "$type": "town.muni.arbiter.config.regoPolicy",
            "policy": policy,
        });

        for (did, arbiter) in self.state.arbiters.clone().iter() {
            let mut updated = arbiter.clone();
            updated.config = new_config.clone();
            updated.version = updated.version.wrapping_add(1);
            self.state.arbiters = self.state.arbiters.update(did.clone(), updated);
        }

        Ok(())
    }

    /// Get the default policy JSON for creating new arbiters.
    pub fn get_default_policy_config(&self) -> String {
        serde_json::json!({
            "$type": "town.muni.arbiter.config.regoPolicy",
            "policy": DEFAULT_POLICY,
        })
        .to_string()
    }

    /// Get the full server state as JSON for the UI.
    pub fn get_state(&self) -> String {
        let arbiters: Vec<serde_json::Value> = self
            .state
            .arbiters
            .iter()
            .map(|(did, a)| {
                let spaces: Vec<serde_json::Value> = a
                    .spaces
                    .iter()
                    .map(|(key, s)| {
                        let members: Vec<serde_json::Value> = s
                            .members
                            .iter()
                            .map(|(m, access)| {
                                let (tag, value) = match m {
                                    Member::MemberDid(d) => ("MemberDid", serde_json::json!(d)),
                                    Member::MemberLocalSpace(k) => {
                                        ("MemberLocalSpace", serde_json::json!(k))
                                    }
                                    Member::MemberRemoteSpace(id) => (
                                        "MemberRemoteSpace",
                                        serde_json::json!({
                                            "arbiterDid": id.arbiter_did,
                                            "spaceKey": id.space_key,
                                        }),
                                    ),
                                };
                                serde_json::json!({
                                    "member": { "tag": tag, "value": value },
                                    "access": access,
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "key": key,
                            "spaceType": s.space_type,
                            "config": s.config,
                            "members": members,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "did": did,
                    "version": a.version,
                    "spaces": spaces,
                })
            })
            .collect();

        serde_json::json!({
            "time": self.state.time,
            "arbiters": arbiters,
        })
        .to_string()
    }

    // -----------------------------------------------------------------------
    // Internal: remote resolution (all in-memory for the simulator)
    // -----------------------------------------------------------------------

    /// Resolve members of a space on another arbiter.
    fn resolve_remote_members(
        &mut self,
        arbiter_did: &str,
        as_arbiter: &str,
        space_key: &str,
    ) -> serde_json::Value {
        let mut arbiter = match self.state.arbiters.get(arbiter_did) {
            Some(a) => a.clone(),
            None => return serde_json::json!([]),
        };

        arbiter.process_operation(
            as_arbiter,
            space_key,
            JobArgs::ResolveMembers,
            &mut self.vm_pool,
        );

        // Auto-resolve any suspensions for this remote call too
        loop {
            match arbiter.result.clone() {
                ArbiterResult::Suspended { job_id, request } => {
                    let resolved = match request.kind {
                        SuspensionKind::ResolveRemote => {
                            let remote_did = request.remote_arbiter_did.as_deref().unwrap_or("");
                            let remote_key = request.space_key.as_deref().unwrap_or("");
                            self.resolve_remote_members(remote_did, arbiter_did, remote_key)
                        }
                    };
                    arbiter.resume_operation(job_id, &resolved, &mut self.vm_pool);
                }
                _ => break,
            }
        }

        let result = match &arbiter.result {
            ArbiterResult::Finished(JobResult::ResolvedMembersList(response)) => {
                response.get("members").cloned().unwrap_or(json!([]))
            }
            _ => json!([]),
        };

        self.state.arbiters = self
            .state
            .arbiters
            .update(arbiter_did.to_string(), arbiter);
        result
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn operation_result_json(result: &ArbiterResult) -> String {
    let json = match result {
        ArbiterResult::Ok => serde_json::json!({"status": "ok"}),
        ArbiterResult::Suspended { job_id, request } => {
            serde_json::json!({
                "status": "suspended",
                "jobId": job_id,
                "request": request,
            })
        }
        ArbiterResult::Finished(r) => match r {
            JobResult::Ok => serde_json::json!({"status": "ok"}),
            JobResult::ResolvedMembersList(response) => {
                let mut obj = serde_json::json!({"status": "ok"});
                if let Some(m) = response.get("members") {
                    obj["members"] = m.clone();
                }
                if let Some(m) = response.get("missingSpaces") {
                    obj["missingSpaces"] = m.clone();
                }
                obj
            }
        },
        ArbiterResult::Deleted => serde_json::json!({"status": "deleted"}),
        ArbiterResult::Err(e) => {
            serde_json::json!({"status": "error", "error": format!("{:?}", e.kind)})
        }
    };
    json.to_string()
}

fn apply_arbiter_result(
    state: &mut ServerState,
    arbiter_did: &str,
    arbiter: Arbiter,
) {
    match &arbiter.result {
        ArbiterResult::Deleted => {
            state.arbiters = state.arbiters.without(arbiter_did);
        }
        _ => {
            state.arbiters = state.arbiters.update(arbiter_did.to_string(), arbiter);
        }
    }
}
