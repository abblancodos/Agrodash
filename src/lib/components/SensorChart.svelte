<script lang="ts">
  import { onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { fetchReadings, fetchLastReading, sensorColor, normaliseSensorLabel, type Reading } from '$lib/api';
  import { downloading } from '$lib/stores/downloading';

  interface Props {
    sensorId:   string;
    sensorType: string;
    label?:     string;
    from:       Date;
    to:         Date;
    live?:      boolean;
    /** Modo sparkline: sin ejes, sin labels, sin tooltip, altura fija 28px */
    spark?:     boolean;
    /** Color override (usado por BoxCard para consistencia) */
    color?:     string;
    points?:    number;
  }

  let {
    sensorId, sensorType, label, from, to,
    live = false, spark = false, color: colorProp, points = 300,
  }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let chart: any = null;
  let loading = $state(true);
  let error = $state('');
  let lastValue = $state<number | null>(null);
  let lastTimestamp = $state<string | null>(null);
  let empty = $state(false);

  const displayLabel = label ?? normaliseSensorLabel(sensorType);
  const color = colorProp ?? sensorColor(sensorType);

  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function renderChart(readings: Reading[]) {
    if (!canvas) return;
    const data = readings.map(r => r.value);

    if (spark) {
      // ── Modo sparkline — solo la línea, sin nada más ──────────────────────
      if (chart) {
        chart.data.datasets[0].data = data;
        chart.update('none');
        return;
      }
      chart = new Chart(canvas, {
        type: 'line',
        data: {
          labels: data.map((_, i) => i),
          datasets: [{
            data,
            borderColor: color,
            backgroundColor: 'transparent',
            borderWidth: 1.5,
            pointRadius: 0,
            pointHoverRadius: 0,
            fill: false,
            tension: 0.3,
          }],
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          animation: false,
          interaction: { mode: 'nearest', intersect: false },
          plugins: { legend: { display: false }, tooltip: { enabled: false } },
          scales: {
            x: { display: false },
            y: { display: false },
          },
        },
      });
      return;
    }

    // ── Modo normal — gráfica completa ────────────────────────────────────
    const labels = readings.map(r =>
      new Date(r.bucket + 'Z').toLocaleTimeString('es-CR', {
        hour: '2-digit', minute: '2-digit', hour12: false,
      })
    );

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
            titleColor:       cssVar('--chart-tooltip-title'),
            bodyColor:        cssVar('--chart-tooltip-body'),
            borderColor:      cssVar('--chart-tooltip-border'),
            borderWidth: 1, padding: 8,
            callbacks: { label: (ctx: any) => ` ${ctx.parsed.y.toFixed(3)}` },
          },
        },
        scales: {
          x: {
            ticks: { color: cssVar('--chart-tick'), font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 6, maxRotation: 0 },
            grid:  { color: cssVar('--chart-grid') },
            border:{ color: cssVar('--chart-grid') },
          },
          y: {
            ticks: { color: cssVar('--chart-tick'), font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 4 },
            grid:  { color: cssVar('--chart-grid') },
            border:{ color: cssVar('--chart-grid') },
          },
        },
      },
    });
  }

  let liveInterval: ReturnType<typeof setInterval> | null = null;
  let rafId: number | null = null;
  let isDownloading = false;

  // Suscripción al store — pausa el live polling durante descarga
  const unsubDownloading = downloading.subscribe(state => {
    isDownloading = state.active;
    if (state.active && liveInterval) {
      clearInterval(liveInterval);
      liveInterval = null;
    }
  });

  async function loadData() {
    // No lanzar fetch si hay una descarga CSV en curso
    if (isDownloading) return;

    loading = true; error = ''; empty = false;
    try {
      const readings: Reading[] = await fetchReadings(sensorId, sensorType, from, to, points);
      if (readings.length === 0) {
        empty = true;
        if (chart) { chart.destroy(); chart = null; }
        if (!spark) {
          const last = await fetchLastReading(sensorId);
          if (last) {
            lastValue = last.value;
            lastTimestamp = new Date(last.bucket + 'Z').toLocaleString('es-CR', {
              day: '2-digit', month: '2-digit', year: '2-digit',
              hour: '2-digit', minute: '2-digit',
            });
          }
        }
        return;
      }
      lastValue = readings[readings.length - 1].value;
      lastTimestamp = null;
      empty = false;
      if (rafId) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        rafId = requestAnimationFrame(() => { renderChart(readings); });
      });
    } catch (e: any) {
      if (!isDownloading) error = e.message ?? 'Error';
    } finally {
      loading = false;
    }
  }

  // Actualiza colores del chart cuando cambia el tema
  $effect(() => {
    if (spark) return;
    const obs = new MutationObserver(() => {
      if (!chart) return;
      chart.options.scales.x.ticks.color = cssVar('--chart-tick');
      chart.options.scales.x.grid.color  = cssVar('--chart-grid');
      chart.options.scales.y.ticks.color = cssVar('--chart-tick');
      chart.options.scales.y.grid.color  = cssVar('--chart-grid');
      chart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => obs.disconnect();
  });

  $effect(() => {
    void from; void to; void sensorId;
    if (isDownloading) return;
    loadData();
    if (live && !liveInterval) {
      liveInterval = setInterval(() => {
        if (!isDownloading) loadData();
      }, 15_000);
    }
    return () => {
      if (liveInterval) clearInterval(liveInterval);
      if (rafId) cancelAnimationFrame(rafId);
    };
  });

  onDestroy(() => {
    chart?.destroy();
    if (liveInterval) clearInterval(liveInterval);
    if (rafId) cancelAnimationFrame(rafId);
    unsubDownloading();
  });
</script>

{#if spark}
  <!-- Modo sparkline: solo el canvas, sin wrapper con header -->
  <div class="spark-wrap">
    {#if loading && !chart}
      <div class="spark-skeleton"></div>
    {:else}
      <canvas bind:this={canvas}></canvas>
    {/if}
  </div>
{:else}
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
{/if}

<style>
  /* Spark mode */
  .spark-wrap { width: 100%; height: 28px; position: relative; }
  .spark-wrap canvas { width: 100% !important; height: 100% !important; display: block; }
  .spark-skeleton {
    width: 100%; height: 100%; border-radius: 2px;
    background: linear-gradient(90deg, var(--skeleton-from, #eee) 25%, var(--skeleton-to, #f8f8f8) 50%, var(--skeleton-from, #eee) 75%);
    background-size: 200% 100%;
    animation: shimmer 1.4s infinite;
  }

  /* Normal mode */
  .sc { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .sc__header { display: flex; align-items: center; gap: calc(6px * var(--font-scale)); }
  .sc__dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .sc__label { font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; letter-spacing: .06em; color: var(--text-muted); text-transform: uppercase; flex: 1; }
  .sc__value { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); font-variant-numeric: tabular-nums; }
  .sc__live { font-size: calc(8px * var(--font-scale)); font-family: 'DM Mono', monospace; letter-spacing: .12em; color: var(--live-color); background: var(--live-bg); border: 1px solid var(--live-border); padding: calc(1px * var(--font-scale)) calc(5px * var(--font-scale)); border-radius: 2px; animation: pulse 2s infinite; }
  @keyframes pulse { 0%,100%{opacity:1}50%{opacity:.5} }

  .sc__body { height: 90px; position: relative; }
  canvas { width: 100% !important; height: 100% !important; }

  .sc__skeleton { width: 100%; height: 100%; border-radius: 3px; background: linear-gradient(90deg, var(--skeleton-from, #eee) 25%, var(--skeleton-to, #f8f8f8) 50%, var(--skeleton-from, #eee) 75%); background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  .sc__error { display: flex; align-items: center; justify-content: center; height: 100%; font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--error-color, #c0392b); }
  .sc__empty { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: calc(4px * var(--font-scale)); }
  .sc__empty-main { font-size: calc(9.5px * var(--font-scale)); font-family: 'DM Mono', monospace; letter-spacing: .05em; color: var(--text-secondary); }
  .sc__empty-last  { font-size: calc(8.5px * var(--font-scale)); font-family: 'DM Mono', monospace; letter-spacing: .04em; color: var(--text-secondary); opacity: .8; }
</style>