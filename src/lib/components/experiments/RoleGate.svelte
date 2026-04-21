<script lang="ts">
  import { userRole } from '$lib/stores/experiment';

  let { minRole = 'editor', children, fallback }:
    { minRole?: 'viewer' | 'editor' | 'admin'; children: any; fallback?: any } = $props();

  const rank = (r: string | null) =>
    r === 'admin' ? 3 : r === 'editor' ? 2 : r === 'viewer' ? 1 : 0;

  const allowed = $derived(rank($userRole) >= rank(minRole));
</script>

{#if allowed}
  {@render children()}
{:else if fallback}
  {@render fallback()}
{/if}
