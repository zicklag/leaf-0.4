<script lang="ts">
  import type { Did } from '$lib/types';
  import CommunitySelector from './CommunitySelector.svelte';
  import PolicyTab from './PolicyTab.svelte';

  let { did }: { did: Did } = $props();
</script>

<div class="flex flex-col h-full">
  <div class="flex items-center px-4 pt-3 pb-3 border-b border-base-200 dark:border-base-800">
    <CommunitySelector {did} />
  </div>

  <PolicyTab arbiterDid={did} />
</div>