<script lang="ts">
  import { onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { experimentStore, activeEvents, experimentContext, objectiveEvaluations } from '$lib/stores/experiment';

  let canvas = $state<HTMLCanvasElement | null>(null);
  let chart: any = null;

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }

  // Elegir la variable más interesante para graficar:
  // la del primer objetivo con estado warning o violation, o la del primero.
  const primaryVar = $derived(() => {
    const evals = $objectiveEvaluations;
    const urgent = evals.find(e => e.status === 'violation' || e.status === 'warning');
    const first  = evals[0];
    const target = urgent ?? first;
    if (!target) return null;
    const cond = target.objective.condition as any;
    return cond.variable as string | null;
  });

  $effect(() => {
    if (!canvas || !primaryVar()) return;
    const varName = primaryVar()!;

    // Puntos de la serie: timestamp + valor de la variable en el contexto en ese momento
    // Reconstruimos el contexto incremental para tener la evolución temporal
    const points: { x: string; y: number }[] = [];
    const sorted = [...$activeEvents].sort(
      (a, b) => new Date(a.recorded_at).getTime() - new Date(b.recorded_at).getTime()
    );

    for (const ev of sorted) {
      const data = ev.data as any;
      let val: number | null = null;
      if (ev.step_key === varName && typeof data.value === 'number') {
        val = data.value;
      } else if (typeof data[varName] === 'number') {
        val = data[varName];
      }
      if (val !== null) {
        points.push({
          x: new Date(ev.recorded_at).toLocaleString('es-CR', {
            day:'2-digit', month:'short', hour:'2-digit', minute:'2-digit', hour12:false,
          }),
          y: val,
        });
      }
    }

    if (points.length === 0) { if (chart) { chart.destroy(); chart = null; } return; }

    // Líneas de referencia de objetivos sobre esta variable
    const refLines = $objectiveEvaluations
      .filter(e => {
        const c = e.objective.condition as any;
        return e.objective.condition_type === 'range' && c.variable === varName;
      })
      .flatMap(e => {
        const c = e.objective.condition as any;
        const lines = [];
        if (c.min != null) lines.push({ y: c.min, color: '#BA7517', label: `mín ${c.min}` });
        if (c.max != null) lines.push({ y: c.max, color: '#639922', label: `máx ${c.max}` });
        return lines;
      });

    const annotations: any = {};
    refLines.forEach((l, i) => {
      annotations[`ref${i}`] = {
        type: 'line', yMin: l.y, yMax: l.y,
        borderColor: l.color, borderWidth: 1, borderDash: [4, 3],
        label: { content: l.label, display: true, position: 'start',
                 font: { size: 9 }, color: l.color },
      };
    });

    const labels = points.map(p => p.x);
    const data   = points.map(p => p.y);
    const tick   = cssVar('--chart-tick');
    const grid   = cssVar('--chart-grid');

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
          label: varName,
          data,
          borderColor: '#378ADD',
          backgroundColor: '#378ADD18',
          borderWidth: 1.5,
          pointRadius: points.length > 50 ? 0 : 3,
          fill: true, tension: 0.3,
        }],
      },
      options: {
        responsive: true, maintainAspectRatio: false,
        animation: { duration: 300 },
        plugins: {
          legend: { display: false },
          annotation: { annotations },
          tooltip: {
            backgroundColor: cssVar('--chart-tooltip-bg'),
            titleColor: cssVar('--chart-tooltip-title'),
            bodyColor: cssVar('--chart-tooltip-body'),
            borderColor: cssVar('--chart-tooltip-border'),
            borderWidth: 1,
          },
        },
        scales: {
          x: { ticks: { color: tick, font: { size: 9, family:"'DM Mono',monospace" }, maxTicksLimit: 6, maxRotation: 0 }, grid: { color: grid }, border: { color: grid } },
          y: { ticks: { color: tick, font: { size: 9, family:"'DM Mono',monospace" }, maxTicksLimit: 5 }, grid: { color: grid }, border: { color: grid } },
        },
      },
    });
  });

  onDestroy(() => chart?.destroy());
</script>

{#if primaryVar()}
  <div class="chart-wrap">
    <div class="chart-label">{primaryVar()}</div>
    <div class="chart-body">
      <canvas bind:this={canvas}></canvas>
    </div>
  </div>
{/if}

<style>
  .chart-wrap { margin-bottom: calc(20px * var(--font-scale)); }
  .chart-label {
    font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace;
    letter-spacing: .06em; color: var(--text-muted);
    text-transform: uppercase; margin-bottom: calc(6px * var(--font-scale));
  }
  .chart-body { height: calc(140px * var(--font-scale)); position: relative; }
  canvas { width: 100% !important; height: 100% !important; }
</style>
