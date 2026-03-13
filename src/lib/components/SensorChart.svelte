<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchReadings, sensorColor, normaliseSensorLabel, type Reading } from '$lib/api';

  interface Props {
    sensorId: string; sensorType: string; label?: string;
    from: Date; to: Date; live?: boolean;
  }
  let { sensorId, sensorType, label, from, to, live = false }: Props = $props();

  let canvas: HTMLCanvasElement;
  let chart: any = null;
  let loading = $state(true);
  let error = $state('');
  let lastValue = $state<number | null>(null);

  const displayLabel = label ?? normaliseSensorLabel(sensorType);
  const color = sensorColor(sensorType);

  // Read CSS variable values at runtime so chart respects current theme
  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  async function loadData() {
    loading = true; error = '';
    try {
      const readings: Reading[] = await fetchReadings(sensorId, sensorType, from, to);
      lastValue = readings.length ? readings[readings.length - 1].value : null;
      renderChart(readings);
    } catch (e: any) { error = e.message ?? 'Error'; }
    finally { loading = false; }
  }

  function renderChart(readings: Reading[]) {
    if (!canvas) return;
    const labels = readings.map(r =>
      new Date(r.bucket).toLocaleTimeString('es-CR', { hour: '2-digit', minute: '2-digit' })
    );
    const data = readings.map(r => r.value);

    if (chart) {
      chart.data.labels = labels;
      chart.data.datasets[0].data = data;
      chart.update('none');
      return;
    }

    import('chart.js/auto').then(({ default: Chart }) => {
      if (chart) chart.destroy();
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
    });
  }

  let liveInterval: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    loadData();
    if (live) liveInterval = setInterval(loadData, 15_000);
    return () => { if (liveInterval) clearInterval(liveInterval); };
  });

  onDestroy(() => { chart?.destroy(); if (liveInterval) clearInterval(liveInterval); });
</script>

<div class="sc">
  <div class="sc__header">
    <span class="sc__dot" style="background:{color}"></span>
    <span class="sc__label">{displayLabel}</span>
    {#if lastValue !== null}<span class="sc__value">{lastValue.toFixed(2)}</span>{/if}
    {#if live}<span class="sc__live">LIVE</span>{/if}
  </div>
  <div class="sc__body">
    {#if loading && !chart}<div class="sc__skeleton"></div>
    {:else if error}<div class="sc__error">{error}</div>
    {:else}<canvas bind:this={canvas}></canvas>{/if}
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
</style>