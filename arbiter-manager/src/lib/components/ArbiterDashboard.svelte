<script lang="ts">
  import type { Did } from '$lib/types';
  import PolicyTab from './PolicyTab.svelte';

  let { did }: { did: Did } = $props();
</script>

<div class="flex flex-col h-full">
  <div class="px-4 pt-3 pb-3 border-b border-base-200 dark:border-base-800">
    <h2 class="text-lg font-semibold text-base-900 dark:text-base-50 font-mono truncate">
      {did}
    </h2>
  </div>

  <PolicyTab arbiterDid={did} />
</div>
