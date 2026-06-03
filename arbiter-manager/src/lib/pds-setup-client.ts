import { Agent, CredentialSession } from '@atproto/api';
import { actorResolver } from './resolver';
import { isActorIdentifier } from '@atcute/lexicons/syntax';

/**
 * PDS client wrapper for the setup flow.
 */
export class PdsSetupClient {
  agent?: Agent;

  async login(user: string, password: string) {
    if (this.agent) return;

    if (!isActorIdentifier(user)) throw new Error('Invalid username');
    const actor = await actorResolver.resolve(user);
    const session = new CredentialSession(new URL(actor.pds));
    await session.login({
      identifier: user,
      password,
    });
    this.agent = new Agent(session);
  }
}
