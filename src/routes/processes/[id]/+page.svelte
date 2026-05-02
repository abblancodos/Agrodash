<!-- src/routes/processes/[id]/+page.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { auth } from '$lib/stores/auth';
  import { processStore } from '$lib/stores/process.svelte';
  import MonitorTab  from '$lib/components/processes/MonitorTab.svelte';
  import LogsTab     from '$lib/components/processes/LogsTab.svelte';
  import ConfigTab   from '$lib/components/processes/ConfigTab.svelte';

  const id = $derived($page.params.id);

  type Tab = 'monitor' | 'logs' | 'config';
  let activeTab = $state<Tab>('monitor');

  const proc    = $derived(processStore.process);
  const loading = $derived(processStore.loading);
  const error   = $derived(processStore.error);

  function statusColor(s: string) {
    return s === 'running' ? '#3da85a' : s === 'error' ? '#e05454' : '#8a9bb0';
  }

  function relTime(ts: string | null) {
    if (!ts) return 'nunca';
    const s = Math.floor((Date.now() - new Date(ts + (ts.endsWith('Z') ? '' : 'Z')).getTime()) / 1000);
    if (s < 60)   return `hace ${s}s`;
    if (s < 3600) return `hace ${Math.floor(s/60)}m`;
    return `hace ${Math.floor(s/3600)}h`;
  }

  // ── Polling en vez de SSE ─────────────────────────────────────────────────
  // El SSE acumula buffer ilimitado y crashea el browser con payloads grandes.
  // Polling cada 8s es suficiente para una página de control de riego.
  const API = (import.meta as any).env?.VITE_API_BASE ?? '';
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function pollStatus() {
    if (document.hidden) return; // no pollear si la pestaña no está visible
    try {
      const res  = await fetch(`${API}/api/v1/processes/${id}/state`, { credentials: 'include' });
      if (!res.ok) return;
      const data = await res.json();
      processStore.patchStatus(data.status, data.last_seen_at);
    } catch {}
  }

  async function pollPipelineStates() {
    if (document.hidden) return;
    const pipelines = proc?.config?.pipelines ?? [];
    for (const pl of pipelines) {
      await processStore.refreshPipelineState(id, pl.id);
    }
  }

  // Intervalo adaptativo: usa el loop_interval del pipeline, mínimo 5s, máximo 30s
  function pollInterval(): number {
    const pipelines = proc?.config?.pipelines ?? [];
    if (!pipelines.length) return 10_000;
    const minLoop = Math.min(...pipelines.map((p: any) => p.loop_interval_seconds ?? 10));
    return Math.max(5, Math.min(30, minLoop)) * 1000;
  }

  onMount(async () => {
    await auth.init();
    await processStore.load(id);

    // Poll adaptativo al loop del pipeline
    const tick = async () => {
      await pollStatus();
      await pollPipelineStates();
      pollTimer = setTimeout(tick, pollInterval()) as any;
    };
    pollTimer = setTimeout(tick, pollInterval()) as any;
  });

  onDestroy(() => {
    processStore.stopSSE(); // por si acaso quedó alguno
    processStore.reset();
    if (pollTimer) clearTimeout(pollTimer as any);
  });

  const TABS: { id: Tab; label: string }[] = [
    { id: 'monitor', label: 'monitor'   },
    { id: 'logs',    label: 'logs'      },
    { id: 'config',  label: 'configurar'},
  ];
</script>

<div class="page">
  <a href="/processes" class="back">← procesos</a>

  {#if loading}
    <div class="loading"><div class="spinner"></div>cargando...</div>

  {:else if error}
    <div class="error-banner">{error}</div>

  {:else if proc}
    <div class="proc-head">
      <div class="title-row">
        <span class="status-dot" style="background:{statusColor(proc.status)}"></span>
        <h1 class="proc-title">{proc.name}</h1>
        <span class="proc-role">{proc.user_role ?? ''}</span>
      </div>
      {#if proc.description}
        <p class="proc-desc">{proc.description}</p>
      {/if}
      <div class="proc-meta">
        <span class="meta">{proc.config?.pipelines?.length ?? 0} pipelines</span>
        <span class="dot">·</span>
        <span class="meta">{proc.status}</span>
        <span class="dot">·</span>
        <span class="meta">último dato {relTime(proc.last_seen_at)}</span>
      </div>
    </div>

    <div class="tabs">
      {#each TABS as t (t.id)}
        <button class="tab" class:active={activeTab === t.id}
          onclick={() => activeTab = t.id}>{t.label}</button>
      {/each}
    </div>

    <div class="tab-body">
      {#if activeTab === 'monitor'}
        <MonitorTab processId={id} />
      {:else if activeTab === 'logs'}
        <LogsTab processId={id} />
      {:else if activeTab === 'config'}
        <ConfigTab processId={id} />
      {/if}
    </div>
  {/if}
</div>

<style>
  .page { max-width: 1200px; margin: 0 auto; padding: calc(24px * var(--font-scale)) calc(20px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(16px * var(--font-scale)); }
  .back { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-decoration: none; }
  .back:hover { color: var(--text-primary); }

  .loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 40px 0; }
  .spinner { width: 16px; height: 16px; border: 2px solid var(--border-subtle); border-top-color: var(--text-muted); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error-banner { background: var(--error-bg); color: var(--error-color); padding: 10px 14px; border-radius: 6px; font-size: calc(13px * var(--font-scale)); }

  .proc-head { display: flex; flex-direction: column; gap: 6px; }
  .title-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .status-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
  .proc-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .proc-role { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 2px 8px; border-radius: 10px; }
  .proc-desc { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .proc-meta { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .meta { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .dot { color: var(--border-default); font-size: 12px; }

  .tabs { display: flex; border-bottom: 0.5px solid var(--border-subtle); }
  .tab { padding: calc(9px * var(--font-scale)) calc(16px * var(--font-scale)); border: none; background: none; cursor: pointer; font-size: calc(13px * var(--font-scale)); color: var(--text-muted); border-bottom: 2px solid transparent; margin-bottom: -0.5px; transition: all .12s; }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }

  .tab-body { min-height: 300px; }
</style>