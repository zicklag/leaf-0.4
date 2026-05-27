<script lang="ts">
  import { Button, Box } from '@foxui/core';
  import {
    managedCommunities,
    selectedArbiterDid,
    removeManagedCommunity,
  } from '$lib/store.svelte';

  import LookupArbiterSheet from './LookupArbiterSheet.svelte';

  let communities = $state<typeof managedCommunities>();
  let selectedDid = $state<string | null>(null);
  let showLookup = $state(false);

  managedCommunities.subscribe((v) => (communities = v));
  selectedArbiterDid.subscribe((v) => (selectedDid = v));

  function select(did: string) {
    selectedArbiterDid.set(did);
  }
</script>

<aside
  class="w-64 shrink-0 border-r border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 flex flex-col overflow-hidden"
>
  <div class="flex-1 overflow-y-auto p-3 space-y-1">
    <h2
      class="text-xs font-semibold uppercase tracking-wider text-base-500 dark:text-base-500 px-2 py-1"
    >
      Communities
    </h2>

    {#if !communities || communities.length === 0}
      <p class="text-sm text-base-400 dark:text-base-500 px-2 py-4">
        No communities yet. Look one up or create a new one.
      </p>
    {:else}
      {#each communities as community (community.did)}
        <div
          role="button"
          tabindex="0"
          class="w-full text-left px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-between group cursor-pointer {selectedDid ===
          community.did
            ? 'bg-accent-100 dark:bg-accent-900/30 text-base-900 dark:text-base-50'
            : 'hover:bg-base-200 dark:hover:bg-base-800'}"
          onclick={() => select(community.did)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              select(community.did);
            }
          }}
        >
          <span class="truncate flex-1 font-medium">{community.label}</span>
          <span
            class="text-xs text-base-400 dark:text-base-500 truncate max-w-24 hidden group-hover:block font-mono"
          >
            {community.did.slice(0, 16)}…
          </span>
          <button
            class="ml-1 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-base-300 dark:hover:bg-base-700 text-base-400 dark:text-base-500 hover:text-red-500 transition-all"
            onclick={(e) => {
              e.stopPropagation();
              removeManagedCommunity(community.did);
              if (selectedDid === community.did) selectedArbiterDid.set(null);
            }}
            aria-label="Remove {community.label}"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      {/each}
    {/if}
  </div>

  <div class="p-3 border-t border-base-200 dark:border-base-800 space-y-2">
    <Button class="w-full" size="sm" variant="secondary" onclick={() => (showLookup = true)}>
      Lookup Community
    </Button>
    <Button class="w-full" size="sm" variant="secondary" disabled>Create Community</Button>
  </div>
</aside>

{#if showLookup}
  <LookupArbiterSheet bind:open={showLookup} />
{/if}
