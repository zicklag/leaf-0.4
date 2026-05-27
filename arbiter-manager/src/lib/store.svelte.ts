import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';
import { queryStore, queryClient } from './query-client';
import { ArbiterClient } from './api';
import { restoreSession, getSession, clearSession, login as authLogin } from './auth';

// Re-export auth helpers so components can import from a single module
export { restoreSession, getSession };
import type {
  AuthSession,
  Did,
  SpaceKey,
  ManagedCommunity,
  SpaceSummary,
  MemberEntry,
  ResolvedMemberEntry,
  MissingSpaceEntry,
} from './types';

const MANAGED_DIDS_KEY = 'arbiter-manager-communities';

// ---------------------------------------------------------------------------
// Auth state
// ---------------------------------------------------------------------------

export const session = writable<AuthSession | null>(null);
export const isAuthenticated = derived(session, ($s) => $s !== null);

// Restore session on init (only runs in browser)
if (browser) {
  const s = restoreSession();
  if (s) session.set(s);
}

export function login() {
  authLogin();
}

export function logout() {
  clearSession();
  session.set(null);
  queryClient.clear();
}

export function setSession(s: AuthSession) {
  session.set(s);
}

// ---------------------------------------------------------------------------
// Arbiter client derived from session
// ---------------------------------------------------------------------------

function getClient(session: AuthSession | null): ArbiterClient | null {
  if (!session) return null;
  return new ArbiterClient(session.pdsUrl, session.accessJwt);
}

// ---------------------------------------------------------------------------
// Managed communities (localStorage persisted list of DIDs)
// ---------------------------------------------------------------------------

function loadManagedCommunities(): ManagedCommunity[] {
  if (!browser) return [];
  try {
    const raw = localStorage.getItem(MANAGED_DIDS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveManagedCommunities(list: ManagedCommunity[]) {
  localStorage.setItem(MANAGED_DIDS_KEY, JSON.stringify(list));
}

export const managedCommunities = writable<ManagedCommunity[]>(loadManagedCommunities());

export function addManagedCommunity(did: Did, label: string) {
  managedCommunities.update((list) => {
    // Don't add duplicates
    if (list.some((c) => c.did === did)) return list;
    const updated = [...list, { did, label, addedAt: Date.now() }];
    saveManagedCommunities(updated);
    return updated;
  });
}

export function removeManagedCommunity(did: Did) {
  managedCommunities.update((list) => {
    const updated = list.filter((c) => c.did !== did);
    saveManagedCommunities(updated);
    return updated;
  });
}

// ---------------------------------------------------------------------------
// Selected arbiter / space
// ---------------------------------------------------------------------------

export const selectedArbiterDid = writable<Did | null>(null);
export const selectedSpaceKey = writable<SpaceKey | null>(null);

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

function client(): ArbiterClient {
  const session = getSession();
  const c = getClient(session);
  if (!c) throw new Error('Not authenticated');
  return c;
}

// ---------------------------------------------------------------------------
// TanStack Query hooks (wrapped as Svelte stores for ergonomic use)
// ---------------------------------------------------------------------------

export const arbiterConfigQuery = queryStore({
  queryKey: ['arbiterConfig'],
  queryFn: async () => {
    const did = getSelectedDid();
    return client().getArbiterConfig(did);
  },
  enabled: false, // We'll use derived keys with TanStack's Svelte integration
});

// For better ergonomics we'll expose query/mutation functions directly
// that components can use with TanStack Query's createQuery / createMutation

export function getSelectedDid(): Did {
  let did: Did | null = null;
  selectedArbiterDid.subscribe((d) => (did = d))();
  if (!did) throw new Error('No arbiter selected');
  return did;
}

export function getSelectedSpace(): SpaceKey | null {
  let key: SpaceKey | null = null;
  selectedSpaceKey.subscribe((k) => (key = k))();
  return key;
}
