<script lang="ts">
  import { Button, Box } from '@foxui/core';
  import { AtprotoHandlePopup, type Profile } from '@foxui/all';
  import { managedCommunities } from '$lib/store.svelte';
  import { goto } from '$app/navigation';
  import PolicyEditor from './PolicyEditor.svelte';
  import { arbiter } from '$lib/arbiter';

  let { arbiterDid }: { arbiterDid?: string } = $props();

  // ── State ───────────────────────────────────────────────────────────────
  let selectedProfile = $state<Profile | undefined>();

  let policy = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveSuccess = $state(false);

  let checkingService = $state(false);
  let noArbiterService = $state(false);

  // ── Load policy when arbiterDid changes ─────────────────────────────────
  $effect(() => {
    if (arbiterDid) loadPolicy();
    else {
      policy = '';
      loading = false;
      error = null;
      noArbiterService = false;
    }
  });

  async function loadPolicy() {
    if (!arbiterDid) return;

    loading = true;
    error = null;
    noArbiterService = false;
    saveSuccess = false;

    try {
      // Verify the DID has an #arbiter service
      checkingService = true;
      const hasService = await arbiter.hasService(arbiterDid);
      checkingService = false;

      if (!hasService) {
        noArbiterService = true;
        loading = false;
        return;
      }

      const config = await arbiter.getConfig(arbiterDid);
      const rawPolicy =
        (config.policy as string) ?? '# Enter your Rego policy here\n\nallow = true\n';
      policy = rawPolicy;
    } catch (e) {
      error = arbiter.formatError(e);
    } finally {
      loading = false;
      checkingService = false;
    }
  }

  async function savePolicy() {
    if (!arbiterDid) return;

    saving = true;
    saveError = null;
    saveSuccess = false;

    try {
      // Fetch current config, update only the policy field
      const config = await arbiter.getConfig(arbiterDid);
      config.policy = policy;
      await arbiter.setConfig(arbiterDid, config);
      saveSuccess = true;
    } catch (e) {
      saveError = arbiter.formatError(e);
    } finally {
      saving = false;
    }
  }

  // ── Handle profile selection from the popup ─────────────────────────────
  function onSelect(profile: Profile) {
    selectedProfile = profile;
    const did = profile.did;
    if (!did || !did.startsWith('did:')) return;

    // Add to managed communities
    managedCommunities.add(did, profile.handle ?? did);
    goto(`/dashboard/${encodeURIComponent(did)}`);
  }
</script>

<div class="flex-1 overflow-auto">
  <div class="p-4 space-y-4">
    <!-- Community selector -->
    <div class="flex flex-col gap-1.5">
      <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
        Community
      </h3>
      <AtprotoHandlePopup onselected={onSelect} />
    </div>

    <!-- Loading / error states -->
    {#if checkingService}
      <Box class="flex items-center gap-3 p-4 text-sm text-base-500">
        <div
          class="animate-spin w-4 h-4 border-2 border-accent-500 border-t-transparent rounded-full"
        ></div>
        Checking arbiter service…
      </Box>
    {/if}

    {#if noArbiterService}
      <Box class="p-4 border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-900/20 rounded-lg">
        <p class="text-sm font-medium text-amber-800 dark:text-amber-300">
          No arbiter service found
        </p>
        <p class="text-xs text-amber-700 dark:text-amber-400 mt-1">
          This account does not have an <code class="font-mono">#arbiter</code> service endpoint on
          its DID document. Only accounts with a Muni Town arbiter can be managed here.
        </p>
      </Box>
    {/if}

    {#if loading}
      <Box class="animate-pulse h-48" />
    {:else if error}
      <Box class="p-4 border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/20 rounded-lg">
        <p class="text-sm font-medium text-red-800 dark:text-red-300">Failed to load policy</p>
        <p class="text-xs text-red-700 dark:text-red-400 mt-1">{error}</p>
        <div class="mt-3">
          <Button size="sm" variant="secondary" onclick={loadPolicy}>Retry</Button>
        </div>
      </Box>
    {:else if arbiterDid && !loading && !noArbiterService && !error}
      <!-- Policy editor -->
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
          Policy (Rego)
        </h3>
        <div class="flex items-center gap-2">
          {#if saveSuccess}
            <span class="text-xs text-emerald-600 dark:text-emerald-400">Saved</span>
          {/if}
          <Button size="sm" onclick={savePolicy} disabled={saving}>
            {saving ? 'Saving…' : 'Save Policy'}
          </Button>
        </div>
      </div>

      <div class="h-125">
        <PolicyEditor bind:value={policy} />
      </div>

      {#if saveError}
        <Box class="text-sm text-red-500 p-3">{saveError}</Box>
      {/if}
    {:else if !arbiterDid}
      <Box class="p-6 text-center text-sm text-base-500 dark:text-base-500">
        Search for a community above to view and edit its authorization policy.
      </Box>
    {/if}
  </div>
</div>