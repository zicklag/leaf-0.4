// Reactive application state using Svelte 5 runes.

import { Simulator } from './simulator';
import type {
  UserAccount,
  OpResult,
  ArbiterSnapshot,
  SpaceSnapshot,
  PolicyCheckLog,
} from './types';

const STORAGE_KEY = 'arbiter-simulator-state';

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

  /** When enabled, access-level-specific UI is replaced with raw JSON editors. */
  advancedMode = $state(
    localStorage.getItem('arbiter-advanced-mode') === 'true'
  );

  toggleAdvancedMode() {
    this.advancedMode = !this.advancedMode;
    localStorage.setItem('arbiter-advanced-mode', String(this.advancedMode));
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
    return this.resolvedError;
  }

  /** @deprecated Use `refreshSnapshot` instead. */
  refreshState() { this.refreshSnapshot(); }

  /** Get current policy text — either from the selected arbiter, or the default. */
  get policy(): string {
    return this.selectedArbiter?.policy ?? this.simulator.defaultPolicy;
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
    this.refreshSnapshot();
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

    // Infer spaceType from the space key
    const spaceType = _spaceKey === '$admin'
      ? 'town.muni.arbiter.config.adminSpace'
      : 'town.muni.arbiter.config.space';

    switch (args.type) {
      case 'CreateSpace':
        result = await this.simulator.createSpace(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType: args.spaceType as string ?? spaceType,
          config: args.config as Record<string, unknown>,
        }, log);
        break;
      case 'DeleteSpace':
        result = await this.simulator.deleteSpace(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType,
        }, log);
        break;
      case 'SetSpaceConfig':
        result = await this.simulator.setSpaceConfig(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType,
          config: args.config as Record<string, unknown>,
        }, log);
        break;
      case 'SetSpaceMemberAccess': {
        const member = args.member as { tag: string; value: unknown };
        const buildRemoteDid = (v: unknown) => {
          if (typeof v === 'string') return v;
          const r = v as { arbiterDid: string; spaceKey: string };
          return `${r.arbiterDid}|town.muni.arbiter.config.space|${r.spaceKey}`;
        };
        const memberDid = member.tag === 'MemberDid'
          ? String(member.value)
          : member.tag === 'MemberLocalSpace'
            ? `space:town.muni.arbiter.config.space/${member.value}`
            : buildRemoteDid(member.value);
        result = await this.simulator.setSpaceMemberAccess(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType,
          memberDid,
          access: args.access as Record<string, unknown>,
        }, log);
        break;
      }
      case 'RemoveSpaceMember': {
        const member = args.member as { tag: string; value: unknown };
        const buildRemoteDid = (v: unknown) => {
          if (typeof v === 'string') return v;
          const r = v as { arbiterDid: string; spaceKey: string };
          return `${r.arbiterDid}|town.muni.arbiter.config.space|${r.spaceKey}`;
        };
        const memberDid = member.tag === 'MemberDid'
          ? String(member.value)
          : member.tag === 'MemberLocalSpace'
            ? `space:town.muni.arbiter.config.space/${member.value}`
            : buildRemoteDid(member.value);
        result = await this.simulator.removeSpaceMember(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType,
          memberDid,
        }, log);
        break;
      }
      case 'ResolveMembers':
        result = await this.simulator.resolveSpaceMembers(arbiterDid, userDid, {
          spaceKey: _spaceKey,
          spaceType,
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

  /** Serialise the full app state into a portable object. */
  private serialise(): Record<string, unknown> {
    return {
      snapshot: this.simulator.snapshot(),
      users: this.users,
      currentUserId: this.currentUserId,
    };
  }

  /** Save current state to localStorage. */
  private saveToStorage(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.serialise()));
    } catch {
      // Ignore storage errors (quota, private browsing, etc.)
    }
  }

  /** Restore state from localStorage. Returns true if state was restored. */
  private restoreFromStorage(): boolean {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return false;
      const saved = JSON.parse(raw);
      if (!saved || typeof saved !== 'object') return false;

      const snap = saved.snapshot;
      if (!snap || !Array.isArray(snap.arbiters)) return false;

      // Restore simulator state
      this.simulator.loadSnapshot(snap);

      // Restore UI state
      if (Array.isArray(saved.users)) {
        this.users = saved.users;
      }
      if (typeof saved.currentUserId === 'string') {
        this.currentUserId = saved.currentUserId;
      }

      this.refreshSnapshot();
      return true;
    } catch {
      return false;
    }
  }

  /** Copy the full config to clipboard as JSON. */
  async copyConfig(): Promise<void> {
    try {
      await navigator.clipboard.writeText(JSON.stringify(this.serialise(), null, 2));
      this.notifications.add('success', 'Config copied to clipboard!');
    } catch {
      this.notifications.add('error', 'Failed to copy config');
    }
  }

  /** Import config from clipboard JSON and restore state. */
  async importConfig(): Promise<void> {
    try {
      const text = await navigator.clipboard.readText();
      const saved = JSON.parse(text);
      if (!saved || typeof saved !== 'object' || !saved.snapshot || !Array.isArray(saved.snapshot.arbiters)) {
        this.notifications.add('error', 'Invalid config format');
        return;
      }

      this.simulator.loadSnapshot(saved.snapshot);
      if (typeof saved.defaultPolicy === 'string') {
        this.simulator.defaultPolicy = saved.defaultPolicy;
      }
      if (Array.isArray(saved.users)) {
        this.users = saved.users;
      }
      this.currentUserId = typeof saved.currentUserId === 'string' ? saved.currentUserId : (saved.users?.[0]?.did ?? null);

      this.refreshSnapshot();
      this.selectedArbiterDid = null;
      this.selectedSpaceKey = null;
      this.resolvedMembers = null;
      this.resolvedMissing = null;
      this.resolvedError = null;
      this.notifications.add('success', 'Config imported!');
    } catch {
      this.notifications.add('error', 'Failed to import config');
    }
  }

  async init() {
    try {
      await this.simulator.init();
      this.loading = false;
      this.applyTheme();

      // Try to restore saved state first
      const restored = this.restoreFromStorage();

      if (!restored) {
        // Fresh start
        this.addUser('Alice');
        this.addUser('Bob');
        this.addUser('Charlie');
        this.refreshSnapshot();
      }
    } catch (e) {
      this.initError = String(e);
      this.loading = false;
    }
  }

  refreshSnapshot() {
    this.snapshot = this.simulator.snapshot();
    this.saveToStorage();
  }

  addUser(label: string): UserAccount {
    const did = label.toLowerCase().replace(/\s+/g, '-');
    const user: UserAccount = { did, label };
    this.users = [...this.users, user];
    if (!this.currentUserId) this.currentUserId = did;
    this.saveToStorage();
    return user;
  }

  removeUser(did: string) {
    this.users = this.users.filter((u) => u.did !== did);
    if (this.currentUserId === did) {
      this.currentUserId = this.users[0]?.did ?? null;
    }
    this.saveToStorage();
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
      const spaceType = this.selectedSpaceKey === '$admin'
        ? 'town.muni.arbiter.config.adminSpace'
        : 'town.muni.arbiter.config.space';
      const result = await this.simulator.resolveSpaceMembers(
        this.selectedArbiterDid,
        this.currentUserId,
        { spaceKey: this.selectedSpaceKey, spaceType },
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
    localStorage.removeItem(STORAGE_KEY);
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
