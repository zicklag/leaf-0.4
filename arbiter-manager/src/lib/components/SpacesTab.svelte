<script lang="ts">
  import { Button, Badge, Box } from '@foxui/core';
  import { queryClient } from '$lib/query-client';
  import { getSession } from '$lib/store.svelte';
  import { ArbiterClient, XrpcRequestError } from '$lib/api';
  import type { Did, SpaceSummary } from '$lib/types';
  import CreateSpaceSheet from './CreateSpaceSheet.svelte';
  import ConfirmModal from './ConfirmModal.svelte';

  let { arbiterDid }: { arbiterDid: Did } = $props();

  let spaces: SpaceSummary[] = $state([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let showCreate = $state(false);
  let deletingKey = $state<string | null>(null);
  let deleteSpaceKey = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      const result = await client.listSpaces(arbiterDid);
      spaces = result.spaces;
    } catch (e) {
      if (e instanceof XrpcRequestError && e.isPermissionDenied) {
        error = "You don't have permission to view spaces on this arbiter.";
      } else {
        error = String(e);
      }
    } finally {
      loading = false;
    }
  }

  async function deletespace(spaceKey: string) {
    deletingKey = spaceKey;
    try {
      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      await client.deleteSpace(arbiterDid, spaceKey);
      spaces = spaces.filter((s) => s.key !== spaceKey);
      deleteSpaceKey = null;
    } catch (e) {
      error = String(e);
    } finally {
      deletingKey = null;
    }
  }

  // Load on mount
  $effect(() => {
    if (arbiterDid) load();
  });

  function handleCreated() {
    showCreate = false;
    load();
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
      Spaces
    </h3>
    <Button size="sm" onclick={() => (showCreate = true)}>Create Space</Button>
  </div>

  {#if loading}
    <div class="space-y-2">
      {#each Array(3) as _}
        <Box class="animate-pulse h-16" />
      {/each}
    </div>
  {:else if error}
    <Box class="text-sm text-red-500 p-4">{error}</Box>
  {:else if spaces.length === 0}
    <Box class="text-sm text-base-500 dark:text-base-500 p-6 text-center">
      No spaces yet. Create your first space to get started.
    </Box>
  {:else}
    <div class="space-y-2">
      {#each spaces as space (space.key)}
        <Box class="flex items-center justify-between p-4">
          <div class="flex items-center gap-3 min-w-0">
            <div class="min-w-0">
              <p class="font-medium text-base-900 dark:text-base-50 truncate">
                {space.key}
              </p>
              <p class="text-xs text-base-500 dark:text-base-500 font-mono truncate">
                {space.spaceType}
              </p>
            </div>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <Badge variant="secondary" size="sm"
              >{space.spaceType.split('.').pop() || 'space'}</Badge
            >
            <Button
              size="sm"
              variant="ghost"
              class="text-red-500 hover:text-red-600"
              onclick={() => (deleteSpaceKey = space.key)}
            >
              Delete
            </Button>
          </div>
        </Box>
      {/each}
    </div>
  {/if}
</div>

{#if showCreate}
  <CreateSpaceSheet bind:open={showCreate} {arbiterDid} oncreated={handleCreated} />
{/if}

{#if deleteSpaceKey}
  <ConfirmModal
    bind:open={deleteSpaceKey}
    title="Delete Space"
    description="Are you sure you want to delete the space '{deleteSpaceKey}'? This cannot be undone."
    confirmLabel="Delete Space"
    danger={true}
    onconfirm={() => deleteSpace(deleteSpaceKey!)}
  />
{/if}
