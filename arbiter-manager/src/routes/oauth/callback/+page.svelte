<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';

  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      const { processOAuthCallback } = await import('$lib/oauth-callback');
      const success = await processOAuthCallback();
      if (success) {
        goto('/', { replaceState: true });
      } else {
        error = 'Authentication failed. Please try again.';
      }
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="flex items-center justify-center min-h-screen">
  {#if error}
    <div class="text-center">
      <h2 class="text-lg font-semibold text-red-500">Authentication Error</h2>
      <p class="text-sm text-base-600 mt-2">{error}</p>
      <a href="/" class="text-accent-500 hover:text-accent-600 mt-4 inline-block">Go back</a>
    </div>
  {:else}
    <div
      class="animate-spin w-6 h-6 border-2 border-accent-500 border-t-transparent rounded-full"
    ></div>
  {/if}
</div>
