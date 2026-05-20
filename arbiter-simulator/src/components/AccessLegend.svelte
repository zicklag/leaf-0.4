<script lang="ts">
  import { accessLabel, accessColor } from '../lib/utils';
  import { ALL_ACCESSES } from '../lib/types';

  let expanded = $state<string | null>(null);

  function toggle(access: string) {
    expanded = expanded === access ? null : access;
  }

  type AccessInfo = {
    description: string;
    includes: string[];
  };

  // Map display labels back to access keys for the includes chips lookup
  const LABEL_TO_KEY: Record<string, string> = {
    'Read Members': 'ReadMemberList',
    'Member': 'IsMember',
    'Add Members': 'AddMembers',
    'Remove Members': 'RemoveMembers',
    'Configure Space': 'ConfigureSpace',
    'Create Spaces': 'CreateSpaces',
    'Delete Spaces': 'RemoveSpace',
    'Owner': 'Owner',
  };

  const ACCESS_INFO: Record<string, AccessInfo> = {
    ReadMemberList: {
      description: 'Read the member list. Not counted as a member of the permissioned space.',
      includes: [],
    },
    IsMember: {
      description: 'Read records in the space. This is the first level that counts as a space member. Write access is enforced by the app, not the arbiter.',
      includes: ['Read Members'],
    },
    AddMembers: {
      description: 'Add new members to the space, granting access equal to or less than your own.',
      includes: ['Read Members', 'Member'],
    },
    RemoveMembers: {
      description: 'Remove members whose access is equal to or lower than yours.',
      includes: ['Read Members', 'Member', 'Add Members'],
    },
    ConfigureSpace: {
      description: 'Change public records and public members settings. Can also write records under the arbiter account.',
      includes: ['Read Members', 'Member', 'Add Members', 'Remove Members'],
    },
    CreateSpaces: {
      description: 'Create new spaces. Only effective when delegated through the $admin space.',
      includes: ['Read Members', 'Member', 'Add Members', 'Remove Members', 'Configure Space'],
    },
    RemoveSpace: {
      description: 'Delete a space. When granted in the $admin space, can delete any space.',
      includes: ['Read Members', 'Member', 'Add Members', 'Remove Members', 'Configure Space', 'Create Spaces'],
    },
    Owner: {
      description: 'Full control. No permission limits. Can remove other admins and owners, and delete the arbiter as the last owner.',
      includes: ['Read Members', 'Member', 'Add Members', 'Remove Members', 'Configure Space', 'Create Spaces', 'Delete Spaces'],
    },
  };
</script>

<section class="access-legend">
  <div class="legend-header">
    <h4>Access Levels</h4>
    <span class="legend-hint">Click for details</span>
  </div>
  <div class="legend-grid">
    {#each ALL_ACCESSES as access}
      {@const info = ACCESS_INFO[access]}
      {@const isOpen = expanded === access}
      <button
        class="legend-item"
        class:expanded={isOpen}
        onclick={() => toggle(access)}
      >
        <div class="legend-swatch" style="background: {accessColor({level: access})}"></div>
        <span class="legend-label" style="color: {accessColor({level: access})}">
          {accessLabel({level: access})}
        </span>
        <span class="expand-icon">{isOpen ? '▾' : '▸'}</span>
      </button>
      <div class="legend-detail" class:visible={isOpen}>
        <div class="detail-inner">
          <p class="detail-desc">{info.description}</p>
          {#if info.includes.length > 0}
            <div class="detail-includes">
              <span class="detail-includes-label">Includes:</span>
              <div class="include-chips">
                {#each info.includes as inc}
                  {@const incKey = LABEL_TO_KEY[inc]}
                  <button
                    class="include-chip"
                    style="color: {accessColor({level: incKey})}"
                    onclick={(e) => { e.stopPropagation(); toggle(incKey); }}
                  >
                    {inc}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</section>

<style>
  .access-legend {
    padding: 16px;
    flex-shrink: 0;
  }

  .legend-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .legend-header h4 {
    margin-bottom: 0;
  }

  .legend-hint {
    font-size: 0.643rem;
    color: var(--text-muted);
    font-style: italic;
  }

  h4 {
    font-size: 0.857rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .legend-grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius-xs);
    border: 1px solid transparent;
    background: none;
    cursor: pointer;
    transition: all 150ms var(--ease-out);
    width: 100%;
    text-align: left;
    font-family: inherit;
    font-size: inherit;
  }

  .legend-item:hover {
    background: var(--accent-subtle);
    border-color: var(--border-light);
  }

  .legend-item.expanded {
    background: var(--accent-subtle);
    border-color: var(--border);
  }

  .legend-swatch {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    transition: transform 150ms var(--ease-out);
  }

  .legend-item:hover .legend-swatch {
    transform: scale(1.3);
  }

  .legend-label {
    font-size: 0.786rem;
    font-weight: 500;
    flex: 1;
  }

  .expand-icon {
    font-size: 0.643rem;
    color: var(--text-muted);
    opacity: 0.5;
    transition: opacity 150ms var(--ease-out);
  }

  .legend-item:hover .expand-icon {
    opacity: 1;
  }

  /* ── Detail section: always in DOM, animated via CSS ── */
  .legend-detail {
    max-height: 0;
    opacity: 0;
    overflow: hidden;
    transition: max-height 200ms var(--ease-out), opacity 200ms var(--ease-out), margin 200ms var(--ease-out);
    margin-top: 0;
  }

  .legend-detail.visible {
    max-height: 200px;
    opacity: 1;
    margin-top: 2px;
    margin-bottom: 2px;
  }

  .detail-inner {
    padding: 8px 8px 8px 24px;
  }

  .detail-desc {
    font-size: 0.786rem;
    color: var(--text-secondary);
    line-height: 1.4;
    margin-bottom: 6px;
  }

  .detail-includes {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .detail-includes-label {
    font-size: 0.714rem;
    font-weight: 500;
    color: var(--text-muted);
  }

  .include-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }

  .include-chip {
    font-size: 0.643rem;
    padding: 2px 8px;
    border-radius: var(--radius-xs);
    background: var(--bg-base);
    border: 1px solid var(--border-light);
    cursor: pointer;
    font-family: inherit;
    font-weight: 500;
    transition: all 150ms var(--ease-out);
  }

  .include-chip:hover {
    background: var(--accent-subtle);
    border-color: currentColor;
    transform: translateY(-1px);
  }
</style>
