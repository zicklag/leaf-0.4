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

// ---------------------------------------------------------------------------
// Map-safe JSON round-trip: serde-wasm-bindgen produces Map objects for
// Rust HashMaps; JSON.stringify/parse need help to handle them.
// ---------------------------------------------------------------------------

function mapReplacer(_key: string, value: unknown): unknown {
  if (value instanceof Map) {
    return { __t: 'M', e: Array.from(value.entries()) };
  }
  return value;
}

function mapReviver(_key: string, value: unknown): unknown {
  if (
    value &&
    typeof value === 'object' &&
    (value as Record<string, unknown>).__t === 'M'
  ) {
    return new Map((value as Record<string, unknown>).e as [unknown, unknown][]);
  }
  return value;
}

// ---------------------------------------------------------------------------
// Deflate compression helpers (native Compression Streams API)
// ---------------------------------------------------------------------------

async function compressToBase64(text: string): Promise<string> {
  const encoder = new TextEncoder();
  const compressed = await new Response(
    new Blob([encoder.encode(text)]).stream().pipeThrough(new CompressionStream('deflate-raw')),
  ).arrayBuffer();
  const bytes = new Uint8Array(compressed);
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

async function decompressFromBase64(encoded: string): Promise<string> {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  const decompressed = await new Response(
    new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw')),
  ).arrayBuffer();
  const decoder = new TextDecoder();
  return decoder.decode(decompressed);
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
  selectedSpaceError = $state<string | null>(null);

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
      const restored = await this.restoreFromUrl();
      if (!restored) {
        this.addUser('Alice');
        this.addUser('Bob');
        this.addUser('Charlie');
      }
      this.refreshState();

      // Re-import configuration from URL hash changes (shared links, back/forward)
      window.addEventListener('hashchange', async () => {
        this.selectedArbiterDid = null;
        this.selectedSpaceKey = null;
        this.selectedSpaceMembers = null;
        this.selectedSpaceError = null;
        await this.restoreFromUrl();
        this.refreshState();
      });
    } catch (e) {
      this.initError = String(e);
      this.loading = false;
    }
  }

  refreshState() {
    this.serverState = this.simulator.getState();
  }

  // ---------------------------------------------------------------------------
  // URL fragment persistence: deflate-compressed, Map-safe JSON round-trip
  // ---------------------------------------------------------------------------

  private async saveToUrl() {
    const snapshot = {
      v: 1,
      server: this.simulator.saveState(),
      users: this.users,
      currentUser: this.currentUserId,
    };
    const json = JSON.stringify(snapshot, mapReplacer);
    try {
      const compressed = await compressToBase64(json);
      history.replaceState(null, '', '#' + compressed);
    } catch (e) {
      // Fallback: store uncompressed if compression is unsupported
      console.warn('Deflate compression failed, falling back to base64:', e);
      history.replaceState(null, '', '#' + btoa(encodeURIComponent(json)));
    }
  }

  private async restoreFromUrl(): Promise<boolean> {
    const hash = window.location.hash.slice(1);
    if (!hash) return false;
    try {
      let json: string;
      // Try deflate-decompressed first, then fall back to plain base64
      try {
        json = await decompressFromBase64(hash);
      } catch {
        json = decodeURIComponent(atob(hash));
      }
      const snapshot = JSON.parse(json, mapReviver);
      if (typeof snapshot !== 'object' || !snapshot) return false;
      if (snapshot.server && typeof snapshot.server === 'object') {
        this.simulator.loadState(snapshot.server);
      }
      if (Array.isArray(snapshot.users)) {
        this.users = snapshot.users;
      }
      if (typeof snapshot.currentUser === 'string') {
        this.currentUserId = snapshot.currentUser;
      }
      return true;
    } catch (e) {
      console.warn('Failed to restore state from URL:', e);
      return false;
    }
  }

  // -----------------------------------------------------------------------

  addUser(label: string): UserAccount {
    const did = generateUserId(label);
    const user: UserAccount = { did, label };
    this.users = [...this.users, user];
    if (!this.currentUserId) this.currentUserId = did;
    void this.saveToUrl();
    return user;
  }

  removeUser(did: string) {
    this.users = this.users.filter((u) => u.did !== did);
    if (this.currentUserId === did) {
      this.currentUserId = this.users[0]?.did ?? null;
    }
    void this.saveToUrl();
  }

  selectUser(did: string) {
    this.currentUserId = did;
    void this.saveToUrl();
    // Re-fetch space members for the new user if a space is already selected
    if (this.selectedArbiterDid && this.selectedSpaceKey) {
      this.fetchSpaceMembers();
    }
  }

  selectArbiter(did: string | null) {
    this.selectedArbiterDid = did;
    this.selectedSpaceKey = null;
    this.selectedSpaceMembers = null;
    this.selectedSpaceError = null;
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
    const userDisplay = this.currentUser?.label ?? this.currentUserId ?? 'Unknown';
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
        this.selectedSpaceError = null;
      } else if (respond) {
        this.selectedSpaceMembers = null;
        this.selectedSpaceError = `The user "${userDisplay}" does not have permission to resolve the member list for this space.`;
      } else {
        this.selectedSpaceMembers = null;
        this.selectedSpaceError = `Could not resolve members for "${userDisplay}".`;
      }
    } catch (e) {
      this.selectedSpaceMembers = null;
      this.selectedSpaceError = `Error resolving members: ${e}`;
    }
  }

  async dispatch(msg: Message): Promise<EffectView[]> {
    const result = await this.simulator.dispatch(msg);
    this.serverState = result.state;
    this.fetchSpaceMembers();
    await this.saveToUrl();
    return result.effects;
  }

  resetAll() {
    history.replaceState(null, '', window.location.pathname);
    location.reload();
  }
  generateArbiterDid() { return generateArbiterDid(); }
}

export const app = new AppState();
(globalThis as any).app = app;
