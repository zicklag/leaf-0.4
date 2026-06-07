<script lang="ts">
  import MonacoEditor from './MonacoEditor.svelte';

  let { value = $bindable(), onChange }: { value?: string; onChange?: (v: string) => void } = $props();
</script>

<MonacoEditor {value} language="rego" {onChange} />