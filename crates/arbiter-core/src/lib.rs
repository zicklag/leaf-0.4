use std::{
  collections::{BTreeMap, BTreeSet},
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
  arbiters: BTreeMap<ArbiterDid, Arbiter>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Arbiter {
  pub did: ArbiterDid,
  pub admins: BTreeSet<UserDid>,
  pub roles: BTreeMap<RoleId, BTreeSet<UserDid>>,
  pub spaces: BTreeMap<SpaceKey, PermSpace>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct PermSpace {
  pub members: BTreeMap<Member, Access>,
}

#[derive(Deserialize, Hash, PartialEq, Eq, Clone, Debug, PartialOrd, Ord)]
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
    if self.arbiters.contains_key(&arbiter_did) {
      anyhow::bail!("Arbiter already exists");
    }

    self.arbiters.insert(
      arbiter_did.clone(),
      Arbiter {
        did: arbiter_did,
        admins: BTreeSet::from_iter([user_did]),
        roles: BTreeMap::default(),
        spaces: BTreeMap::default(),
      },
    );

    Ok(())
  }

  pub fn add_admin(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    new_admin: UserDid,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    arbiter.admins.insert(new_admin);

    Ok(())
  }

  pub fn remove_admin(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    admin: UserDid,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    if !arbiter.admins.remove(&admin) {
      anyhow::bail!("User is not an admin");
    }
    Ok(())
  }

  pub fn create_space(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    space_key: SpaceKey,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    if arbiter.spaces.contains_key(&space_key) {
      anyhow::bail!("Space already exists");
    }
    arbiter.spaces.insert(space_key, PermSpace::default());
    Ok(())
  }

  pub fn set_space_member_access(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    space_key: SpaceKey,
    member: Member,
    access: Access,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    let Some(space) = arbiter.spaces.get_mut(&space_key) else {
      anyhow::bail!("Space does not exist");
    };
    space.members.insert(member, access);
    Ok(())
  }

  pub fn remove_space_member(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    space_key: SpaceKey,
    member: Member,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    let Some(space) = arbiter.spaces.get_mut(&space_key) else {
      anyhow::bail!("Space does not exist");
    };
    if space.members.remove(&member).is_none() {
      anyhow::bail!("`{member:?}` was not a member of space `{space_key}`");
    }
    Ok(())
  }

  pub fn create_role(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    role_id: RoleId,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    if arbiter.roles.contains_key(&role_id) {
      anyhow::bail!("Role already exists");
    }
    arbiter.roles.insert(role_id, Default::default());

    Ok(())
  }

  pub fn add_role_member(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    role_id: RoleId,
    member: UserDid,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    let Some(role) = arbiter.roles.get_mut(&role_id) else {
      anyhow::bail!("Role already exists");
    };
    role.insert(member);
    Ok(())
  }

  pub fn remove_role_member(
    &mut self,
    user_did: UserDid,
    arbiter_did: ArbiterDid,
    role_id: RoleId,
    member: UserDid,
  ) -> anyhow::Result<()> {
    let Some(arbiter) = self.arbiters.get_mut(&arbiter_did) else {
      anyhow::bail!("Arbiter does not exist");
    };
    if !arbiter.admins.contains(&user_did) {
      anyhow::bail!("User must be admin to add admin");
    }
    let Some(role) = arbiter.roles.get_mut(&role_id) else {
      anyhow::bail!("Role already exists");
    };
    if !role.remove(&member) {
      anyhow::bail!("Member `{member}` not in role `{role_id}`");
    }
    Ok(())
  }
}
