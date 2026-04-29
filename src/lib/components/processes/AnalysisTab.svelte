<!-- src/lib/components/processes/AnalysisTab.svelte -->
<script lang="ts">
  import { processStore, type ProcessReading } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const proc      = $derived($processStore.process);
  const pipelines = $derived(proc?.config?.pipelines ?? []);

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

  // Seleccionar primer pipeline por defecto
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
    } catch {}
    finally { loading = false; }
  }

  function refresh() {
    const preset = PRESETS.find(p => p.label === timePreset);
    if (preset) load(preset.hours);
  }

  // Labels del pipeline seleccionado
  const labels = $derived(() => {
    const pl = pipelines.find((p: any) => p.id === selectedPipeline);
    const src = pl?.nodes.find((n: any) => n.type === 'postgres_sensor');
    return src?.sensors?.map((s: any) => s.label) ?? [];
  });

  // ── SVG chart ─────────────────────────────────────────────────────────────
  const W = 800; const H = 260;
  const PAD = { top: 10, right: 20, bottom: 40, left: 56 };
  const iw = W - PAD.left - PAD.right;
  const ih = H - PAD.top  - PAD.bottom;

  const chartData = $derived(() => {
    if (readings.length < 2) return null;
    const n = readings[0].filtered?.length ?? readings[0].raw?.length ?? 0;
    if (!n) return null;

    const allVals: number[] = [];
    for (const r of readings) {
      if (chartMode !== 'filtered') r.raw?.forEach(v => allVals.push(v));
      if (chartMode !== 'raw')      r.filtered?.forEach(v => allVals.push(v));
    }
    if (!allVals.length) return null;

    const minV = Math.min(...allVals);
    const maxV = Math.max(...allVals);
    const rv = maxV - minV || 1;
    const minT = new Date(readings[0].ts).getTime();
    const maxT = new Date(readings[readings.length-1].ts).getTime();
    const rt = maxT - minT || 1;

    const sx = (ts: string) => ((new Date(ts).getTime() - minT) / rt) * iw;
    const sy = (v: number)  => ih - ((v - minV) / rv) * ih;

    const rawPaths:  string[] = Array(n).fill('');
    const filtPaths: string[] = Array(n).fill('');

    for (const r of readings) {
      const x = sx(r.ts);
      for (let i = 0; i < n; i++) {
        if (chartMode !== 'filtered' && r.raw?.[i] != null) {
          const y = sy(r.raw[i]);
          rawPaths[i] += rawPaths[i] ? ` L${x.toFixed(1)},${y.toFixed(1)}` : `M${x.toFixed(1)},${y.toFixed(1)}`;
        }
        if (chartMode !== 'raw' && r.filtered?.[i] != null) {
          const y = sy(r.filtered[i]);
          filtPaths[i] += filtPaths[i] ? ` L${x.toFixed(1)},${y.toFixed(1)}` : `M${x.toFixed(1)},${y.toFixed(1)}`;
        }
      }
    }

    const ticks = Array.from({length: 5}, (_, i) => ({
      y: sy(minV + (rv * i) / 4),
      label: (minV + (rv * i) / 4).toFixed(3),
    }));

    const xTicks = [0, 0.25, 0.5, 0.75, 1].map(f => ({
      x: f * iw,
      label: new Date(minT + f * rt).toLocaleTimeString('es-CR', {
        month: '2-digit', day: '2-digit',
        hour: '2-digit', minute: '2-digit', hour12: false,
      }),
    }));

    // Bandas de actuador ON
    const actBands: {x0: number; x1: number}[] = [];
    for (let i = 1; i < readings.length; i++) {
      if (readings[i].actuator === 'on') {
        actBands.push({ x0: sx(readings[i-1].ts), x1: sx(readings[i].ts) });
      }
    }

    return { n, rawPaths, filtPaths, ticks, xTicks, actBands };
  });
</script>

