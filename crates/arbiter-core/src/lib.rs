use std::{
  collections::{HashMap, HashSet},
  hash::Hash,
};

use serde::Deserialize;

#[cfg(test)]
mod mbt;

pub type Did = String;
pub type ArbiterDid = Did;
pub type UserDid = Did;
pub type RoleId = String;
pub type SpaceKey = String;

#[derive(Default, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ArbiterService {
  arbiters: HashMap<ArbiterDid, Arbiter>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Arbiter {
  pub did: ArbiterDid,
  pub admins: HashSet<UserDid>,
  pub roles: HashMap<RoleId, HashSet<UserDid>>,
  pub spaces: HashMap<SpaceKey, PermSpace>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PermSpace {
  pub members: HashMap<Member, Access>,
}

#[derive(Deserialize, Hash, PartialEq, Eq, Clone, Debug)]
#[serde(tag = "tag", content = "value")]
pub enum Member {
  User(UserDid),
  Role(RoleId),
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "tag")]
pub enum Access {
  Read,
  Write,
}

impl ArbiterService {
  pub fn create(&mut self, user_did: UserDid, arbiter_did: ArbiterDid) -> anyhow::Result<()> {
    if !self.arbiters.contains_key(&arbiter_did) {
      anyhow::bail!("Arbiter already exists");
    }

    self.arbiters.insert(
      arbiter_did.clone(),
      Arbiter {
        did: arbiter_did,
        admins: HashSet::from_iter([user_did]),
        roles: HashMap::default(),
        spaces: HashMap::default(),
      },
    );

    Ok(())
  }
}
