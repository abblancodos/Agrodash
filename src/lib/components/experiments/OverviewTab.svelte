<script lang="ts">
  import { experimentStore, activeEvents, objectiveEvaluations } from '$lib/stores/experiment';
  import ObjectiveStatus from './ObjectiveStatus.svelte';
  import ObjectiveChart from './ObjectiveChart.svelte';
  import CollaboratorList from './CollaboratorList.svelte';

  const exp    = $derived($experimentStore.experiment!);
  const events = $derived($activeEvents);
  const evals  = $derived($objectiveEvaluations);

  const totalEntries    = $derived(events.length);
  const lastEntry       = $derived(events.at(-1));
  const violations      = $derived(evals.filter(e => e.status === 'violation').length);
  const warnings        = $derived(evals.filter(e => e.status === 'warning').length);

  function relTime(iso: string) {
    const secs = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
    if (secs < 60) return `hace ${secs}s`;
    if (secs < 3600) return `hace ${Math.floor(secs/60)} min`;
    if (secs < 86400) return `hace ${Math.floor(secs/3600)} h`;
    return `hace ${Math.floor(secs/86400)} días`;
  }
</script>

<div class="overview">
  <!-- Stats -->
  <div class="stat-grid">
    <div class="stat-card">
      <div class="stat-label">entries</div>
      <div class="stat-val">{totalEntries}</div>
    </div>
    <div class="stat-card">
      <div class="stat-label">última entry</div>
      <div class="stat-val stat-val--sm">
        {lastEntry ? relTime(lastEntry.recorded_at) : '—'}
      </div>
    </div>
    <div class="stat-card" class:stat-card--warn={warnings > 0 && violations === 0}
                           class:stat-card--viol={violations > 0}>
      <div class="stat-label">objetivos</div>
      <div class="stat-val">
        {#if violations > 0}
          <span style="color:#A32D2D">{violations} violación{violations > 1 ? 'es' : ''}</span>
        {:else if warnings > 0}
          <span style="color:#854F0B">{warnings} advertencia{warnings > 1 ? 's' : ''}</span>
        {:else if evals.length > 0}
          <span style="color:#3B6D11">todos OK</span>
        {:else}
          <span style="color:var(--text-muted)">sin objetivos</span>
        {/if}
      </div>
    </div>
    <div class="stat-card">
      <div class="stat-label">estado</div>
      <div class="stat-val stat-val--sm">{exp.status}</div>
    </div>
  </div>

  <!-- Gráfica principal -->
  <ObjectiveChart />

  <!-- Objetivos -->
  {#if evals.length > 0}
    <div class="section">
      <div class="section-title">objetivos</div>
      {#each evals as evaluation (evaluation.objective.id)}
        <ObjectiveStatus {evaluation} />
      {/each}
    </div>
  {:else}
    <div class="empty-hint">
      Sin objetivos definidos —
      <a href="#definitions" onclick|preventDefault={() => {}}>
        agregá uno en definitions
      </a>
    </div>
  {/if}

  <!-- Colaboradores -->
  <div class="section">
    <CollaboratorList />
  </div>

  <!-- Empty state -->
  {#if totalEntries === 0}
    <div class="empty-state">
      <p class="empty-state__title">el experimento está vacío</p>
      <p class="empty-state__sub">
        Empezá por definir las <strong>constantes y expresiones</strong> en la pestaña
        <em>definitions</em>, luego agregá entries en <em>entries</em>.
      </p>
    </div>
  {/if}
</div>

<style>
  .overview { display: flex; flex-direction: column; gap: calc(20px * var(--font-scale)); }

  .stat-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: calc(10px * var(--font-scale)); }
  .stat-card { background: var(--bg-elevated); border-radius: 8px; padding: calc(12px * var(--font-scale)); }
  .stat-card--warn { border: 0.5px solid #BA7517; }
  .stat-card--viol { border: 0.5px solid #E24B4A; }
  .stat-label { font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); margin-bottom: 4px; }
  .stat-val { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .stat-val--sm { font-size: calc(14px * var(--font-scale)); }

  .section { }
  .section-title { font-size: calc(11px * var(--font-scale)); font-weight: 500; color: var(--text-secondary); letter-spacing: .06em; text-transform: uppercase; margin-bottom: calc(10px * var(--font-scale)); }

  .empty-hint { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); }
  .empty-hint a { color: var(--text-primary); }

  .empty-state { background: var(--bg-elevated); border-radius: 8px; padding: calc(24px * var(--font-scale)); text-align: center; }
  .empty-state__title { font-size: calc(14px * var(--font-scale)); color: var(--text-secondary); margin-bottom: 8px; }
  .empty-state__sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }

  @media (max-width: 640px) {
    .stat-grid { grid-template-columns: repeat(2, 1fr); }
  }
</style>
