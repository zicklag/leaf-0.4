use regorus::{
  PolicyModule, Value,
  languages::rego::compiler::Compiler,
  rvm::{
    RegoVM,
    vm::{ExecutionMode, ExecutionState, SuspendReason},
  },
};

static POLICY: &str = r#"
package test
import rego.v1

default allow := false

allow if {
    user_exists(input.user_id) == "found"
}
"#;

static EXTENSIONS: &str = r#"
# Wrapper function around the host await builtin
user_exists(username) := result if {
    result := __builtin_host_await(username, "user_exists")
}
"#;

fn main() -> anyhow::Result<()> {
  let compiled_policy = regorus::compile_policy_with_entrypoint(
    regorus::Value::Object(Default::default()),
    &[PolicyModule {
      id: "".into(),
      content: {
        let mut s = String::from(POLICY);
        s.push_str(EXTENSIONS);
        s
      }
      .into(),
    }],
    "data.test.allow".into(),
  )?;

  let program = Compiler::compile_from_policy(&compiled_policy, &["data.test.allow"])?;

  let mut vm = RegoVM::new();
  vm.load_program(program);
  vm.set_input(Value::from_json_str(
    r#"{
    "user_id": "user_abc123"
  }"#,
  )?);
  vm.set_execution_mode(ExecutionMode::Suspendable);

  vm.execute()?;

  loop {
    match vm.execution_state() {
      ExecutionState::Suspended {
        reason,
        ..
      } => {
        match reason {
          SuspendReason::HostAwait {
            argument,
            identifier,
            ..
          } => {
            let identifier_str = identifier.as_string()?.to_string();

            match identifier_str.as_str() {
              "user_exists" => {
                // Extract the query from the argument
                let query = &**argument.as_string()?;

                // Check for user
                let response = if query == "user_abc123" {
                  "found".to_string()
                } else {
                  "not_found".to_string()
                };

                // Provide the response to the async call
                vm.resume(Some(Value::from(response.as_str())))?;
              }
              other => {
                anyhow::bail!("Unknown async builtin identifier: {other:?}");
              }
            }
          }
          _ => panic!(),
        }
      }
      ExecutionState::Completed { result } => {
        println!("\n[VM] Completed — result: {result:?}");
        // Convert to bool for display
        let allowed = result.as_bool().copied().unwrap_or(false);
        println!("[RESULT] allow = {allowed}");
        break;
      }
      state => panic!("Unexpected state: {state:?}"),
    }
  }

  Ok(())
}