<div class="analysis">

  <!-- Toolbar -->
  <div class="toolbar">
    <!-- Selector de pipeline -->
    <div class="pl-selector">
      {#each pipelines as pl (pl.id)}
        <button class="pl-btn" class:active={selectedPipeline === pl.id}
          onclick={() => selectedPipeline = pl.id}>{pl.label}</button>
      {/each}
    </div>

    <div class="toolbar-right">
      <!-- Modo -->
      <div class="btn-group">
        <button class="cmbtn" class:active={chartMode==='raw'}      onclick={() => chartMode='raw'}>crudo</button>
        <button class="cmbtn" class:active={chartMode==='filtered'} onclick={() => chartMode='filtered'}>filtrado</button>
        <button class="cmbtn" class:active={chartMode==='both'}     onclick={() => chartMode='both'}>ambos</button>
      </div>
      <!-- Tiempo -->
      <div class="btn-group">
        {#each PRESETS as p (p.label)}
          <button class="cmbtn" class:active={timePreset===p.label}
            onclick={() => timePreset=p.label}>{p.label}</button>
        {/each}
      </div>
      <button class="cmbtn" onclick={refresh} disabled={loading}>↺</button>
    </div>
  </div>

  <!-- Chart -->
  {#if loading}
    <div class="chart-msg">cargando...</div>
  {:else}
    {@const cd = chartData()}
    {#if !cd}
      <div class="chart-msg">sin lecturas en este rango</div>
    {:else}
      <div class="chart-wrap">
        <svg viewBox="0 0 {W} {H}" class="chart-svg" preserveAspectRatio="xMidYMid meet">
          <g transform="translate({PAD.left},{PAD.top})">

            <!-- Bandas actuador ON -->
            {#each cd.actBands as band (band.x0)}
              <rect x={band.x0} y="0" width={band.x1 - band.x0} height={ih}
                fill="rgba(61,168,90,0.08)"/>
            {/each}

            <!-- Grid Y -->
            {#each cd.ticks as tick (tick.label)}
              <line x1="0" y1={tick.y} x2={iw} y2={tick.y} stroke="var(--border-subtle)" stroke-width="0.5"/>
              <text x="-8" y={tick.y+4} text-anchor="end" font-size="9" fill="var(--text-muted)">{tick.label}</text>
            {/each}

            <!-- Grid X -->
            {#each cd.xTicks as tick (tick.label)}
              <text x={tick.x} y={ih+22} text-anchor="middle" font-size="8" fill="var(--text-muted)">{tick.label}</text>
            {/each}

            <!-- Paths crudos -->
            {#if chartMode !== 'filtered'}
              {#each cd.rawPaths as path, i (i)}
                <path d={path} stroke={COLORS[i%COLORS.length]} stroke-width="1"
                  stroke-dasharray="3,3" stroke-opacity="0.45" fill="none"/>
              {/each}
            {/if}

            <!-- Paths filtrados -->
            {#if chartMode !== 'raw'}
              {#each cd.filtPaths as path, i (i)}
                <path d={path} stroke={COLORS[i%COLORS.length]} stroke-width="1.8" fill="none"/>
              {/each}
            {/if}

          </g>
        </svg>

        <!-- Leyenda -->
        <div class="legend">
          {#each labels().slice(0, cd.n) as label, i (i)}
            <div class="li">
              <span class="ld" style="background:{COLORS[i%COLORS.length]}"></span>
              <span class="ll">{label}</span>
            </div>
          {/each}
          {#if chartMode === 'both'}
            <div class="li">
              <span class="ll muted">— — crudo &nbsp;·&nbsp; — filtrado</span>
            </div>
          {/if}
          <div class="li">
            <span class="act-band-sample"></span>
            <span class="ll muted">actuador ON</span>
          </div>
        </div>
      </div>
    {/if}
  {/if}

</div>

<style>
  .analysis { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  .toolbar { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .toolbar-right { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }

  .pl-selector { display: flex; gap: 4px; flex-wrap: wrap; }
  .pl-btn { padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .pl-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
  .pl-btn.active { background: var(--bg-elevated); color: var(--text-primary); border-color: var(--text-primary); }

  .btn-group { display: flex; gap: 3px; }
  .cmbtn { padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .cmbtn:hover { background: var(--interactive-hover); }
  .cmbtn.active { background: var(--bg-elevated); color: var(--text-primary); }
  .cmbtn:disabled { opacity: 0.5; }

  .chart-msg { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-align: center; padding: 60px 0; }
  .chart-wrap { background: var(--bg-elevated); border: 0.5px solid var(--border-subtle); border-radius: 8px; padding: calc(8px * var(--font-scale)); }
  .chart-svg { width: 100%; display: block; }
  .legend { display: flex; flex-wrap: wrap; gap: 10px; padding: 6px 4px 2px; }
  .li { display: flex; align-items: center; gap: 5px; }
  .ld { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .ll { font-size: calc(10px * var(--font-scale)); color: var(--text-secondary); font-family: 'DM Mono', monospace; }
  .ll.muted { color: var(--text-muted); }
  .act-band-sample { width: 16px; height: 10px; background: rgba(61,168,90,0.2); border-radius: 2px; flex-shrink: 0; }
</style>
