// Type declarations for the arbiter-wasm2 WASM module.
// Built from crates/arbiter-wasm2 via wasm-pack build.

declare module 'arbiter-wasm2' {
  export class SimulationEngine {
    constructor();
    create_arbiter(arbiter_did: string, owner_did: string, config_json: string): void;
    process_operation(
      arbiter_did: string,
      user_did: string,
      space_key: string,
      args_json: string,
      resolved_remotes_json: string,
    ): string;
    provide_resolved_remotes(
      arbiter_did: string,
      job_id: number,
      resolved_remotes_json: string,
    ): string;
    validate_policy(policy: string): string;
    update_all_policies(policy: string): void;
    get_default_policy_config(): string;
    get_state(): string;
  }

  export default function init(): Promise<void>;
}
