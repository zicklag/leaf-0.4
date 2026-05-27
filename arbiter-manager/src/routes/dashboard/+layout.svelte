<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { isAuthenticated } from '$lib/store.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';

  let authenticated = $state(false);

  isAuthenticated.subscribe((v) => (authenticated = v));

  $effect(() => {
    if (!authenticated && browser) {
      goto('/', { replaceState: true });
    }
  });

  let { children } = $props();
</script>

{#if authenticated}
  <Sidebar />
  <main class="flex-1 overflow-auto">
    {@render children()}
  </main>
{/if}
