<!-- src/lib/components/processes/MonitorTab.svelte -->
<script lang="ts">
  import { untrack }    from 'svelte';
  import { processStore, type ProcessReading } from '$lib/stores/process';
  import PipelineCard   from './PipelineCard.svelte';

  let { processId }: { processId: string } = $props();

  const proc       = $derived(processStore.process);
  const canOperate = $derived(['operator','admin'].includes(proc?.user_role ?? ''));
  const pipelines  = $derived(proc?.config?.pipelines ?? []);
  const states     = $derived(processStore.pipelineStates);
  const status     = $derived(proc?.status ?? 'unknown');
  const lastCycle  = $derived(processStore.lastCycle);

  // Preset de tiempo global — todos los pipelines lo comparten
  let timePreset = $state('6h');
  const hoursMap: Record<string, number> = { '1h':1, '6h':6, '24h':24, '7d':168 };

  // Readings por pipeline
  let readings  = $state<Record<string, ProcessReading[]>>({});
  let rdLoading = $state<Record<string, boolean>>({});

  async function loadReadings(pid: string) {
    const hours = hoursMap[timePreset] ?? 6;
    rdLoading = { ...rdLoading, [pid]: true };
    try {
      const data = await processStore.fetchReadings(processId, pid, hours, 1000);
      readings = { ...readings, [pid]: data };
    } catch {}
    finally { rdLoading = { ...rdLoading, [pid]: false }; }
  }

  function setPreset(label: string) {
    timePreset = label;
    untrack(() => { for (const pl of pipelines) loadReadings(pl.id); });
  }

  $effect(() => {
    void lastCycle;
    const pls = pipelines;
    untrack(() => { for (const pl of pls) loadReadings(pl.id); });
  });

  // ── Controles globales ─────────────────────────────────────────────────────
  let ctrlBusy  = $state(false);
  let ctrlError = $state('');

  async function toggleProcess() {
    ctrlBusy = true; ctrlError = '';
    try {
      if (status === 'running') await processStore.stop(processId);
      else                      await processStore.start(processId);
    } catch (e: any) { ctrlError = e.message; }
    finally { ctrlBusy = false; }
  }

  async function allOff() {
    // Mandar ClearOverride + Override off a todos los actuadores de todos los pipelines
    for (const pl of pipelines) {
      const ns = states[pl.id]?.node_states ?? {};
      for (const [nid, n] of Object.entries(ns) as any) {
        if (n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator') {
          processStore.wsSend({
            type: 'command', cmd: 'Override', action: 'off',
            pipeline_id: pl.id, actuator_id: nid,
          });
        }
      }
    }
  }

  // ── Stats ──────────────────────────────────────────────────────────────────
  const activeCount = $derived(
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
    <div class="g-left">
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
    </div>

    <div class="g-right">
      <!-- Preset global -->
      <div class="presets">
        {#each ['1h','6h','24h','7d'] as p (p)}
          <button class="pbtn" class:active={timePreset === p}
            onclick={() => setPreset(p)}>{p}</button>
        {/each}
      </div>

      {#if canOperate && activeCount > 0}
        <button class="all-off-btn" onclick={allOff}>
          ■ apagar todo
        </button>
      {/if}

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

  <!-- Grid de pipelines -->
  {#if pipelines.length === 0}
    <div class="empty">Sin pipelines — configurá uno en el tab configurar.</div>
  {:else}
    <div class="pl-grid">
      {#each pipelines as pl (pl.id)}
        <PipelineCard
          {processId}
          pipeline={pl}
          state={states[pl.id]}
          readings={readings[pl.id] ?? []}
          rdLoading={rdLoading[pl.id] ?? false}
          {timePreset}
          {canOperate}
        />
      {/each}
    </div>
  {/if}

</div>

<style>
  .monitor { display:flex; flex-direction:column; gap:calc(10px * var(--font-scale)); }

  /* ── Barra global ── */
  .global-bar { display:flex; align-items:center; justify-content:space-between; padding:calc(7px * var(--font-scale)) calc(12px * var(--font-scale)); background:var(--bg-elevated); border-radius:8px; border:0.5px solid var(--border-subtle); flex-wrap:wrap; gap:8px; }
  .g-left  { display:flex; align-items:center; gap:8px; }
  .g-right { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
  .g-stats { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
  .g-item  { display:flex; align-items:baseline; gap:4px; }
  .g-num   { font-size:calc(15px * var(--font-scale)); font-weight:600; font-family:'DM Mono',monospace; color:var(--text-primary); }
  .g-lbl   { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); }
  .g-sep   { color:var(--border-default); }
  .g-error { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }

  .status-badge   { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; padding:1px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); }
  .status-running { background:#EAF3DE; color:#3B6D11; }
  .status-stopping{ background:#FEF3C7; color:#92400E; }
  .status-error   { background:#FCEBEB; color:#A32D2D; }

  .presets { display:flex; gap:2px; }
  .pbtn    { padding:2px 7px; border:0.5px solid var(--border-default); border-radius:5px; background:none; cursor:pointer; font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .pbtn:hover  { background:var(--interactive-hover); }
  .pbtn.active { background:var(--bg-elevated); color:var(--text-primary); border-color:var(--text-muted); }

  .all-off-btn { padding:calc(3px * var(--font-scale)) calc(9px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:5px; background:none; cursor:pointer; font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:#e05454; border-color:#e0545444; }
  .all-off-btn:hover { background:#FCEBEB; }

  .action-btn { padding:calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border-radius:6px; border:0.5px solid var(--border-default); background:none; cursor:pointer; font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); }
  .action-btn:hover:not(:disabled)        { background:var(--interactive-hover); }
  .action-btn:disabled                    { opacity:.4; cursor:default; }
  .action-btn.running                     { color:#e05454; border-color:#e0545444; }
  .action-btn.running:hover:not(:disabled){ background:#FCEBEB; }

  /* ── Grid 2 columnas ── */
  .pl-grid { display:grid; grid-template-columns:repeat(2, minmax(0,1fr)); gap:calc(8px * var(--font-scale)); }
  @media (max-width: 640px) { .pl-grid { grid-template-columns:1fr; } }

  .empty { color:var(--text-muted); font-size:calc(13px * var(--font-scale)); padding:40px 0; text-align:center; }
</style>