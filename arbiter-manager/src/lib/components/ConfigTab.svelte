<script lang="ts">
  import { Button, Box, Input } from '@foxui/core';
  import { getSession } from '$lib/store.svelte';
  import { ArbiterClient, XrpcRequestError } from '$lib/api';
  import type { Did } from '$lib/types';

  let { arbiterDid }: { arbiterDid: Did } = $props();

  let configJson = $state('');
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);

  async function load() {
    loading = true;
    error = null;
    try {
      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      const config = await client.getArbiterConfig(arbiterDid);
      configJson = JSON.stringify(config, null, 2);
    } catch (e) {
      if (e instanceof XrpcRequestError && e.isPermissionDenied) {
        error = "You don't have permission to view this arbiter's configuration.";
      } else {
        error = String(e);
      }
    } finally {
      loading = false;
    }
  }

  async function save() {
    saving = true;
    saveError = null;
    saveSuccess = false;
    try {
      const config = JSON.parse(configJson);
      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      await client.setArbiterConfig(arbiterDid, config);
      saveSuccess = true;
    } catch (e) {
      if (e instanceof SyntaxError) {
        saveError = 'Invalid JSON. Please check your syntax.';
      } else if (e instanceof XrpcRequestError && e.isPermissionDenied) {
        saveError = "You don't have permission to update this arbiter's configuration.";
      } else {
        saveError = String(e);
      }
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    if (arbiterDid) load();
  });
</script>

<div class="space-y-4 max-w-2xl">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
      Arbiter Configuration
    </h3>
    {#if !error && configJson}
      <Button size="sm" onclick={save} disabled={saving}>
        {saving ? 'Saving…' : 'Save Config'}
      </Button>
    {/if}
  </div>

  {#if loading}
    <Box class="animate-pulse h-32" />
  {:else if error}
    <Box class="text-sm text-red-500 p-4">{error}</Box>
  {:else}
    <Box class="p-0 overflow-hidden">
      <textarea
        class="w-full min-h-48 p-4 text-sm font-mono bg-transparent border-0 outline-none resize-y text-base-900 dark:text-base-50"
        bind:value={configJson}
        spellcheck="false"
      ></textarea>
    </Box>

    {#if saveError}
      <Box class="text-sm text-red-500 p-3">{saveError}</Box>
    {/if}
    {#if saveSuccess}
      <Box class="text-sm text-emerald-600 dark:text-emerald-400 p-3">
        Configuration saved successfully.
      </Box>
    {/if}
  {/if}
</div>
