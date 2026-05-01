<!-- src/lib/components/processes/AnalysisTab.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { processStore, type ProcessReading } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const proc      = $derived($processStore.process);
  const pipelines = $derived(proc?.config?.pipelines ?? []);
  const states    = $derived($processStore.pipelineStates);

  let selectedPipeline = $state('');
  let chartMode        = $state<'raw' | 'filtered' | 'both'>('both');
  let timePreset       = $state('6h');
  let readings         = $state<ProcessReading[]>([]);
  let loading          = $state(false);

  const PRESETS = [
    { label: '1h',  hours: 1   },
    { label: '6h',  hours: 6   },
    { label: '24h', hours: 24  },
    { label: '7d',  hours: 168 },
  ];

  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  $effect(() => {
    if (pipelines.length && !selectedPipeline) {
      selectedPipeline = pipelines[0].id;
    }
  });

  $effect(() => {
    if (selectedPipeline) {
      const preset = PRESETS.find(p => p.label === timePreset);
      if (preset) load(preset.hours);
    }
  });

  async function load(hours: number) {
    if (!selectedPipeline) return;
    loading = true;
    try {
      readings = await processStore.fetchReadings(processId, selectedPipeline, hours, 1000);
      renderCharts();
    } catch {}
    finally { loading = false; }
  }

  function refresh() {
    const preset = PRESETS.find(p => p.label === timePreset);
    if (preset) load(preset.hours);
  }

  // ── Labels del pipeline seleccionado ──────────────────────────────────────
  const sensorLabels = $derived(() => {
    const pl = pipelines.find((p: any) => p.id === selectedPipeline);
    return pl?.nodes.find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? [];
  });

  // ── Estadísticas del pipeline seleccionado ────────────────────────────────
  const pipelineStats = $derived(() => {
    const s = states[selectedPipeline];
    if (!s) return null;

    const nodeVals = Object.values(s.node_states ?? {});
    const kalman   = nodeVals.find((n: any) => n.node_type === 'kalman');
    const mah      = nodeVals.find((n: any) => n.node_type === 'mahalanobis');
    const actuator = nodeVals.find((n: any) =>
      n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator');

    // Estadísticas de readings
    const vwcVals = readings
      .map(r => (r.filtered ?? r.raw)?.[0])
      .filter((v): v is number => v != null);

    const mean = vwcVals.length
      ? vwcVals.reduce((a, b) => a + b, 0) / vwcVals.length
      : null;
    const std  = mean != null && vwcVals.length > 1
      ? Math.sqrt(vwcVals.reduce((a, v) => a + (v - mean) ** 2, 0) / vwcVals.length)
      : null;
    const minV = vwcVals.length ? Math.min(...vwcVals) : null;
    const maxV = vwcVals.length ? Math.max(...vwcVals) : null;

    // Tiempo ON
    const onReadings = readings.filter(r => r.actuator === 'on').length;
    const pctOn = readings.length ? (onReadings / readings.length * 100) : null;

    return {
      // Kalman
      x:       kalman?.data?.x?.[0] ?? null,
      p:       kalman?.data?.p?.[0] ?? null,
      n:       kalman?.data?.n ?? null,
      // Mahalanobis
      last_d:  mah?.data?.last_d ?? null,
      hyst:    mah?.data?.hyst ?? null,
      // Actuador
      act:     actuator?.data?.last_action ?? null,
      total_on: actuator?.data?.total_on ?? null,
      // Estadísticas de la serie
      mean, std, minV, maxV, pctOn,
      cycle:   s.cycle,
      ready:   s.is_ready,
    };
  });

  // ── Chart.js ──────────────────────────────────────────────────────────────
  let canvasSignal: HTMLCanvasElement | null = null;
  let canvasMah:    HTMLCanvasElement | null = null;
  let chartSignal:  Chart | null = null;
  let chartMah:     Chart | null = null;

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }

  function formatTs(iso: string): string {
    const d = new Date(iso.endsWith('Z') ? iso : iso + 'Z');
    return d.toLocaleTimeString('es-CR', {
      month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false,
    });
  }

  function renderCharts() {
    if (!readings.length) return;
    const labels = readings.map(r => formatTs(r.ts));
    const tick   = cssVar('--chart-tick');
    const grid   = cssVar('--chart-grid');

    const nDims = readings.find(r => r.filtered?.length)?.filtered?.length
               ?? readings.find(r => r.raw?.length)?.raw?.length ?? 1;

    // ── Chart 1: señal raw + filtered + bandas ON ──────────────────────────
    if (canvasSignal) {
      const datasets: any[] = [];

      for (let i = 0; i < nDims; i++) {
        const color = COLORS[i % COLORS.length];
        const label = sensorLabels()[i] ?? `s${i+1}`;

        if (chartMode !== 'filtered') {
          datasets.push({
            label: `${label} (crudo)`,
            data:  readings.map(r => r.raw?.[i] ?? null),
            borderColor: color + '66',
            backgroundColor: 'transparent',
            borderWidth: 1,
            borderDash: [4, 3],
            pointRadius: 0,
            fill: false,
            tension: 0.2,
          });
        }
        if (chartMode !== 'raw') {
          datasets.push({
            label: `${label} (filtrado)`,
            data:  readings.map(r => r.filtered?.[i] ?? null),
            borderColor: color,
            backgroundColor: color + '18',
            borderWidth: 1.8,
            pointRadius: 0,
            pointHoverRadius: 4,
            fill: i === 0,
            tension: 0.3,
          });
        }
      }

      // Banda de actuador como dataset de fondo
      datasets.push({
        label: 'actuador ON',
        data:  readings.map(r => r.actuator === 'on' ? Infinity : null),
        borderColor: 'transparent',
        backgroundColor: 'rgba(61,168,90,0.08)',
        fill: 'stack',
        pointRadius: 0,
        tension: 0,
        yAxisID: 'yAct',
      });

      if (chartSignal) {
        chartSignal.data.labels   = labels;
        chartSignal.data.datasets = datasets;
        chartSignal.update('none');
      } else if (canvasSignal) {
        chartSignal = new Chart(canvasSignal, {
          type: 'line',
          data: { labels, datasets },
          options: {
            responsive: true, maintainAspectRatio: false,
            animation: { duration: 200 },
            interaction: { mode: 'index', intersect: false },
            plugins: {
              legend: {
                display: true, position: 'top',
                labels: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, boxWidth: 12, padding: 8,
                  filter: (item: any) => item.text !== 'actuador ON' },
              },
              tooltip: {
                backgroundColor: cssVar('--chart-tooltip-bg'),
                titleColor: cssVar('--chart-tooltip-title'),
                bodyColor: cssVar('--chart-tooltip-body'),
                borderColor: cssVar('--chart-tooltip-border'),
                borderWidth: 1, padding: 8,
                callbacks: {
                  label: (ctx: any) => ctx.dataset.label === 'actuador ON' ? null
                    : ` ${ctx.dataset.label}: ${ctx.parsed.y?.toFixed(4) ?? '—'}`,
                  afterBody: (items: any[]) => {
                    const i = items[0]?.dataIndex;
                    if (i == null) return [];
                    const r = readings[i];
                    const lines = [];
                    if (r?.actuator) lines.push(`actuador: ${r.actuator}`);
                    if (r?.p_diag?.[0] != null) lines.push(`P: ${r.p_diag[0].toExponential(2)}`);
                    return lines;
                  },
                },
              },
            },
            scales: {
              x: {
                ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 8, maxRotation: 0 },
                grid: { color: grid }, border: { color: grid },
              },
              y: {
                ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 5 },
                grid: { color: grid }, border: { color: grid },
              },
              yAct: { display: false, min: 0, max: 1 },
            },
          },
        });
      }
    }

    // ── Chart 2: distancia Mahalanobis en el tiempo ────────────────────────
    // Los p_diag están en process_readings — usamos p_diag[0] como proxy de incertidumbre
    // y necesitamos leer last_d del pipeline_states histórico
    // Por ahora graficamos p_diag[0] (incertidumbre Kalman) que sí tenemos
    if (canvasMah) {
      const pData = readings.map(r => r.p_diag?.[0] ?? null);
      const hasPData = pData.some(v => v != null);

      if (!hasPData) {
        if (chartMah) { chartMah.destroy(); chartMah = null; }
        return;
      }

      const mahDatasets = [
        {
          label: 'incertidumbre P (Kalman)',
          data:  pData,
          borderColor: '#7c6fcd',
          backgroundColor: '#7c6fcd18',
          borderWidth: 1.5,
          pointRadius: 0,
          fill: true,
          tension: 0.3,
        },
      ];

      if (chartMah) {
        chartMah.data.labels   = labels;
        chartMah.data.datasets = mahDatasets;
        chartMah.update('none');
      } else if (canvasMah) {
        chartMah = new Chart(canvasMah, {
          type: 'line',
          data: { labels, datasets: mahDatasets },
          options: {
            responsive: true, maintainAspectRatio: false,
            animation: { duration: 200 },
            interaction: { mode: 'index', intersect: false },
            plugins: {
              legend: {
                display: true, position: 'top',
                labels: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, boxWidth: 12, padding: 8 },
              },
              tooltip: {
                backgroundColor: cssVar('--chart-tooltip-bg'),
                titleColor: cssVar('--chart-tooltip-title'),
                bodyColor: cssVar('--chart-tooltip-body'),
                borderColor: cssVar('--chart-tooltip-border'),
                borderWidth: 1, padding: 8,
                callbacks: {
                  label: (ctx: any) => ` ${ctx.dataset.label}: ${ctx.parsed.y?.toExponential(3) ?? '—'}`,
                },
              },
            },
            scales: {
              x: {
                ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 8, maxRotation: 0 },
                grid: { color: grid }, border: { color: grid },
              },
              y: {
                ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 4 },
                grid: { color: grid }, border: { color: grid },
              },
            },
          },
        });
      }
    }
  }

  // Observar tema
  let obs: MutationObserver | null = null;
  onMount(() => {
    obs = new MutationObserver(() => {
      for (const c of [chartSignal, chartMah]) {
        if (!c) continue;
        const tick = cssVar('--chart-tick');
        const grid = cssVar('--chart-grid');
        (c.options.scales as any).x.ticks.color = tick;
        (c.options.scales as any).x.grid.color  = grid;
        (c.options.scales as any).y.ticks.color = tick;
        (c.options.scales as any).y.grid.color  = grid;
        c.update('none');
      }
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  });

  onDestroy(() => {
    chartSignal?.destroy();
    chartMah?.destroy();
    obs?.disconnect();
  });

  function fmtSeconds(s: number): string {
    if (s < 60)   return `${s.toFixed(0)}s`;
    if (s < 3600) return `${(s/60).toFixed(1)}min`;
    return `${(s/3600).toFixed(2)}h`;
  }

  function mahColor(d: number | null): string {
    if (d == null) return 'var(--text-muted)';
    if (d > 2.0) return '#e07b54';
    if (d > 0.8) return '#e8a838';
    return '#3da85a';
  }
</script>

<div class="analysis">

  <!-- Toolbar -->
  <div class="toolbar">
    <div class="pl-selector">
      {#each pipelines as pl (pl.id)}
        <button class="pl-btn" class:active={selectedPipeline === pl.id}
          onclick={() => { selectedPipeline = pl.id; }}>
          {pl.label}
        </button>
      {/each}
    </div>
    <div class="toolbar-right">
      <div class="btn-group">
        <button class="cmbtn" class:active={chartMode==='raw'}      onclick={() => { chartMode='raw';      refresh(); }}>crudo</button>
        <button class="cmbtn" class:active={chartMode==='filtered'} onclick={() => { chartMode='filtered'; refresh(); }}>filtrado</button>
        <button class="cmbtn" class:active={chartMode==='both'}     onclick={() => { chartMode='both';     refresh(); }}>ambos</button>
      </div>
      <div class="btn-group">
        {#each PRESETS as p (p.label)}
          <button class="cmbtn" class:active={timePreset===p.label}
            onclick={() => timePreset=p.label}>{p.label}</button>
        {/each}
      </div>
      <button class="cmbtn" onclick={refresh} disabled={loading}>↺</button>
    </div>
  </div>

  <!-- Stats panel -->
  {@const st = pipelineStats()}
  {#if st}
    <div class="stats-panel">

      <!-- Kalman -->
      <div class="stats-group">
        <span class="stats-group-title">Kalman</span>
        <div class="stats-row">
          <div class="stat">
            <span class="stat-l">x̂ estimado</span>
            <span class="stat-v">{st.x?.toFixed(4) ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">incertidumbre P</span>
            <span class="stat-v">{st.p != null ? st.p.toExponential(3) : '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">muestras</span>
            <span class="stat-v">{st.n ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">estado</span>
            <span class="stat-v" style="color:{st.ready ? '#3da85a' : '#e8a838'}">
              {st.ready ? 'convergido' : 'warmup'}
            </span>
          </div>
        </div>
      </div>

      <!-- Mahalanobis -->
      <div class="stats-group">
        <span class="stats-group-title">Mahalanobis</span>
        <div class="stats-row">
          <div class="stat">
            <span class="stat-l">distancia d</span>
            <span class="stat-v" style="color:{mahColor(st.last_d)}">
              {st.last_d?.toFixed(4) ?? '—'}
            </span>
          </div>
          <div class="stat">
            <span class="stat-l">decisión</span>
            <span class="stat-v">{st.hyst ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">actuador</span>
            <span class="stat-v" style="color:{st.act === 'on' ? '#3da85a' : st.act === 'off' ? '#e05454' : 'var(--text-muted)'}">
              {st.act?.toUpperCase() ?? '—'}
            </span>
          </div>
          <div class="stat">
            <span class="stat-l">total ON</span>
            <span class="stat-v">{st.total_on != null ? fmtSeconds(st.total_on) : '—'}</span>
          </div>
        </div>
      </div>

      <!-- Serie -->
      <div class="stats-group">
        <span class="stats-group-title">Serie ({timePreset})</span>
        <div class="stats-row">
          <div class="stat">
            <span class="stat-l">media</span>
            <span class="stat-v">{st.mean?.toFixed(4) ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">desv. estándar</span>
            <span class="stat-v">{st.std?.toFixed(4) ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">mín</span>
            <span class="stat-v">{st.minV?.toFixed(4) ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">máx</span>
            <span class="stat-v">{st.maxV?.toFixed(4) ?? '—'}</span>
          </div>
          <div class="stat">
            <span class="stat-l">% tiempo ON</span>
            <span class="stat-v">{st.pctOn != null ? st.pctOn.toFixed(1) + '%' : '—'}</span>
          </div>
        </div>
      </div>

    </div>
  {/if}

  <!-- Chart 1: señal -->
  {#if loading}
    <div class="chart-msg">cargando...</div>
  {:else if readings.length < 2}
    <div class="chart-msg">sin lecturas en este rango</div>
  {:else}
    <div class="chart-wrap">
      <div class="chart-label">señal · raw vs filtrado · bandas = actuador ON</div>
      <div class="chart-body">
        <canvas bind:this={canvasSignal}></canvas>
      </div>
    </div>

    <!-- Chart 2: incertidumbre Kalman -->
    {#if readings.some(r => r.p_diag?.length)}
      <div class="chart-wrap">
        <div class="chart-label">incertidumbre Kalman (P) — converge hacia 0</div>
        <div class="chart-body chart-body--sm">
          <canvas bind:this={canvasMah}></canvas>
        </div>
      </div>
    {/if}
  {/if}

</div>

<style>
  .analysis { display:flex; flex-direction:column; gap:calc(12px * var(--font-scale)); }

  /* Toolbar */
  .toolbar       { display:flex; align-items:center; justify-content:space-between; gap:8px; flex-wrap:wrap; }
  .toolbar-right { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
  .pl-selector   { display:flex; gap:4px; flex-wrap:wrap; }
  .pl-btn { padding:calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:6px; background:none; cursor:pointer; font-size:calc(12px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .pl-btn:hover { background:var(--interactive-hover); }
  .pl-btn.active { background:var(--bg-elevated); color:var(--text-primary); border-color:var(--text-primary); }
  .btn-group { display:flex; gap:3px; }
  .cmbtn { padding:calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:6px; background:none; cursor:pointer; font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .cmbtn:hover { background:var(--interactive-hover); }
  .cmbtn.active { background:var(--bg-elevated); color:var(--text-primary); }
  .cmbtn:disabled { opacity:0.5; }

  /* Stats panel */
  .stats-panel { display:flex; flex-wrap:wrap; gap:calc(10px * var(--font-scale)); }
  .stats-group { background:var(--bg-elevated); border:0.5px solid var(--border-subtle); border-radius:8px; padding:calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); flex:1; min-width:200px; }
  .stats-group-title { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); letter-spacing:.08em; display:block; margin-bottom:calc(8px * var(--font-scale)); }
  .stats-row { display:flex; flex-wrap:wrap; gap:calc(12px * var(--font-scale)); }
  .stat { display:flex; flex-direction:column; gap:2px; }
  .stat-l { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; white-space:nowrap; }
  .stat-v { font-size:calc(15px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; color:var(--text-primary); }

  /* Charts */
  .chart-msg  { font-size:calc(12px * var(--font-scale)); color:var(--text-muted); text-align:center; padding:60px 0; }
  .chart-wrap { background:var(--bg-elevated); border:0.5px solid var(--border-subtle); border-radius:8px; padding:calc(10px * var(--font-scale)); display:flex; flex-direction:column; gap:6px; }
  .chart-label { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .chart-body { height:calc(180px * var(--font-scale)); position:relative; }
  .chart-body canvas { width:100% !important; height:100% !important; }
  .chart-body--sm { height:calc(100px * var(--font-scale)); }
</style>