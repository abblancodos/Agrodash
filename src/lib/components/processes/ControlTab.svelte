<!-- src/lib/components/processes/ControlTab.svelte -->
<script lang="ts">
  import {
    processStore, canOperate,
    type ProcessReading, type SelfTestReport, type CheckStatus,
  } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const proc      = $derived($processStore.process);
  const pipelines = $derived(proc?.config?.pipelines ?? []);
  const states    = $derived($processStore.pipelineStates);

  // ── Estado del proceso ────────────────────────────────────────────────────
  const procStatus = $derived(proc?.status ?? 'unknown');
  const isRunning  = $derived(procStatus === 'running');

  let ctrlBusy  = $state(false);
  let ctrlError = $state('');

  async function toggleProcess() {
    ctrlBusy = true; ctrlError = '';
    try {
      if (isRunning) {
        await processStore.stop(processId);
      } else {
        await processStore.start(processId);
      }
    } catch (e: any) {
      ctrlError = e.message;
    } finally {
      ctrlBusy = false;
    }
  }

  // ── Self-test ─────────────────────────────────────────────────────────────
  let testReport  = $state<SelfTestReport | null>(null);
  let testLoading = $state(false);
  let testError   = $state('');

  async function runTest(healthOnly: boolean) {
    testLoading = true; testError = ''; testReport = null;
    try {
      testReport = await processStore.selfTest(processId, healthOnly);
    } catch (e: any) {
      testError = e.message;
    } finally {
      testLoading = false;
    }
  }

  function statusColor(s: CheckStatus): string {
    return s === 'ok' ? '#3da85a' : s === 'warn' ? '#e07b20' : '#e05454';
  }
  function statusSymbol(s: CheckStatus): string {
    return s === 'ok' ? '✓' : s === 'warn' ? '⚠' : '✗';
  }

  // ── Pipeline detail ───────────────────────────────────────────────────────
  let expanded  = $state<string | null>(null);
  let readings  = $state<Record<string, ProcessReading[]>>({});
  let rdLoading = $state<Record<string, boolean>>({});
  let ovBusy    = $state<string | null>(null);
  let ovError   = $state('');

  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  function toggleExpand(id: string) {
    if (expanded === id) { expanded = null; return; }
    expanded = id;
    if (!readings[id]) loadReadings(id);
  }

  async function loadReadings(pipelineId: string) {
    rdLoading = { ...rdLoading, [pipelineId]: true };
    try {
      const data = await processStore.fetchReadings(processId, pipelineId, 2, 200);
      readings = { ...readings, [pipelineId]: data };
    } catch {}
    finally { rdLoading = { ...rdLoading, [pipelineId]: false }; }
  }

  async function sendOverride(pipelineId: string, action: 'on' | 'off' | 'clear') {
    ovBusy = `${pipelineId}-${action}`; ovError = '';
    try {
      const cmd = action === 'clear'
        ? { cmd: 'ClearOverride', pipeline_id: pipelineId }
        : { cmd: 'Override', action, pipeline_id: pipelineId };
      await processStore.command(processId, cmd);
      await processStore.refreshPipelineState(processId, pipelineId);
    } catch (e: any) { ovError = e.message; }
    finally { ovBusy = null; }
  }

  // ── Helpers de estado ─────────────────────────────────────────────────────
  function getActuatorState(pipelineId: string): string | null {
    const s = states[pipelineId];
    if (!s) return null;
    const actNode = Object.values(s.node_states ?? {})
      .find(n => n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator');
    return actNode?.data?.last_action ?? null;
  }

  function getFilteredValues(pipelineId: string): number[] | null {
    const s = states[pipelineId];
    if (!s) return null;
    const kalman = Object.values(s.node_states ?? {})
      .find(n => n.node_type === 'kalman');
    if (kalman?.data?.x) return kalman.data.x;
    const filt = Object.values(s.node_states ?? {})
      .find(n => ['moving_avg','ewma','lowpass'].includes(n.node_type));
    return filt?.data?.y ?? filt?.data?.x_hat ?? null;
  }

  function getLabels(pipelineId: string): string[] {
    const pl = pipelines.find((p: any) => p.id === pipelineId);
    if (!pl) return [];
    const src = pl.nodes.find((n: any) => n.type === 'postgres_sensor');
    return src?.sensors?.map((s: any) => s.label) ?? [];
  }

  function getMahalanobis(pipelineId: string): number | null {
    const s = states[pipelineId];
    if (!s) return null;
    const dec = Object.values(s.node_states ?? {})
      .find(n => n.node_type === 'mahalanobis');
    return dec?.data?.last_d ?? null;
  }

  function isReady(pipelineId: string): boolean {
    return states[pipelineId]?.is_ready ?? false;
  }

  function buildMiniChart(pipelineId: string): string | null {
    const data = readings[pipelineId];
    if (!data?.length) return null;
    const W = 300; const H = 60;
    const allVals = data.flatMap(r => r.filtered ?? r.raw ?? []);
    if (!allVals.length) return null;
    const minV = Math.min(...allVals);
    const maxV = Math.max(...allVals);
    const rv = maxV - minV || 1;
    const minT = new Date(data[0].ts).getTime();
    const maxT = new Date(data[data.length-1].ts).getTime();
    const rt = maxT - minT || 1;
    const n = data[0].filtered?.length ?? data[0].raw?.length ?? 0;
    const paths = Array.from({length: n}, (_, i) => {
      let d = '';
      for (const r of data) {
        const v = (r.filtered ?? r.raw)?.[i];
        if (v == null) continue;
        const x = ((new Date(r.ts).getTime() - minT) / rt) * W;
        const y = H - ((v - minV) / rv) * H;
        d += d ? ` L${x.toFixed(1)},${y.toFixed(1)}` : `M${x.toFixed(1)},${y.toFixed(1)}`;
      }
      return `<path d="${d}" stroke="${COLORS[i % COLORS.length]}" stroke-width="1.5" fill="none"/>`;
    });
    return `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" style="width:100%;height:60px">${paths.join('')}</svg>`;
  }

  const activeCount = $derived(
    pipelines.filter((p: any) => getActuatorState(p.id) === 'on').length
  );
  const readyCount = $derived(
    pipelines.filter((p: any) => isReady(p.id)).length
  );
</script>

<div class="ctrl">

  <!-- ── Barra global ─────────────────────────────────────────────────────── -->
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
      <span class="gs-item">
        <span class="status-dot" class:running={isRunning} class:stopped={procStatus==='stopped'} class:error={procStatus==='error'}></span>
        <span class="gs-label">{procStatus}</span>
      </span>
    </div>

    <div class="global-right">
      {#if ctrlError}
        <span class="ctrl-error">{ctrlError}</span>
      {/if}

      <!-- Self-test (solo infra, no health-only) -->
      {#if $canOperate}
        <button
          class="test-btn"
          disabled={testLoading}
          onclick={() => runTest(isRunning)}
          title={isRunning ? 'Health check de agentes activos' : 'Verificar infraestructura antes de arrancar'}
        >
          {testLoading ? '...' : isRunning ? '⚕ health' : '⚕ test infra'}
        </button>

        <!-- Start / Stop -->
        <button
          class="ctrl-btn"
          class:running={isRunning}
          disabled={ctrlBusy || procStatus === 'error'}
          onclick={toggleProcess}
        >
          {#if ctrlBusy}
            ...
          {:else if isRunning}
            ⏹ detener
          {:else}
            ▶ iniciar
          {/if}
        </button>
      {/if}
    </div>
  </div>

  <!-- ── Resultado del self-test ──────────────────────────────────────────── -->
  {#if testReport}
    <div class="test-panel">
      <div class="test-header">
        <span class="test-overall" style="color:{statusColor(testReport.overall)}">
          {statusSymbol(testReport.overall)}
          {testReport.overall === 'ok' ? 'todo bien' : testReport.overall === 'warn' ? 'advertencias' : 'errores detectados'}
        </span>
        <span class="test-duration">{testReport.duration_ms}ms</span>
        <button class="test-close" onclick={() => testReport = null}>✕</button>
      </div>
      <div class="test-checks">
        {#each testReport.checks as check}
          <div class="check-row">
            <span class="check-sym" style="color:{statusColor(check.status)}">{statusSymbol(check.status)}</span>
            <span class="check-name">{check.name}</span>
            <span class="check-detail">{check.detail}</span>
            {#if check.latency_ms != null}
              <span class="check-lat">{check.latency_ms}ms</span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if testError}
    <div class="test-error">Error al ejecutar test: {testError}</div>
  {/if}

  {#if pipelines.length === 0}
    <div class="empty">
      Sin pipelines configurados. Ir al tab <strong>configurar</strong> para agregar pipelines.
    </div>

  {:else}
    <!-- ── Grid de pipelines ───────────────────────────────────────────────── -->
    <div class="pipeline-list">
      {#each pipelines as pl (pl.id)}
        {@const actState  = getActuatorState(pl.id)}
        {@const vals      = getFilteredValues(pl.id)}
        {@const labels    = getLabels(pl.id)}
        {@const ready     = isReady(pl.id)}
        {@const mah       = getMahalanobis(pl.id)}
        {@const isExpanded = expanded === pl.id}
        {@const isOn      = actState === 'on'}

        <div class="pipeline-card" class:valve-on={isOn} class:expanded={isExpanded}>

          <button class="card-header" onclick={() => toggleExpand(pl.id)}>
            <div class="ch-left">
              <span class="chevron" class:open={isExpanded}>▶</span>
              <span class="pl-label">{pl.label}</span>
              {#if !ready}
                <span class="badge badge-warmup">warmup</span>
              {/if}
              {#if !isRunning}
                <span class="badge badge-offline">offline</span>
              {/if}
            </div>
            <div class="ch-right">
              {#if vals?.length}
                <span class="val-preview" style="color:{COLORS[0]}">
                  {vals[0].toFixed(3)}
                </span>
              {/if}
              <span class="act-indicator" class:on={isOn} class:off={actState === 'off'}>
                {actState?.toUpperCase() ?? '—'}
              </span>
            </div>
          </button>

          {#if isExpanded}
            <div class="card-detail">

              {#if vals?.length}
                <div class="vals-grid">
                  {#each vals as v, i (i)}
                    <div class="val-item">
                      <span class="val-label" style="color:{COLORS[i % COLORS.length]}">
                        {labels[i] ?? `s${i+1}`}
                      </span>
                      <span class="val-num">{v.toFixed(4)}</span>
                    </div>
                  {/each}
                </div>
              {/if}

              <div class="decision-row">
                {#if mah != null}
                  <span class="ds-item">
                    <span class="ds-label">d Mahalanobis</span>
                    <span class="ds-val mono">{mah.toFixed(3)}</span>
                  </span>
                {/if}
                {#if states[pl.id]?.cycle}
                  <span class="ds-item">
                    <span class="ds-label">ciclo</span>
                    <span class="ds-val mono">{states[pl.id].cycle}</span>
                  </span>
                {/if}
              </div>

              {#if rdLoading[pl.id]}
                <div class="chart-loading">cargando chart...</div>
              {:else}
                {@const svg = buildMiniChart(pl.id)}
                {#if svg}
                  <div class="mini-chart">{@html svg}</div>
                {/if}
              {/if}

              {#if $canOperate && isRunning}
                <div class="override-row">
                  <span class="ov-label">override</span>
                  <button class="ovbtn ovbtn--on"
                    disabled={ovBusy !== null}
                    onclick={() => sendOverride(pl.id, 'on')}>ON</button>
                  <button class="ovbtn ovbtn--off"
                    disabled={ovBusy !== null}
                    onclick={() => sendOverride(pl.id, 'off')}>OFF</button>
                  <button class="ovbtn ovbtn--auto"
                    disabled={ovBusy !== null}
                    onclick={() => sendOverride(pl.id, 'clear')}>↺ auto</button>
                  {#if states[pl.id]?.override_active}
                    <span class="ov-active-badge">override activo</span>
                  {/if}
                </div>
              {/if}

              {#if ovError}
                <span class="ov-error">{ovError}</span>
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

  /* ── Global bar ─────────────────────────────────────────────────────────── */
  .global-bar { display: flex; align-items: center; justify-content: space-between; padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; border: 0.5px solid var(--border-subtle); }
  .global-stats { display: flex; align-items: center; gap: 10px; }
  .gs-item { display: flex; align-items: center; gap: 4px; }
  .gs-num { font-size: calc(16px * var(--font-scale)); font-weight: 600; font-family: 'DM Mono', monospace; color: var(--text-primary); }
  .gs-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .gs-sep { color: var(--border-default); }
  .global-right { display: flex; align-items: center; gap: 8px; }

  /* status dot */
  .status-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  .status-dot.running { background: #3da85a; }
  .status-dot.stopped { background: var(--text-muted); }
  .status-dot.error   { background: #e05454; }

  /* ctrl buttons */
  .ctrl-btn { padding: calc(5px * var(--font-scale)) calc(14px * var(--font-scale)); border-radius: 6px; border: 0.5px solid var(--border-default); background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); transition: background .15s, color .15s; }
  .ctrl-btn:hover:not(:disabled) { background: var(--interactive-hover); }
  .ctrl-btn:disabled { opacity: 0.4; cursor: default; }
  .ctrl-btn.running { color: #e05454; border-color: #e0545444; }
  .ctrl-btn.running:hover:not(:disabled) { background: #FCEBEB; }
  .ctrl-error { font-size: calc(11px * var(--font-scale)); color: #e05454; font-family: 'DM Mono', monospace; }

  .test-btn { padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border-radius: 6px; border: 0.5px solid var(--border-default); background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); }
  .test-btn:hover:not(:disabled) { background: var(--interactive-hover); color: var(--text-secondary); }
  .test-btn:disabled { opacity: 0.4; cursor: default; }

  /* ── Self-test panel ────────────────────────────────────────────────────── */
  .test-panel { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; overflow: hidden; }
  .test-header { display: flex; align-items: center; gap: 10px; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .test-overall { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 600; }
  .test-duration { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-left: auto; }
  .test-close { margin-left: 8px; background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: calc(12px * var(--font-scale)); padding: 2px 4px; }
  .test-close:hover { color: var(--text-secondary); }

  .test-checks { display: flex; flex-direction: column; }
  .check-row { display: flex; align-items: baseline; gap: calc(8px * var(--font-scale)); padding: calc(6px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .check-row:last-child { border-bottom: none; }
  .check-sym { font-size: calc(12px * var(--font-scale)); font-weight: 600; flex-shrink: 0; width: 14px; }
  .check-name { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-primary); flex-shrink: 0; min-width: 160px; }
  .check-detail { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); flex: 1; }
  .check-lat { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; flex-shrink: 0; }
  .test-error { font-size: calc(12px * var(--font-scale)); color: #e05454; padding: 8px; font-family: 'DM Mono', monospace; }

  /* ── Empty ──────────────────────────────────────────────────────────────── */
  .empty { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 32px 0; text-align: center; line-height: 1.6; }

  /* ── Pipeline list ──────────────────────────────────────────────────────── */
  .pipeline-list { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .pipeline-card { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; overflow: hidden; transition: border-color .15s; }
  .pipeline-card.valve-on { border-color: #3da85a66; }
  .pipeline-card.expanded { border-color: var(--text-primary); }

  .card-header { width: 100%; display: flex; align-items: center; justify-content: space-between; padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); background: none; border: none; cursor: pointer; text-align: left; transition: background .1s; }
  .card-header:hover { background: var(--interactive-hover); }
  .ch-left { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); }
  .ch-right { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); }
  .chevron { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); transition: transform .15s; display: inline-block; flex-shrink: 0; }
  .chevron.open { transform: rotate(90deg); }
  .pl-label { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }

  .badge-warmup  { font-size: calc(10px * var(--font-scale)); padding: 1px 7px; border-radius: 10px; background: var(--bg-inset); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .badge-offline { font-size: calc(10px * var(--font-scale)); padding: 1px 7px; border-radius: 10px; background: var(--bg-inset); color: var(--text-muted); font-family: 'DM Mono', monospace; }

  .val-preview { font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; }
  .act-indicator { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 600; padding: 2px 8px; border-radius: 4px; background: var(--bg-inset); color: var(--text-muted); }
  .act-indicator.on  { background: #EAF3DE; color: #3B6D11; }
  .act-indicator.off { background: #FCEBEB; color: #A32D2D; }

  .card-detail { padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }

  .vals-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: calc(6px * var(--font-scale)); }
  .val-item { display: flex; flex-direction: column; gap: 2px; }
  .val-label { font-size: calc(10px * var(--font-scale)); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .val-num { font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; color: var(--text-primary); }

  .decision-row { display: flex; align-items: center; gap: calc(16px * var(--font-scale)); flex-wrap: wrap; }
  .ds-item { display: flex; flex-direction: column; gap: 1px; }
  .ds-label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); }
  .ds-val { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); }
  .mono { font-family: 'DM Mono', monospace; }

  .chart-loading { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .mini-chart { background: var(--bg-elevated); border-radius: 6px; padding: 4px; overflow: hidden; }

  .override-row { display: flex; align-items: center; gap: 6px; padding-top: calc(6px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); flex-wrap: wrap; }
  .ov-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .ovbtn { padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); }
  .ovbtn:disabled { opacity: 0.4; }
  .ovbtn--on  { color: #3da85a; border-color: #3da85a44; } .ovbtn--on:hover  { background: #EAF3DE; }
  .ovbtn--off { color: #e05454; border-color: #e0545444; } .ovbtn--off:hover { background: #FCEBEB; }
  .ovbtn--auto:hover { background: var(--interactive-hover); }
  .ov-active-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 10px; background: #FEF3C7; color: #92400E; font-family: 'DM Mono', monospace; }
  .ov-error { font-size: calc(11px * var(--font-scale)); color: #e05454; font-family: 'DM Mono', monospace; }
</style>