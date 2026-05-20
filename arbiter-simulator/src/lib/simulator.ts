import init, { SimulationEngine } from 'arbiter-wasm2';
import type {
  JobArgs,
  OperationResult,
  ServerStateView,
} from './types';

export class Simulator {
  private engine: SimulationEngine | null = null;
  private initialized = false;
  private _policy: string = '';
  private _defaultPolicy: string = '';

  /// Set of disabled (offline) arbiters. Remote resolution to these fails.
  disabledArbiters: Set<string> = new Set();

  /// Get the current global policy.
  get policy(): string {
    return this._policy || this.getDefaultPolicy();
  }

  /// Set a new global policy and apply it to ALL existing arbiters.
  /// This bypasses normal authorization (super-admin operation).
  setPolicy(policy: string): void {
    this._policy = policy;
    if (this.engine) {
      this.engine.update_all_policies(policy);
    }
  }

  /// Get the default policy from the engine (compiled-in default).
  getDefaultPolicy(): string {
    if (this._defaultPolicy) return this._defaultPolicy;
    if (!this.engine) return 'package arbiter\nimport rego.v1\ndefault allow := false';
    try {
      const config = JSON.parse(this.engine.get_default_policy_config());
      this._defaultPolicy = config.policy ?? '';
      return this._defaultPolicy;
    } catch {
      return 'package arbiter\nimport rego.v1\ndefault allow := false';
    }
  }

  /// Validate a policy string. Returns empty string if valid, error if invalid.
  validatePolicy(policy: string): string {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.validate_policy(policy);
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    await init();
    try {
      this.engine = new SimulationEngine();
      this._policy = this.getDefaultPolicy();
      this.initialized = true;
    } catch (e) {
      console.error('[sim] failed to create SimulationEngine:', e);
      throw e;
    }
  }

  getState(): ServerStateView {
    if (!this.engine) throw new Error('Not initialized');
    return JSON.parse(this.engine.get_state());
  }

  /// Process an operation, auto-resolving remote spaces.
  /// Returns the final operation result (after all resolutions complete).
  async processOperation(
    arbiterDid: string,
    userDid: string,
    spaceKey: string,
    args: JobArgs,
  ): Promise<ProcessResult> {
    if (!this.engine) throw new Error('Not initialized');

    const argsJson = JSON.stringify(args);
    let resolvedRemotes: Record<string, unknown[]> = {};
    const resolutionLog: ResolutionStep[] = [];

    for (let depth = 0; depth < 10; depth++) {
      const resultJson = this.engine.process_operation(
        arbiterDid, userDid, spaceKey, argsJson, JSON.stringify(resolvedRemotes),
      );
      const result: OperationResult = JSON.parse(resultJson);

      if (result.status === 'needsResolution') {
        // Resolve each remote space by fetching its members
        const newRemotes: Record<string, unknown[]> = { ...resolvedRemotes };

        for (const space of result.spaces) {
          const remoteKey = `${space.remoteArbiterDid}|${space.spaceKey}`;
          if (newRemotes[remoteKey]) continue; // already resolved

          // If the remote arbiter is offline, mark as unresolvable
          if (this.disabledArbiters.has(space.remoteArbiterDid)) {
            newRemotes[remoteKey] = [];
            resolutionLog.push({
              remoteArbiterDid: space.remoteArbiterDid,
              spaceKey: space.spaceKey,
              memberCount: 0,
              offline: true,
            });
            continue;
          }

          const remoteResult = this.resolveRemoteMembers(
            space.remoteArbiterDid,
            arbiterDid,
            space.spaceKey,
          );

          newRemotes[remoteKey] = remoteResult;
          resolutionLog.push({
            remoteArbiterDid: space.remoteArbiterDid,
            spaceKey: space.spaceKey,
            memberCount: remoteResult.length,
          });
        }

        resolvedRemotes = newRemotes;
        continue;
      }

      // Get final state
      const state = JSON.parse(this.engine.get_state()) as ServerStateView;

      return {
        result,
        state,
        resolutionLog,
      };
    }

    // Depth limit reached — treat as error
    throw new Error('Remote resolution depth limit exceeded');
  }

  /// Create a new arbiter with the default policy.
  createArbiter(arbiterDid: string, ownerDid: string): void {
    if (!this.engine) throw new Error('Not initialized');

    const config = {
      $type: 'town.muni.arbiter.config.regoPolicy',
      policy: this.policy,
    };

    this.engine.create_arbiter(arbiterDid, ownerDid, JSON.stringify(config));
  }

  /// Current global policy (used for new arbiters).
  createArbiterWithConfig(
    arbiterDid: string,
    ownerDid: string,
    config: Record<string, unknown>,
  ): void {
    if (!this.engine) throw new Error('Not initialized');
    this.engine.create_arbiter(arbiterDid, ownerDid, JSON.stringify(config));
  }

  /// Resolve members for a space (called internally for remote resolution).
  private resolveRemoteMembers(
    arbiterDid: string,
    userDid: string,
    spaceKey: string,
  ): unknown[] {
    if (!this.engine) throw new Error('Not initialized');

    // Resolve as the calling arbiter — the remote arbiter checks permissions.
    const fetchArgs: JobArgs = { type: 'ResolveMembers' };
    const resultJson = this.engine.process_operation(
      arbiterDid, userDid, spaceKey, JSON.stringify(fetchArgs), '{}',
    );
    const result: OperationResult = JSON.parse(resultJson);

    if (result.status === 'ok' && result.members) {
      return result.members;
    }

    return [];
  }
}

export interface ResolutionStep {
  remoteArbiterDid: string;
  spaceKey: string;
  memberCount: number;
  offline?: boolean;
}

export interface ProcessResult {
  result: OperationResult;
  state: ServerStateView;
  resolutionLog: ResolutionStep[];
}


