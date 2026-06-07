<script lang="ts">
  import { Button, Box, Sheet } from '@foxui/core';
  import { AtprotoHandlePopup, type Profile } from '@foxui/all';
  import MonacoEditor from './MonacoEditor.svelte';
  import { arbiter } from '$lib/arbiter';

  let { did }: { did: string } = $props();

  // ── State ──────────────────────────────────────────────────────────────
  let spaces = $state<{ spaceKey: string; spaceType: string; config?: Record<string, unknown> }[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let selectedKey = $state<string | null>(null);
  let selectedSpace = $derived(spaces.find((s) => s.spaceKey === selectedKey) ?? null);

  // Space config editing
  let editingType = $state('');
  let editingConfigJson = $state('');
  let configSaving = $state(false);
  let configSaveError = $state<string | null>(null);
  let configSaveSuccess = $state(false);

  // Members
  let members = $state<{ member: Record<string, unknown>; access: Record<string, unknown> }[]>([]);
  let membersLoading = $state(false);
  let membersError = $state<string | null>(null);

  // Create space
  let showCreateModal = $state(false);
  let createKey = $state('');
  let createType = $state('');
  let createConfigJson = $state('{\n  "$type": "town.muni.arbiter.defs#yourType"\n}');
  let creating = $state(false);
  let createError = $state<string | null>(null);

  // Delete space
  let showDeleteConfirm = $state(false);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);

  // Add member
  let showAddMember = $state(false);
  let addMemberType = $state<'did' | 'localSpace' | 'remoteSpace'>('did');
  let addMemberDid = $state('');
  let addMemberLocalKey = $state('');
  let addMemberRemoteDid = $state('');
  let addMemberRemoteKey = $state('');
  let addMemberAccessJson = $state('{}');
  let addingMember = $state(false);
  let addMemberError = $state<string | null>(null);

  // Remove member
  let showRemoveMember = $state(false);
  let removingMember: Record<string, unknown> | null = $state(null);
  let removingMemberLabel = $state('');
  let removingMemberAccess = $state('');
  let removingMemberKey = $state(false);

  // ── Load spaces ────────────────────────────────────────────────────────
  $effect(() => {
    if (did) loadSpaces();
    else {
      spaces = [];
      selectedKey = null;
    }
  });

  async function loadSpaces() {
    if (!did) return;
    loading = true;
    error = null;
    try {
      spaces = await arbiter.listSpaces(did);
      // If selected space no longer exists, deselect
      if (selectedKey && !spaces.some((s) => s.spaceKey === selectedKey)) {
        selectedKey = null;
      }
    } catch (e) {
      error = arbiter.formatError(e);
    } finally {
      loading = false;
    }
  }

  // ── Select space ───────────────────────────────────────────────────────
  async function selectSpace(key: string) {
    selectedKey = key;
    const space = spaces.find((s) => s.spaceKey === key);
    if (space) {
      editingType = space.spaceType;
      editingConfigJson = JSON.stringify(space.config ?? {}, null, 2);
      configSaveSuccess = false;
      configSaveError = null;
      loadMembers(key);
    }
  }

  // ── Save space config ──────────────────────────────────────────────────
  async function saveSpaceConfig() {
    if (!selectedKey) return;
    configSaving = true;
    configSaveError = null;
    configSaveSuccess = false;

    try {
      let parsedConfig: Record<string, unknown>;
      try {
        parsedConfig = JSON.parse(editingConfigJson);
      } catch {
        configSaveError = 'Invalid JSON in config';
        return;
      }
      await arbiter.setSpaceConfig(did, selectedKey, editingType, parsedConfig);
      configSaveSuccess = true;

      // Refresh spaces list to reflect changes
      await loadSpaces();
    } catch (e) {
      configSaveError = arbiter.formatError(e);
    } finally {
      configSaving = false;
    }
  }

  // ── Members ────────────────────────────────────────────────────────────
  async function loadMembers(key: string) {
    membersLoading = true;
    membersError = null;
    try {
      members = await arbiter.getSpaceMembers(did, key);
    } catch (e) {
      membersError = arbiter.formatError(e);
    } finally {
      membersLoading = false;
    }
  }

  // ── Create space ───────────────────────────────────────────────────────
  async function createSpace() {
    if (!createKey.trim() || !createType.trim()) {
      createError = 'Space key and type are required';
      return;
    }

    creating = true;
    createError = null;

    try {
      let parsedConfig: Record<string, unknown>;
      try {
        parsedConfig = JSON.parse(createConfigJson);
      } catch {
        createError = 'Invalid JSON in config';
        return;
      }

      // Ensure $type is present
      if (!parsedConfig.$type) {
        createError = 'Config must include a "$type" field';
        return;
      }

      await arbiter.createSpace(did, createKey.trim(), createType.trim(), parsedConfig);
      showCreateModal = false;
      createKey = '';
      createType = '';
      createConfigJson = '{\n  "$type": "town.muni.arbiter.defs#yourType"\n}';
      await loadSpaces();
      selectedKey = createKey.trim();
    } catch (e) {
      createError = arbiter.formatError(e);
    } finally {
      creating = false;
    }
  }

  // ── Delete space ───────────────────────────────────────────────────────
  async function confirmDelete() {
    if (!selectedKey) return;
    deleting = true;
    deleteError = null;
    try {
      await arbiter.deleteSpace(did, selectedKey);
      showDeleteConfirm = false;
      selectedKey = null;
      await loadSpaces();
    } catch (e) {
      deleteError = arbiter.formatError(e);
    } finally {
      deleting = false;
    }
  }

  // ── Add member ─────────────────────────────────────────────────────────
  function memberLabel(m: { member: Record<string, unknown> }): string {
    const mb = m.member;
    const t = (mb as any).$type;
    if (typeof t === 'string' && t.endsWith('#memberDid')) return `DID: ${(mb as any).did ?? '?'}`;
    if (typeof t === 'string' && t.endsWith('#memberLocalSpace')) return `Local Space: ${(mb as any).spaceKey ?? '?'}`;
    if (typeof t === 'string' && t.endsWith('#memberRemoteSpace')) return `Remote: ${(mb as any).arbiterDid ?? '?'}/${(mb as any).spaceKey ?? '?'}`;
    return JSON.stringify(mb);
  }

  async function addMember() {
    addingMember = true;
    addMemberError = null;

    try {
      let member: Record<string, unknown>;
      let access: Record<string, unknown>;

      switch (addMemberType) {
        case 'did': {
          if (!addMemberDid.startsWith('did:')) {
            addMemberError = 'Please enter a valid DID';
            return;
          }
          member = { $type: 'town.muni.arbiter.defs#memberDid', did: addMemberDid };
          break;
        }
        case 'localSpace': {
          if (!addMemberLocalKey.trim()) {
            addMemberError = 'Space key is required';
            return;
          }
          member = { $type: 'town.muni.arbiter.defs#memberLocalSpace', spaceKey: addMemberLocalKey.trim() };
          break;
        }
        case 'remoteSpace': {
          if (!addMemberRemoteDid.startsWith('did:') || !addMemberRemoteKey.trim()) {
            addMemberError = 'Remote DID and space key are required';
            return;
          }
          member = {
            $type: 'town.muni.arbiter.defs#memberRemoteSpace',
            arbiterDid: addMemberRemoteDid,
            spaceKey: addMemberRemoteKey.trim(),
          };
          break;
        }
      }

      try {
        access = JSON.parse(addMemberAccessJson);
      } catch {
        addMemberError = 'Invalid JSON in access config';
        return;
      }

      if (selectedKey) {
        await arbiter.setSpaceMemberAccess(did, selectedKey, member, access);
        showAddMember = false;
        resetAddMemberForm();
        loadMembers(selectedKey);
      }
    } catch (e) {
      addMemberError = arbiter.formatError(e);
    } finally {
      addingMember = false;
    }
  }

  function resetAddMemberForm() {
    addMemberDid = '';
    addMemberLocalKey = '';
    addMemberRemoteDid = '';
    addMemberRemoteKey = '';
    addMemberAccessJson = '{}';
    addMemberError = null;
  }

  function openAddMember() {
    resetAddMemberForm();
    showAddMember = true;
  }

  // ── Remove member ──────────────────────────────────────────────────────
  function confirmRemoveMember(m: { member: Record<string, unknown>; access: Record<string, unknown> }) {
    removingMember = m.member;
    removingMemberLabel = memberLabel(m);
    removingMemberAccess = JSON.stringify(m.access, null, 2);
    showRemoveMember = true;
  }

  async function doRemoveMember() {
    if (!removingMember || !selectedKey) return;
    removingMemberKey = true;
    try {
      await arbiter.removeSpaceMember(did, selectedKey, removingMember);
      showRemoveMember = false;
      removingMember = null;
      loadMembers(selectedKey);
    } catch (e) {
      addMemberError = arbiter.formatError(e);
    } finally {
      removingMemberKey = false;
    }
  }

  // ── Handle profile selection ──────────────────────────────────────────
  function onMemberProfileSelect(profile: Profile) {
    addMemberDid = profile.did;
  }
