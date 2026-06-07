<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { Button } from '@foxui/core';
  import { managedCommunities } from '$lib/store.svelte';
  import LookupArbiterSheet from './LookupArbiterSheet.svelte';

  let showLookup = $state(false);

  let currentDid = $derived(page.params.did as string | undefined);

  function select(did: string) {
    goto(`/dashboard/${encodeURIComponent(did)}`);
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

    {#if !managedCommunities.communities || managedCommunities.communities.length === 0}
      <p class="text-sm text-base-400 dark:text-base-500 px-2 py-4">
        No communities yet. Look one up or create a new one.
      </p>
    {:else}
      {#each managedCommunities.communities as community (community.did)}
        <div
          role="button"
          tabindex="0"
          class="w-full text-left px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-between group cursor-pointer {currentDid ===
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
              managedCommunities.remove(community.did);
              if (currentDid === community.did) goto('/dashboard');
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