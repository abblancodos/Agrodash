<!-- src/lib/components/processes/ControlTab.svelte -->
<script lang="ts">
  import { processStore, canOperate } from '$lib/stores/process';
  import PipelineChart from './PipelineChart.svelte';

  let { processId }: { processId: string } = $props();

  const proc      = $derived($processStore.process);
  const pipelines = $derived(proc?.config?.pipelines ?? []);
  const states    = $derived($processStore.pipelineStates);
  const status    = $derived(proc?.status ?? 'unknown');

  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  // ── Ciclo de vida ──────────────────────────────────────────────────────────
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

  // ── Self-test ──────────────────────────────────────────────────────────────
  const testResult  = $derived($processStore.testResult);
  const testLoading = $derived($processStore.testLoading);
  let testError = $state('');
  let testOpen  = $state(false);

  async function runTest() {
    testError = ''; testOpen = true;
    try { await processStore.selfTest(processId); }
    catch (e: any) { testError = e.message; }
  }

  // ── Override ───────────────────────────────────────────────────────────────
  let ovBusy  = $state<string | null>(null);
  let ovError = $state('');

  async function sendOverride(pipelineId: string, action: 'on' | 'off' | 'clear') {
    ovBusy = `${pipelineId}-${action}`; ovError = '';
    try {
      const cmd = action === 'clear'
        ? { cmd: 'ClearOverride', pipeline_id: pipelineId }
        : { cmd: 'Override', action, pipeline_id: pipelineId };
      await processStore.command(processId, cmd);
      await processStore.refreshPipelineState(processId, pipelineId);
    } catch (e: any) { ovError = e.message; }
    finally           { ovBusy = null; }
  }

  // ── Expand ─────────────────────────────────────────────────────────────────
  let expanded = $state<string | null>(null);

  function toggleExpand(id: string) {
    expanded = expanded === id ? null : id;
  }

  // ── Helpers ────────────────────────────────────────────────────────────────
  function getActuatorState(pid: string): string | null {
    return Object.values(states[pid]?.node_states ?? {})
      .find((n: any) => n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator')
      ?.data?.last_action ?? null;
  }

  function getFilteredValues(pid: string): number[] | null {
    const s = states[pid];
    if (!s) return null;
    const kalman = Object.values(s.node_states ?? {}).find((n: any) => n.node_type === 'kalman');
    if (kalman?.data?.x) return kalman.data.x;
    const filt = Object.values(s.node_states ?? {})
      .find((n: any) => ['moving_avg','ewma','lowpass'].includes(n.node_type));
    return filt?.data?.y ?? filt?.data?.x_hat ?? null;
  }

  function getSensorLabels(pid: string): string[] {
    return pipelines.find((p: any) => p.id === pid)
      ?.nodes.find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? [];
  }

  function getMahalanobis(pid: string): number | null {
    return Object.values(states[pid]?.node_states ?? {})
      .find((n: any) => n.node_type === 'mahalanobis')?.data?.last_d ?? null;
  }

  const activeCount = $derived(pipelines.filter((p: any) => getActuatorState(p.id) === 'on').length);
  const readyCount  = $derived(pipelines.filter((p: any) => states[p.id]?.is_ready).length);

  function statusIcon(s: string)   { return s === 'ok' ? '✓' : s === 'warn' ? '⚠' : s === 'error' ? '✗' : 'i'; }
  function overallColor(s: string) { return s === 'ok' ? '#3da85a' : s === 'warn' ? '#e8a838' : '#e05454'; }
</script>

<div class="ctrl">

  <!-- Barra global -->
  <div class="global-bar">
    <div class="global-stats">
      <span class="gs-item">
        <span class="gs-num" style="color:#3da85a">{activeCount}</span>
        <span class="gs-label">activos</span>
      </span>
      <span class="gs-sep">·</span>
      <span class="gs-item">
        <span class="gs-num">{readyCount}/{pipelines.length}</span>
        <span class="gs-label">listos</span>
      </span>
      <span class="gs-sep">·</span>
      <span class="status-badge status-{status}">{status}</span>
    </div>
    <div class="global-right">
      {#if ctrlError}<span class="ctrl-error">{ctrlError}</span>{/if}
      {#if $canOperate}
        <button class="action-btn" disabled={testLoading} onclick={runTest}>
          {testLoading ? '…' : '⬡ health'}
        </button>
        <button
          class="action-btn" class:running={status === 'running'}
          disabled={ctrlBusy || status === 'error'}
          onclick={toggleProcess}
        >
          {#if ctrlBusy}…{:else if status === 'running'}■ detener{:else}▶ iniciar{/if}
        </button>
      {:else if status !== 'running'}
        <span class="agent-offline">proceso detenido</span>
      {/if}
    </div>
  </div>

  <!-- Panel self-test -->
  {#if testOpen && (testResult || testLoading || testError)}
    <div class="test-panel">
      <div class="test-header">
        <span class="test-title">infraestructura</span>
        {#if testResult}
          <span class="test-overall" style="color:{overallColor(testResult.overall)}">{testResult.overall}</span>
        {/if}
        <button class="test-close" onclick={() => { testOpen = false; processStore.clearTestResult(); }}>✕</button>
      </div>
      {#if testLoading}
        <div class="test-msg">verificando…</div>
      {:else if testError}
        <div class="test-msg" style="color:#e05454">{testError}</div>
      {:else if testResult}
        <div class="test-checks">
          {#each testResult.checks as check (check.name)}
            <div class="check-row check-{check.status}">
              <span class="check-icon">{statusIcon(check.status)}</span>
              <span class="check-name">{check.name}</span>
              <span class="check-detail">{check.detail}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Pipelines -->
  {#if pipelines.length === 0}
    <div class="empty">Sin pipelines configurados.</div>
  {:else}
    {#if !Object.keys(states).length && status !== 'running'}
      <div class="offline-hint">agente detenido — usá ▶ iniciar para arrancar</div>
    {/if}

    <div class="pipeline-list">
      {#each pipelines as pl (pl.id)}
        {@const actState   = getActuatorState(pl.id)}
        {@const vals       = getFilteredValues(pl.id)}
        {@const sLabels    = getSensorLabels(pl.id)}
        {@const mah        = getMahalanobis(pl.id)}
        {@const isExpanded = expanded === pl.id}
        {@const isOn       = actState === 'on'}

        <div class="pipeline-card" class:valve-on={isOn} class:expanded={isExpanded}>

          <button class="card-header" onclick={() => toggleExpand(pl.id)}>
            <div class="ch-left">
              <span class="chevron" class:open={isExpanded}>▶</span>
              <span class="pl-label">{pl.label}</span>
              {#if !states[pl.id]?.is_ready && status === 'running'}
                <span class="badge-warmup">warmup</span>
              {/if}
            </div>
            <div class="ch-right">
              {#if vals?.length}
                {#each vals.slice(0, 3) as v, i (i)}
                  <span class="val-chip" style="color:{COLORS[i % COLORS.length]}">{v.toFixed(3)}</span>
                {/each}
              {/if}
              <span class="act-pill" class:on={isOn} class:off={actState === 'off'}>
                {actState?.toUpperCase() ?? '—'}
              </span>
            </div>
          </button>

          {#if isExpanded}
            <div class="card-detail">

              <!-- Valores actuales -->
              {#if vals?.length || mah != null || states[pl.id]?.cycle}
                <div class="vals-grid">
                  {#each vals ?? [] as v, i (i)}
                    <div class="val-item">
                      <span class="val-label" style="color:{COLORS[i % COLORS.length]}">{sLabels[i] ?? `s${i+1}`}</span>
                      <span class="val-num">{v.toFixed(4)}</span>
                    </div>
                  {/each}
                  {#if mah != null}
                    <div class="val-item">
                      <span class="val-label" style="color:#888">d mah</span>
                      <span class="val-num">{mah.toFixed(3)}</span>
                    </div>
                  {/if}
                  {#if states[pl.id]?.cycle}
                    <div class="val-item">
                      <span class="val-label" style="color:#888">ciclo</span>
                      <span class="val-num">{states[pl.id].cycle}</span>
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Chart — componente aislado, maneja su propio lifecycle -->
              <PipelineChart
                {processId}
                pipelineId={pl.id}
                labels={sLabels}
              />

              <!-- Override -->
              {#if $canOperate}
                <div class="override-row">
                  <span class="ov-label">override</span>
                  <button class="ovbtn ovbtn--on"  disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'on')}>ON</button>
                  <button class="ovbtn ovbtn--off" disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'off')}>OFF</button>
                  <button class="ovbtn ovbtn--auto"disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'clear')}>↺ auto</button>
                  {#if states[pl.id]?.override_active}<span class="ov-active">override activo</span>{/if}
                  {#if ovError}<span class="ov-error">{ovError}</span>{/if}
                </div>
              {/if}

            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

</div>

<style>
  .ctrl { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  .global-bar    { display:flex; align-items:center; justify-content:space-between; padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background:var(--bg-elevated); border-radius:8px; border:0.5px solid var(--border-subtle); flex-wrap:wrap; gap:8px; }
  .global-stats  { display:flex; align-items:center; gap:10px; flex-wrap:wrap; }
  .gs-item       { display:flex; align-items:baseline; gap:4px; }
  .gs-num        { font-size:calc(16px * var(--font-scale)); font-weight:600; font-family:'DM Mono',monospace; color:var(--text-primary); }
  .gs-label      { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); }
  .gs-sep        { color:var(--border-default); }
  .global-right  { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
  .agent-offline { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; font-style:italic; }
  .ctrl-error    { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }

  .status-badge   { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; padding:2px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); }
  .status-running  { background:#EAF3DE; color:#3B6D11; }
  .status-stopping { background:#FEF3C7; color:#92400E; }
  .status-error   { background:#FCEBEB; color:#A32D2D; }

  .action-btn { padding:calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border-radius:6px; border:0.5px solid var(--border-default); background:none; cursor:pointer; font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); transition:background .15s; }
  .action-btn:hover:not(:disabled) { background:var(--interactive-hover); }
  .action-btn:disabled { opacity:0.4; cursor:default; }
  .action-btn.running { color:#e05454; border-color:#e0545444; }
  .action-btn.running:hover:not(:disabled) { background:#FCEBEB; }

  .test-panel  { background:var(--bg-surface); border:0.5px solid var(--border-default); border-radius:10px; overflow:hidden; }
  .test-header { display:flex; align-items:center; gap:10px; padding:calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom:0.5px solid var(--border-subtle); }
  .test-title  { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); flex:1; }
  .test-overall{ font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:600; }
  .test-close  { background:none; border:none; cursor:pointer; color:var(--text-muted); font-size:13px; padding:2px 4px; border-radius:4px; }
  .test-close:hover { background:var(--interactive-hover); }
  .test-msg    { padding:14px; font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); font-style:italic; }
  .test-checks { display:flex; flex-direction:column; }
  .check-row   { display:grid; grid-template-columns:18px 1fr 2fr; gap:8px; align-items:start; padding:calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom:0.5px solid var(--border-subtle); }
  .check-row:last-child { border-bottom:none; }
  .check-icon  { font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:600; text-align:center; }
  .check-name  { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-primary); }
  .check-detail{ font-size:calc(11px * var(--font-scale)); color:var(--text-muted); word-break:break-all; }
  .check-ok    .check-icon { color:#3da85a; }
  .check-warn  .check-icon { color:#e8a838; }
  .check-error .check-icon { color:#e05454; }
  .check-warn  { background:#FEF3C710; }
  .check-error { background:#FCEBEB10; }

  .empty        { color:var(--text-muted); font-size:calc(13px * var(--font-scale)); padding:32px 0; text-align:center; }
  .offline-hint { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-style:italic; padding:calc(4px * var(--font-scale)) 0; }
  .pipeline-list{ display:flex; flex-direction:column; gap:calc(6px * var(--font-scale)); }

  .pipeline-card { background:var(--bg-surface); border:0.5px solid var(--border-default); border-radius:10px; overflow:hidden; transition:border-color .15s; }
  .pipeline-card.valve-on { border-color:#3da85a55; }
  .pipeline-card.expanded  { border-color:var(--text-muted); }

  .card-header { width:100%; display:flex; align-items:center; justify-content:space-between; padding:calc(11px * var(--font-scale)) calc(14px * var(--font-scale)); background:none; border:none; cursor:pointer; text-align:left; }
  .card-header:hover { background:var(--interactive-hover); }
  .ch-left  { display:flex; align-items:center; gap:8px; }
  .ch-right { display:flex; align-items:center; gap:8px; }
  .chevron  { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); transition:transform .15s; display:inline-block; }
  .chevron.open { transform:rotate(90deg); }
  .pl-label { font-size:calc(14px * var(--font-scale)); font-weight:500; color:var(--text-primary); }
  .badge-warmup { font-size:calc(10px * var(--font-scale)); padding:1px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .val-chip { font-size:calc(13px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; }
  .act-pill { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:600; padding:2px 8px; border-radius:4px; background:var(--bg-inset); color:var(--text-muted); }
  .act-pill.on  { background:#EAF3DE; color:#3B6D11; }
  .act-pill.off { background:#FCEBEB; color:#A32D2D; }

  .card-detail { padding:calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); border-top:0.5px solid var(--border-subtle); display:flex; flex-direction:column; gap:calc(12px * var(--font-scale)); }

  .vals-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(90px,1fr)); gap:calc(6px * var(--font-scale)); }
  .val-item  { display:flex; flex-direction:column; gap:2px; }
  .val-label { font-size:calc(10px * var(--font-scale)); font-weight:500; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .val-num   { font-size:calc(14px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; color:var(--text-primary); }

  .override-row { display:flex; align-items:center; gap:6px; padding-top:calc(6px * var(--font-scale)); border-top:0.5px solid var(--border-subtle); flex-wrap:wrap; }
  .ov-label { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .ovbtn    { padding:calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:6px; background:none; cursor:pointer; font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); }
  .ovbtn:disabled { opacity:0.4; }
  .ovbtn--on:hover   { background:#EAF3DE; color:#3da85a; }
  .ovbtn--off:hover  { background:#FCEBEB; color:#e05454; }
  .ovbtn--auto:hover { background:var(--interactive-hover); }
  .ov-active { font-size:calc(10px * var(--font-scale)); padding:2px 7px; border-radius:10px; background:#FEF3C7; color:#92400E; font-family:'DM Mono',monospace; }
  .ov-error  { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }
</style>