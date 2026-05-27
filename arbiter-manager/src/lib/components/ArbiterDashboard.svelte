<script lang="ts">
  import { Tabs } from '@foxui/core';
  import type { Did } from '$lib/types';
  import SpacesTab from './SpacesTab.svelte';
  import MembersTab from './MembersTab.svelte';
  import ConfigTab from './ConfigTab.svelte';
  import PolicyTab from './PolicyTab.svelte';

  let { did }: { did: Did } = $props();

  let activeTab = $state('Spaces');
</script>

<div class="flex flex-col h-full">
  <div class="px-4 pt-3 pb-0 border-b border-base-200 dark:border-base-800">
    <div class="flex items-center justify-between mb-0">
      <h2 class="text-lg font-semibold text-base-900 dark:text-base-50 font-mono text-sm truncate">
        {did}
      </h2>
    </div>
    <Tabs
      items={[
        { name: 'Spaces', onclick: () => (activeTab = 'Spaces') },
        { name: 'Members', onclick: () => (activeTab = 'Members') },
        { name: 'Config', onclick: () => (activeTab = 'Config') },
        { name: 'Policy', onclick: () => (activeTab = 'Policy') },
      ]}
      active={activeTab}
    />
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === 'Spaces'}
      <SpacesTab arbiterDid={did} />
    {:else if activeTab === 'Members'}
      <MembersTab arbiterDid={did} />
    {:else if activeTab === 'Config'}
      <ConfigTab arbiterDid={did} />
    {:else if activeTab === 'Policy'}
      <PolicyTab arbiterDid={did} />
    {/if}
  </div>
</div>
