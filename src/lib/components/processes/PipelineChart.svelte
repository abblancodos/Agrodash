<!-- PipelineChart.svelte — Chart.js para process_readings, mismo estilo que SensorChart -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { processStore, type ProcessReading } from '$lib/stores/process';

  interface Props {
    processId:  string;
    pipelineId: string;
    labels:     string[];   // nombres de los sensores
  }

  let { processId, pipelineId, labels }: Props = $props();

  const COLORS = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  let canvas  = $state<HTMLCanvasElement | null>(null);
  let chart:  Chart | null = null;
  let loading = $state(true);
  let empty   = $state(false);
  let error   = $state('');

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }

  function formatLabel(iso: string): string {
    const d = new Date(iso.endsWith('Z') ? iso : iso + 'Z');
    return d.toLocaleTimeString('es-CR', { hour: '2-digit', minute: '2-digit', hour12: false });
  }

  function render(data: ProcessReading[]) {
    if (!canvas) return;
    const xlabels = data.map(r => formatLabel(r.ts));
    const tick    = cssVar('--chart-tick');
    const grid    = cssVar('--chart-grid');

    const nDims = data.find(r => r.filtered?.length)?.filtered?.length
               ?? data.find(r => r.raw?.length)?.raw?.length ?? 1;

    const datasets: any[] = [];
    for (let i = 0; i < nDims; i++) {
      const color = COLORS[i % COLORS.length];
      datasets.push({
        label:            labels[i] ?? `s${i+1}`,
        data:             data.map(r => (r.filtered ?? r.raw)?.[i] ?? null),
        borderColor:      color,
        backgroundColor:  color + '18',
        borderWidth:      1.5,
        pointRadius:      data.length > 80 ? 0 : 2,
        pointHoverRadius: 4,
        fill:             i === 0,
        tension:          0.3,
      });
    }

    // Raw punteado si hay filtered
    if (data.some(r => r.filtered && r.raw)) {
      datasets.push({
        label:           `${labels[0] ?? 's1'} raw`,
        data:            data.map(r => r.raw?.[0] ?? null),
        borderColor:     COLORS[0] + '55',
        backgroundColor: 'transparent',
        borderWidth:     1,
        borderDash:      [4, 3],
        pointRadius:     0,
        fill:            false,
        tension:         0.3,
      });
    }

    if (chart) {
      chart.data.labels   = xlabels;
      chart.data.datasets = datasets;
      chart.update('none');
      return;
    }

    chart = new Chart(canvas, {
      type: 'line',
      data: { labels: xlabels, datasets },
      options: {
        responsive:          true,
        maintainAspectRatio: false,
        animation:           { duration: 200 },
        interaction:         { mode: 'index', intersect: false },
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
            callbacks: { label: (ctx: any) => ` ${ctx.dataset.label}: ${ctx.parsed.y?.toFixed(4) ?? '—'}` },
          },
        },
        scales: {
          x: {
            ticks:  { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 6, maxRotation: 0 },
            grid:   { color: grid }, border: { color: grid },
          },
          y: {
            ticks:  { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 4 },
            grid:   { color: grid }, border: { color: grid },
          },
        },
      },
    });
  }

  async function load() {
    loading = true; empty = false; error = '';
    try {
      const data = await processStore.fetchReadings(processId, pipelineId, 2, 300);
      if (!data.length) { empty = true; return; }
      render(data);
    } catch (e: any) {
      error = e.message ?? 'Error';
    } finally {
      loading = false;
    }
  }

  // Observar cambios de tema
  let obs: MutationObserver | null = null;

  onMount(() => {
    load();
    obs = new MutationObserver(() => {
      if (!chart) return;
      const tick = cssVar('--chart-tick');
      const grid = cssVar('--chart-grid');
      chart.options.scales!.x!.ticks!.color = tick as any;
      (chart.options.scales!.x!.grid as any).color = grid;
      chart.options.scales!.y!.ticks!.color = tick as any;
      (chart.options.scales!.y!.grid as any).color = grid;
      chart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  });

  onDestroy(() => {
    chart?.destroy();
    obs?.disconnect();
  });
</script>

<div class="wrap">
  {#if loading && !chart}
    <div class="skeleton"></div>
  {:else if error}
    <div class="msg">{error}</div>
  {:else if empty}
    <div class="msg">Sin lecturas en las últimas 2h</div>
  {:else}
    <canvas bind:this={canvas}></canvas>
  {/if}
</div>

<style>
  .wrap         { height: calc(90px * var(--font-scale)); position: relative; }
  .wrap canvas  { width: 100% !important; height: 100% !important; }
  .skeleton     { width: 100%; height: 100%; border-radius: 3px;
                  background: linear-gradient(90deg, var(--skeleton-from) 25%, var(--skeleton-to) 50%, var(--skeleton-from) 75%);
                  background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }
  .msg { display:flex; align-items:center; justify-content:center; height:100%;
         font-size: calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
</style>