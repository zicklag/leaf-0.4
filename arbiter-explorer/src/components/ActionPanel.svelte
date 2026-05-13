<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { buildMessage, parseSpaceId } from '../lib/utils';
  import { ALL_ACCESSES, ACCESS_LABELS } from '../lib/types';
  import type { Access, Member } from '../lib/types';

  let { currentUser, selectedArbiter, selectedSpace, serverState } = $derived(app);

  // --- create arbiter ---
  let newArbiterDid = $state('');

  // --- create space ---
  let newSpaceKey = $state('');

  // --- add member ---
  let showAddMember = $state(false);
  let newMemberType = $state<'MemberUser' | 'MemberLocalSpace' | 'MemberRemoteSpace'>('MemberUser');
  let newMemberValue = $state('');
  let newMemberAccess = $state<Access>('ReadMemberList');

  // --- configure space ---
  let showConfigure = $state(false);

  async function handleCreateArbiter(e: Event) {
    e.preventDefault();
    if (!currentUser || !newArbiterDid.trim()) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, newArbiterDid.trim(), '$admin', {
        type: 'createArbiter',
      }),
    );
    console.log('[createArbiter] effects:', JSON.stringify(result, null, 2));
    const respond = result.find((r) => r.effectType === 'Respond');
    console.log('[createArbiter] respond:', respond);
    if (respond?.ok) {
      app.notifications.add('success', `Arbiter "${newArbiterDid.trim()}" created`);
      newArbiterDid = '';
    } else {
      app.notifications.add(
        'error',
        respond?.error ?? 'Failed to create arbiter',
      );
    }
  }

  async function handleCreateSpace(e: Event) {
    e.preventDefault();
    if (!currentUser || !selectedArbiter || !newSpaceKey.trim()) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiter.did, newSpaceKey.trim(), {
        type: 'createSpace',
      }),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', `Space "${newSpaceKey.trim()}" created`);
      newSpaceKey = '';
    } else {
      app.notifications.add(
        'error',
        respond?.error ?? 'Failed to create space',
      );
    }
  }

  async function handleAddMember(e: Event) {
    e.preventDefault();
    if (!currentUser || !selectedSpace || !newMemberValue.trim()) return;

    let member: Member;
    if (newMemberType === 'MemberRemoteSpace') {
      const parsed = parseSpaceId(newMemberValue.trim());
      if (!parsed) {
        app.notifications.add('error', 'Invalid remote space format. Use arbiterDid:spaceKey');
        return;
      }
      member = { tag: 'MemberRemoteSpace', value: newMemberValue.trim() };
    } else {
      member = { tag: newMemberType, value: newMemberValue.trim() };
    }

    const result = await app.dispatch(
      buildMessage(
        currentUser.did,
        selectedArbiter!.did,
        selectedSpace!.key,
        {
          type: 'setMemberAccess',
          member,
          access: newMemberAccess,
        },
      ),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Member access set');
      newMemberValue = '';
      showAddMember = false;
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to set member');
    }
  }

  async function handleRemoveMember(memberEntry: { memberType: string; value: string }) {
    if (!currentUser || !selectedSpace) return;
    const member: Member = {
      tag: memberEntry.memberType as Member['tag'],
      value: memberEntry.value,
    };
    const result = await app.dispatch(
      buildMessage(
        currentUser.did,
        selectedArbiter!.did,
        selectedSpace!.key,
        {
          type: 'removeMember',
          member,
        },
      ),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Member removed');
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to remove member');
    }
  }

  async function handleDeleteSpace() {
    if (!currentUser || !selectedSpace) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiter!.did, selectedSpace!.key, {
        type: 'deleteSpace',
      }),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Space deleted');
      app.selectArbiter(selectedArbiter!.did);
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to delete space');
    }
  }

  async function handleDeleteArbiter() {
    if (!currentUser || !selectedArbiter) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiter.did, '$admin', {
        type: 'deleteArbiter',
      }),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Arbiter deleted');
      app.selectArbiter(null);
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to delete arbiter');
    }
  }

  async function handleConfigureSpace() {
    if (!currentUser || !selectedSpace) return;
    const result = await app.dispatch(
      buildMessage(
        currentUser.did,
        selectedArbiter!.did,
        selectedSpace!.key,
        {
          type: 'configureSpace',
          publicRecords: selectedSpace.config.publicRecords,
          publicMembers: selectedSpace.config.publicMembers,
        },
      ),
    );
    const respond = result.find((r) => r.effectType === 'Respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Space configured');
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to configure space');
    }
  }