</script>

<div class="flex h-full">
  <!-- Left panel: space list -->
  <div class="w-60 shrink-0 border-r border-base-200 dark:border-base-800 flex flex-col">
    <div class="flex items-center justify-between px-3 py-2.5 border-b border-base-200 dark:border-base-800">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-base-500">Spaces</h3>
      <Button size="sm" variant="secondary" onclick={() => (showCreateModal = true)}>
        + New
      </Button>
    </div>

    <div class="flex-1 overflow-y-auto p-2 space-y-1">
      {#if loading}
        <div class="flex items-center gap-2 p-3 text-sm text-base-400">
          <div class="animate-spin w-4 h-4 border-2 border-accent-500 border-t-transparent rounded-full"></div>
          Loading…
        </div>
      {:else if error}
        <Box class="p-3 text-sm text-red-500">{error}</Box>
        <Button size="sm" variant="secondary" onclick={loadSpaces}>Retry</Button>
      {:else if spaces.length === 0}
        <p class="text-sm text-base-400 dark:text-base-500 px-2 py-4 text-center">
          No spaces yet.
        </p>
      {:else}
        {#each spaces as space (space.spaceKey)}
          <button
            class="w-full text-left px-3 py-2 rounded-lg text-sm transition-colors cursor-pointer {selectedKey ===
            space.spaceKey
              ? 'bg-accent-100 dark:bg-accent-900/30 text-base-900 dark:text-base-50'
              : 'hover:bg-base-200 dark:hover:bg-base-800'}"
            onclick={() => selectSpace(space.spaceKey)}
          >
            <div class="font-medium truncate">{space.spaceKey}</div>
            <div class="text-xs text-base-400 font-mono truncate">{space.spaceType}</div>
          </button>
        {/each}
      {/if}
    </div>
  </div>

  <!-- Right panel: space detail -->
  <div class="flex-1 overflow-y-auto p-4 space-y-6">
    {#if !selectedKey}
      <div class="flex items-center justify-center h-full">
        <p class="text-sm text-base-400 dark:text-base-500">Select a space to view and edit its configuration.</p>
      </div>
    {:else}
      <!-- Space header -->
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold text-base-900 dark:text-base-50">{selectedKey}</h2>
          <p class="text-xs text-base-400 font-mono">{selectedSpace?.spaceType}</p>
        </div>
        <div class="flex items-center gap-2">
          {#if configSaveSuccess}
            <span class="text-xs text-emerald-600 dark:text-emerald-400">Saved</span>
          {/if}
          <Button variant="secondary" size="sm" class="text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/30 border-red-200 dark:border-red-800" onclick={() => (showDeleteConfirm = true)}>
            Delete
          </Button>
        </div>
      </div>

      <!-- Config editor -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">Config</h3>
          <div class="flex items-center gap-2">
            <label class="text-xs text-base-400" for="space-type-input">Type (NSID):</label>
            <input
              id="space-type-input"
              bind:value={editingType}
              class="text-xs px-2 py-1 rounded border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 text-base-700 dark:text-base-300 font-mono w-48"
              placeholder="town.muni.arbiter.defs#yourType"
            />
            <Button size="sm" onclick={saveSpaceConfig} disabled={configSaving}>
              {configSaving ? 'Saving…' : 'Save'}
            </Button>
          </div>
        </div>
        {#if configSaveError}
          <Box class="text-sm text-red-500 mb-2">{configSaveError}</Box>
        {/if}
        <div class="h-64">
          <MonacoEditor bind:value={editingConfigJson} language="json" />
        </div>
      </div>

      <!-- Members section -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm font-semibold text-base-700 dark:text-base-300 uppercase tracking-wider">Members</h3>
          <Button size="sm" variant="secondary" onclick={openAddMember}>
            + Add Member
          </Button>
        </div>

        {#if membersLoading}
          <div class="flex items-center gap-2 p-3 text-sm text-base-400">
            <div class="animate-spin w-4 h-4 border-2 border-accent-500 border-t-transparent rounded-full"></div>
            Loading members…
          </div>
        {:else if membersError}
          <Box class="text-sm text-red-500 mb-2">{membersError}</Box>
          <Button size="sm" variant="secondary" onclick={() => loadMembers(selectedKey!)}>Retry</Button>
        {:else if members.length === 0}
          <p class="text-sm text-base-400 dark:text-base-500">No members in this space yet.</p>
        {:else}
          <div class="space-y-2">
            {#each members as m, i (i)}
              <div class="flex items-start justify-between p-3 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950/50">
                <div class="flex-1 min-w-0">
                  <p class="text-sm font-medium text-base-800 dark:text-base-200">{memberLabel(m)}</p>
                  {#if Object.keys(m.access).length > 0}
                    <pre class="mt-1 text-xs text-base-400 font-mono whitespace-pre-wrap break-words max-h-24 overflow-y-auto">{JSON.stringify(m.access, null, 2)}</pre>
                  {/if}
                </div>
                <button
                  class="ml-2 p-1.5 rounded hover:bg-red-100 dark:hover:bg-red-900/30 text-base-400 hover:text-red-500 transition-colors shrink-0"
                  onclick={() => confirmRemoveMember(m)}
                  aria-label="Remove member"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<!-- Create Space Sheet -->
<Sheet
  bind:open={showCreateModal}
  title="Create Space"
  closeButton
>
  <div class="space-y-4 py-2">
    <div class="flex flex-col gap-1">
      <label class="text-xs font-medium text-base-600 dark:text-base-400" for="create-key">Space Key</label>
      <input
        id="create-key"
        bind:value={createKey}
        class="px-3 py-2 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 text-sm text-base-800 dark:text-base-200 font-mono"
        placeholder="e.g. mods, verified-users, team-a"
      />
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-xs font-medium text-base-600 dark:text-base-400" for="create-type">Space Type (NSID)</label>
      <input
        id="create-type"
        bind:value={createType}
        class="px-3 py-2 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 text-sm text-base-800 dark:text-base-200 font-mono"
        placeholder="e.g. town.muni.arbiter.defs#memberDid"
      />
    </div>

    <div class="flex flex-col gap-1">
      <span class="text-xs font-medium text-base-600 dark:text-base-400">Config (JSON)</span>
      <p class="text-xs text-base-400 mb-1">Must include a <code class="font-mono">$type</code> field.</p>
      <div class="h-48">
        <MonacoEditor bind:value={createConfigJson} language="json" />
      </div>
    </div>

    {#if createError}
      <Box class="text-sm text-red-500">{createError}</Box>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (showCreateModal = false)}>Cancel</Button>
    <Button onclick={createSpace} disabled={creating}>
      {creating ? 'Creating…' : 'Create'}
    </Button>
  {/snippet}
</Sheet>

<!-- Delete Space Confirmation Sheet -->
<Sheet
  bind:open={showDeleteConfirm}
  title="Delete Space"
  closeButton
>
  <p class="text-sm text-base-600 dark:text-base-400 py-2">
    Are you sure you want to delete <strong class="text-base-800 dark:text-base-200">{selectedKey}</strong>?
    This action cannot be undone.
  </p>
  {#if deleteError}
    <Box class="text-sm text-red-500 mt-2">{deleteError}</Box>
  {/if}

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (showDeleteConfirm = false)}>Cancel</Button>
    <Button variant="secondary" class="text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/30" onclick={confirmDelete} disabled={deleting}>
      {deleting ? 'Deleting…' : 'Delete'}
    </Button>
  {/snippet}
</Sheet>

<!-- Add Member Sheet -->
<Sheet
  bind:open={showAddMember}
  title="Add Member"
  closeButton
>
  <div class="space-y-4 py-2">
    <!-- Member type selector -->
    <div class="flex flex-col gap-1">
      <span class="text-xs font-medium text-base-600 dark:text-base-400">Member Type</span>
      <div class="flex gap-2">
        {#each ['did', 'localSpace', 'remoteSpace'] as t}
          <button
            class="px-3 py-1.5 text-xs rounded-lg border transition-colors {addMemberType === t
              ? 'bg-accent-100 dark:bg-accent-900/30 border-accent-300 dark:border-accent-700 text-accent-700 dark:text-accent-300'
              : 'border-base-200 dark:border-base-800 text-base-500 hover:bg-base-200 dark:hover:bg-base-800'}"
            onclick={() => (addMemberType = t as typeof addMemberType)}
          >
            {t === 'did' ? 'DID' : t === 'localSpace' ? 'Local Space' : 'Remote Space'}
          </button>
        {/each}
      </div>
    </div>

    {#if addMemberType === 'did'}
      <div class="flex flex-col gap-1">
        <span class="text-xs font-medium text-base-600 dark:text-base-400">DID</span>
        <AtprotoHandlePopup onselected={onMemberProfileSelect} />
        {#if addMemberDid}
          <p class="text-xs text-base-400 font-mono">{addMemberDid}</p>
        {/if}
      </div>
    {:else if addMemberType === 'localSpace'}
      <div class="flex flex-col gap-1">
        <label class="text-xs font-medium text-base-600 dark:text-base-400" for="add-local-key">Local Space Key</label>
        <input
          id="add-local-key"
          bind:value={addMemberLocalKey}
          class="px-3 py-2 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 text-sm text-base-800 dark:text-base-200 font-mono"
          placeholder="e.g. mods"
        />
      </div>
    {:else}
      <div class="flex flex-col gap-1">
        <span class="text-xs font-medium text-base-600 dark:text-base-400">Remote Arbiter DID</span>
        <AtprotoHandlePopup
          onselected={(p) => {
            addMemberRemoteDid = p.did;
          }}
        />
        {#if addMemberRemoteDid}
          <p class="text-xs text-base-400 font-mono">{addMemberRemoteDid}</p>
        {/if}
      </div>
      <div class="flex flex-col gap-1">
        <label class="text-xs font-medium text-base-600 dark:text-base-400" for="add-remote-key">Remote Space Key</label>
        <input
          id="add-remote-key"
          bind:value={addMemberRemoteKey}
          class="px-3 py-2 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 text-sm text-base-800 dark:text-base-200 font-mono"
          placeholder="e.g. mods"
        />
      </div>
    {/if}

    <!-- Access config -->
    <div class="flex flex-col gap-1">
      <span class="text-xs font-medium text-base-600 dark:text-base-400">Access Config (JSON)</span>
      <div class="h-32">
        <MonacoEditor bind:value={addMemberAccessJson} language="json" />
      </div>
    </div>

    {#if addMemberError}
      <Box class="text-sm text-red-500">{addMemberError}</Box>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (showAddMember = false)}>Cancel</Button>
    <Button onclick={addMember} disabled={addingMember}>
      {addingMember ? 'Adding…' : 'Add Member'}
    </Button>
  {/snippet}
</Sheet>

<!-- Remove Member Confirmation Sheet -->
<Sheet
  bind:open={showRemoveMember}
  title="Remove Member"
  closeButton
>
  <p class="text-sm text-base-600 dark:text-base-400 py-2">
    Are you sure you want to remove this member from <strong class="text-base-800 dark:text-base-200">{selectedKey}</strong>?
  </p>
  <div class="p-3 rounded-lg border border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950/50 mb-2">
    <p class="text-sm font-medium text-base-800 dark:text-base-200">{removingMemberLabel}</p>
    {#if removingMemberAccess && removingMemberAccess !== '{}'}
      <pre class="mt-1 text-xs text-base-400 font-mono">{removingMemberAccess}</pre>
    {/if}
  </div>
  {#if addMemberError}
    <Box class="text-sm text-red-500">{addMemberError}</Box>
  {/if}

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (showRemoveMember = false)}>Cancel</Button>
    <Button variant="secondary" class="text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/30" onclick={doRemoveMember} disabled={removingMemberKey}>
      {removingMemberKey ? 'Removing…' : 'Remove'}
    </Button>
  {/snippet}
</Sheet>