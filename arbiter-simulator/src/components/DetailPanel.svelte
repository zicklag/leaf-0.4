<script lang="ts">
  import type { Member } from '../lib/types';
  import { app } from '../lib/simulation-store.svelte';
  import { accessLabel, accessColor, shortDid } from '../lib/utils';
  import { ALL_ACCESSES, ACCESS_LABELS } from '../lib/types';

  let {
    selectedSpaceMembers,
    selectedSpaceMissing,
    selectedSpaceError,
    currentUser,
    selectedArbiterDid,
    selectedSpaceKey,
    serverState,
    users,
  } = $derived(app);

  let selectedArbiter = $derived(
    serverState?.arbiters.find((a) => a.did === selectedArbiterDid) ?? null,
  );

  let selectedSpace = $derived(
    selectedArbiter?.spaces.find((s) => s.key === selectedSpaceKey) ?? null,
  );

  // Sort resolved members by access level (highest first), then by DID
  let sortedMembers = $derived(
    selectedSpaceMembers
      ? [...selectedSpaceMembers].sort((a, b) => {
          const levelDiff = accessLevelNum(b.access) - accessLevelNum(a.access);
          if (levelDiff !== 0) return levelDiff;
          return a.did.localeCompare(b.did);
        })
      : [],
  );

  let currentUserAccess = $derived(
    currentUser && selectedSpaceMembers
      ? selectedSpaceMembers.find((m) => m.did === currentUser.did)?.access ?? null
      : null,
  );

  function accessLevelNum(access: Record<string, unknown>): number {
    const level = typeof access.level === 'string' ? access.level : 'ReadMemberList';
    const idx = ALL_ACCESSES.indexOf(level as typeof ALL_ACCESSES[number]);
    return idx >= 0 ? idx : 0;
  }

  // --- Add member state ---
  let newMemberType = $state<'MemberDid' | 'MemberLocalSpace' | 'MemberRemoteSpace'>('MemberDid');
  let newMemberValue = $state('');
  let newMemberAccess = $state<(typeof ALL_ACCESSES)[number]>('ReadMemberList');
  let remoteArbiterDid = $state('');
  let remoteSpaceKey = $state('');

  let remoteSpaces = $derived(
    serverState?.arbiters.find((a) => a.did === remoteArbiterDid)?.spaces ?? [],
  );

  function resetMemberForm() {
    newMemberValue = '';
    remoteArbiterDid = '';
    remoteSpaceKey = '';
  }

  function accessConfig(level: string) {
    return { $type: 'town.muni.arbiter.config.accessLevel', level };
  }

  async function handleAddMember(e: Event) {
    e.preventDefault();
    if (!currentUser || !selectedSpace) return;

    let member: Member | null = null;
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
      selectedArbiterDid!, currentUser.did, selectedSpace.key,
      { type: 'SetSpaceMemberAccess', member: member!, access: accessConfig(newMemberAccess) },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Member access set');
      resetMemberForm();
    } else {
      app.notifications.add('error', result.status === 'error' ? result.error : 'Failed to set member');
    }
  }

  async function handleRemoveMember(memberEntry: { member: { tag: string; value: unknown } }) {
    if (!currentUser || !selectedSpace) return;
    const member = memberEntry.member as Member;
    const result = await app.processOperation(
      selectedArbiterDid!, currentUser.did, selectedSpace.key,
      { type: 'RemoveSpaceMember', member },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Member removed');
    } else {
      app.notifications.add('error', result.status === 'error' ? result.error : 'Failed to remove member');
    }
  }

  async function togglePublicMembers() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const result = await app.processOperation(
      selectedArbiterDid, currentUser.did, selectedSpace.key,
      {
        type: 'SetSpaceConfig',
        spaceType: selectedSpace.spaceType,
        config: {
          ...selectedSpace.config,
          publicMembers: !selectedSpace.config.publicMembers,
        },
      },
    );
    if (result.status === 'error') {
      app.notifications.add('error', `${currentUser?.label ?? currentUser.did}: ${result.error || 'Permission denied'}`);
    }
  }

  async function togglePublicRecords() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const result = await app.processOperation(
      selectedArbiterDid, currentUser.did, selectedSpace.key,
      {
        type: 'SetSpaceConfig',
        spaceType: selectedSpace.spaceType,
        config: {
          ...selectedSpace.config,
          publicRecords: !selectedSpace.config.publicRecords,
        },
      },
    );
    if (result.status === 'error') {
      app.notifications.add('error', `${currentUser?.label ?? currentUser.did}: ${result.error || 'Permission denied'}`);
    }
  }

  async function handleDeleteSpace() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const result = await app.processOperation(
      selectedArbiterDid, currentUser.did, selectedSpace.key,
      { type: 'DeleteSpace' },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', 'Space deleted');
      app.selectArbiter(selectedArbiterDid);
    } else {
      app.notifications.add('error', result.status === 'error' ? result.error : 'Failed to delete space');
    }
  }
