<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import { page } from '$app/state';
  import CommunitySelector from '$lib/components/CommunitySelector.svelte';
  import { managedCommunities } from '$lib/store.svelte';

  let { children } = $props();

  const did = $derived(page.params.did as string);

  const recentAccounts = $derived(managedCommunities.communities);
  let showRecent = $state(false);

  $effect(() => {
    if (!auth.session) {
      goto('/', { replaceState: true });
    }
  });
</script>

{#if auth.session}
  <div class="flex flex-col h-full basis-250 min-w-0">
    <!-- Top bar: recent accounts + community selector (shared across all dashboard pages) -->
    <div
      class="flex items-center gap-3 px-4 pt-3 pb-3 border-b border-base-200 dark:border-base-800"
    >
      <div class="relative">
        <button
          class="flex items-center gap-1 text-xs text-base-500 hover:text-base-700 dark:hover:text-base-300 transition-colors px-2 py-1 rounded hover:bg-base-200 dark:hover:bg-base-800"
          onclick={() => (showRecent = !showRecent)}
          onblur={() => setTimeout(() => (showRecent = false), 200)}
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            ><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" /><circle
              cx="12"
              cy="7"
              r="4"
            /></svg
          >
          <span>Recent</span>
          {#if showRecent}
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"><path d="M18 15l-6-6-6 6" /></svg
            >
          {:else}
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"><path d="M6 9l6 6 6-6" /></svg
            >
          {/if}
        </button>
        {#if showRecent}
          <div
            class="absolute left-0 top-full mt-1 w-56 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 shadow-lg z-50 py-1"
          >
            {#if recentAccounts.length === 0}
              <p class="text-xs text-base-400 px-3 py-2">No recent accounts</p>
            {:else}
              {#each recentAccounts as account (account.did)}
                <a
                  href="/dashboard/{encodeURIComponent(account.did)}"
                  class="block px-3 py-2 text-sm hover:bg-base-200 dark:hover:bg-base-800 transition-colors {account.did ===
                  did
                    ? 'bg-accent-100 dark:bg-accent-900/30 text-accent-700 dark:text-accent-300'
                    : ''}"
                  onclick={() => (showRecent = false)}
                >
                  <span class="font-medium">{account.label}</span>
                  <span class="text-xs text-base-400 font-mono ml-2"
                    >{account.did.slice(0, 12)}…</span
                  >
                </a>
              {/each}
            {/if}
          </div>
        {/if}
      </div>

      <CommunitySelector {did} />
    </div>

    <main class="flex-1 overflow-hidden h-full">
      {@render children()}
    </main>
  </div>
{/if}
