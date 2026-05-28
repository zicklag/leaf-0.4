<script lang="ts">
  import type { Access } from '../lib/types';
  import { app } from '../lib/simulation-store.svelte';
  import { ALL_ACCESSES, ACCESS_LABELS } from '../lib/types';
  import { accessColor, parseMemberDid } from '../lib/utils';

  let { currentUser, selectedArbiter, selectedSpace, serverState, users } =
    $derived(app);

  // --- create space ---
  let newSpaceKey = $state('');

  // --- add member ---
  let showAddMember = $state(false);
  let newMemberType = $state<'MemberDid' | 'MemberLocalSpace' | 'MemberRemoteSpace'>('MemberDid');
  let newMemberValue = $state('');
  let newMemberAccess = $state<Access>('ReadMemberList');
  let remoteArbiterDid = $state('');
  let remoteSpaceKey = $state('');
  let memberFocusEl: HTMLElement | undefined = $state();

  let remoteSpaces = $derived(
    serverState?.arbiters.find((a) => a.did === remoteArbiterDid)?.spaces ?? [],
  );

  function resetMemberForm() {
    newMemberValue = '';
    remoteArbiterDid = '';
    remoteSpaceKey = '';
  }

  // Build an access config object from the selected level.
  function accessConfig(level: Access) {
    return { $type: 'town.muni.arbiter.config.accessLevel', level };
  }

  async function handleCreateSpace(e: Event) {
    e.preventDefault();
    const key = newSpaceKey.trim();
    if (!currentUser || !selectedArbiter || !key) return;
    newSpaceKey = '';
    const result = await app.processOperation(
      selectedArbiter.did, currentUser.did, key,
      {
        type: 'CreateSpace',
        spaceType: 'town.muni.arbiter.config.space',
        config: {
          $type: 'town.muni.arbiter.config.space',
          publicRecords: false,
          publicMembers: false,
        },
      },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', `Space "${key}" created`);
    } else {
      app.notifications.add('error', result.error ?? 'Failed to create space');
    }
  }

  async function handleAddMember(e: Event) {
    e.preventDefault();
    if (!currentUser || !selectedSpace) return;

    let member: { tag: string; value: unknown } | null = null;
    if (newMemberType === 'MemberRemoteSpace') {
      if (!remoteArbiterDid || !remoteSpaceKey) {
        app.notifications.add('error', 'Please select both a remote arbiter and space');
        return;
      }
      member = {
        tag: 'MemberRemoteSpace',
        value: { arbiterDid: remoteArbiterDid, spaceKey: remoteSpaceKey },
      };
    } else {
      if (!newMemberValue.trim()) return;
      member = { tag: newMemberType, value: newMemberValue.trim() };
    }

    const result = await app.processOperation(
      selectedArbiter!.did, currentUser.did, selectedSpace.key,
      {
        type: 'SetSpaceMemberAccess',
        member: member!,
        access: accessConfig(newMemberAccess),
      },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Member access set');
      resetMemberForm();
      setTimeout(() => memberFocusEl?.focus(), 50);
    } else {
      app.notifications.add('error', result.error ?? 'Failed to set member');
    }
  }

  async function handleRemoveMember(entry: { did: string }) {
    if (!currentUser || !selectedSpace) return;
    // Extract the space key from the full DID for local/remote spaces
    const memberValue = entry.did.startsWith('space:')
      ? (() => { const rest = entry.did.slice(6); const i = rest.lastIndexOf('/'); return i >= 0 ? rest.slice(i + 1) : rest; })()
      : entry.did.includes('|')
        ? (() => { const parts = entry.did.split('|'); return { arbiterDid: parts[0], spaceKey: parts[2] ?? parts[1] }; })()
        : entry.did;
    const result = await app.processOperation(
      selectedArbiter!.did, currentUser.did, selectedSpace.key,
      {
        type: 'RemoveSpaceMember',
        member: { tag: entry.did.startsWith('space:') ? 'MemberLocalSpace' : entry.did.includes('|') ? 'MemberRemoteSpace' : 'MemberDid', value: memberValue },
      },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Member removed');
    } else {
      app.notifications.add('error', result.error ?? 'Failed to remove member');
    }
  }

  async function handleDeleteSpace() {
    if (!currentUser || !selectedSpace) return;
    const result = await app.processOperation(
      selectedArbiter!.did, currentUser.did, selectedSpace.key,
      { type: 'DeleteSpace' },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Space deleted');
      app.selectArbiter(selectedArbiter!.did);
    } else {
      app.notifications.add('error', result.error ?? 'Failed to delete space');
    }
  }

  async function handleDeleteArbiter() {
    if (!currentUser || !selectedArbiter) return;
    const result = await app.processOperation(
      selectedArbiter.did, currentUser.did, '$admin',
      { type: 'DeleteArbiter' },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Arbiter deleted');
      app.selectArbiter(null);
    } else {
      app.notifications.add('error', result.error ?? 'Failed to delete arbiter');
    }
  }

  async function handleConfigureSpace() {
    if (!currentUser || !selectedSpace) return;
    const result = await app.processOperation(
      selectedArbiter!.did, currentUser.did, selectedSpace.key,
      {
        type: 'SetSpaceConfig',
        spaceType: selectedSpace.spaceType,
        config: { ...selectedSpace.config },
      },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Space configured');
    } else {
      app.notifications.add('error', result.error ?? 'Failed to configure space');
    }
  }

  function memberDisplay(entry: { did: string }): string {
    const info = parseMemberDid(entry.did);
    switch (info.kind) {
      case 'user': return `👤 ${info.display}`;
      case 'localspace': return `📁 ${info.display}`;
      case 'remotespace': return `🌐 ${info.display}`;
    }
  }

  function accessLevelStr(access: Record<string, unknown>): string {
    if (typeof access.level === 'string') return access.level;
    return 'ReadMemberList';
  }
