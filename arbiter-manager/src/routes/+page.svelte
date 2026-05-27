<script lang="ts">
  import { onMount } from 'svelte';
  import { isAuthenticated } from '$lib/store.svelte';
  import { processOAuthCallback } from '$lib/oauth-callback';
  import TopBar from '$lib/components/TopBar.svelte';
  import MarketingHero from '$lib/components/MarketingHero.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import ArbiterDashboard from '$lib/components/ArbiterDashboard.svelte';
  import WelcomePrompt from '$lib/components/WelcomePrompt.svelte';

  let authenticated = $state(false);
  let authReady = $state(false);

  isAuthenticated.subscribe((v) => (authenticated = v));

  onMount(async () => {
    await processOAuthCallback();
    authReady = true;
  });
</script>

{#if !authReady}
  <div class="flex items-center justify-center h-screen">
    <div
      class="animate-spin w-6 h-6 border-2 border-accent-500 border-t-transparent rounded-full"
    ></div>
  </div>
{:else}
  <div class="flex flex-col h-screen">
    <TopBar />

    {#if authenticated}
      <div class="flex flex-1 overflow-hidden">
        <Sidebar />
        <main class="flex-1 overflow-auto">
          <ArbiterDashboard />
          <WelcomePrompt />
        </main>
      </div>
    {:else}
      <MarketingHero />
    {/if}
  </div>
{/if}
