/**
 * Managed communities (localStorage persisted list of DIDs).
 *
 * Uses Svelte 5 runes ($state, $effect) for reactivity and
 * arktype for runtime type validation.
 *
 * Pattern: class with $state fields + singleton instance (see auth.svelte.ts).
 */

import { type } from 'arktype';

export const STORAGE_KEY = 'arbiter-manager-communities';

type Did = string;

const managedCommunityTy = type({
  did: 'string',
  label: 'string',
  addedAt: 'number',
});
export type ManagedCommunity = typeof managedCommunityTy.infer;

export class ManagedCommunities {
  communities: ManagedCommunity[] = $state([]);

  constructor() {
    this.communities = this.#load();
  }

  #load(): ManagedCommunity[] {
    try {
      const raw = globalThis.localStorage.getItem(STORAGE_KEY);
      const parsed = JSON.parse(raw || '[]');
      const result = managedCommunityTy.array()(parsed);
      return result instanceof type.errors ? [] : result;
    } catch {
      return [];
    }
  }

  add(did: Did, label: string) {
    // Don't add duplicates
    if (this.communities.some((c) => c.did === did)) return;
    this.communities.push({ did, label, addedAt: Date.now() });
  }

  remove(did: Did) {
    const index = this.communities.findIndex((c) => c.did === did);
    if (index !== -1) this.communities.splice(index, 1);
  }
}

export const managedCommunities = new ManagedCommunities();

// Auto-persist on every change
$effect.root(() => {
  $effect(() => {
    globalThis.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify($state.snapshot(managedCommunities.communities)),
    );
  });
});
