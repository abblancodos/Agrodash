<!-- PipelineChart.svelte — chart expandido con fetch propio, controles de eje -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { processStore, type ProcessReading } from '$lib/stores/process.svelte';

  interface Props {
    processId:    string;
    pipelineId:   string;
    labels:       string[];       // sensor labels
    hours:        number;         // ventana de tiempo
    loggerTag?:   string;         // tag del Logger para filtrar scope_values
    upstreamType?: string;        // tipo de nodo upstream del Logger
  }

  let { processId, pipelineId, labels, hours, loggerTag, upstreamType }: Props = $props();

  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  let canvas   = $state<HTMLCanvasElement | null>(null);
  let chart:   Chart | null = null;
  let readings = $state<ProcessReading[]>([]);
  let loading  = $state(true);
  let empty    = $state(false);

  // Controles de eje
  let lockY  = $state(false);
  let lockX  = $state(false);
  let yMin   = $state('');
  let yMax   = $state('');
  let xFrom  = $state('');
  let xTo    = $state('');
  let showControls = $state(false);

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }
  function fmtTs(iso: string): string {
    const d = new Date(iso.endsWith('Z') ? iso : iso + 'Z');
    return d.toLocaleTimeString('es-CR', { hour:'2-digit', minute:'2-digit', hour12: false });
  }

  async function load() {
    loading = true; empty = false;
    try {
      const data = await processStore.fetchReadings(processId, pipelineId, hours, 1000);
      if (!data.length) { empty = true; return; }
      readings = data;
      render();
    } catch { empty = true; }
    finally { loading = false; }
  }

  function buildDatasets(): any[] {
    const tag   = loggerTag;
    const ntype = upstreamType ?? '';
    const color = COLORS[0];
    const datasets: any[] = [];

    // Señal principal según el tipo de nodo upstream
    if (!tag || ntype === 'kalman' || ntype === 'ewma' || ntype === 'lowpass' || ntype === 'moving_avg') {
      const nDims = readings.find(r => r.filtered?.length)?.filtered?.length
                 ?? readings.find(r => r.raw?.length)?.raw?.length ?? 1;
      for (let i = 0; i < nDims; i++) {
        const c = COLORS[i % COLORS.length];
        const lbl = labels[i] ?? `s${i+1}`;
        datasets.push({
          label: `${lbl} (crudo)`,
          data:  readings.map(r => r.raw?.[i] ?? null),
          borderColor: c + '55', backgroundColor: 'transparent',
          borderWidth: 1, borderDash: [4,3], pointRadius: 0, fill: false, tension: 0.2,
        });
        datasets.push({
          label: `${lbl} (filtrado)`,
          data:  readings.map(r => r.filtered?.[i] ?? null),
          borderColor: c, backgroundColor: c + '18',
          borderWidth: 1.8, pointRadius: 0, fill: i === 0, tension: 0.3,
        });
      }
    } else if (ntype === 'mahalanobis') {
      datasets.push({
        label: tag ?? 'mahalanobis',
        data:  readings.map(r => {
          const v = r.scope_values?.[tag ?? ''];
          return Array.isArray(v) ? v[0] : (typeof v === 'number' ? v : null);
        }),
        borderColor: '#e07b54', backgroundColor: '#e07b5418',
        borderWidth: 1.8, pointRadius: 0, fill: true, tension: 0.3,
      });
    } else if (ntype === 'mqtt_actuator' || ntype === 'http_actuator') {
      datasets.push({
        label: tag ?? 'actuador',
        data:  readings.map(r => {
          const v = r.scope_values?.[tag ?? ''];
          if (v === 'on')  return 1;
          if (v === 'off') return 0;
          return r.actuator === 'on' ? 1 : 0;
        }),
        borderColor: '#3da85a', backgroundColor: '#3da85a18',
        borderWidth: 1.5, pointRadius: 0, fill: true, tension: 0,
      });
    }

    // Banda actuador ON
    datasets.push({
      label: '_act',
      data:  readings.map(r => r.actuator === 'on' ? Infinity : null),
      borderColor: 'transparent', backgroundColor: 'rgba(61,168,90,0.07)',
      fill: 'stack', pointRadius: 0, tension: 0, yAxisID: 'yAct',
    });

    return datasets;
  }

  function applyAxisLimits(axis: any, isY: boolean) {
    if (isY && lockY && yMin !== '' && yMax !== '') {
      axis.min = parseFloat(yMin);
      axis.max = parseFloat(yMax);
    } else if (isY) {
      const span = (axis.max - axis.min) || 0.01;
      axis.min -= span * 0.12; axis.max += span * 0.12;
    }
  }

  function getXRange(): { min?: number; max?: number } {
    if (!lockX || (!xFrom && !xTo)) return {};
    const now   = Date.now();
    const fromH = xFrom !== '' ? parseFloat(xFrom) : 9999;
    const toH   = xTo   !== '' ? parseFloat(xTo)   : 0;
    const fromMs = now - fromH * 3_600_000;
    const toMs   = now - toH   * 3_600_000;
    const indices = readings
      .map((r, i) => ({ i, t: new Date(r.ts.endsWith('Z') ? r.ts : r.ts + 'Z').getTime() }))
      .filter(({ t }) => t >= fromMs && t <= toMs);
    if (!indices.length) return {};
    return { min: indices[0].i, max: indices[indices.length-1].i };
  }

  function render() {
    if (!canvas || !readings.length) return;
    const datasets = buildDatasets();
    const xlabels  = readings.map(r => fmtTs(r.ts));
    const tick     = cssVar('--chart-tick');
    const grid     = cssVar('--chart-grid');
    const xRange   = getXRange();

    if (chart) {
      chart.data.labels   = xlabels;
      chart.data.datasets = datasets;
      const sc = chart.options.scales as any;
      sc.x.min = xRange.min; sc.x.max = xRange.max;
      chart.update('none');
      return;
    }

    chart = new Chart(canvas, {
      type: 'line',
      data: { labels: xlabels, datasets },
      options: {
        responsive: true, maintainAspectRatio: false,
        animation: { duration: 150 },
        interaction: { mode: 'index', intersect: false },
        plugins: {
          legend: {
            display: datasets.filter(d => d.label !== '_act').length > 1,
            position: 'top',
            labels: { color: tick, font: { size: 9, family: "'DM Mono',monospace" },
              boxWidth: 10, padding: 8, filter: (i: any) => i.text !== '_act' },
          },
          tooltip: {
            backgroundColor: cssVar('--chart-tooltip-bg'),
            titleColor: cssVar('--chart-tooltip-title'),
            bodyColor: cssVar('--chart-tooltip-body'),
            borderColor: cssVar('--chart-tooltip-border'),
            borderWidth: 1, padding: 8,
            callbacks: {
              label: (ctx: any) => ctx.dataset.label === '_act' ? null
                : ` ${ctx.dataset.label}: ${ctx.parsed.y?.toFixed(4) ?? '—'}`,
            },
          },
        },
        scales: {
          x: {
            offset: true,
            min: xRange.min, max: xRange.max,
            ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 8, maxRotation: 0 },
            grid: { color: grid }, border: { color: grid },
          },
          y: {
            ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 5 },
            grid: { color: grid }, border: { color: grid },
            afterDataLimits: (axis: any) => applyAxisLimits(axis, true),
          },
          yAct: { display: false, min: 0, max: 1 },
        },
      },
    });
  }

  function applyControls() {
    if (!chart) return;
    const sc = chart.options.scales as any;
    const xRange = getXRange();
    sc.x.min = xRange.min; sc.x.max = xRange.max;
    if (lockY && yMin !== '' && yMax !== '') {
      sc.y.min = parseFloat(yMin); sc.y.max = parseFloat(yMax);
      sc.y.afterDataLimits = undefined;
    } else {
      sc.y.min = undefined; sc.y.max = undefined;
      sc.y.afterDataLimits = (axis: any) => applyAxisLimits(axis, true);
    }
    chart.update('none');
  }

  function resetAll() {
    lockY = false; lockX = false;
    yMin = ''; yMax = ''; xFrom = ''; xTo = '';
    applyControls();
  }

  // Recargar cuando cambia hours desde el padre
  $effect(() => { void hours; load(); });

  let obs: MutationObserver | null = null;
  onMount(() => {
    obs = new MutationObserver(() => {
      if (!chart) return;
      const tick = cssVar('--chart-tick');
      const grid = cssVar('--chart-grid');
      (chart.options.scales as any).x.ticks.color = tick;
      (chart.options.scales as any).x.grid.color  = grid;
      (chart.options.scales as any).y.ticks.color = tick;
      (chart.options.scales as any).y.grid.color  = grid;
      chart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  });
  onDestroy(() => { chart?.destroy(); obs?.disconnect(); });
</script>

<!-- Controles de eje -->
<div class="chart-controls">
  <button class="ctrl-btn" class:active={showControls}
    onclick={() => showControls = !showControls}>⊞ ejes</button>
  {#if showControls}
    <div class="axis-group">
      <label class="axis-lbl">
        <input type="checkbox" bind:checked={lockY} onchange={applyControls}/> Y
      </label>
      <input class="axis-in" type="number" step="0.001" placeholder="mín"
        bind:value={yMin} oninput={applyControls} disabled={!lockY}/>
      <span class="axis-sep">—</span>
      <input class="axis-in" type="number" step="0.001" placeholder="máx"
        bind:value={yMax} oninput={applyControls} disabled={!lockY}/>
    </div>
    <div class="axis-group">
      <label class="axis-lbl">
        <input type="checkbox" bind:checked={lockX} onchange={applyControls}/> X (h)
      </label>
      <input class="axis-in" type="number" step="0.5" placeholder="desde"
        bind:value={xFrom} oninput={applyControls} disabled={!lockX}/>
      <span class="axis-sep">→</span>
      <input class="axis-in" type="number" step="0.5" placeholder="hasta"
        bind:value={xTo} oninput={applyControls} disabled={!lockX}/>
    </div>
    <button class="ctrl-btn" onclick={resetAll}>⌖</button>
  {/if}
</div>

<div class="chart-wrap">
  {#if loading}
    <div class="sk"></div>
  {:else if empty}
    <div class="msg">sin lecturas</div>
  {:else}
    <canvas bind:this={canvas}></canvas>
  {/if}
</div>

<style>
  .chart-controls { display:flex; align-items:center; gap:6px; flex-wrap:wrap; margin-bottom:6px; }
  .ctrl-btn  { padding:calc(3px * var(--font-scale)) calc(8px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:5px; background:none; cursor:pointer; font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .ctrl-btn.active { background:var(--bg-elevated); color:var(--text-primary); }
  .ctrl-btn:hover  { background:var(--interactive-hover); }
  .axis-group { display:flex; align-items:center; gap:4px; }
  .axis-lbl   { display:flex; align-items:center; gap:4px; font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; cursor:pointer; white-space:nowrap; }
  .axis-in    { width:60px; padding:2px 5px; border:0.5px solid var(--border-default); border-radius:4px; background:var(--bg-surface); color:var(--text-primary); font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; }
  .axis-in:disabled { opacity:.4; }
  .axis-sep   { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); }

  .chart-wrap { height:calc(160px * var(--font-scale)); position:relative; }
  .chart-wrap canvas { width:100% !important; height:100% !important; }
  .sk  { width:100%; height:100%; border-radius:4px;
         background:linear-gradient(90deg,var(--bg-inset) 25%,var(--bg-elevated) 50%,var(--bg-inset) 75%);
         background-size:200% 100%; animation:shimmer 1.4s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }
  .msg { display:flex; align-items:center; justify-content:center; height:100%;
         font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
</style>