import { writable } from 'svelte/store';
import { browser } from '$app/environment';

// Re-export auth helpers so components can import from a single module
import type { Did, ManagedCommunity } from './types';

const MANAGED_DIDS_KEY = 'arbiter-manager-communities';

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

// For better ergonomics we'll expose query/mutation functions directly
// that components can use with TanStack Query's createQuery / createMutation
