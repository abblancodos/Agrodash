<!-- src/routes/experiments/[id]/+layout.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/stores/auth';
  import { experimentStore } from '$lib/stores/experiment';
  import { preferences } from '$lib/stores/preferences';
  import ExperimentHelpPanel from '$lib/components/experiments/ExperimentHelpPanel.svelte';

  let { children } = $props();

  const id = $derived($page.params.id ?? '');
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let ready = $state(false);

  onMount(async () => {
    await auth.init();
    await experimentStore.load(id);

    const exp = $experimentStore.experiment;

    if (!exp && !$experimentStore.loading) {
      goto('/experiments');
      return;
    }

    if (exp && !exp.public && !exp.user_role) {
      goto('/experiments');
      return;
    }

    ready = true;

    pollInterval = setInterval(() => {
      experimentStore.reloadEvents(id);
    }, 30_000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    experimentStore.reset();
  });
</script>

{#if $experimentStore.loading || !ready}
  <div class="loading-shell">
    <div class="spinner"></div>
    <span>cargando experimento...</span>
  </div>
{:else if $experimentStore.error}
  <div class="error-shell">
    <p>{$experimentStore.error}</p>
    <a href="/experiments">← volver a experimentos</a>
  </div>
{:else if $experimentStore.experiment}
  <!-- Sobreescribir --font-scale con el valor de experimentos -->
  <div class="exp-shell" style="--font-scale: {$preferences.expFontScaleValue}">
    {@render children()}
    <ExperimentHelpPanel />
  </div>
{/if}

<style>
  .loading-shell, .error-shell {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 12px; min-height: 40vh;
    color: var(--text-secondary); font-size: 14px;
  }
  .spinner {
    width: 20px; height: 20px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--text-muted);
    border-radius: 50%;
    animation: spin .8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error-shell a { color: var(--text-primary); font-size: 13px; }
  .exp-shell { display: contents; }
</style>