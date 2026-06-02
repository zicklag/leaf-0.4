<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';

  $effect(() => {
    if (!auth.session) {
      goto('/', { replaceState: true });
    }
  });

  let { children } = $props();
</script>

{#if auth.session}
  <Sidebar />
  <main class="flex-1 overflow-auto">
    {@render children()}
  </main>
{/if}
