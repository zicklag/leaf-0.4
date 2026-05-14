<script lang="ts">
  import { accessLabel, accessColor } from '../lib/utils';
  import { ALL_ACCESSES } from '../lib/types';
</script>

<section class="access-legend">
  <h4>Access Levels</h4>
  <div class="legend-grid">
    {#each ALL_ACCESSES as access}
      <div class="legend-item">
        <div class="legend-swatch" style="background: {accessColor(access)}"></div>
        <span class="legend-label" style="color: {accessColor(access)}">
          {accessLabel(access)}
        </span>
      </div>
    {/each}
  </div>
</section>

<style>
  .access-legend {
    padding: 16px;
    flex-shrink: 0;
  }

  h4 {
    font-size: 0.857rem;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .legend-grid {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
    border-radius: var(--radius-xs);
  }

  .legend-swatch {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .legend-label {
    font-size: 0.786rem;
    font-weight: 500;
  }
</style>
