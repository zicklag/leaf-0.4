<script lang="ts">
  import { page } from '$app/state';
  import { Tabs } from '@foxui/core';

  let { children } = $props();

  const did = $derived(page.params.did as string);

  const activeTab = $derived.by(() => {
    const path = page.url.pathname;
    if (path.endsWith('/spaces')) return 'Spaces';
    return 'Policy';
  });
</script>

<div class="flex flex-col h-full">
  <!-- Tabs navigation -->
  <Tabs
    items={[
      { name: 'Policy', href: `/dashboard/${encodeURIComponent(did)}` },
      { name: 'Spaces', href: `/dashboard/${encodeURIComponent(did)}/spaces` },
    ]}
    active={activeTab}
    class="px-4 pt-0"
  />

  <!-- Page content -->
  <div class="flex-1 min-h-0 h-full">
    {@render children()}
  </div>
</div>