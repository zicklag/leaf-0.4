<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import MarketingHero from '$lib/components/MarketingHero.svelte';

  // If already authenticated on the root page, go to the dashboard
  $effect(() => {
    if (auth.session) {
      goto('/dashboard', { replaceState: true });
    }
  });
</script>

{#if !auth.session}
  <MarketingHero />
{/if}
