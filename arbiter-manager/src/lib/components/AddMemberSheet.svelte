<script lang="ts">
  import { Sheet, Input, Button, Select } from '@foxui/core';
  import { getSession } from '$lib/store.svelte';
  import { ArbiterClient } from '$lib/api';
  import type { Did, SpaceKey, MemberUnion, Access } from '$lib/types';
  import { ALL_ACCESSES, ACCESS_LABELS } from '$lib/types';

  let {
    open,
    arbiterDid,
    spaceKey,
    onadded,
  }: {
    open: boolean;
    arbiterDid: Did;
    spaceKey: SpaceKey;
    onadded?: () => void;
  } = $props();

  let memberType: 'did' | 'localSpace' | 'remoteSpace' = $state('did');
  let memberDid = $state('');
  let localSpaceKey = $state('');
  let remoteArbiterDid = $state('');
  let remoteSpaceKey = $state('');
  let accessLevel = $state<string>('IsMember');
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function add() {
    loading = true;
    error = null;

    try {
      let member: MemberUnion;

      switch (memberType) {
        case 'did':
          if (!memberDid.trim()) {
            error = 'DID is required';
            loading = false;
            return;
          }
          member = { $type: 'town.muni.arbiter.defs#memberDid', did: memberDid.trim() };
          break;
        case 'localSpace':
          if (!localSpaceKey.trim()) {
            error = 'Space key is required';
            loading = false;
            return;
          }
          member = {
            $type: 'town.muni.arbiter.defs#memberLocalSpace',
            spaceKey: localSpaceKey.trim(),
          };
          break;
        case 'remoteSpace':
          if (!remoteArbiterDid.trim() || !remoteSpaceKey.trim()) {
            error = 'Remote arbiter DID and space key are required';
            loading = false;
            return;
          }
          member = {
            $type: 'town.muni.arbiter.defs#memberRemoteSpace',
            arbiterDid: remoteArbiterDid.trim(),
            spaceKey: remoteSpaceKey.trim(),
          };
          break;
      }

      const access: Record<string, unknown> = { $type: `town.muni.arbiter.access#${accessLevel}` };

      const session = getSession();
      if (!session) throw new Error('Not authenticated');
      const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      await client.setSpaceMemberAccess(arbiterDid, spaceKey, member, access);

      // Reset form
      memberDid = '';
      localSpaceKey = '';
      remoteArbiterDid = '';
      remoteSpaceKey = '';
      onadded?.();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  let accessItems = ALL_ACCESSES.map((a) => ({ value: a, label: ACCESS_LABELS[a] }));
</script>

<Sheet bind:open title="Add Member" description="Add a member to the space.">
  <div class="flex flex-col gap-4 py-2">
    <!-- Member type selector -->
    <div class="flex flex-col gap-2">
      <label for="member-type-group" class="text-sm font-medium text-base-700 dark:text-base-300"
        >Member Type</label
      >
      <div id="member-type-group" class="flex gap-2" role="radiogroup">
        {#each ['did', 'localSpace', 'remoteSpace'] as type}
          <Button
            size="sm"
            variant={memberType === type ? 'primary' : 'secondary'}
            role="radio"
            aria-checked={memberType === type}
            onclick={() => (memberType = type as typeof memberType)}
          >
            {type === 'did' ? 'User DID' : type === 'localSpace' ? 'Local Space' : 'Remote Space'}
          </Button>
        {/each}
      </div>
    </div>

    <!-- DID input -->
    {#if memberType === 'did'}
      <div class="flex flex-col gap-2">
        <label for="member-did" class="text-sm font-medium text-base-700 dark:text-base-300">
          User DID
        </label>
        <Input id="member-did" bind:value={memberDid} placeholder="did:plc:abc123" />
      </div>
    {/if}

    <!-- Local space input -->
    {#if memberType === 'localSpace'}
      <div class="flex flex-col gap-2">
        <label for="local-space-key" class="text-sm font-medium text-base-700 dark:text-base-300">
          Space Key
        </label>
        <Input id="local-space-key" bind:value={localSpaceKey} placeholder="e.g. admin" />
      </div>
    {/if}

    <!-- Remote space inputs -->
    {#if memberType === 'remoteSpace'}
      <div class="flex flex-col gap-2">
        <label for="remote-arbiter" class="text-sm font-medium text-base-700 dark:text-base-300">
          Remote Arbiter DID
        </label>
        <Input id="remote-arbiter" bind:value={remoteArbiterDid} placeholder="did:plc:xyz789" />
      </div>
      <div class="flex flex-col gap-2">
        <label for="remote-space" class="text-sm font-medium text-base-700 dark:text-base-300">
          Remote Space Key
        </label>
        <Input id="remote-space" bind:value={remoteSpaceKey} placeholder="e.g. shared" />
      </div>
    {/if}

    <!-- Access level -->
    <div class="flex flex-col gap-2">
      <label for="access-level" class="text-sm font-medium text-base-700 dark:text-base-300">
        Access Level
      </label>
      <Select
        bind:value={accessLevel}
        type="single"
        items={accessItems}
        placeholder="Select access level"
      />
    </div>

    {#if error}
      <p class="text-sm text-red-500">{error}</p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (open = false)}>Cancel</Button>
    <Button onclick={add} disabled={loading}>
      {loading ? 'Adding…' : 'Add Member'}
    </Button>
  {/snippet}
</Sheet>
