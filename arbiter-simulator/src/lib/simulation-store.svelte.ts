// Reactive application state using Svelte 5 runes.

import { Simulator } from './simulator';
import type {
  UserAccount,
  OpResult,
  ArbiterSnapshot,
  SpaceSnapshot,
  PolicyCheckLog,
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

  // Snapshot refreshed after each mutation / selection
  snapshot = $state<{ arbiters: ArbiterSnapshot[] }>({ arbiters: [] });

  selectedArbiterDid = $state<string | null>(null);
  selectedSpaceKey = $state<string | null>(null);

  // Resolved member view for the selected space
  resolvedMembers = $state<Array<{ did: string; access: Record<string, unknown> }> | null>(null);
  resolvedMissing = $state<Array<{ arbiterDid: string; spaceKey: string }> | null>(null);
  resolvedError = $state<string | null>(null);

  // Policy check log for the most recent operation
  lastPolicyLog = $state<PolicyCheckLog | null>(null);

  private lastResolveKey = '';

  get currentUser() {
    return this.users.find((u) => u.did === this.currentUserId) ?? null;
  }

  get selectedArbiter(): ArbiterSnapshot | null {
    return this.snapshot.arbiters.find((a) => a.did === this.selectedArbiterDid) ?? null;
  }

  get selectedSpace(): SpaceSnapshot | null {
    return this.selectedArbiter?.spaces.find((s) => s.key === this.selectedSpaceKey) ?? null;
  }

  // Backward-compatible aliases for existing components
  // These will be removed once components are migrated.

  /** @deprecated Use `snapshot` instead. */
  get serverState() { return this.snapshot; }

  /** @deprecated Use `resolvedMembers` instead. */
  get selectedSpaceMembers() {
    return this.resolvedMembers as Array<{ did: string; access: Record<string, unknown> } & Record<string, unknown>> | null;
  }

  /** @deprecated Use `resolvedMissing` instead. */
  get selectedSpaceMissing() { return this.resolvedMissing; }

  /** @deprecated Use `resolvedError` instead. */
  get selectedSpaceError() {
    return this.resolvedError ? `Permission denied: ${this.resolvedError}` : null;
  }

  /** @deprecated Use `refreshSnapshot` instead. */
  refreshState() { this.refreshSnapshot(); }

  /** Get current policy text from the simulator. */
  get policy(): string {
    return this.simulator.defaultPolicy;
  }

  /** Validate Rego policy text, returning an error string or null on success. */
  validatePolicy(policy: string): string | null {
    try {
      const err = this.simulator.validatePolicy(policy);
      return err ?? null;
    } catch (e) {
      return String(e);
    }
  }

  /** Apply a policy to all arbiters. */
  setPolicy(policy: string): void {
    this.simulator.applyPolicyToAll(policy);
  }

  /** Get the default policy text. */
  getDefaultPolicy(): string {
    return this.simulator.getDefaultPolicy();
  }

  isArbiterOffline(did: string) { return this.simulator.isArbiterOffline(did); }

  toggleArbiterOffline(did: string) {
    this.simulator.toggleArbiterOffline(did);
    this.refreshSnapshot();
    // Re-fetch resolved members since remote access may have changed
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.lastResolveKey = '';
      this.fetchResolvedMembers();
    }
  }

  /** @deprecated Use `runOp` with explicit method calls instead. */
  async processOperation(
    arbiterDid: string,
    userDid: string,
    _spaceKey: string,
    args: { type: string; [key: string]: unknown },
  ): Promise<{ status: string; error?: string; members?: unknown[] }> {
    // Map old JobArgs type strings to new simulator methods
    const log: PolicyCheckLog = { steps: [], result: undefined, allowed: false };
    let result: OpResult;

    switch (args.type) {
      case 'CreateSpace':
        result = await this.simulator.createSpace(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType: args.spaceType as string,
          config: args.config as Record<string, unknown>,
        }, log);
        break;
      case 'DeleteSpace':
        result = await this.simulator.deleteSpace(arbiterDid, userDid, {
          spaceKey: _spaceKey,
        }, log);
        break;
      case 'SetSpaceConfig':
        result = await this.simulator.setSpaceConfig(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          config: args.config as Record<string, unknown>,
        }, log);
        break;
      case 'SetSpaceMemberAccess': {
        const member = args.member as { tag: string; value: unknown };
        const memberDid = member.tag === 'MemberDid'
          ? String(member.value)
          : member.tag === 'MemberLocalSpace'
            ? `space:${member.value}`
            : `${(member.value as { arbiterDid: string }).arbiterDid}|${(member.value as { spaceKey: string }).spaceKey}`;
        result = await this.simulator.setSpaceMemberAccess(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          memberDid,
          access: args.access as Record<string, unknown>,
        }, log);
        break;
      }
      case 'RemoveSpaceMember': {
        const member = args.member as { tag: string; value: unknown };
        const memberDid = member.tag === 'MemberDid'
          ? String(member.value)
          : member.tag === 'MemberLocalSpace'
            ? `space:${member.value}`
            : `${(member.value as { arbiterDid: string }).arbiterDid}|${(member.value as { spaceKey: string }).spaceKey}`;
        result = await this.simulator.removeSpaceMember(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          memberDid,
        }, log);
        break;
      }
      case 'ResolveMembers':
        result = await this.simulator.resolveSpaceMembers(arbiterDid, userDid, {
          spaceKey: _spaceKey,
        }, log);
        break;
      case 'DeleteArbiter':
        // Simulate deletion via the arbiter's own policy (admin space)
        result = await this.simulator.deleteArbiter(arbiterDid, userDid, log);
        break;
      default:
        result = { status: 'error', error: `Unknown operation: ${args.type}` };
    }

    this.lastPolicyLog = log;
    this.refreshSnapshot();

    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.lastResolveKey = '';
      this.fetchResolvedMembers();
    }

    return result;
  }

  async init() {
    try {
      await this.simulator.init();
      this.loading = false;
      this.applyTheme();
      this.addUser('Alice');
      this.addUser('Bob');
      this.addUser('Charlie');
      this.refreshSnapshot();
    } catch (e) {
      this.initError = String(e);
      this.loading = false;
    }
  }

  refreshSnapshot() {
    this.snapshot = this.simulator.snapshot();
  }

  addUser(label: string): UserAccount {
    const did = label.toLowerCase().replace(/\s+/g, '-');
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
      this.fetchResolvedMembers();
    }
  }

  selectArbiter(did: string | null) {
    this.selectedArbiterDid = did;
    this.selectedSpaceKey = null;
    this.resolvedMembers = null;
    this.resolvedMissing = null;
    this.resolvedError = null;
  }

  selectSpace(arbiterDid: string, spaceKey: string) {
    this.selectedArbiterDid = arbiterDid;
    this.selectedSpaceKey = spaceKey;
    this.fetchResolvedMembers();
  }

  /** Fetch resolved members for the selected space. */
  private async fetchResolvedMembers() {
    if (!this.selectedArbiterDid || !this.selectedSpaceKey) return;
    if (!this.currentUserId) return;

    const key = `${this.selectedArbiterDid}/${this.selectedSpaceKey}/${this.currentUserId}`;
    if (key === this.lastResolveKey) return;
    this.lastResolveKey = key;

    try {
      const log: PolicyCheckLog = { steps: [], result: undefined, allowed: false };
      const result = await this.simulator.resolveSpaceMembers(
        this.selectedArbiterDid,
        this.currentUserId,
        { spaceKey: this.selectedSpaceKey },
        log,
      );

      if (result.status === 'ok') {
        this.resolvedMembers = result.members ?? null;
        this.resolvedMissing = result.missingSpaces?.map((m) => ({
          arbiterDid: m.arbiterDid,
          spaceKey: m.spaceKey,
        })) ?? null;
        this.resolvedError = null;
      } else {
        this.resolvedMembers = null;
        this.resolvedMissing = null;
        this.resolvedError = `Permission denied: ${result.error}`;
      }

      this.lastPolicyLog = log;
      this.refreshSnapshot();
    } catch (e) {
      this.resolvedMembers = null;
      this.resolvedMissing = null;
      this.resolvedError = `Error: ${e}`;
    }
  }

  /** Run an XRPC operation and refresh state. */
  async runOp(
    arbiterDid: string,
    userDid: string,
    operation: (log?: PolicyCheckLog) => Promise<OpResult>,
  ): Promise<OpResult> {
    const log: PolicyCheckLog = { steps: [], result: undefined, allowed: false };
    const result = await operation(log);
    this.lastPolicyLog = log;
    this.refreshSnapshot();

    // Re-fetch members if a space is selected
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.lastResolveKey = '';
      this.fetchResolvedMembers();
    }

    return result;
  }

  resetAll() {
    history.replaceState(null, '', window.location.pathname);
    location.reload();
  }

  generateArbiterDid(): string {
    const count = this.snapshot.arbiters.length + 1;
    return `arbiter${count}`;
  }
}

export const app = new AppState();
// Expose for debugging
(globalThis as unknown as Record<string, unknown>).app = app;
