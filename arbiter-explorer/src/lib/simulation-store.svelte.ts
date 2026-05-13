// Reactive application state using Svelte 5 runes.

import { Simulator } from './simulator';
import { generateUserId, generateArbiterDid } from './utils';
import type {
  UserAccount,
  ServerStateView,
  MemberEntryView,
  MissingSpaceView,
  EffectView,
  Message,
} from './types';

// --- Notifications ---
class NotificationStore {
  items = $state<AppNotification[]>([]);
  private counter = 0;

  add(type: 'success' | 'error' | 'info', message: string) {
    const id = ++this.counter;
    this.items = [...this.items, { id, type, message }];
    setTimeout(() => {
      this.items = this.items.filter((n) => n.id !== id);
    }, 4000);
  }

  dismiss(id: number) {
    this.items = this.items.filter((n) => n.id !== id);
  }
}

interface AppNotification {
  id: number;
  type: 'success' | 'error' | 'info';
  message: string;
}

// --- Main state ---
class AppState {
  simulator = new Simulator();
  notifications = new NotificationStore();

  loading = $state(true);
  initError = $state<string | null>(null);

  users = $state<UserAccount[]>([]);
  currentUserId = $state<string | null>(null);

  serverState = $state<ServerStateView | null>(null);

  selectedArbiterDid = $state<string | null>(null);
  selectedSpaceKey = $state<string | null>(null);
  selectedSpaceMembers = $state<{
    resolved: MemberEntryView[];
    missing: MissingSpaceView[];
  } | null>(null);

  get currentUser() {
    return this.users.find((u) => u.did === this.currentUserId) ?? null;
  }

  get selectedArbiter() {
    return this.serverState?.arbiters.find(
      (a) => a.did === this.selectedArbiterDid,
    ) ?? null;
  }

  get selectedSpace() {
    return this.selectedArbiter?.spaces.find(
      (s) => s.key === this.selectedSpaceKey,
    ) ?? null;
  }

  async init() {
    try {
      await this.simulator.init();
      this.loading = false;
      this.refreshState();
    } catch (e) {
      this.initError = String(e);
      this.loading = false;
    }
  }

  refreshState() {
    this.serverState = this.simulator.getState();
  }

  addUser(label: string): UserAccount {
    const did = generateUserId(label);
    const user: UserAccount = { did, label };
    this.users = [...this.users, user];
    if (!this.currentUserId) this.currentUserId = did;
    return user;
  }

  removeUser(did: string) {
    this.users = this.users.filter((u) => u.did !== did);
    if (this.currentUserId === did) {
      this.currentUserId = this.users[0]?.did ?? null;
    }
  }

  selectUser(did: string) { this.currentUserId = did; }

  selectArbiter(did: string | null) {
    this.selectedArbiterDid = did;
    this.selectedSpaceKey = null;
    this.selectedSpaceMembers = null;
  }

  selectSpace(arbiterDid: string, spaceKey: string) {
    this.selectedArbiterDid = arbiterDid;
    this.selectedSpaceKey = spaceKey;
    this.fetchSpaceMembers();
  }

  /// Fetch resolved members for the selected space by sending FetchMembers
  /// through the engine, auto-resolving delegations.
  private fetchSpaceMembers() {
    if (!this.selectedArbiterDid || !this.selectedSpaceKey) return;
    try {
      const effects = this.simulator.fetchMembers(
        this.selectedArbiterDid,
        this.selectedSpaceKey,
        this.currentUserId ?? '',
      );
      const respond = effects.find(
        (e): e is Extract<EffectView, { effectType: 'respond' }> =>
          e.effectType === 'respond',
      );
      if (respond && respond.ok) {
        this.selectedSpaceMembers = {
          resolved: respond.member_list,
          missing: respond.missing_spaces,
        };
      } else {
        this.selectedSpaceMembers = null;
      }
    } catch {
      this.selectedSpaceMembers = null;
    }
  }

  async dispatch(msg: Message): Promise<EffectView[]> {
    const result = await this.simulator.dispatch(msg);
    this.serverState = result.state;
    this.fetchSpaceMembers();
    return result.effects;
  }

  resetAll() { location.reload(); }
  generateArbiterDid() { return generateArbiterDid(); }
}

export const app = new AppState();
