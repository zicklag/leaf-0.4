<script lang="ts">
  import { Modal, Button } from '@foxui/core';

  let {
    open,
    title,
    description,
    confirmLabel = 'Confirm',
    danger = false,
    onconfirm,
    oncancel,
  }: {
    open: boolean;
    title: string;
    description: string;
    confirmLabel?: string;
    danger?: boolean;
    onconfirm: () => void;
    oncancel?: () => void;
  } = $props();
</script>

<Modal bind:open {title}>
  <div class="space-y-4 py-2">
    <p class="text-sm text-base-600 dark:text-base-400">{description}</p>
  </div>

  <div class="flex justify-end gap-2 mt-4">
    <Button
      variant="secondary"
      onclick={() => {
        oncancel?.();
        open = false;
      }}
    >
      Cancel
    </Button>
    <Button
      class={danger ? 'red' : ''}
      onclick={() => {
        onconfirm();
        open = false;
      }}
    >
      {confirmLabel}
    </Button>
  </div>
</Modal>
