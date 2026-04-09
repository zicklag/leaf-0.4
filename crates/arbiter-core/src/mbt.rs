use quint_connect::*;

use super::ArbiterService;

impl State<ArbiterService> for ArbiterService {
  fn from_driver(driver: &ArbiterService) -> Result<Self> {
    Ok(driver.clone())
  }
}

impl Driver for super::ArbiterService {
  type State = ArbiterService;

  #[allow(unused)]
  fn step(&mut self, step: &Step) -> Result {
    switch!(step {
        init => *self == Default::default(),
        create(user_did, arbiter_did) => self.create(user_did, arbiter_did),
    })
  }
}

// Run multiple traces in simulation mode
#[quint_run(spec = "../spec/arbiter.qnt")]
fn simulation() -> impl Driver {
  ArbiterService::default()
}
