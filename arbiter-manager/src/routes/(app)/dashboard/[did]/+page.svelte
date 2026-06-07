<script lang="ts">
  import { page } from '$app/state';
  import ArbiterDashboard from '$lib/components/ArbiterDashboard.svelte';

  const did = $derived(page.params.did as string);
</script>

<ArbiterDashboard {did} />