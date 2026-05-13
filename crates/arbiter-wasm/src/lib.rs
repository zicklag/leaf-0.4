//! WebAssembly bindings for the arbiter. Paper-thin wrapper — zero business
//! logic, only type conversion. Uses `tsify` (js feature) to auto-generate
//! TypeScript definitions via wasm-bindgen's typescript_custom_section.
//! All view types derive Tsify + into_wasm_abi/from_wasm_abi so they
//! cross the wasm boundary as proper typed objects (via serde-wasm-bindgen).

use arbiter_core::core::*;
use arbiter_core::server::*;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// View types (Tsify → auto-generated TS + serde-wasm-bindgen serialization)
// ---------------------------------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS: &'static str = r#"
  type UserDid = string;
  type ArbiterDid = string;
  type SpaceKey = string;
  type JobId = number;
"#;

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MemberEntryView {
  pub member_type: String,
  pub value: String,
  pub access: Access,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SpaceConfigView {
  pub public_records: bool,
  pub public_members: bool,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SpaceView {
  pub key: String,
  pub config: SpaceConfigView,
  pub members: Vec<MemberEntryView>,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ArbiterView {
  pub did: String,
  pub version: i64,
  pub spaces: Vec<SpaceView>,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PendingJobView {
  pub id: i64,
  pub user_did: String,
  pub space_key: String,
  pub args_type: String,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ServerStateView {
  pub time: i64,
  pub arbiters: Vec<ArbiterView>,
  pub pending_jobs: Vec<PendingJobView>,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MissingSpaceView {
  pub arbiter_did: String,
  pub space_key: String,
  pub access: Access,
}

#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(tag = "effectType", rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum EffectView {
  Respond {
    req_id: i64,
    ok: bool,
    member_list: Vec<MemberEntryView>,
    missing_spaces: Vec<MissingSpaceView>,
    error: String,
  },
  SendMessage {
    to_did: String,
    arbiter_did: String,
    space_key: String,
    src_job_id: i64,
    resolver_depth: i64,
  },
  ArbiterChanged {
    arbiter_did: String,
  },
  ArbiterDeleted {
    arbiter_did: String,
  },
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct ArbiterEngine {
  server: Server,
  next_job_id: i64,
}

#[wasm_bindgen]
impl ArbiterEngine {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    ArbiterEngine {
      server: Server::default(),
      next_job_id: 1,
    }
  }

  /// Process a JSON-serialized Message. Returns typed EffectView[].
  pub fn handle_message(&mut self, mut msg: Message) -> Result<Vec<EffectView>, JsValue> {
    if msg.src_job_id == 0 {
      msg.src_job_id = self.next_job_id;
      self.next_job_id += 1;
    }
    let (new_server, effects) = self.server.handle_message(&msg);
    self.server = new_server;
    Ok(effects.into_iter().map(to_effect).collect())
  }

  pub fn tick(&mut self) -> Result<Vec<EffectView>, JsValue> {
    let (new_server, effects) = self.server.tick();
    self.server = new_server;
    Ok(effects.into_iter().map(to_effect).collect())
  }

  pub fn get_state(&self) -> Result<ServerStateView, JsValue> {
    Ok(build_state(&self.server))
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_effect(effect: ServerEffect) -> EffectView {
  match effect {
    ServerEffect::Respond { req_id, result } => {
      let (ok, member_list, missing_spaces, error) = match result {
        Ok(JobResult::Ok) => (true, vec![], vec![], String::new()),
        Ok(JobResult::ResolvedMembersList(list)) => {
          let members: Vec<_> = list
            .member_list
            .iter()
            .map(|(did, a)| MemberEntryView {
              member_type: "User".into(),
              value: did.clone(),
              access: a.clone(),
            })
            .collect();
          let missing: Vec<_> = list
            .missing_spaces
            .iter()
            .map(|(id, a)| MissingSpaceView {
              arbiter_did: id.arbiter_did.clone(),
              space_key: id.space_key.clone(),
              access: a.clone(),
            })
            .collect();
          (true, members, missing, String::new())
        }
        Err(e) => (false, vec![], vec![], format!("{:?}", e)),
      };
      EffectView::Respond {
        req_id,
        ok,
        member_list,
        missing_spaces,
        error,
      }
    }
    ServerEffect::SendMessage { to_did, msg } => EffectView::SendMessage {
      to_did,
      arbiter_did: msg.arbiter_did,
      space_key: msg.space_key,
      src_job_id: msg.src_job_id,
      resolver_depth: msg.resolver_depth,
    },
    ServerEffect::ArbiterChanged { arbiter_did } => EffectView::ArbiterChanged { arbiter_did },
    ServerEffect::ArbiterDeleted { arbiter_did } => EffectView::ArbiterDeleted { arbiter_did },
  }
}

fn build_state(server: &Server) -> ServerStateView {
  let mut dids: Vec<&ArbiterDid> = server.arbiters.keys().collect();
  dids.sort();
  let arbiters: Vec<_> = dids
    .iter()
    .map(|did| {
      let a = &server.arbiters[*did];
      let mut skeys: Vec<&SpaceKey> = a.spaces.keys().collect();
      skeys.sort();
      let spaces: Vec<_> = skeys
        .iter()
        .map(|key| {
          let s = &a.spaces[*key];
          let mut entries: Vec<_> = s
            .members
            .iter()
            .map(|(m, acc)| {
              let (mt, v) = match m {
                Member::User(d) => ("User".into(), d.clone()),
                Member::LocalSpace(k) => ("LocalSpace".into(), k.clone()),
                Member::RemoteSpace(id) => (
                  "RemoteSpace".into(),
                  format!("{}:{}", id.arbiter_did, id.space_key),
                ),
              };
              MemberEntryView {
                member_type: mt,
                value: v,
                access: *acc,
              }
            })
            .collect();
          entries.sort_by(|a, b| a.value.cmp(&b.value));
          SpaceView {
            key: (*key).clone(),
            config: SpaceConfigView {
              public_records: s.config.public_records,
              public_members: s.config.public_members,
            },
            members: entries,
          }
        })
        .collect();
      ArbiterView {
        did: (*did).clone(),
        version: a.version,
        spaces,
      }
    })
    .collect();

  let mut pending: Vec<_> = vec![];
  for (_, a) in &server.arbiters {
    let mut q: Vec<&JobId> = a.job_queue.keys().collect();
    q.sort();
    for jid in q {
      let j = &a.job_queue[jid];
      pending.push(PendingJobView {
        id: *jid,
        user_did: j.user_did.clone(),
        space_key: j.space_key.clone(),
        args_type: format!("{:?}", j.args),
      });
    }
  }
  pending.sort_by(|a, b| a.id.cmp(&b.id));
  ServerStateView {
    time: server.time,
    arbiters,
    pending_jobs: pending,
  }
}