</script>

<section class="action-panel">
  <div class="section-header">
    <h3>Actions</h3>
  </div>

  {#if !currentUser}
    <p class="empty-hint">Select a user from the list above.</p>
  {:else}
    {#if selectedArbiter}
      <div class="context-label mono">
        Arbiter: {selectedArbiter.did}
      </div>

      <form class="action-form" onsubmit={handleCreateSpace}>
        <label for="space-key">Create Space</label>
        <div class="input-row">
          <input
            id="space-key"
            type="text"
            placeholder="e.g. team, project, …"
            bind:value={newSpaceKey}
          />
          <button class="btn btn-primary btn-sm" type="submit">Create</button>
        </div>
      </form>

      <button
        class="btn btn-danger btn-sm"
        style="margin-top: 8px; width: 100%"
        onclick={handleDeleteArbiter}
      >
        Delete Arbiter
      </button>

      {#if selectedSpace}
        <div class="context-label mono">
          Space: {selectedSpace.key}
        </div>

        <button
          class="btn btn-sm"
          style="width: 100%; margin-bottom: 8px"
          onclick={() => (showAddMember = !showAddMember)}
        >
          {showAddMember ? "Cancel" : "+ Add Member"}
        </button>

        {#if showAddMember}
          <form class="action-form" onsubmit={handleAddMember}>
            <label for="member-type">Member Type</label>
            <select
              id="member-type"
              bind:value={newMemberType}
              onchange={resetMemberForm}
            >
              <option value="MemberDid">User (DID)</option>
              <option value="MemberLocalSpace">Local Space</option>
              <option value="MemberRemoteSpace">Remote Space</option>
            </select>

            {#if newMemberType === "MemberDid"}
              <label for="member-user">DID</label>
              <select
                id="member-user"
                bind:value={newMemberValue}
                bind:this={memberFocusEl}
              >
                <option value="">-- Select DID --</option>
                {#each users as user}
                  <option value={user.did}>👤 {user.label} ({user.did})</option>
                {/each}
                {#each serverState?.arbiters ?? [] as arbiter}
                  <option value={arbiter.did}>🏛️ {arbiter.did}</option>
                {/each}
              </select>
            {:else if newMemberType === "MemberLocalSpace"}
              <label for="member-localspace">Space</label>
              <select
                id="member-localspace"
                bind:value={newMemberValue}
                bind:this={memberFocusEl}
              >
                <option value="">-- Select space --</option>
                {#each selectedArbiter!.spaces as space}
                  <option value={space.key}>{space.key}</option>
                {/each}
              </select>
            {:else if newMemberType === "MemberRemoteSpace"}
              <label for="member-remote-arbiter">Remote Arbiter</label>
              <select
                id="member-remote-arbiter"
                bind:value={remoteArbiterDid}
                bind:this={memberFocusEl}
              >
                <option value="">-- Select arbiter --</option>
                {#each serverState?.arbiters ?? [] as arbiter}
                  <option value={arbiter.did}>{arbiter.did}</option>
                {/each}
              </select>
              {#if remoteArbiterDid}
                <label for="member-remote-space">Remote Space</label>
                <select id="member-remote-space" bind:value={remoteSpaceKey}>
                  <option value="">-- Select space --</option>
                  {#each remoteSpaces as space}
                    <option value={space.key}>{space.key}</option>
                  {/each}
                </select>
              {/if}
            {/if}

            <label for="member-access">Access Level</label>
            <select id="member-access" bind:value={newMemberAccess}>
              {#each ALL_ACCESSES as a}
                <option value={a} style="color: {accessColor({$type: '', level: a})}">{ACCESS_LABELS[a]}</option>
              {/each}
            </select>

            <button class="btn btn-primary btn-sm" type="submit">Set Access</button>
          </form>
        {/if}

        <!-- Current raw members -->
        {#if selectedSpace.members.length > 0}
          <div class="member-actions">
            <span class="member-label">Current Members</span>
            {#each selectedSpace.members as m}
              <div class="member-row">
                <span class="mono truncate">{memberDisplay(m)}</span>
                <span class="access-badge">{accessLevelStr(m.access)}</span>
                <button
                  class="btn btn-sm remove-btn"
                  onclick={() => handleRemoveMember(m)}
                  title="Remove"
                >
                  ×
                </button>
              </div>
            {/each}
          </div>
        {/if}

        <button
          class="btn btn-danger btn-sm"
          style="margin-top: 8px; width: 100%"
          onclick={handleDeleteSpace}
        >
          Delete Space
        </button>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .action-panel { padding: 16px; flex: 1; }
  .section-header { margin-bottom: 12px; }
  .section-header h3 { font-weight: 600; }
  .empty-hint { color: var(--text-muted); font-size: 0.857rem; }
  .action-form { display: flex; flex-direction: column; gap: 4px; margin-bottom: 12px; padding-bottom: 12px; border-bottom: 1px solid var(--border-light); }
  .action-form label { margin-top: 6px; }
  .action-form label:first-child { margin-top: 0; }
  .input-row { display: flex; gap: 6px; }
  .input-row input { flex: 1; }
  .context-label { font-size: 0.714rem; color: var(--accent-text); background: var(--accent-subtle); padding: 3px 8px; border-radius: var(--radius-xs); margin-bottom: 8px; }
  .member-actions { margin-top: 4px; }
  .member-label { display: block; margin-bottom: 6px; font-size: 0.857rem; font-weight: 500; color: var(--text-secondary); }
  .member-row { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: var(--radius-xs); margin-bottom: 2px; font-size: 0.786rem; }
  .member-row:hover { background: var(--accent-subtle); }
  .access-badge { font-size: 0.714rem; padding: 1px 6px; border-radius: var(--radius-xs); background: var(--border); color: var(--text-secondary); flex-shrink: 0; }
  .remove-btn { flex-shrink: 0; }
</style>
