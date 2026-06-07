<script lang="ts">
  import { Box, Button } from '@foxui/core';
  import { managedCommunities } from '$lib/store.svelte';
  import LookupArbiterSheet from './LookupArbiterSheet.svelte';

  let showLookup = $state(false);
</script>

<div class="flex items-center justify-center h-full p-8">
  <Box class="max-w-md w-full text-center space-y-4 p-8">
    {#if managedCommunities.communities && managedCommunities.communities.length > 0}
      <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Select a Community</h2>
      <p class="text-sm text-base-600 dark:text-base-400">
        Choose a community from the sidebar to view and manage its authorization settings.
      </p>
    {:else}
      <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Get Started</h2>
      <p class="text-sm text-base-600 dark:text-base-400">
        Look up an existing community by its DID or handle, or create a new one.
      </p>
      <div class="flex flex-col gap-2 pt-2">
        <Button onclick={() => (showLookup = true)}>Lookup a Community</Button>
      </div>
    {/if}
  </Box>
</div>

{#if showLookup}
  <LookupArbiterSheet bind:open={showLookup} />
{/if}
