<script lang="ts">
  import type { Access, Member } from 'arbiter-wasm';
  import { app } from '../lib/simulation-store.svelte';
  import { accessLabel, accessLevel, accessColor, shortDid, buildMessage, buildMemberFromEntry } from '../lib/utils';
  import { ALL_ACCESSES, ACCESS_LABELS } from '../lib/types';

  let {
    selectedSpace,
    selectedSpaceMembers,
    selectedSpaceError,
    currentUser,
    selectedArbiterDid,
    serverState,
    users,
  } = $derived(app);

  let selectedArbiter = $derived(
    serverState?.arbiters.find((a) => a.did === selectedArbiterDid) ?? null,
  );

  let sortedMembers = $derived(
    selectedSpaceMembers?.resolved
      ? [...selectedSpaceMembers.resolved].sort((a, b) => {
          const levelDiff = accessLevel(b.access) - accessLevel(a.access);
          if (levelDiff !== 0) return levelDiff;
          return a.value.localeCompare(b.value);
        })
      : [],
  );

  let currentUserAccess = $derived(
    currentUser && selectedSpaceMembers?.resolved
      ? selectedSpaceMembers.resolved.find(
          (m) => m.memberType === 'User' && m.value === currentUser.did,
        )?.access ?? null
      : null,
  );

  // --- Add member state (always visible) ---
  let newMemberType = $state<'MemberUser' | 'MemberLocalSpace' | 'MemberRemoteSpace'>('MemberUser');
  let newMemberValue = $state('');
  let newMemberAccess = $state<Access>('ReadMemberList');
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

    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid!, selectedSpace!.key, {
        type: 'setMemberAccess',
        member: member!,
        access: newMemberAccess,
      }),
    );
    const respond = result.find((r) => r.effectType === 'respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Member access set');
      resetMemberForm();
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to set member');
    }
  }

  async function handleRemoveMember(memberEntry: { memberType: string; value: string }) {
    if (!currentUser || !selectedSpace) return;
    const member = buildMemberFromEntry(memberEntry);
    if (!member) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid!, selectedSpace.key, {
        type: 'removeMember',
        member,
      }),
    );
    const respond = result.find((r) => r.effectType === 'respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Member removed');
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to remove member');
    }
  }

  async function togglePublicMembers() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const effects = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid, selectedSpace.key, {
        type: 'configureSpace',
        public_records: selectedSpace.config.publicRecords,
        public_members: !selectedSpace.config.publicMembers,
      }),
    );
    const respond = effects.find((e) => e.effectType === 'respond');
    if (respond && !respond.ok) {
      const who = currentUser?.label ?? currentUser?.did ?? 'unknown';
      app.notifications.add('error', `User "${who}": ${respond.error || 'Permission denied'}`);
    }
  }

  async function togglePublicRecords() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const effects = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid, selectedSpace.key, {
        type: 'configureSpace',
        public_records: !selectedSpace.config.publicRecords,
        public_members: selectedSpace.config.publicMembers,
      }),
    );
    const respond = effects.find((e) => e.effectType === 'respond');
    if (respond && !respond.ok) {
      const who = currentUser?.label ?? currentUser?.did ?? 'unknown';
      app.notifications.add('error', `User "${who}": ${respond.error || 'Permission denied'}`);
    }
  }

  async function handleDeleteSpace() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid, selectedSpace.key, {
        type: 'deleteSpace',
      }),
    );
    const respond = result.find((r) => r.effectType === 'respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Space deleted');
      app.selectArbiter(selectedArbiterDid);
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to delete space');
    }
  }

  function memberIcon(memberType: string): string {
    switch (memberType) {
      case 'User': return '👤';
      case 'RemoteSpace': return '🌐';
      default: return '📁';
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
                <span class="row-name mono truncate">👤 {shortDid(member.value)}</span>
                <div class="access-bar">
                  <div
                    class="access-fill"
                    style="width: {((accessLevel(member.access) + 1) / ALL_ACCESSES.length) * 100}%; background: {accessColor(member.access)}"
                  ></div>
                </div>
                <span class="row-access-label" style="color: {accessColor(member.access)}">
                  {accessLabel(member.access)}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        {#if selectedSpaceMembers.missing.length > 0}
          <div class="missing-section">
            <h5>Unresolved Spaces</h5>
            {#each selectedSpaceMembers.missing as ms}
              <div class="missing-row">
                <span class="row-name mono truncate">❓ {ms.arbiterDid}/{ms.spaceKey}</span>
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

    <!-- Add Member (always visible) -->
    <section class="panel-section add-member-section">
      <h4>Add Member</h4>
      <form class="add-member-form" onsubmit={handleAddMember}>
        <!-- Row 1: type + access level -->
        <div class="form-grid">
          <select bind:value={newMemberType} onchange={resetMemberForm}>
            <option value="MemberUser">DID</option>
            <option value="MemberLocalSpace">Local Space</option>
            <option value="MemberRemoteSpace">Remote Space</option>
          </select>
          <select bind:value={newMemberAccess} style="color: {accessColor(newMemberAccess)}">
            {#each ALL_ACCESSES as a}
              <option value={a} style="color: {accessColor(a)}">{ACCESS_LABELS[a]}</option>
            {/each}
          </select>
        </div>

        <!-- Row 2: value selector -->
        {#if newMemberType === 'MemberUser'}
          <select bind:value={newMemberValue}>
            <option value="">-- Select DID --</option>
            {#each users as user}
              <option value={user.did}>&#x1F464; {user.label} ({user.did})</option>
            {/each}
            {#each serverState?.arbiters ?? [] as arbiter}
              <option value={arbiter.did}>&#x1F3DB;&#xFE0F; {arbiter.did}</option>
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
          <!-- Remote: both dropdowns always visible side-by-side -->
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

    <!-- Direct Members (below add member) -->
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
              <span class="row-icon">{memberIcon(m.memberType)}</span>
              <span class="row-name mono truncate">
                {m.memberType === 'RemoteSpace' ? shortDid(m.value, 28) : shortDid(m.value, 20)}
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

    <!-- Delete Space — at the bottom -->
    <section class="panel-section bottom-section">
      <button class="btn btn-danger btn-sm delete-btn" onclick={handleDeleteSpace}>
        Delete Space
      </button>
    </section>
  </aside>
{/if}

<style>
  .detail-panel {
    width: 340px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  /* ── Panel Header ── */
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .panel-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .space-icon {
    font-size: 1rem;
    flex-shrink: 0;
  }

  .panel-title h3 {
    font-weight: 600;
    font-size: 1rem;
  }

  .space-arbiter {
    font-size: 0.714rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close-btn {
    flex-shrink: 0;
  }

  /* ── Panel Sections ── */
  .panel-section {
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-light);
  }

  .panel-section h4 {
    font-size: 0.857rem;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .panel-section h5 {
    font-size: 0.786rem;
    font-weight: 600;
    color: var(--text-muted);
    margin: 8px 0 6px;
  }

  .section-row-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }

  .section-row-header h4 {
    margin-bottom: 0;
  }

  .member-count {
    font-size: 0.714rem;
    padding: 1px 7px;
    border-radius: var(--radius-xs);
    background: var(--border);
    color: var(--text-muted);
    font-weight: 600;
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: 0.857rem;
    font-style: italic;
  }

  .error-hint {
    color: oklch(0.5 0.15 20);
    font-size: 0.857rem;
    line-height: 1.4;
  }

  /* ── Bottom section: extra spacing ── */
  .bottom-section {
    padding: 20px 16px;
    border-bottom: none;
    margin-top: auto;
  }

  /* ── Configuration Toggles ── */
  .config-section {
    background: var(--bg-base);
  }

  .config-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    background: var(--bg-raised);
    cursor: pointer;
    transition: all 150ms var(--ease-out);
    margin-bottom: 4px;
  }

  .config-toggle:hover {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .config-toggle.active {
    border-color: var(--accent);
    background: oklch(0.58 0.18 65 / 0.08);
  }

  .config-label {
    font-size: 0.857rem;
    font-weight: 500;
  }

  .toggle-indicator {
    font-size: 0.786rem;
    font-weight: 600;
    color: var(--text-muted);
    transition: color 150ms var(--ease-out);
  }

  .config-toggle.active .toggle-indicator {
    color: var(--accent-text);
  }

  /* ── Shared grid layouts ── */
  .member-grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .member-row,
  .direct-member-row,
  .missing-row {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius-xs);
    font-size: 0.857rem;
    transition: background 150ms var(--ease-out);
  }

  .member-row:hover,
  .direct-member-row:hover {
    background: var(--accent-subtle);
  }

  /* Direct member rows get an extra grid column for the remove button */
  .direct-member-row {
    grid-template-columns: auto 1fr auto auto;
  }

  .row-icon {
    font-size: 0.857rem;
  }

  .row-name {
    font-size: 0.786rem;
    min-width: 0;
  }

  .row-access-label {
    font-size: 0.714rem;
    font-weight: 600;
    width: 90px;
    text-align: right;
    white-space: nowrap;
  }

  .row-access-label.compact {
    width: auto;
  }

  .access-bar {
    width: 60px;
    height: 6px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .access-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 300ms var(--ease-out);
  }

  .remove-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 1rem;
    padding: 0 2px;
    line-height: 1;
    opacity: 0;
    transition: opacity 150ms var(--ease-out);
  }

  .direct-member-row:hover .remove-btn {
    opacity: 1;
  }

  .remove-btn:hover {
    color: oklch(0.5 0.15 20);
  }

  /* ── Missing section ── */
  .missing-section {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border-light);
  }

  .missing-row {
    opacity: 1;
  }

  /* ── Resolving Badge ── */
  .resolving-badge {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 10px;
    margin-bottom: 10px;
    background: var(--accent-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
  }

  .badge-label {
    font-size: 0.714rem;
    font-weight: 500;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .badge-user {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.857rem;
    font-weight: 600;
  }

  .badge-access {
    font-size: 0.786rem;
    font-weight: 600;
  }

  .badge-access.no-access {
    color: var(--text-muted);
    font-weight: 400;
  }

  /* ── Add Member Form ── */
  .add-member-section {
    background: var(--bg-base);
  }

  .add-member-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .add-member-form select {
    width: 100%;
  }

  .add-member-form .btn {
    margin-top: 2px;
  }

  /* ── Delete Button ── */
  .delete-btn {
    width: 100%;
  }
</style>