</script>

{#if selectedSpace}
  <aside class="detail-panel">
    <!-- Space Header -->
    <div class="panel-header">
      <div class="panel-title">
        <span class="space-icon">{selectedSpace.key === '$admin' ? '👑' : '📁'}</span>
        <h3>{selectedSpace.key}</h3>
        <span class="space-arbiter mono">{selectedArbiterDid}</span>
      </div>
      <button
        class="btn btn-sm close-btn"
        onclick={() => app.selectArbiter(app.selectedArbiterDid!)}
        title="Close"
      >
        ×
      </button>
    </div>

    <!-- Configuration -->
    <section class="panel-section config-section">
      <h4>Configuration</h4>
      <button
        class="config-toggle"
        class:active={selectedSpace.config.publicMembers}
        onclick={togglePublicMembers}
      >
        <span class="config-label">Public Members</span>
        <span class="toggle-indicator">{selectedSpace.config.publicMembers ? 'On' : 'Off'}</span>
      </button>
      <button
        class="config-toggle"
        class:active={selectedSpace.config.publicRecords}
        onclick={togglePublicRecords}
      >
        <span class="config-label">Public Records</span>
        <span class="toggle-indicator">{selectedSpace.config.publicRecords ? 'On' : 'Off'}</span>
      </button>
    </section>

    <!-- Resolved Members -->
    <section class="panel-section">
      <h4>Resolved Members</h4>

      {#if selectedSpaceMembers}
        {#if currentUser}
          <div class="resolving-badge">
            <span class="badge-label">Resolving as</span>
            <span class="badge-user mono">
              👤 {currentUser.label ?? shortDid(currentUser.did)}
              {#if currentUserAccess}
                <span class="badge-access" style="color: {accessColor(currentUserAccess)}">
                  {accessLabel(currentUserAccess)}
                </span>
              {:else}
                <span class="badge-access no-access">No access</span>
              {/if}
            </span>
          </div>
        {/if}

        {#if sortedMembers.length === 0}
          <p class="empty-hint">No resolved members</p>
        {:else}
          <div class="member-grid">
            {#each sortedMembers as member}
              <div class="member-row">
                <span class="row-name mono truncate">👤 {shortDid(member.did)}</span>
                <div class="access-bar">
                  <div
                    class="access-fill"
                    style="width: {((accessLevelNum(member.access) + 1) / ALL_ACCESSES.length) * 100}%; background: {accessColor(member.access)}"
                  ></div>
                </div>
                <span class="row-access-label" style="color: {accessColor(member.access)}">
                  {accessLabel(member.access)}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        {#if selectedSpaceMissing && selectedSpaceMissing.length > 0}
          <div class="missing-section">
            <h5>
              Unresolved Spaces
              <span class="info-tip" data-tooltip="The remote arbiter could not provide its member list for this space. This happens when the requesting arbiter doesn't have permission to read the remote space's members, or when the remote arbiter is offline.">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="12" cy="12" r="10"/>
                  <line x1="12" y1="16" x2="12" y2="12"/>
                  <line x1="12" y1="8" x2="12.01" y2="8"/>
                </svg>
              </span>
            </h5>
            {#each selectedSpaceMissing as ms}
              <div class="missing-row">
                <span class="row-name mono truncate">❓ {ms.space.arbiterDid}/{ms.space.spaceKey}</span>
                <span class="row-access-label" style="color: {accessColor(ms.access)}">{accessLabel(ms.access)}</span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if selectedSpaceError}
        <p class="error-hint">{selectedSpaceError}</p>
      {:else}
        <p class="empty-hint">Select a space to see computed members</p>
      {/if}
    </section>

    <!-- Add Member -->
    <section class="panel-section add-member-section">
      <h4>Add Member</h4>
      <form class="add-member-form" onsubmit={handleAddMember}>
        <div class="form-grid">
          <select bind:value={newMemberType} onchange={resetMemberForm}>
            <option value="MemberDid">DID</option>
            <option value="MemberLocalSpace">Local Space</option>
            <option value="MemberRemoteSpace">Remote Space</option>
          </select>
          <select bind:value={newMemberAccess} style="color: {accessColor({$type: '', level: newMemberAccess})}">
            {#each ALL_ACCESSES as a}
              <option value={a} style="color: {accessColor({$type: '', level: a})}">{ACCESS_LABELS[a]}</option>
            {/each}
          </select>
        </div>

        {#if newMemberType === 'MemberDid'}
          <select bind:value={newMemberValue}>
            <option value="">-- Select DID --</option>
            {#each users as user}
              <option value={user.did}>👤 {user.label} ({user.did})</option>
            {/each}
            {#each serverState?.arbiters ?? [] as arbiter}
              <option value={arbiter.did}>🏛️ {arbiter.did}</option>
            {/each}
          </select>
        {:else if newMemberType === 'MemberLocalSpace'}
          <select bind:value={newMemberValue}>
            <option value="">-- Select local space --</option>
            {#each selectedArbiter?.spaces ?? [] as space}
              <option value={space.key}>{space.key}</option>
            {/each}
          </select>
        {:else}
          <div class="form-grid">
            <select bind:value={remoteArbiterDid}>
              <option value="">Remote Arbiter</option>
              {#each serverState?.arbiters ?? [] as arbiter}
                <option value={arbiter.did}>{arbiter.did}</option>
              {/each}
            </select>
            <select bind:value={remoteSpaceKey} disabled={!remoteArbiterDid}>
              <option value="">Remote Space</option>
              {#each remoteSpaces as space}
                <option value={space.key}>{space.key}</option>
              {/each}
            </select>
          </div>
        {/if}

        <button class="btn btn-primary btn-sm" type="submit">Set Access</button>
      </form>
    </section>

    <!-- Direct Members -->
    <section class="panel-section">
      <div class="section-row-header">
        <h4>Direct Members</h4>
        <span class="member-count">{selectedSpace.members.length}</span>
      </div>
      {#if selectedSpace.members.length === 0}
        <p class="empty-hint">No members added yet.</p>
      {:else}
        <div class="member-grid">
          {#each selectedSpace.members as m}
            <div class="direct-member-row">
              <span class="row-icon">{m.member.tag === 'MemberDid' ? '👤' : m.member.tag === 'MemberRemoteSpace' ? '🌐' : '📁'}</span>
              <span class="row-name mono truncate">
                {m.member.tag === 'MemberDid'
                  ? shortDid(m.member.value as string, 20)
                  : m.member.tag === 'MemberRemoteSpace'
                    ? shortDid((m.member.value as {arbiterDid: string}).arbiterDid, 20)
                    : m.member.value as string}
              </span>
              <span class="row-access-label compact" style="color: {accessColor(m.access)}">
                {accessLabel(m.access)}
              </span>
              <button
                class="remove-btn"
                onclick={() => handleRemoveMember(m)}
                title="Remove member"
              >
                ×
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Delete Space -->
    <section class="panel-section bottom-section">
      <button class="btn btn-danger btn-sm delete-btn" onclick={handleDeleteSpace}>
        Delete Space
      </button>
    </section>
  </aside>
{/if}

<style>
  .detail-panel { width: 340px; flex-shrink: 0; background: var(--bg-surface); border-left: 1px solid var(--border); overflow-y: auto; display: flex; flex-direction: column; }
  .panel-header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid var(--border); }
  .panel-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .space-icon { font-size: 1rem; flex-shrink: 0; }
  .panel-title h3 { font-weight: 600; font-size: 1rem; }
  .space-arbiter { font-size: 0.714rem; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close-btn { flex-shrink: 0; }
  .panel-section { padding: 14px 16px; border-bottom: 1px solid var(--border-light); }
  .panel-section h4 { font-size: 0.857rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 10px; text-transform: uppercase; letter-spacing: 0.04em; }
  .section-row-header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .section-row-header h4 { margin-bottom: 0; }
  .member-count { font-size: 0.714rem; padding: 1px 7px; border-radius: var(--radius-xs); background: var(--border); color: var(--text-muted); font-weight: 600; }
  .empty-hint { color: var(--text-muted); font-size: 0.857rem; font-style: italic; }
  .error-hint { color: oklch(0.5 0.15 20); font-size: 0.857rem; line-height: 1.4; }
  .bottom-section { padding: 20px 16px; border-bottom: none; margin-top: auto; }
  .config-section { background: var(--bg-base); }
  .config-toggle { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--radius-xs); background: var(--bg-raised); cursor: pointer; transition: all 150ms var(--ease-out); margin-bottom: 4px; color: var(--text-primary); }
  .config-toggle:hover { border-color: var(--accent); background: var(--accent-subtle); }
  .config-toggle.active { border-color: var(--accent); background: oklch(0.58 0.18 65 / 0.08); }
  .config-label { font-size: 0.857rem; font-weight: 500; }
  .toggle-indicator { font-size: 0.786rem; font-weight: 600; color: var(--text-muted); transition: color 150ms var(--ease-out); }
  .config-toggle.active .toggle-indicator { color: var(--accent-text); }
  .member-grid { display: flex; flex-direction: column; gap: 2px; }
  .member-row, .direct-member-row { display: grid; grid-template-columns: 1fr auto auto; align-items: center; gap: 8px; padding: 5px 8px; border-radius: var(--radius-xs); font-size: 0.857rem; transition: background 150ms var(--ease-out); }
  .member-row:hover, .direct-member-row:hover { background: var(--accent-subtle); }
  .direct-member-row { grid-template-columns: auto 1fr auto auto; }
  .row-icon { font-size: 0.857rem; }
  .row-name { font-size: 0.786rem; min-width: 0; }
  .row-access-label { font-size: 0.714rem; font-weight: 600; width: 90px; text-align: right; white-space: nowrap; }
  .row-access-label.compact { width: auto; }
  .access-bar { width: 60px; height: 6px; background: var(--border); border-radius: 3px; overflow: hidden; flex-shrink: 0; }
  .access-fill { height: 100%; border-radius: 3px; transition: width 300ms var(--ease-out); }
  .remove-btn { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 1rem; padding: 0 2px; line-height: 1; opacity: 0; transition: opacity 150ms var(--ease-out); }
  .direct-member-row:hover .remove-btn { opacity: 1; }
  .remove-btn:hover { color: oklch(0.5 0.15 20); }
  .resolving-badge { display: flex; flex-direction: column; gap: 3px; padding: 8px 10px; margin-bottom: 10px; background: var(--accent-subtle); border: 1px solid var(--border); border-radius: var(--radius-xs); }
  .badge-label { font-size: 0.714rem; font-weight: 500; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .badge-user { display: flex; align-items: center; gap: 6px; font-size: 0.857rem; font-weight: 600; }
  .badge-access { font-size: 0.786rem; font-weight: 600; }
  .badge-access.no-access { color: var(--text-muted); font-weight: 400; }
  .add-member-section { background: var(--bg-base); }
  .add-member-form { display: flex; flex-direction: column; gap: 6px; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .add-member-form select { width: 100%; }
  .add-member-form .btn { margin-top: 2px; }
  .missing-section { margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border-light); }
  .missing-section h5 { font-size: 0.786rem; font-weight: 600; color: oklch(0.5 0.12 30); margin-bottom: 4px; }
  .missing-row { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 8px; padding: 4px 8px; border-radius: var(--radius-xs); font-size: 0.857rem; }
  .delete-btn { width: 100%; }
</style>
