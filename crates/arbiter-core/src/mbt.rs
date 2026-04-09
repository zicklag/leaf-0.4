use std::collections::BTreeMap;

use quint_connect::*;
use serde::Deserialize;

use crate::{Arbiter, ArbiterDid, ArbiterService};

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
struct ArbiterState {
  arbiters: BTreeMap<ArbiterDid, Arbiter>,
  error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
struct ArbiterDriver {
  service: ArbiterService,
  error: bool,
}

impl State<ArbiterDriver> for ArbiterState {
  fn from_driver(driver: &ArbiterDriver) -> Result<Self> {
    Ok(Self {
      arbiters: driver.service.arbiters.clone(),
      error: driver.error,
    })
  }
}

impl Driver for ArbiterDriver {
  type State = ArbiterState;

  #[allow(unused, non_snake_case)]
  fn step(&mut self, step: &Step) -> Result {
    macro_rules! call {
        ($f:ident, $($args:ident),*) => {
          let error = self.service.$f($($args),*).is_err();
          self.error = error;
        };
    }
    switch!(step {
        init => std::mem::take(self),
        createArbiterAny(userDid, arbiterDid) => call!(create, userDid, arbiterDid),
        addArbiterAdminAny(userDid, arbiterDid, newAdminDid) => call!(add_admin, userDid, arbiterDid, newAdminDid),
        removeArbiterAdminAny(userDid, arbiterDid, removedAdmin) => call!(remove_admin, userDid, arbiterDid, removedAdmin),
        createSpaceAny(userDid, arbiterDid, spaceKey) => call!(create_space, userDid, arbiterDid, spaceKey),
        setSpaceMemberAccessAny(userDid, arbiterDid, spaceKey, member, access) => call!(set_space_member_access, userDid, arbiterDid, spaceKey, member, access),
        removeSpaceMemberAny(userDid, arbiterDid, spaceKey, member) => call!(remove_space_member, userDid, arbiterDid, spaceKey, member),
        createRoleAny(userDid, arbiterDid, roleId) => call!(create_role, userDid, arbiterDid, roleId),
        addRoleMemberAny(userDid, arbiterDid, roleId, member) => call!(add_role_member, userDid, arbiterDid, roleId, member),
        removeRoleMemberAny(userDid, arbiterDid, roleId, member) => call!(remove_role_member, userDid, arbiterDid, roleId, member),
    })
  }
}

// Run multiple traces in simulation mode
#[test]
fn simulation() {
  let driver = { ArbiterDriver::default() };
  let config = quint_connect::runner::Config {
    test_name: "simulation".to_string(),
    gen_config: quint_connect::runner::RunConfig {
      spec: "./spec/arbiter.qnt".to_string(),
      main: None,
      init: None,
      step: None,
      max_samples: Some(250),
      max_steps: Some(150),
      seed: quint_connect::runner::gen_random_seed().to_string(),
    },
  };
  if let Err(err) = quint_connect::runner::run_test(driver, config) {
    panic!("{err:#?}")
  }
}
