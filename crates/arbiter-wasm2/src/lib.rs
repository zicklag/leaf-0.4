//! WebAssembly bindings for arbiter-core2.
//!
//! Provides a `SimulationEngine` that wraps the sans-IO core state machine
//! for use in the browser-based arbiter simulator. All complex types cross
//! the WASM boundary as JSON strings.

use arbiter_core2::core::*;
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
}

#[wasm_bindgen]
impl SimulationEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            state: ServerState::default(),
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
    pub fn process_operation(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args_json: &str,
        resolved_remotes_json: &str,
    ) -> String {
        let args: JobArgs = match serde_json::from_str(args_json) {
            Ok(a) => a,
            Err(e) => {
                return serde_json::json!({"status": "error", "error": e.to_string()}).to_string()
            }
        };
        let resolved_remotes: serde_json::Value = match serde_json::from_str(resolved_remotes_json)
        {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({"status": "error", "error": e.to_string()}).to_string()
            }
        };

        let mut arbiter = match self.state.arbiters.get(arbiter_did) {
            Some(a) => a.clone(),
            None => return serde_json::json!({"status": "error", "error": "ArbiterNotExists"}).to_string(),
        };

        arbiter.process_operation(user_did, space_key, args, &resolved_remotes);

        let result = operation_result_json(&arbiter.result);
        apply_arbiter_result(&mut self.state, arbiter_did, arbiter);
        result
    }

    /// Provide resolved remote members for a queued job.
    pub fn provide_resolved_remotes(
        &mut self,
        arbiter_did: &str,
        job_id: i64,
        resolved_remotes_json: &str,
    ) -> String {
        let resolved_remotes: serde_json::Value = match serde_json::from_str(resolved_remotes_json)
        {
            Ok(r) => r,
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

        arbiter.provide_resolved_remotes(job_id, &resolved_remotes);

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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn operation_result_json(result: &ArbiterResult) -> String {
    let json = match result {
        ArbiterResult::Ok => serde_json::json!({"status": "ok"}),
        ArbiterResult::NeedsResolution { job_id, spaces } => {
            serde_json::json!({
                "status": "needsResolution",
                "jobId": job_id,
                "spaces": spaces,
            })
        }
        ArbiterResult::Finished(r) => match r {
            JobResult::Ok => serde_json::json!({"status": "ok"}),
            JobResult::ResolvedMembersList(response) => {
                // response is { members: [...], missingSpaces: [...] }
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
