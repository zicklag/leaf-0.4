// Reactive application state using Svelte 5 runes.

import { Simulator } from './simulator';
import { generateUserId, generateArbiterDid } from './utils';
import type {
  UserAccount,
  ServerStateView,
  ArbiterView,
  SpaceView,
  MemberEntryView,
  JobArgs,
  OperationOk,
  OperationResult,
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
  darkTheme = $state(
    localStorage.getItem('arbiter-dark-theme') !== null
      ? localStorage.getItem('arbiter-dark-theme') === 'true'
      : window.matchMedia('(prefers-color-scheme: dark)').matches
  );

  toggleTheme() {
    this.darkTheme = !this.darkTheme;
    localStorage.setItem('arbiter-dark-theme', String(this.darkTheme));
    this.applyTheme();
  }

  applyTheme() {
    document.documentElement.classList.toggle('dark', this.darkTheme);
  }

  simulator = new Simulator();
  notifications = new NotificationStore();

  loading = $state(true);
  initError = $state<string | null>(null);

  users = $state<UserAccount[]>([]);
  currentUserId = $state<string | null>(null);

  serverState = $state<ServerStateView | null>(null);

  selectedArbiterDid = $state<string | null>(null);
  selectedSpaceKey = $state<string | null>(null);
  selectedSpaceMembers = $state<OperationOk['members'] | null>(null);
  selectedSpaceError = $state<string | null>(null);

  private lastSpaceRefresh = '';
  private lastRefreshUser = '';

  get currentUser() {
    return this.users.find((u) => u.did === this.currentUserId) ?? null;
  }

  get selectedArbiter(): ArbiterView | null {
    return this.serverState?.arbiters.find(
      (a) => a.did === this.selectedArbiterDid,
    ) ?? null;
  }

  get selectedSpace(): SpaceView | null {
    return this.selectedArbiter?.spaces.find(
      (s) => s.key === this.selectedSpaceKey,
    ) ?? null;
  }

  async init() {
    try {
      await this.simulator.init();
      this.loading = false;
      this.applyTheme();
      this.addUser('Alice');
      this.addUser('Bob');
      this.addUser('Charlie');
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

  selectUser(did: string) {
    this.currentUserId = did;
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.fetchSpaceMembers();
    }
  }

  selectArbiter(did: string | null) {
    this.selectedArbiterDid = did;
    this.selectedSpaceKey = null;
    this.selectedSpaceMembers = null;
    this.selectedSpaceMissing = null;
    this.selectedSpaceError = null;
  }

  selectSpace(arbiterDid: string, spaceKey: string) {
    this.selectedArbiterDid = arbiterDid;
    this.selectedSpaceKey = spaceKey;
    this.fetchSpaceMembers();
  }

  /// Fetch resolved members for the selected space.
  private async fetchSpaceMembers() {
    if (!this.selectedArbiterDid || !this.selectedSpaceKey) return;
    if (!this.currentUserId) return;

    const key = `${this.selectedArbiterDid}/${this.selectedSpaceKey}/${this.currentUserId}`;
    if (key === this.lastSpaceRefresh) return;
    this.lastSpaceRefresh = key;

    try {
      const result = await this.simulator.processOperation(
        this.selectedArbiterDid,
        this.currentUserId,
        this.selectedSpaceKey,
        { type: 'ResolveMembers' },
      );

      if (result.result.status === 'ok') {
        this.selectedSpaceMembers = result.result.members ?? null;
        this.selectedSpaceMissing = result.result.missingSpaces ?? null;
        this.selectedSpaceError = null;
      } else if (result.result.status === 'error') {
        this.selectedSpaceMembers = null;
        this.selectedSpaceMissing = null;
        this.selectedSpaceError = `Permission denied for "${this.currentUser?.label}": ${result.result.error}`;
      } else {
        this.selectedSpaceMembers = null;
        this.selectedSpaceMissing = null;
        this.selectedSpaceError = `Could not resolve members.`;
      }

      // Update the full state
      this.serverState = result.state;
    } catch (e) {
      this.selectedSpaceMembers = null;
      this.selectedSpaceError = `Error: ${e}`;
    }
  }

  /// Process an operation and update state.
  async processOperation(
    arbiterDid: string,
    userDid: string,
    spaceKey: string,
    args: JobArgs,
  ): Promise<OperationResult> {
    const result = await this.simulator.processOperation(
      arbiterDid, userDid, spaceKey, args,
    );
    this.serverState = result.state;

    // Re-fetch members if a space is selected
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.lastSpaceRefresh = '';
      this.fetchSpaceMembers();
    }

    return result.result;
  }

  resetAll() {
    history.replaceState(null, '', window.location.pathname);
    location.reload();
  }

  toggleArbiterOffline(did: string) {
    const s = this.simulator.disabledArbiters;
    if (s.has(did)) {
      s.delete(did);
    } else {
      s.add(did);
    }
    this.refreshState();
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.fetchSpaceMembers();
    }
  }

  isArbiterOffline(did: string): boolean {
    return this.simulator.disabledArbiters.has(did);
  }

  generateArbiterDid() { return generateArbiterDid(); }
}

export const app = new AppState();
(globalThis as any).app = app;
