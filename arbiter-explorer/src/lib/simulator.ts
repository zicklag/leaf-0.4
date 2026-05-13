// Orchestrator wrapping the wasm ArbiterEngine.
// Sends JSON-serialized Messages to wasm, receives JS objects via
// serde-wasm-bindgen. Handles the simulation loop: routes SendMessage
// effects by driving the engine recursively through message exchanges.

import init, { ArbiterEngine } from 'arbiter-wasm';
import type { Message, EffectView, ServerStateView } from './types';

export class Simulator {
  private engine: ArbiterEngine | null = null;
  private initialized = false;
  private nextJobId = 1;

  async init(): Promise<void> {
    if (this.initialized) return;
    await init({ module_or_path: '/wasm/arbiter_wasm_bg.wasm' });
    this.engine = new ArbiterEngine();
    this.initialized = true;
  }

  async dispatch(msg: Message): Promise<SimulationResult> {
    if (!this.engine) throw new Error('Not initialized');
    if (msg.srcJobId === 0) msg.srcJobId = this.nextJobId++;
    const effects = this.simulate(msg);
    return { effects, state: this.getState() };
  }

  getState(): ServerStateView {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.get_state() as unknown as ServerStateView;
  }

  fetchMembers(arbiterDid: string, spaceKey: string, userDid: string): EffectView[] {
    const msg: Message = {
      userDid, arbiterDid, spaceKey,
      srcJobId: this.nextJobId++, resolverDepth: 5,
      kind: { type: 'fetchMembers' },
    };
    return this.simulate(msg);
  }

  tick(): EffectView[] {
    if (!this.engine) throw new Error('Not initialized');
    return this.engine.tick() as unknown as EffectView[];
  }

  // --- simulation loop ---

  private simulate(msg: Message): EffectView[] {
    const all: EffectView[] = [];
    const queue: Message[] = [msg];

    while (queue.length > 0) {
      const current = queue.shift()!;
      const effects = this.sendRaw(current);

      for (const eff of effects) {
        if (eff.effectType === 'SendMessage') {
          const resolved = this.resolveRemote(eff);
          const respond = resolved.find((e): e is Extract<EffectView, { effectType: 'Respond' }> =>
            e.effectType === 'Respond',
          );
          if (respond?.ok) {
            queue.push({
              userDid: '', arbiterDid: eff.arbiterDid, spaceKey: eff.spaceKey,
              srcJobId: eff.srcJobId, resolverDepth: eff.resolverDepth,
              kind: {
                type: 'replyResolvedMembers',
                members: {
                  memberList: Object.fromEntries(respond.memberList.map(m => [m.value, m.access])),
                  missingSpaces: {},
                },
              },
            });
          } else {
            all.push(eff);
          }
        } else {
          all.push(eff);
        }
      }
    }
    return all;
  }

  private resolveRemote(send: EffectView & { effectType: 'SendMessage' }): EffectView[] {
    return this.simulate({
      userDid: '', arbiterDid: send.arbiterDid, spaceKey: send.spaceKey,
      srcJobId: this.nextJobId++, resolverDepth: send.resolverDepth,
      kind: { type: 'fetchMembers' },
    });
  }

  private sendRaw(msg: Message): EffectView[] {
    console.log('[sim] →', msg);
    const result = this.engine!.handle_message(JSON.stringify(msg));
    console.log('[sim] ←', result);
    return result as unknown as EffectView[];
  }
}

export interface SimulationResult {
  effects: EffectView[];
  state: ServerStateView;
}
