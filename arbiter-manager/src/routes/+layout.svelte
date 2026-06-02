<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from '$lib/components/TopBar.svelte';
  import { processOAuthCallback } from '$lib/oauth-callback';
  import '../app.css';

  let ready = $state(false);

  onMount(async () => {
    await processOAuthCallback();
    ready = true;
  });

  let { children } = $props();
</script>

{#if !ready}
  <div class="flex items-center justify-center h-screen">
    <div
      class="animate-spin w-6 h-6 border-2 border-accent-500 border-t-transparent rounded-full"
    ></div>
  </div>
{:else}
  <div class="flex flex-col h-screen">
    <TopBar />
    <div class="flex flex-1 justify-center">
      {@render children()}
    </div>
  </div>
{/if}
