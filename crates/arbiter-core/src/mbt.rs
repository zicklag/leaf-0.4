use quint_connect::*;

use super::ArbiterService;

impl State<ArbiterService> for ArbiterService {
  fn from_driver(driver: &ArbiterService) -> Result<Self> {
    Ok(driver.clone())
  }
}

impl Driver for super::ArbiterService {
  type State = ArbiterService;

  #[allow(unused, non_snake_case)]
  fn step(&mut self, step: &Step) -> Result {
    switch!(step {
        init => std::mem::take(self),
        createArbiterAny(userDid, arbiterDid) => self.create(userDid, arbiterDid)?,
        addArbiterAdminAny(userDid, arbiterDid, newAdminDid) => self.add_admin(userDid, arbiterDid, newAdminDid)?,
        removeArbiterAdminAny(userDid, arbiterDid, removedAdmin) => self.remove_admin(userDid, arbiterDid, removedAdmin)?,
        createSpaceAny(userDid, arbiterDid, spaceKey) => self.create_space(userDid, arbiterDid, spaceKey)?,
        setSpaceMemberAccessAny(userDid, arbiterDid, spaceKey, member, access) => self.set_space_member_access(userDid, arbiterDid, spaceKey, member, access)?,
        removeSpaceMemberAny(userDid, arbiterDid, spaceKey, member) => self.remove_space_member(userDid, arbiterDid, spaceKey, member)?,
        createRoleAny(userDid, arbiterDid, roleId) => self.create_role(userDid, arbiterDid, roleId)?,
        addRoleMemberAny(userDid, arbiterDid, roleId, member) => self.add_role_member(userDid, arbiterDid, roleId, member)?,
        removeRoleMemberAny(userDid, arbiterDid, roleId, member) => self.remove_role_member(userDid, arbiterDid, roleId, member)?,
    })
  }
}

// Run multiple traces in simulation mode
#[quint_run(spec = "./spec/arbiter.qnt")]
fn simulation() -> impl Driver {
  ArbiterService::default()
}
