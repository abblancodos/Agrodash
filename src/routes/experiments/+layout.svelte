<!-- src/routes/experiments/+layout.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { auth } from '$lib/stores/auth';

  let { children } = $props();

  onMount(() => auth.init());
</script>

{@render children()}