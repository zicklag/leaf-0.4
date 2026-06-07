<script lang="ts">
  import { page } from '$app/state';
  import PolicyTab from '$lib/components/PolicyTab.svelte';

  const did = $derived(page.params.did as string);
</script>

<PolicyTab arbiterDid={did} />