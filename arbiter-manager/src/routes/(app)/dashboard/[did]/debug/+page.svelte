<script lang="ts">
  import { page } from '$app/state';
  import DebugTab from '$lib/components/DebugTab.svelte';

  const did = $derived(page.params.did as string);
</script>

<DebugTab {did} />