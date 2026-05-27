<script lang="ts">
  import { Sheet, Input, Button } from '@foxui/core';
  import { getSession } from '$lib/store.svelte';
  import { ArbiterClient } from '$lib/api';
  import type { Did, SpaceKey } from '$lib/types';

  let {
    open,
    arbiterDid,
    oncreated,
  }: {
    open: boolean;
    arbiterDid: Did;
    oncreated?: () => void;
  } = $props();

  let spaceKey = $state('');
  let spaceType = $state('town.muni.arbiter.space.config.default');
  let configJson = $state('{}');
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function create() {
    if (!spaceKey.trim()) {
      error = 'Space key is required';
      return;
    }

    loading = true;
    error = null;

    try {
      let config: Record<string, unknown> = {};
      try {
        config = JSON.parse(configJson);
      } catch {
        error = 'Invalid config JSON';
        loading = false;
        return;
      }

      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      await client.createSpace(arbiterDid, spaceKey.trim(), spaceType.trim(), config);

      // Reset form
      spaceKey = '';
      configJson = '{}';
      oncreated?.();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<Sheet bind:open title="Create Space" description="Add a new space to your arbiter.">
  <div class="flex flex-col gap-4 py-2">
    <div class="flex flex-col gap-2">
      <label for="space-key" class="text-sm font-medium text-base-700 dark:text-base-300">
        Space Key
      </label>
      <Input id="space-key" bind:value={spaceKey} placeholder="e.g. admin, team, moderators" />
    </div>

    <div class="flex flex-col gap-2">
      <label for="space-type" class="text-sm font-medium text-base-700 dark:text-base-300">
        Space Type
      </label>
      <Input
        id="space-type"
        bind:value={spaceType}
        placeholder="town.muni.arbiter.space.config.default"
      />
    </div>

    <div class="flex flex-col gap-2">
      <label for="space-config" class="text-sm font-medium text-base-700 dark:text-base-300">
        Config (JSON)
      </label>
      <textarea
        id="space-config"
        class="w-full min-h-24 p-3 text-sm font-mono border border-base-200 dark:border-base-700 rounded-lg bg-base-50 dark:bg-base-950 text-base-900 dark:text-base-50 outline-none resize-y"
        bind:value={configJson}
        spellcheck="false"
      ></textarea>
    </div>

    {#if error}
      <p class="text-sm text-red-500">{error}</p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (open = false)}>Cancel</Button>
    <Button onclick={create} disabled={loading}>
      {loading ? 'Creating…' : 'Create Space'}
    </Button>
  {/snippet}
</Sheet>
