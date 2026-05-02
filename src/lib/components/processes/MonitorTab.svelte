<!-- src/lib/components/processes/MonitorTab.svelte -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { processStore, type ProcessReading } from '$lib/stores/process';
  import PipelineCard from './PipelineCard.svelte';

  let { processId }: { processId: string } = $props();

  const proc      = $derived(processStore.process);
  const canOperate = $derived(['operator','admin'].includes(processStore.process?.user_role ?? ''));
  const pipelines = $derived(proc?.config?.pipelines ?? []);
  const states    = $derived(processStore.pipelineStates);
  const status    = $derived(proc?.status ?? 'unknown');
  const lastCycle = $derived(processStore.lastCycle);

  // Readings por pipeline
  let readings   = $state<Record<string, ProcessReading[]>>({});
  let rdLoading  = $state<Record<string, boolean>>({});
  let timePresets = $state<Record<string, string>>({});

  function getPreset(pid: string) { return timePresets[pid] ?? '6h'; }

  async function loadReadings(pid: string) {
    const hours = ({ '1h':1,'6h':6,'24h':24,'7d':168 })[getPreset(pid)] ?? 6;
    rdLoading = { ...rdLoading, [pid]: true };
    try {
      const data = await processStore.fetchReadings(processId, pid, hours, 1000);
      readings = { ...readings, [pid]: data };
    } catch {}
    finally { rdLoading = { ...rdLoading, [pid]: false }; }
  }

  function setPreset(pid: string, label: string) {
    timePresets = { ...timePresets, [pid]: label };
    loadReadings(pid);
  }

  // Auto-refrescar con el ciclo.
  // untrack() evita que las escrituras en readings/rdLoading dentro de loadReadings
  // vuelvan a disparar este effect → previene el loop infinito effect_update_depth_exceeded.
  $effect(() => {
    void lastCycle;           // dependencia explícita: re-corre cuando cambia el ciclo
    const pls = pipelines;   // captura pipelines como dependencia
    untrack(() => {
      for (const pl of pls) loadReadings(pl.id);
    });
  });

  // ── Barra global ───────────────────────────────────────────────────────────
  let ctrlBusy  = $state(false);
  let ctrlError = $state('');

  async function toggleProcess() {
    ctrlBusy = true; ctrlError = '';
    try {
      if (status === 'running') await processStore.stop(processId);
      else                      await processStore.start(processId);
    } catch (e: any) { ctrlError = e.message; }
    finally          { ctrlBusy = false; }
  }

  const activeCount  = $derived(
    pipelines.filter((p: any) => {
      const s = states[p.id];
      return Object.values(s?.node_states ?? {})
        .some((n: any) => (n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator')
          && n.data?.last_action === 'on');
    }).length
  );
  const readyCount = $derived(pipelines.filter((p: any) => states[p.id]?.is_ready).length);
</script>

<div class="monitor">

  <!-- Barra global -->
  <div class="global-bar">
    <div class="g-stats">
      <span class="g-item">
        <span class="g-num" style="color:#3da85a">{activeCount}</span>
        <span class="g-lbl">activos</span>
      </span>
      <span class="g-sep">·</span>
      <span class="g-item">
        <span class="g-num">{readyCount}/{pipelines.length}</span>
        <span class="g-lbl">listos</span>
      </span>
      <span class="g-sep">·</span>
      <span class="status-badge status-{status}">{status}</span>
    </div>
    <div class="g-right">
      {#if ctrlError}<span class="g-error">{ctrlError}</span>{/if}
      {#if canOperate}
        <button class="action-btn" class:running={status === 'running'}
          disabled={ctrlBusy || status === 'error'}
          onclick={toggleProcess}>
          {#if ctrlBusy}…{:else if status === 'running'}■ detener{:else}▶ iniciar{/if}
        </button>
      {/if}
    </div>
  </div>

  <!-- Cards de pipeline -->
  {#if pipelines.length === 0}
    <div class="empty">Sin pipelines — configurá uno en el tab configurar.</div>
  {:else}
    <div class="pipeline-list">
      {#each pipelines as pl (pl.id)}
        <PipelineCard
          {processId}
          pipeline={pl}
          state={states[pl.id]}
          readings={readings[pl.id] ?? []}
          rdLoading={rdLoading[pl.id] ?? false}
          timePreset={getPreset(pl.id)}
          onSetPreset={(label) => setPreset(pl.id, label)}
        />
      {/each}
    </div>
  {/if}

</div>

<style>
  .monitor { display:flex; flex-direction:column; gap:calc(12px * var(--font-scale)); }

  .global-bar  { display:flex; align-items:center; justify-content:space-between; padding:calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background:var(--bg-elevated); border-radius:8px; border:0.5px solid var(--border-subtle); flex-wrap:wrap; gap:8px; }
  .g-stats     { display:flex; align-items:center; gap:10px; flex-wrap:wrap; }
  .g-item      { display:flex; align-items:baseline; gap:4px; }
  .g-num       { font-size:calc(16px * var(--font-scale)); font-weight:600; font-family:'DM Mono',monospace; color:var(--text-primary); }
  .g-lbl       { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); }
  .g-sep       { color:var(--border-default); }
  .g-right     { display:flex; align-items:center; gap:8px; }
  .g-error     { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }

  .status-badge    { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; padding:2px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); }
  .status-running  { background:#EAF3DE; color:#3B6D11; }
  .status-stopping { background:#FEF3C7; color:#92400E; }
  .status-error    { background:#FCEBEB; color:#A32D2D; }

  .action-btn { padding:calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border-radius:6px; border:0.5px solid var(--border-default); background:none; cursor:pointer; font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); }
  .action-btn:hover:not(:disabled)       { background:var(--interactive-hover); }
  .action-btn:disabled                   { opacity:.4; cursor:default; }
  .action-btn.running                    { color:#e05454; border-color:#e0545444; }
  .action-btn.running:hover:not(:disabled){ background:#FCEBEB; }

  .empty         { color:var(--text-muted); font-size:calc(13px * var(--font-scale)); padding:40px 0; text-align:center; }
  .pipeline-list { display:flex; flex-direction:column; gap:calc(8px * var(--font-scale)); }
</style>