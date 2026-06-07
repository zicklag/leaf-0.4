<script lang="ts">
  import { page } from '$app/state';
  import SpacesTab from '$lib/components/SpacesTab.svelte';

  const did = $derived(page.params.did as string);
</script>

<SpacesTab {did} />