<script lang="ts">
  import UserList from './UserList.svelte';
  import ArbiterActions from './ArbiterActions.svelte';
  import AccessLegend from './AccessLegend.svelte';
</script>

<aside class="sidebar">
  <div class="sidebar-scroll">
    <UserList />
    <ArbiterActions />
  </div>
  <AccessLegend />
</aside>

<style>
  .sidebar {
    width: 280px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar-scroll {
    flex: 1;
    overflow-y: auto;
  }
</style>
