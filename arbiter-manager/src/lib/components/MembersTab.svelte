<script lang="ts">
  import { Button, Badge, Box, Select } from '@foxui/core';
  import type { Did, SpaceKey, SpaceSummary, ResolvedMemberEntry, MemberUnion } from '$lib/types';
  import { ACCESS_LABELS, ALL_ACCESSES } from '$lib/types';
  import AddMemberSheet from './AddMemberSheet.svelte';
  import ConfirmModal from './ConfirmModal.svelte';

  let { arbiterDid }: { arbiterDid: Did } = $props();

  let spaces: SpaceSummary[] = $state([]);
  let selectedSpace = $state<SpaceKey | null>(null);
  let members: ResolvedMemberEntry[] = $state([]);
  let loading = $state(true);
  let loadingMembers = $state(false);
  let error = $state<string | null>(null);
  let showAddMember = $state(false);
  let removeMemberDid = $state<string | null>(null);

  async function loadSpaces() {
    loading = true;
    error = null;
    try {
      // const session = getSession();
      // if (!session) throw new Error('Not authenticated');
      // const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      // const result = await client.listSpaces(arbiterDid);
      // spaces = result.spaces;
      // if (result.spaces.length > 0 && !selectedSpace) {
      //   selectedSpace = result.spaces[0].key;
      // }
    } catch (e) {
      // if (e instanceof XrpcRequestError && e.isPermissionDenied) {
      //   error = "You don't have permission to view spaces on this arbiter.";
      // } else {
      //   error = String(e);
      // }
    } finally {
      loading = false;
    }
  }

  async function loadMembers() {
    if (!selectedSpace) return;
    loadingMembers = true;
    try {
      // const session = getSession();
      // if (!session) throw new Error('Not authenticated');
      // const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      // const result = await client.resolveSpaceMembers(arbiterDid, selectedSpace);
      // members = result.members;
    } catch (e) {
      // if (e instanceof XrpcRequestError && e.isPermissionDenied) {
      //   members = [];
      //   error = "You don't have permission to view members of this space.";
      // } else {
      //   error = String(e);
      // }
    } finally {
      loadingMembers = false;
    }
  }

  async function removeMember(did: string) {
    if (!selectedSpace) return;
    try {
      // const session = getSession();
      // if (!session) throw new Error('Not authenticated');
      // const client = new ArbiterClient(session.pdsUrl, session.accessJwt);
      // const member: MemberUnion = { $type: 'town.muni.arbiter.defs#memberDid', did };
      // await client.removeSpaceMember(arbiterDid, selectedSpace, member);
      // members = members.filter((m) => m.did !== did);
      // removeMemberDid = null;
    } catch (e) {
      error = String(e);
    }
  }

  function handleMemberAdded() {
    showAddMember = false;
    loadMembers();
  }

  $effect(() => {
    if (arbiterDid) loadSpaces();
  });

  $effect(() => {
    if (selectedSpace) loadMembers();
  });

  function getAccessLabel(access: Record<string, unknown>): string {
    // Try to match access object against known access level keys
    for (const level of ALL_ACCESSES) {
      // if (access[level] !== undefined || access.$type?.endsWith(level)) {
      //   return ACCESS_LABELS[level];
      // }
    }
    // Fallback: show the access type
    return (access.$type as string)?.split('#').pop() || 'Custom';
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">
      Members
    </h3>
    <div class="flex items-center gap-2">
      {#if spaces.length > 0}
        <Select
          type="single"
          items={spaces.map((s) => ({ value: s.key, label: s.key }))}
          placeholder="Select space"
        />
        <Button size="sm" onclick={() => (showAddMember = true)}>Add Member</Button>
      {/if}
    </div>
  </div>

  {#if loading}
    <Box class="animate-pulse h-24" />
  {:else if error && members.length === 0}
    <Box class="text-sm text-red-500 p-4">{error}</Box>
  {:else if !selectedSpace}
    <Box class="text-sm text-base-500 dark:text-base-500 p-6 text-center">
      Select a space to view its members.
    </Box>
  {:else if loadingMembers}
    <Box class="animate-pulse h-24" />
  {:else if members.length === 0}
    <Box class="text-sm text-base-500 dark:text-base-500 p-6 text-center">
      No members in this space yet.
    </Box>
  {:else}
    <div class="space-y-2">
      {#each members as member (member.did)}
        <Box class="flex items-center justify-between p-3">
          <div class="flex items-center gap-3 min-w-0">
            <div
              class="w-8 h-8 rounded-full bg-accent-200 dark:bg-accent-800 flex items-center justify-center text-xs font-semibold text-accent-700 dark:text-accent-300 shrink-0"
            >
              {member.did.charAt(0).toUpperCase()}
            </div>
            <div class="min-w-0">
              <p class="text-sm font-medium text-base-900 dark:text-base-50 truncate font-mono">
                {member.did}
              </p>
            </div>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <Badge size="sm">{getAccessLabel(member.access)}</Badge>
            <Button
              size="sm"
              variant="ghost"
              class="text-red-500 hover:text-red-600"
              onclick={() => (removeMemberDid = member.did)}
            >
              Remove
            </Button>
          </div>
        </Box>
      {/each}
    </div>
  {/if}
</div>

{#if showAddMember}
  <AddMemberSheet
    open={false}
    {arbiterDid}
    spaceKey={selectedSpace ?? ''}
    onadded={handleMemberAdded}
  />
{/if}

{#if removeMemberDid}
  <ConfirmModal
    open={false}
    title="Remove Member"
    description="Are you sure you want to remove this member from the space?"
    confirmLabel="Remove"
    danger={true}
    onconfirm={() => removeMember(removeMemberDid!)}
  />
{/if}