</script>

<section class="action-panel">
  <div class="section-header">
    <h3>Actions</h3>
  </div>

  {#if !currentUser}
    <p class="empty-hint">Select a user from the list above.</p>
  {:else}
    <!-- Create Arbiter -->
    <form class="action-form" onsubmit={handleCreateArbiter}>
      <label for="arbiter-did">Create Arbiter</label>
      <div class="input-row">
        <input
          id="arbiter-did"
          type="text"
          placeholder={app.generateArbiterDid()}
          bind:value={newArbiterDid}
        />
        <button class="btn btn-primary btn-sm" type="submit">Create</button>
      </div>
    </form>

    <!-- Arbiter selected: show space actions -->
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

      <!-- Space selected: show member actions -->
      {#if selectedSpace}
        <div class="context-label mono">
          Space: {selectedSpace.key}
        </div>

        <button
          class="btn btn-sm"
          style="width: 100%; margin-bottom: 8px"
          onclick={() => (showAddMember = !showAddMember)}
        >
          {showAddMember ? 'Cancel' : '+ Add Member'}
        </button>

        {#if showAddMember}
          <form class="action-form" onsubmit={handleAddMember}>
            <label for="member-type">Member Type</label>
            <select
              id="member-type"
              bind:value={newMemberType}
            >
              <option value="MemberUser">User (DID)</option>
              <option value="MemberLocalSpace">Local Space</option>
              <option value="MemberRemoteSpace">Remote Space</option>
            </select>

            <label for="member-value">
              {newMemberType === 'MemberUser'
                ? 'User DID'
                : newMemberType === 'MemberRemoteSpace'
                  ? 'arbiterDid:spaceKey'
                  : 'Space Key'}
            </label>
            <input
              id="member-value"
              type="text"
              placeholder={
                newMemberType === 'MemberUser'
                  ? 'did:example:…'
                  : newMemberType === 'MemberRemoteSpace'
                    ? 'did:example:arb:space'
                    : 'my-space'
              }
              bind:value={newMemberValue}
            />

            <label for="member-access">Access Level</label>
            <select id="member-access" bind:value={newMemberAccess}>
              {#each ALL_ACCESSES as a}
                <option value={a}>{ACCESS_LABELS[a]}</option>
              {/each}
            </select>

            <button class="btn btn-primary btn-sm" type="submit">
              Set Access
            </button>
          </form>
        {/if}

        <!-- Remove members from space -->
        {#if selectedSpace.members.length > 0}
          <div class="member-actions">
            <span class="member-label">Current Members</span>
            {#each selectedSpace.members as m}
              <div class="member-row">
                <span class="mono truncate">{m.memberType}:{m.value}</span>
                <span class="access-badge">{m.access}</span>
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
  .action-panel {
    padding: 16px;
    flex: 1;
  }

  .section-header {
    margin-bottom: 12px;
  }

  .section-header h3 {
    font-weight: 600;
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: 0.857rem;
  }

  .action-form {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border-light);
  }

  .action-form label {
    margin-top: 6px;
  }

  .action-form label:first-child {
    margin-top: 0;
  }

  .input-row {
    display: flex;
    gap: 6px;
  }

  .input-row input {
    flex: 1;
  }

  .context-label {
    font-size: 0.714rem;
    color: var(--accent-text);
    background: var(--accent-subtle);
    padding: 3px 8px;
    border-radius: var(--radius-xs);
    margin-bottom: 8px;
  }

  .member-actions {
    margin-top: 4px;
  }

  .member-label {
    display: block;
    margin-bottom: 6px;
    font-size: 0.857rem;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .member-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: var(--radius-xs);
    margin-bottom: 2px;
    font-size: 0.786rem;
  }

  .member-row:hover {
    background: var(--accent-subtle);
  }

  .access-badge {
    font-size: 0.714rem;
    padding: 1px 6px;
    border-radius: var(--radius-xs);
    background: var(--border);
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .remove-btn {
    flex-shrink: 0;
  }
</style>
