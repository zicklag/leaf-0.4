import init, { ArbiterEngine } from 'arbiter-wasm';
import type { Message, EffectView, ServerStateView } from './types';

export class Simulator {
  private engine: ArbiterEngine | null = null;
  private initialized = false;
  private nextJobId = 1;
  disabledArbiters: Set<string> = new Set();

  async init(): Promise<void> {
    if (this.initialized) return;
    await init();
    try {
      this.engine = new ArbiterEngine();
      this.initialized = true;
    } catch (e) {
      console.error('[sim] failed to create ArbiterEngine:', e);
      throw e;
    }
  }

  async dispatch(msg: Message): Promise<SimulationResult> {
    if (!this.engine) throw new Error('Not initialized');
    if (msg.srcJobId === 0) msg.srcJobId = this.nextJobId++;
    const effects = this.simulate(msg);
    return { effects, state: this.getState() };
  }

  getState(): ServerStateView {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.get_state();
  }

  saveState(): unknown {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.save_state();
  }

  loadState(obj: unknown): void {
    if (!this.engine) throw new Error('Not initialized');
    this.engine.load_state(obj);
  }

  fetchMembers(arbiterDid: string, spaceKey: string, userDid: string): EffectView[] {
    return this.simulate({
      userDid, arbiterDid, spaceKey,
      srcJobId: this.nextJobId++, resolverDepth: 5,
      kind: { type: 'fetchMembers' },
    });
  }

  tick(): EffectView[] {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.tick();
  }

  // --- simulation loop ---

  private simulate(msg: Message): EffectView[] {
    const all: EffectView[] = [];
    const queue: Message[] = [msg];

    while (queue.length > 0) {
      const current = queue.shift()!;
      const effects = this.sendRaw(current);

      for (const eff of effects) {
        if (eff.effectType === 'sendMessage') {
          // Check if the target arbiter is disabled (simulates going offline)
          if (this.disabledArbiters.has(eff.arbiter_did)) {
            // Arbiter is offline — treat as unresolvable
            queue.push({
              userDid: current.arbiterDid,
              arbiterDid: eff.arbiter_did,
              spaceKey: eff.space_key,
              srcJobId: eff.src_job_id,
              resolverDepth: eff.resolver_depth,
              kind: {
                type: 'replyResolvedMembers',
                members: {
                  memberList: new Map(),
                  missingSpaces: new Map([[
                    { arbiterDid: eff.arbiter_did, spaceKey: eff.space_key },
                    'ReadMemberList' as const,
                  ]]),
                },
              },
            });
            continue;
          }

          // The originating arbiter is the one the current message was sent to.
          // Forward the resolution request as coming from that arbiter, not the
          // original user — the remote arbiter needs to check the arbiter's access.
          const resolved = this.resolveRemote(eff, current.arbiterDid);
          const respond = resolved.find(
            (e): e is Extract<EffectView, { effectType: 'respond' }> =>
              e.effectType === 'respond',
          );
          if (respond?.ok) {
            queue.push({
              userDid: current.arbiterDid,
              arbiterDid: eff.arbiter_did,
              spaceKey: eff.space_key,
              srcJobId: eff.src_job_id,
              resolverDepth: eff.resolver_depth,
              kind: {
                type: 'replyResolvedMembers',
                members: {
                  memberList: new Map(respond.member_list.map(m => [m.value, m.access])),
                  missingSpaces: new Map(),
                },
              },
            });
          } else {
            // Remote arbiter couldn't resolve — send reply with the space in
            // missingSpaces so the original arbiter marks it unresolved and
            // continues with whatever members it already resolved.
            queue.push({
              userDid: current.arbiterDid,
              arbiterDid: eff.arbiter_did,
              spaceKey: eff.space_key,
              srcJobId: eff.src_job_id,
              resolverDepth: eff.resolver_depth,
              kind: {
                type: 'replyResolvedMembers',
                members: {
                  memberList: new Map(),
                  missingSpaces: new Map([[
                    { arbiterDid: eff.arbiter_did, spaceKey: eff.space_key },
                    'ReadMemberList' as const,
                  ]]),
                },
              },
            });
          }
        } else {
          all.push(eff);
        }
      }
    }
    return all;
  }

  private resolveRemote(
    send: EffectView & { effectType: 'sendMessage' },
    origArbiterDid: string,
  ): EffectView[] {
    return this.simulate({
      userDid: origArbiterDid,
      arbiterDid: send.arbiter_did,
      spaceKey: send.space_key,
      srcJobId: this.nextJobId++,
      resolverDepth: send.resolver_depth,
      kind: { type: 'fetchMembers' },
    });
  }

  private sendRaw(msg: Message): EffectView[] {
    console.log('[sim] →', msg);
    return this.engine!.handle_message(msg);
  }
}

export interface SimulationResult {
  effects: EffectView[];
  state: ServerStateView;
}
