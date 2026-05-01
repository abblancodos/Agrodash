<!-- src/lib/components/processes/ControlTab.svelte -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { processStore, canOperate, type ProcessReading, type TestCheck } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const proc      = $derived($processStore.process);
  const pipelines = $derived(proc?.config?.pipelines ?? []);
  const states    = $derived($processStore.pipelineStates);
  const status    = $derived(proc?.status ?? 'unknown');

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

  // ── Pipeline expand ────────────────────────────────────────────────────────
  let expanded = $state<string | null>(null);

  function toggleExpand(id: string) {
    if (expanded === id) { expanded = null; destroyChart(id); return; }
    if (expanded)        { destroyChart(expanded); }
    expanded = id;
    // Mostrar skeleton inmediatamente — mountCanvas dispara loadAndRender
    rdLoading = { ...rdLoading, [id]: true };
    rdEmpty   = { ...rdEmpty,   [id]: false };
    // Reset chart existente para forzar re-fetch al expandir
    if (charts[id]) { charts[id].destroy(); charts = { ...charts, [id]: null }; }
  }

  // ── Chart.js (igual a SensorChart) ────────────────────────────────────────
  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  // Map de canvases por pipeline_id — actualizado via acción use:mountCanvas
  const canvasMap = new Map<string, HTMLCanvasElement>();
  let charts    = $state<Record<string, any>>({});
  let rdLoading = $state<Record<string, boolean>>({});
  let rdEmpty   = $state<Record<string, boolean>>({});

  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function formatLabel(isoStr: string): string {
    const d = new Date(isoStr.endsWith('Z') ? isoStr : isoStr + 'Z');
    return d.toLocaleTimeString('es-CR', { hour: '2-digit', minute: '2-digit', hour12: false });
  }

  function destroyChart(id: string) {
    charts[id]?.destroy();
    charts = { ...charts, [id]: null };
  }

  async function loadAndRender(pipelineId: string) {
    rdLoading = { ...rdLoading, [pipelineId]: true };
    rdEmpty   = { ...rdEmpty,   [pipelineId]: false };
    try {
      const data = await processStore.fetchReadings(processId, pipelineId, 2, 300);
      if (!data.length) { rdEmpty = { ...rdEmpty, [pipelineId]: true }; return; }
      renderChart(pipelineId, data);
    } catch {}
    finally { rdLoading = { ...rdLoading, [pipelineId]: false }; }
  }

  function renderChart(pipelineId: string, data: ProcessReading[]) {
    const canvas = canvasMap.get(pipelineId);
    if (!canvas) return;

    const labels = data.map(r => formatLabel(r.ts));
    const tick   = cssVar('--chart-tick');
    const grid   = cssVar('--chart-grid');

    // Series: filtered (una por dimensión) + logger nodes si existen
    const pl      = pipelines.find((p: any) => p.id === pipelineId);
    const sensors = pl?.nodes
      .find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? [];
    const loggers = pl?.nodes
      .filter((n: any) => n.type === 'logger')
      .map((n: any) => n.tag ?? n.id) ?? [];

    // Cuántas dimensiones tiene filtered
    const nDims = data.find(r => r.filtered?.length)?.filtered?.length
               ?? data.find(r => r.raw?.length)?.raw?.length
               ?? 1;

    const datasets: any[] = [];

    // Una serie por dimensión (filtered si existe, fallback a raw)
    for (let i = 0; i < nDims; i++) {
      const label  = sensors[i] ?? `s${i + 1}`;
      const color  = COLORS[i % COLORS.length];
      const values = data.map(r => (r.filtered ?? r.raw)?.[i] ?? null);
      datasets.push({
        label,
        data:            values,
        borderColor:     color,
        backgroundColor: color + '18',
        borderWidth:     1.5,
        pointRadius:     data.length > 80 ? 0 : 2,
        pointHoverRadius: 4,
        fill:            i === 0,   // solo el primero con fill
        tension:         0.3,
        yAxisID:         'y',
      });
    }

    // Serie de raw (punteada, misma dimensión 0) si hay filtered
    if (data.some(r => r.filtered && r.raw)) {
      datasets.push({
        label:           `${sensors[0] ?? 's1'} (raw)`,
        data:            data.map(r => r.raw?.[0] ?? null),
        borderColor:     COLORS[0] + '55',
        backgroundColor: 'transparent',
        borderWidth:     1,
        borderDash:      [4, 3],
        pointRadius:     0,
        fill:            false,
        tension:         0.3,
        yAxisID:         'y',
      });
    }

    // Logger nodes (p_diag[0] como proxy si están configurados)
    if (loggers.length && data.some(r => r.p_diag?.length)) {
      datasets.push({
        label:           'P diag',
        data:            data.map(r => r.p_diag?.[0] ?? null),
        borderColor:     '#aaa',
        backgroundColor: 'transparent',
        borderWidth:     1,
        borderDash:      [2, 4],
        pointRadius:     0,
        fill:            false,
        tension:         0.3,
        yAxisID:         'y2',
      });
    }

    // Si ya existe el chart, actualizar datos
    if (charts[pipelineId]) {
      const c = charts[pipelineId];
      c.data.labels = labels;
      c.data.datasets = datasets;
      c.update('none');
      return;
    }

    const scales: any = {
      x: {
        ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 6, maxRotation: 0 },
        grid:  { color: grid }, border: { color: grid },
      },
      y: {
        ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 4 },
        grid:  { color: grid }, border: { color: grid },
      },
    };

    if (datasets.some(d => d.yAxisID === 'y2')) {
      scales.y2 = {
        position: 'right',
        ticks:    { color: '#aaa', font: { size: 8, family: "'DM Mono',monospace" }, maxTicksLimit: 3 },
        grid:     { drawOnChartArea: false },
      };
    }

    const newChart = new Chart(canvas, {
      type: 'line',
      data: { labels, datasets },
      options: {
        responsive:           true,
        maintainAspectRatio:  false,
        animation:            { duration: 200 },
        interaction:          { mode: 'index', intersect: false },
        plugins: {
          legend: {
            display:  datasets.length > 1,
            position: 'top',
            labels:   { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, boxWidth: 12, padding: 8 },
          },
          tooltip: {
            backgroundColor: cssVar('--chart-tooltip-bg'),
            titleColor:      cssVar('--chart-tooltip-title'),
            bodyColor:       cssVar('--chart-tooltip-body'),
            borderColor:     cssVar('--chart-tooltip-border'),
            borderWidth: 1, padding: 8,
            callbacks: {
              label: (ctx: any) => ` ${ctx.dataset.label}: ${ctx.parsed.y?.toFixed(4) ?? '—'}`,
            },
          },
        },
        scales,
      },
    });

    charts = { ...charts, [pipelineId]: newChart };

  }

  // Observar cambios de tema (dark/light) igual que SensorChart
  $effect(() => {
    const obs = new MutationObserver(() => {
      for (const id of Object.keys(charts)) {
        const c = charts[id];
        if (!c) continue;
        const tick = cssVar('--chart-tick');
        const grid = cssVar('--chart-grid');
        c.options.scales.x.ticks.color = tick;
        c.options.scales.x.grid.color  = grid;
        c.options.scales.y.ticks.color = tick;
        c.options.scales.y.grid.color  = grid;
        c.update('none');
      }
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => obs.disconnect();
  });

  // Acción Svelte: registra el canvas en el Map cuando se monta
  function mountCanvas(node: HTMLCanvasElement, pipelineId: string) {
    canvasMap.set(pipelineId, node);
    // Trigger render ahora que el canvas está disponible
    loadAndRender(pipelineId);
    return {
      destroy() { canvasMap.delete(pipelineId); }
    };
  }

  // mountCanvas (acción use:) se encarga de cargar cuando el canvas se monta

  onDestroy(() => {
    for (const c of Object.values(charts)) c?.destroy();
  });

  // ── Helpers de estado ──────────────────────────────────────────────────────
  function getActuatorState(pipelineId: string): string | null {
    const s = states[pipelineId];
    if (!s) return null;
    return Object.values(s.node_states ?? {})
      .find((n: any) => n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator')
      ?.data?.last_action ?? null;
  }

  function getFilteredValues(pipelineId: string): number[] | null {
    const s = states[pipelineId];
    if (!s) return null;
    const kalman = Object.values(s.node_states ?? {}).find((n: any) => n.node_type === 'kalman');
    if (kalman?.data?.x) return kalman.data.x;
    const filt = Object.values(s.node_states ?? {})
      .find((n: any) => ['moving_avg','ewma','lowpass'].includes(n.node_type));
    return filt?.data?.y ?? filt?.data?.x_hat ?? null;
  }

  function getLabels(pipelineId: string): string[] {
    const pl = pipelines.find((p: any) => p.id === pipelineId);
    return pl?.nodes.find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? [];
  }

  function getMahalanobis(pipelineId: string): number | null {
    const s = states[pipelineId];
    if (!s) return null;
    return Object.values(s.node_states ?? {})
      .find((n: any) => n.node_type === 'mahalanobis')
      ?.data?.last_d ?? null;
  }

  function isReady(pipelineId: string)   { return states[pipelineId]?.is_ready ?? false; }
  function isOverride(pipelineId: string){ return states[pipelineId]?.override_active ?? false; }

  const activeCount = $derived(pipelines.filter((p: any) => getActuatorState(p.id) === 'on').length);
  const readyCount  = $derived(pipelines.filter((p: any) => isReady(p.id)).length);

  // ── Self-test helpers ──────────────────────────────────────────────────────
  function statusIcon(s: string)  { return s === 'ok' ? '✓' : s === 'warn' ? '⚠' : s === 'error' ? '✗' : 'i'; }
  function overallColor(s: string){ return s === 'ok' ? '#3da85a' : s === 'warn' ? '#e8a838' : '#e05454'; }
</script>

<div class="ctrl">

  <!-- ── Barra global ───────────────────────────────────────────────────── -->
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
        <button class="action-btn test-btn" disabled={testLoading} onclick={runTest}>
          {testLoading ? '…' : '⬡ health'}
        </button>
        <button
          class="action-btn" class:running={status === 'running'}
          disabled={ctrlBusy || status === 'error'}
          onclick={toggleProcess}
        >
          {#if ctrlBusy}…
          {:else if status === 'running'}■ detener
          {:else}▶ iniciar{/if}
        </button>
      {:else if status !== 'running'}
        <span class="agent-offline">proceso detenido</span>
      {/if}
    </div>
  </div>

  <!-- ── Panel de self-test ─────────────────────────────────────────────── -->
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
        <div class="test-loading">verificando…</div>
      {:else if testError}
        <div class="test-error">{testError}</div>
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

  <!-- ── Sin pipelines ──────────────────────────────────────────────────── -->
  {#if pipelines.length === 0}
    <div class="empty">Sin pipelines configurados.</div>
  {:else}
    {#if Object.keys(states).length === 0 && status !== 'running'}
      <div class="offline-hint">agente detenido — usá ▶ iniciar para arrancar</div>
    {/if}

    <!-- ── Pipeline cards ─────────────────────────────────────────────── -->
    <div class="pipeline-list">
      {#each pipelines as pl (pl.id)}
        {@const actState   = getActuatorState(pl.id)}
        {@const vals       = getFilteredValues(pl.id)}
        {@const labels     = getLabels(pl.id)}
        {@const ready      = isReady(pl.id)}
        {@const mah        = getMahalanobis(pl.id)}
        {@const isExpanded = expanded === pl.id}
        {@const isOn       = actState === 'on'}

        <div class="pipeline-card" class:valve-on={isOn} class:expanded={isExpanded}>

          <!-- Header siempre visible -->
          <button class="card-header" onclick={() => toggleExpand(pl.id)}>
            <div class="ch-left">
              <span class="chevron" class:open={isExpanded}>▶</span>
              <span class="pl-label">{pl.label}</span>
              {#if !ready && status === 'running'}
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

          <!-- Detalle expandido -->
          {#if isExpanded}
            <div class="card-detail">

              <!-- Valores actuales -->
              {#if vals?.length}
                <div class="vals-grid">
                  {#each vals as v, i (i)}
                    <div class="val-item">
                      <span class="val-label" style="color:{COLORS[i % COLORS.length]}">{labels[i] ?? `s${i+1}`}</span>
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

              <!-- Chart al estilo SensorChart -->
              <div class="chart-wrap">
                {#if rdLoading[pl.id]}
                  <div class="chart-skeleton"></div>
                {:else if rdEmpty[pl.id]}
                  <div class="chart-empty">Sin lecturas en las últimas 2h</div>
                {:else}
                  <canvas use:mountCanvas={pl.id}></canvas>
                {/if}
              </div>

              <!-- Override -->
              {#if $canOperate}
                <div class="override-row">
                  <span class="ov-label">override</span>
                  <button class="ovbtn ovbtn--on"  disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'on')}>ON</button>
                  <button class="ovbtn ovbtn--off" disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'off')}>OFF</button>
                  <button class="ovbtn ovbtn--auto"disabled={ovBusy !== null} onclick={() => sendOverride(pl.id, 'clear')}>↺ auto</button>
                  {#if isOverride(pl.id)}<span class="ov-active">override activo</span>{/if}
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

  /* Global bar */
  .global-bar { display: flex; align-items: center; justify-content: space-between; padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; border: 0.5px solid var(--border-subtle); }
  .global-stats { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .gs-item  { display: flex; align-items: baseline; gap: 4px; }
  .gs-num   { font-size: calc(16px * var(--font-scale)); font-weight: 600; font-family: 'DM Mono', monospace; color: var(--text-primary); }
  .gs-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .gs-sep   { color: var(--border-default); }
  .global-right { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); flex-wrap: wrap; }
  .agent-offline { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; font-style: italic; }
  .ctrl-error    { font-size: calc(11px * var(--font-scale)); color: #e05454; font-family: 'DM Mono', monospace; }

  /* Status badge */
  .status-badge   { font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; padding: 2px 7px; border-radius: 10px; background: var(--bg-inset); color: var(--text-muted); }
  .status-running { background: #EAF3DE; color: #3B6D11; }
  .status-error   { background: #FCEBEB; color: #A32D2D; }

  /* Botones */
  .action-btn { padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border-radius: 6px; border: 0.5px solid var(--border-default); background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); transition: background .15s; }
  .action-btn:hover:not(:disabled) { background: var(--interactive-hover); }
  .action-btn:disabled { opacity: 0.4; cursor: default; }
  .action-btn.running  { color: #e05454; border-color: #e0545444; }
  .action-btn.running:hover:not(:disabled) { background: #FCEBEB; }
  .test-btn { color: var(--text-muted); }

  /* Self-test */
  .test-panel  { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; overflow: hidden; }
  .test-header { display: flex; align-items: center; gap: 10px; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .test-title  { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); flex: 1; }
  .test-overall{ font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 600; }
  .test-close  { margin-left: auto; background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 13px; padding: 2px 4px; border-radius: 4px; }
  .test-close:hover { background: var(--interactive-hover); }
  .test-loading{ padding: 14px; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-style: italic; font-family: 'DM Mono', monospace; }
  .test-error  { padding: 14px; font-size: calc(12px * var(--font-scale)); color: #e05454; font-family: 'DM Mono', monospace; }
  .test-checks { display: flex; flex-direction: column; }
  .check-row   { display: grid; grid-template-columns: 18px 1fr 2fr; gap: 8px; align-items: start; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .check-row:last-child { border-bottom: none; }
  .check-icon  { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 600; text-align: center; }
  .check-name  { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-primary); }
  .check-detail{ font-size: calc(11px * var(--font-scale)); color: var(--text-muted); word-break: break-all; }
  .check-ok    .check-icon { color: #3da85a; }
  .check-warn  .check-icon { color: #e8a838; }
  .check-error .check-icon { color: #e05454; }
  .check-warn  { background: #FEF3C710; }
  .check-error { background: #FCEBEB10; }

  /* Layout */
  .empty        { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 32px 0; text-align: center; }
  .offline-hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-style: italic; padding: calc(4px * var(--font-scale)) 0; }
  .pipeline-list{ display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }

  /* Pipeline card */
  .pipeline-card { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; overflow: hidden; transition: border-color .15s; }
  .pipeline-card.valve-on  { border-color: #3da85a55; }
  .pipeline-card.expanded  { border-color: var(--border-strong, var(--text-muted)); }

  .card-header { width: 100%; display: flex; align-items: center; justify-content: space-between; padding: calc(11px * var(--font-scale)) calc(14px * var(--font-scale)); background: none; border: none; cursor: pointer; text-align: left; }
  .card-header:hover { background: var(--interactive-hover); }
  .ch-left  { display: flex; align-items: center; gap: 8px; }
  .ch-right { display: flex; align-items: center; gap: 8px; }
  .chevron  { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); transition: transform .15s; display: inline-block; }
  .chevron.open { transform: rotate(90deg); }
  .pl-label { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .badge-warmup { font-size: calc(10px * var(--font-scale)); padding: 1px 7px; border-radius: 10px; background: var(--bg-inset); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .val-chip { font-size: calc(13px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; }
  .act-pill { font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 600; padding: 2px 8px; border-radius: 4px; background: var(--bg-inset); color: var(--text-muted); }
  .act-pill.on  { background: #EAF3DE; color: #3B6D11; }
  .act-pill.off { background: #FCEBEB; color: #A32D2D; }

  /* Card detail */
  .card-detail { padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  .vals-grid  { display: grid; grid-template-columns: repeat(auto-fill, minmax(90px, 1fr)); gap: calc(6px * var(--font-scale)); }
  .val-item   { display: flex; flex-direction: column; gap: 2px; }
  .val-label  { font-size: calc(10px * var(--font-scale)); font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .val-num    { font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; color: var(--text-primary); }

  /* Chart — igual a SensorChart .sc__body */
  .chart-wrap { height: calc(90px * var(--font-scale)); position: relative; }
  .chart-wrap canvas { width: 100% !important; height: 100% !important; }
  .chart-skeleton { width: 100%; height: 100%; border-radius: 3px; background: linear-gradient(90deg, var(--skeleton-from) 25%, var(--skeleton-to) 50%, var(--skeleton-from) 75%); background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  .chart-empty    { display: flex; align-items: center; justify-content: center; height: 100%; font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  /* Override */
  .override-row { display: flex; align-items: center; gap: 6px; padding-top: calc(6px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); flex-wrap: wrap; }
  .ov-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .ovbtn    { padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); }
  .ovbtn:disabled { opacity: 0.4; }
  .ovbtn--on:hover   { background: #EAF3DE; color: #3da85a; }
  .ovbtn--off:hover  { background: #FCEBEB; color: #e05454; }
  .ovbtn--auto:hover { background: var(--interactive-hover); }
  .ov-active { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 10px; background: #FEF3C7; color: #92400E; font-family: 'DM Mono', monospace; }
  .ov-error  { font-size: calc(11px * var(--font-scale)); color: #e05454; font-family: 'DM Mono', monospace; }
</style>