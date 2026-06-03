<script lang="ts">
  import { Button, Box } from '@foxui/core';
  import type { Did } from '$lib/types';
  import PolicyEditor from './PolicyEditor.svelte';

  let { arbiterDid }: { arbiterDid: Did } = $props();

  let policy = $state('');
  let loading = $state(true);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);

  async function loadPolicy() {
    loading = true;
    error = null;
    try {
      // const { getSession } = await import('$lib/store.svelte');
      // const { ArbiterClient, XrpcRequestError } = await import('$lib/api');
      // const session = getSession();
      // if (!session) throw new Error('Not authenticated');
      // const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      // // Try to get policy from arbiter config or a dedicated endpoint
      // // For now, attempt to get config which may contain policy
      // const config = await client.getArbiterConfig(arbiterDid);
      // policy =
      //   ((config as Record<string, unknown>)?.policy as string) ||
      //   '# Enter your Rego policy here\n\nallow = true\n';
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function savePolicy() {
    saving = true;
    saveError = null;
    saveSuccess = false;
    try {
      // const { getSession } = await import('$lib/store.svelte');
      // const { ArbiterClient } = await import('$lib/api');
      // const session = getSession();
      // if (!session) throw new Error('Not authenticated');
      // const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      // // Update config with policy field
      // const config = await client.getArbiterConfig(arbiterDid);
      // await client.setArbiterConfig(arbiterDid, { ...config, policy });
      // saveSuccess = true;
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    if (arbiterDid) loadPolicy();
  });
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
      Policy
    </h3>
    {#if !loading && !error}
      <div class="flex items-center gap-2">
        {#if saveSuccess}
          <span class="text-xs text-emerald-600 dark:text-emerald-400">Saved</span>
        {/if}
        <Button size="sm" onclick={savePolicy} disabled={saving || loading}>
          {saving ? 'Saving…' : 'Save Policy'}
        </Button>
      </div>
    {/if}
  </div>

  <!-- {#if loading}
    <Box class="animate-pulse h-48" />
  {:else if error}
    <Box class="text-sm text-red-500 p-4">{error}</Box>
  {:else}
    <PolicyEditor bind:value={policy} />
    {#if saveError}
      <Box class="text-sm text-red-500 p-3">{saveError}</Box>
    {/if}
  {/if} -->
</div>
