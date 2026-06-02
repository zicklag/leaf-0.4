<script lang="ts">
  import { page } from '$app/stores';
  import ArbiterDashboard from '$lib/components/ArbiterDashboard.svelte';

  let currentPage = $state(page);
  page.subscribe((p) => (currentPage = p));

  const did = $derived(currentPage.params.did as string);
</script>

<ArbiterDashboard {did} />
