<script lang="ts">
  import { onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { fetchReadings, fetchLastReading, sensorColor, normaliseSensorLabel, type Reading } from '$lib/api';

  interface Props {
    sensorId: string; sensorType: string; label?: string;
    from: Date; to: Date; live?: boolean;
  }
  let { sensorId, sensorType, label, from, to, live = false }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let chart: any = null;
  let loading = $state(true);
  let error = $state('');
  let lastValue = $state<number | null>(null);
  let lastTimestamp = $state<string | null>(null);
  let empty = $state(false);

  const displayLabel = label ?? normaliseSensorLabel(sensorType);
  const color = sensorColor(sensorType);

  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function renderChart(readings: Reading[]) {
    if (!canvas) return;
    const labels = readings.map(r =>
      new Date(r.bucket + 'Z').toLocaleTimeString('es-CR', { hour: '2-digit', minute: '2-digit', hour12: false })
    );
    const data = readings.map(r => r.value);

    if (chart) {
      chart.data.labels = labels;
      chart.data.datasets[0].data = data;
      chart.update('none');
      return;
    }

    chart = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [{
          label: displayLabel, data,
          borderColor: color,
          backgroundColor: color + '18',
          borderWidth: 1.5,
          pointRadius: readings.length > 80 ? 0 : 2,
          pointHoverRadius: 4,
          fill: true, tension: 0.3,
        }],
      },
      options: {
        responsive: true, maintainAspectRatio: false,
        animation: { duration: 300 },
        interaction: { mode: 'index', intersect: false },
        plugins: {
          legend: { display: false },
          tooltip: {
            backgroundColor: cssVar('--chart-tooltip-bg'),
            titleColor: cssVar('--chart-tooltip-title'),
            bodyColor: cssVar('--chart-tooltip-body'),
            borderColor: cssVar('--chart-tooltip-border'),
            borderWidth: 1, padding: 8,
            callbacks: { label: (ctx: any) => ` ${ctx.parsed.y.toFixed(3)}` },
          },
        },
        scales: {
          x: {
            ticks: { color: cssVar('--chart-tick'), font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 6, maxRotation: 0 },
            grid: { color: cssVar('--chart-grid') }, border: { color: cssVar('--chart-grid') },
          },
          y: {
            ticks: { color: cssVar('--chart-tick'), font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 4 },
            grid: { color: cssVar('--chart-grid') }, border: { color: cssVar('--chart-grid') },
          },
        },
      },
    });
  }

  let liveInterval: ReturnType<typeof setInterval> | null = null;
  let rafId: number | null = null;

  async function loadData() {
    loading = true; error = ''; empty = false;
    try {
      const readings: Reading[] = await fetchReadings(sensorId, sensorType, from, to);
      if (readings.length === 0) {
        empty = true;
        if (chart) { chart.destroy(); chart = null; }
        const last = await fetchLastReading(sensorId);
        if (last) {
          lastValue = last.value;
          lastTimestamp = new Date(last.bucket + 'Z').toLocaleString('es-CR', {
            day: '2-digit', month: '2-digit', year: '2-digit',
            hour: '2-digit', minute: '2-digit',
          });
        }
        return;
      }
      lastValue = readings[readings.length - 1].value;
      lastTimestamp = null;
      empty = false;
      // Use rAF to ensure canvas is in DOM after loading=false triggers re-render
      if (rafId) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        rafId = requestAnimationFrame(() => {
          renderChart(readings);
        });
      });
    } catch (e: any) { error = e.message ?? 'Error'; }
    finally { loading = false; }
  }

  // Update chart colors when theme changes
  $effect(() => {
    const obs = new MutationObserver(() => {
      if (!chart) return;
      chart.options.scales.x.ticks.color = cssVar('--chart-tick');
      chart.options.scales.x.grid.color = cssVar('--chart-grid');
      chart.options.scales.x.border.color = cssVar('--chart-grid');
      chart.options.scales.y.ticks.color = cssVar('--chart-tick');
      chart.options.scales.y.grid.color = cssVar('--chart-grid');
      chart.options.scales.y.border.color = cssVar('--chart-grid');
      chart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => obs.disconnect();
  });

  $effect(() => {
    // Re-run when from/to/sensorId change
    void from; void to; void sensorId;
    loadData();
    if (live) liveInterval = setInterval(loadData, 15_000);
    return () => {
      if (liveInterval) clearInterval(liveInterval);
      if (rafId) cancelAnimationFrame(rafId);
    };
  });

  onDestroy(() => {
    chart?.destroy();
    if (liveInterval) clearInterval(liveInterval);
    if (rafId) cancelAnimationFrame(rafId);
  });
</script>

<div class="sc">
  <div class="sc__header">
    <span class="sc__dot" style="background:{color}"></span>
    <span class="sc__label">{displayLabel}</span>
    {#if lastValue !== null && !empty}<span class="sc__value">{lastValue.toFixed(2)}</span>{/if}
    {#if live}<span class="sc__live">LIVE</span>{/if}
  </div>
  <div class="sc__body">
    {#if loading && !chart}<div class="sc__skeleton"></div>
    {:else if error}<div class="sc__error">{error}</div>
    {:else if empty}
      <div class="sc__empty">
        <span class="sc__empty-main">Sin datos en el rango seleccionado</span>
        {#if lastTimestamp}
          <span class="sc__empty-last">Último dato: {lastTimestamp} · {lastValue?.toFixed(2)}</span>
        {/if}
      </div>
    {:else}
      <canvas bind:this={canvas}></canvas>
    {/if}
  </div>
</div>

<style>
  .sc { display:flex; flex-direction:column; gap:6px; }

  .sc__header { display:flex; align-items:center; gap:6px; }
  .sc__dot { width:7px; height:7px; border-radius:50%; flex-shrink:0; }
  .sc__label { font-size:10px; font-family:'DM Mono',monospace; letter-spacing:.06em; color:var(--text-muted); text-transform:uppercase; flex:1; }
  .sc__value { font-size:11px; font-family:'DM Mono',monospace; color:var(--text-secondary); font-variant-numeric:tabular-nums; }

  .sc__live { font-size:8px; font-family:'DM Mono',monospace; letter-spacing:.12em; color:var(--live-color); background:var(--live-bg); border:1px solid var(--live-border); padding:1px 5px; border-radius:2px; animation:pulse 2s infinite; }
  @keyframes pulse { 0%,100%{opacity:1}50%{opacity:.5} }

  .sc__body { height:90px; position:relative; }
  canvas { width:100% !important; height:100% !important; }

  .sc__skeleton { width:100%; height:100%; border-radius:3px; background:linear-gradient(90deg,var(--skeleton-from) 25%,var(--skeleton-to) 50%,var(--skeleton-from) 75%); background-size:200% 100%; animation:shimmer 1.4s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  .sc__error { display:flex; align-items:center; justify-content:center; height:100%; font-size:10px; font-family:'DM Mono',monospace; color:var(--error-color); }

  .sc__empty {
    display:flex; flex-direction:column; align-items:center; justify-content:center;
    height:100%; gap:4px;
  }
  .sc__empty-main {
    font-size:9.5px; font-family:'DM Mono',monospace; letter-spacing:.05em;
    color:var(--text-muted);
  }
  .sc__empty-last {
    font-size:8.5px; font-family:'DM Mono',monospace; letter-spacing:.04em;
    color:var(--text-muted); opacity:.6;
  }
</style>