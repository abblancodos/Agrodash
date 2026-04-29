<!-- src/routes/processes/[id]/+page.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { auth } from '$lib/stores/auth';
  import { processStore, canOperate, canAdmin } from '$lib/stores/process';
  import ControlTab from '$lib/components/processes/ControlTab.svelte';
  import ParamsTab  from '$lib/components/processes/ParamsTab.svelte';
  import LogsTab    from '$lib/components/processes/LogsTab.svelte';
  import ConfigTab  from '$lib/components/processes/ConfigTab.svelte';

  const id = $derived($page.params.id);

  type Tab = 'control' | 'params' | 'logs' | 'historial' | 'config';
  let activeTab = $state<Tab>('control');

  const proc     = $derived($processStore.process);
  const loading  = $derived($processStore.loading);
  const error    = $derived($processStore.error);
  const valveEvs = $derived($processStore.valveEvents);

  function statusDot(s: string) {
    return s === 'running' ? '#3da85a' : s === 'error' ? '#e05454' : '#8a9bb0';
  }
  function relTime(ts: string | null) {
    if (!ts) return 'nunca';
    const m = Math.floor((Date.now() - new Date(ts).getTime()) / 60000);
    if (m < 1) return 'ahora mismo';
    if (m < 60) return `hace ${m}m`;
    return `hace ${Math.floor(m/60)}h`;
  }

  onMount(async () => {
    await auth.init();
    await processStore.load(id);
    processStore.startSSE(id, 5);
  });

  onDestroy(() => {
    processStore.stopSSE();
    processStore.reset();
  });
</script>

<div class="page">
  <a href="/processes" class="back">← procesos</a>

  {#if loading}
    <div class="loading"><div class="spinner"></div>cargando...</div>

  {:else if error}
    <div class="error-banner">{error}</div>

  {:else if proc}
    <!-- Header -->
    <div class="proc-head">
      <div class="proc-head__info">
        <div class="proc-title-row">
          <span class="status-dot" style="background:{statusDot(proc.status)}"></span>
          <h1 class="proc-title">{proc.name}</h1>
          <span class="proc-role">{proc.user_role ?? ''}</span>
        </div>
        {#if proc.description}
          <p class="proc-desc">{proc.description}</p>
        {/if}
        <div class="proc-meta">
          <span class="meta-item">{proc.type}</span>
          <span class="meta-sep">·</span>
          <span class="meta-item">último dato {relTime(proc.last_seen_at)}</span>
          {#if proc.last_state?.kalman?.n_updates != null}
            <span class="meta-sep">·</span>
            <span class="meta-item">{proc.last_state.kalman.n_updates} ciclos Kalman</span>
          {/if}
        </div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="tabs">
      <button class="tab" class:active={activeTab === 'control'}
        onclick={() => activeTab = 'control'}>control</button>
      <button class="tab" class:active={activeTab === 'params'}
        onclick={() => activeTab = 'params'}>parámetros</button>
      <button class="tab" class:active={activeTab === 'logs'}
        onclick={() => activeTab = 'logs'}>logs</button>
      <button class="tab" class:active={activeTab === 'historial'}
        onclick={() => activeTab = 'historial'}>historial</button>
      <button class="tab" class:active={activeTab === 'config'}
        onclick={() => activeTab = 'config'}>configurar</button>
    </div>

    <div class="tab-body">
      {#if activeTab === 'control'}
        <ControlTab processId={id} />

      {:else if activeTab === 'params'}
        <ParamsTab processId={id} />

      {:else if activeTab === 'logs'}
        <LogsTab processId={id} />

      {:else if activeTab === 'config'}
        <ConfigTab processId={id} />

      {:else if activeTab === 'historial'}
        <!-- Historial de válvulas -->
        {#if valveEvs.length === 0}
          <div class="empty">no hay eventos de válvulas registrados</div>
        {:else}
          <div class="hist-table">
            <div class="hist-head">
              <span>timestamp</span><span>línea</span><span>estado</span>
              <span>modo</span><span>Kalman</span>
            </div>
            {#each valveEvs as ev (ev.id)}
              <div class="hist-row">
                <span class="hist-ts">{new Date(ev.ts).toLocaleString('es-CR', {day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit',second:'2-digit',hour12:false})}</span>
                <span class="hist-linea">Línea {ev.linea}</span>
                <span class="hist-estado" class:on={ev.estado} class:off={!ev.estado}>
                  {ev.estado ? 'ON' : 'OFF'}
                </span>
                <span class="hist-modo">{ev.modo}</span>
                <span class="hist-kalman">
                  {ev.kalman_convergido == null ? '—' : ev.kalman_convergido ? '✓' : '⟳'}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .page { max-width: 1100px; margin: 0 auto; padding: calc(24px * var(--font-scale)) calc(20px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(16px * var(--font-scale)); }

  .back { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-decoration: none; }
  .back:hover { color: var(--text-primary); }

  .loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 40px 0; }
  .spinner { width: 16px; height: 16px; border: 2px solid var(--border-subtle); border-top-color: var(--text-muted); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error-banner { background: var(--error-bg); color: var(--error-color); padding: 10px 14px; border-radius: 6px; font-size: calc(13px * var(--font-scale)); }

  /* Header */
  .proc-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
  .proc-title-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .status-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
  .proc-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .proc-role { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 2px 8px; border-radius: 10px; }
  .proc-desc { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); margin-top: 4px; }
  .proc-meta { display: flex; align-items: center; gap: 6px; margin-top: 6px; flex-wrap: wrap; }
  .meta-item { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .meta-sep { color: var(--border-default); }

  /* Tabs */
  .tabs { display: flex; border-bottom: 0.5px solid var(--border-subtle); gap: 0; }
  .tab { padding: calc(9px * var(--font-scale)) calc(16px * var(--font-scale)); border: none; background: none; cursor: pointer; font-size: calc(13px * var(--font-scale)); color: var(--text-muted); border-bottom: 2px solid transparent; margin-bottom: -0.5px; transition: all .12s; }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }

  .tab-body { min-height: 200px; }
  .empty { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); text-align: center; padding: 40px 0; }

  /* Historial */
  .hist-table { border: 0.5px solid var(--border-subtle); border-radius: 8px; overflow: hidden; font-size: calc(12px * var(--font-scale)); }
  .hist-head { display: grid; grid-template-columns: 160px 80px 60px 80px 60px; gap: 12px; padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale)); background: var(--bg-elevated); border-bottom: 0.5px solid var(--border-subtle); color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }
  .hist-row { display: grid; grid-template-columns: 160px 80px 60px 80px 60px; gap: 12px; padding: calc(7px * var(--font-scale)) calc(12px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); align-items: center; }
  .hist-row:last-child { border-bottom: none; }
  .hist-row:hover { background: var(--interactive-hover); }
  .hist-ts { color: var(--text-muted); font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale)); }
  .hist-linea { font-family: 'DM Mono', monospace; }
  .hist-estado { font-weight: 500; font-family: 'DM Mono', monospace; }
  .hist-estado.on  { color: #3da85a; }
  .hist-estado.off { color: #e05454; }
  .hist-modo { color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .hist-kalman { color: var(--text-muted); text-align: center; }

  @media (max-width: 640px) {
    .hist-head, .hist-row { grid-template-columns: 1fr 50px 50px 60px 40px; gap: 8px; }
  }
</style>
