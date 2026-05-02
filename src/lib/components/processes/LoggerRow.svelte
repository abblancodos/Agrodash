<!-- src/lib/components/processes/LoggerRow.svelte -->
<!-- Fila de Logger: sparkline inline + valor actual, expandible al click -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import type { ProcessReading } from '$lib/stores/process.svelte';

  let {
    tag, nodeType, nodeData,
    readings, color, expanded, ontoggle,
  }: {
    tag:       string;
    nodeType:  string;
    nodeData:  any;
    readings:  ProcessReading[];
    color:     string;
    expanded:  boolean;
    ontoggle:  () => void;
  } = $props();

  // ── Valor actual formateado ────────────────────────────────────────────────
  const currentValue = $derived.by(() => {
    if (!nodeData) return '—';
    switch (nodeType) {
      case 'kalman':
        return nodeData.x?.[0]?.toFixed(4) ?? '—';
      case 'ewma': case 'lowpass': case 'moving_avg':
        return (nodeData.y?.[0] ?? nodeData.x?.[0])?.toFixed(4) ?? '—';
      case 'mahalanobis':
        return nodeData.last_d != null ? `d=${nodeData.last_d.toFixed(3)}` : '—';
      case 'mqtt_actuator': case 'http_actuator':
        return nodeData.last_action?.toUpperCase() ?? '—';
      case 'postgres_sensor':
        return 'fuente';
      default:
        return nodeData.x?.[0]?.toFixed(4) ?? nodeData.y?.[0]?.toFixed(4) ?? '—';
    }
  });

  const valueColor = $derived.by(() => {
    if (nodeType === 'mqtt_actuator' || nodeType === 'http_actuator') {
      if (nodeData?.last_action === 'on')  return '#3da85a';
      if (nodeData?.last_action === 'off') return '#e05454';
    }
    if (nodeType === 'mahalanobis') {
      const d = nodeData?.last_d;
      if (d > 2.0) return '#e07b54';
      if (d > 0.8) return '#e8a838';
      return '#3da85a';
    }
    return color;
  });

  // ── Sparkline ──────────────────────────────────────────────────────────────
  let sparkCanvas: HTMLCanvasElement | null = null;
  let sparkChart:  Chart | null = null;

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }

  function buildSparkData(): number[] {
    return readings.map(r => {
      switch (nodeType) {
        case 'kalman':
          return r.filtered?.[0] ?? r.raw?.[0] ?? NaN;
        case 'ewma': case 'lowpass': case 'moving_avg':
          return r.filtered?.[0] ?? NaN;
        case 'mahalanobis':
          return r.scope_values?.[tag] != null
            ? (Array.isArray(r.scope_values[tag]) ? (r.scope_values[tag] as number[])[0] : NaN)
            : NaN;
        case 'mqtt_actuator': case 'http_actuator': {
          const v = r.scope_values?.[tag];
          if (v === 'on')  return 1;
          if (v === 'off') return 0;
          return r.actuator === 'on' ? 1 : 0;
        }
        default:
          return r.filtered?.[0] ?? r.raw?.[0] ?? NaN;
      }
    }).filter(v => !isNaN(v));
  }

  function renderSpark() {
    if (!sparkCanvas || !readings.length) return;
    const data   = buildSparkData();
    if (!data.length) return;
    const labels = readings.slice(-data.length).map(() => '');
    const tick   = cssVar('--chart-tick');

    if (sparkChart) {
      sparkChart.data.labels          = labels;
      sparkChart.data.datasets[0].data = data;
      sparkChart.update('none');
      return;
    }

    sparkChart = new Chart(sparkCanvas, {
      type: 'line',
      data: {
        labels,
        datasets: [{
          data, borderColor: color, backgroundColor: color + '18',
          borderWidth: 1.5, pointRadius: 0, fill: true, tension: 0.3,
        }],
      },
      options: {
        responsive: true, maintainAspectRatio: false, animation: false,
        plugins: { legend: { display: false }, tooltip: { enabled: false } },
        scales: {
          x: { display: false, offset: true },
          y: {
            display: false,
            afterDataLimits: (axis: any) => {
              const span = (axis.max - axis.min) || 0.01;
              axis.min -= span * 0.15; axis.max += span * 0.15;
            },
          },
        },
      },
    });
  }

  $effect(() => {
    void readings;
    renderSpark();
  });

  // Tema
  let obs: MutationObserver | null = null;
  onMount(() => {
    obs = new MutationObserver(() => {
      if (!sparkChart) return;
      sparkChart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  });
  onDestroy(() => {
    sparkChart?.destroy();
    obs?.disconnect();
  });
</script>

<button class="logger-row" onclick={ontoggle} class:expanded>
  <span class="lr-tag">{tag}</span>
  <span class="lr-type">{nodeType}</span>
  <div class="lr-spark">
    <canvas bind:this={sparkCanvas}></canvas>
  </div>
  <span class="lr-val" style="color:{valueColor}">{currentValue}</span>
  <span class="lr-chevron" class:open={expanded}>▶</span>
</button>

<style>
  .logger-row  { width:100%; display:grid; grid-template-columns:120px 90px 1fr 80px 16px; align-items:center; gap:calc(8px * var(--font-scale)); padding:calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background:none; border:none; border-top:0.5px solid var(--border-subtle); cursor:pointer; text-align:left; transition:background .1s; }
  .logger-row:hover  { background:var(--interactive-hover); }
  .logger-row.expanded { background:var(--bg-elevated); }
  .lr-tag    { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-primary); font-weight:500; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .lr-type   { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .lr-spark  { height:calc(32px * var(--font-scale)); position:relative; }
  .lr-spark canvas { width:100% !important; height:100% !important; }
  .lr-val    { font-size:calc(13px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; text-align:right; white-space:nowrap; }
  .lr-chevron{ font-size:calc(9px * var(--font-scale)); color:var(--text-muted); transition:transform .15s; display:inline-block; }
  .lr-chevron.open { transform:rotate(90deg); }
</style>