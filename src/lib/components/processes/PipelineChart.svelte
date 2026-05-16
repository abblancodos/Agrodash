<!-- PipelineChart.svelte
     Chart expandido con:
     - Variables disponibles detectadas dinámicamente desde scope_values
     - Selector de tiempo: presets 1h/6h/24h/7d + date picker desde/hasta
     - Actuador como región sombreada (no línea)
     - Eje Y bloqueable, leyenda interactiva
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Chart from 'chart.js/auto';
  import { processStore, type ProcessReading } from '$lib/stores/process';

  interface Props {
    processId:     string;
    pipelineId:    string;
    labels:        string[];      // sensor labels del postgres_sensor
    loggerTag?:    string;        // tag del Logger
    upstreamType?: string;        // tipo de nodo upstream
  }

  let { processId, pipelineId, labels, loggerTag, upstreamType }: Props = $props();

  const COLORS = ['#4a90d9','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0','#3da85a'];
  const ACT_COLOR = '#3da85a';

  type Preset = '1h' | '6h' | '24h' | '7d' | 'custom';
  const PRESETS: Preset[] = ['1h', '6h', '24h', '7d', 'custom'];
  const PRESET_HOURS: Record<string, number> = { '1h': 1, '6h': 6, '24h': 24, '7d': 168 };

  let preset     = $state<Preset>('6h');
  let customFrom = $state('');
  let customTo   = $state('');

  function toLocalDatetimeInput(d: Date): string {
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  function activateCustom() {
    const now = new Date();
    const from = new Date(now.getTime() - 6 * 3600_000);
    if (!customFrom) customFrom = toLocalDatetimeInput(from);
    if (!customTo)   customTo   = toLocalDatetimeInput(now);
  }

  interface VarDef {
    key:     string;
    label:   string;
    group:   string;
    enabled: boolean;
    color:   string;
    isAct:   boolean;
    axisId:  string;
  }

  let availableVars = $state<VarDef[]>([]);
  let readings      = $state<ProcessReading[]>([]);
  let loading       = $state(true);
  let empty         = $state(false);

  let canvas = $state<HTMLCanvasElement | null>(null);
  let chart: Chart | null = null;

  let lockY = $state(false);
  let yMin  = $state('');
  let yMax  = $state('');
  let showAxisPanel = $state(false);

  function cssVar(n: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  }
  function fmtTs(iso: string): string {
    const d = new Date(iso.endsWith('Z') ? iso : iso + 'Z');
    return d.toLocaleTimeString('es-CR', { hour:'2-digit', minute:'2-digit', hour12: false });
  }
  function fallbackVector(r: ProcessReading, dim: number): number | null {
    if (!r.scope_values) return null;
    for (const val of Object.values(r.scope_values)) {
      if (Array.isArray(val) && val.length > dim && typeof val[dim] === 'number')
        return val[dim] as number;
    }
    return null;
  }
  function getFiltered(r: ProcessReading, dim: number): number | null {
    return r.filtered?.[dim] ?? fallbackVector(r, dim);
  }

  function detectVars(rs: ProcessReading[]): VarDef[] {
    const tag    = loggerTag ?? '';
    const ntype  = upstreamType ?? '';
    const vars: VarDef[] = [];
    let ci = 0;

    const SIGNAL_TYPES = ['kalman','ewma','lowpass','moving_avg','postgres_sensor'];
    const ACT_TYPES    = ['actuator','mqtt_actuator','http_actuator'];

    // Señal cruda / filtrada para nodos de señal y decisores
    const showRawFiltered = !ntype || SIGNAL_TYPES.includes(ntype) ||
      ['hysteresis','sprt','mahalanobis'].includes(ntype);

    if (showRawFiltered) {
      const nDims = rs.find(r => r.filtered?.length)?.filtered?.length
                 ?? rs.find(r => r.raw?.length)?.raw?.length ?? 1;
      for (let i = 0; i < nDims; i++) {
        const lbl = labels[i] ?? `s${i+1}`;
        const c   = COLORS[ci++ % COLORS.length];
        if (rs.some(r => r.raw?.[i] != null)) {
          vars.push({ key: `raw_${i}`, label: `${lbl} (crudo)`,
            group: 'señal', enabled: false, color: c + '66', isAct: false, axisId: 'y' });
        }
        if (rs.some(r => getFiltered(r, i) != null)) {
          vars.push({ key: `filtered_${i}`, label: `${lbl} (filtrado)`,
            group: 'señal', enabled: true, color: c, isAct: false, axisId: 'y' });
        }
      }
    }

    // Variables de scope_values que pertenecen a este logger
    const scopeKeys = new Set<string>();
    for (const r of rs) {
      if (!r.scope_values) continue;
      for (const k of Object.keys(r.scope_values)) {
        if (k === tag || k.startsWith(tag + ':')) scopeKeys.add(k);
      }
    }

    const sorted = [...scopeKeys].sort((a, b) =>
      a === tag ? -1 : b === tag ? 1 : a.localeCompare(b));

    const METRIC_LABELS: Record<string, string> = {
      '':        'señal',
      'decision':'decisión ON/OFF',
      'low':     'umbral inferior',
      'high':    'umbral superior',
      'val':     'valor reducido',
      'p':       'incertidumbre P',
      'k':       'ganancia K',
      'innov':   'innovación',
      'd':       'distancia Mahalanobis',
      'llr':     'LLR (SPRT)',
    };

    for (const k of sorted) {
      const suffix  = k === tag ? '' : k.slice(tag.length + 1);
      const baseKey = suffix.replace(/\[\d+\]$/, '');
      const idx     = suffix.match(/\[(\d+)\]$/)?.[1];
      const metaLbl = METRIC_LABELS[baseKey] ?? suffix;
      const label   = idx != null ? `${metaLbl} [${idx}]` : metaLbl;

      const isActType    = ACT_TYPES.includes(ntype);
      const isDecision   = suffix === 'decision';
      const isThreshold  = suffix === 'low' || suffix === 'high';
      const isActSignal  = isActType && k === tag;
      const renderAsAct  = isActSignal || isDecision;

      // Para nodos de señal, la señal del logger (tag) ya está cubierta por raw/filtered — omitir
      if (k === tag && SIGNAL_TYPES.includes(ntype) && !isActType) continue;

      const c = renderAsAct ? ACT_COLOR : COLORS[ci++ % COLORS.length];
      const group = renderAsAct ? 'actuador'
        : isThreshold ? 'umbrales'
        : ['p','k','innov'].some(x => baseKey.startsWith(x)) ? 'kalman interno'
        : baseKey === 'd' || baseKey === 'llr' ? 'decisor interno'
        : 'señal';

      const enabled = isDecision || isActSignal || (k === tag && !SIGNAL_TYPES.includes(ntype));

      vars.push({
        key: `scope_${k}`, label, group, enabled,
        color: c, isAct: renderAsAct, axisId: renderAsAct ? 'yAct' : 'y',
      });
    }

    return vars;
  }

  async function load() {
    loading = true; empty = false;
    try {
      let since: Date | undefined;
      let until: Date | undefined;
      if (preset === 'custom') {
        if (customFrom) since = new Date(customFrom);
        if (customTo)   until = new Date(customTo);
      } else {
        const h = PRESET_HOURS[preset] ?? 6;
        since = new Date(Date.now() - h * 3_600_000);
      }

      const data = await processStore.fetchReadings(
        processId, pipelineId,
        preset === 'custom' ? 168 : (PRESET_HOURS[preset] ?? 6),
        2000, since, until,
      );

      if (!data.length) { empty = true; return; }
      readings = data;
      if (!availableVars.length) availableVars = detectVars(data);
      render();
    } catch { empty = true; }
    finally { loading = false; }
  }

  function buildDatasets(): any[] {
    const datasets: any[] = [];

    // Regiones sombreadas del actuador primero (van al fondo)
    for (const v of availableVars.filter(v => v.isAct && v.enabled)) {
      const sk = v.key.startsWith('scope_') ? v.key.slice(6) : null;
      const data = readings.map(r => {
        if (sk) {
          const sv = r.scope_values?.[sk];
          if (sv === 'on')  return 1;
          if (sv === 'off') return 0;
          if (Array.isArray(sv)) return sv[0] > 0.5 ? 1 : 0;
        }
        return r.actuator === 'on' ? 1 : 0;
      });
      datasets.push({
        label: v.label, data,
        borderColor: 'transparent',
        backgroundColor: ACT_COLOR + '28',
        fill: { target: { value: 0 }, above: ACT_COLOR + '28', below: 'transparent' },
        stepped: 'before', pointRadius: 0, tension: 0, yAxisID: 'yAct',
      });
      datasets.push({
        label: '_act_border', data,
        borderColor: ACT_COLOR + '88', backgroundColor: 'transparent',
        borderWidth: 1.5, stepped: 'before', pointRadius: 0, tension: 0, yAxisID: 'yAct',
      });
    }

    // Resto de variables
    let firstFiltered = true;
    for (const v of availableVars.filter(v => !v.isAct && v.enabled)) {
      let data: (number | null)[];
      if (v.key.startsWith('raw_')) {
        const i = parseInt(v.key.slice(4));
        data = readings.map(r => r.raw?.[i] ?? null);
      } else if (v.key.startsWith('filtered_')) {
        const i = parseInt(v.key.slice(9));
        data = readings.map(r => getFiltered(r, i));
      } else if (v.key.startsWith('scope_')) {
        const sk = v.key.slice(6);
        data = readings.map(r => {
          const sv = r.scope_values?.[sk];
          if (Array.isArray(sv) && typeof sv[0] === 'number') return sv[0];
          return null;
          return null;
        });
      } else continue;

      const isThreshold = v.group === 'umbrales';
      const isFilt      = v.key.startsWith('filtered_');
      const fillThis    = isFilt && firstFiltered;
      if (isFilt) firstFiltered = false;

      datasets.push({
        label: v.label, data,
        borderColor: v.color,
        backgroundColor: isThreshold ? 'transparent' : v.color + '18',
        borderWidth: isThreshold ? 1 : 1.8,
        borderDash: isThreshold ? [6, 3] : (v.key.startsWith('raw_') ? [4, 3] : []),
        pointRadius: 0, fill: fillThis, tension: 0.3,
        yAxisID: v.axisId,
      });
    }

    return datasets;
  }

  function applyAxisLimits(axis: any) {
    if (lockY && yMin !== '' && yMax !== '') {
      axis.min = parseFloat(yMin); axis.max = parseFloat(yMax);
    } else {
      const span = (axis.max - axis.min) || 0.01;
      axis.min -= span * 0.12; axis.max += span * 0.12;
    }
  }

  function render() {
    if (!canvas || !readings.length) return;
    const datasets = buildDatasets();
    const xlabels  = readings.map(r => fmtTs(r.ts));
    const tick = cssVar('--chart-tick'), grid = cssVar('--chart-grid');
    const hasY   = datasets.some(d => d.yAxisID !== 'yAct');

    if (chart) {
      chart.data.labels = xlabels; chart.data.datasets = datasets;
      chart.update('none'); return;
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
            display: datasets.filter(d => !d.label.startsWith('_')).length > 1,
            position: 'top',
            labels: { color: tick, font: { size: 9, family: "'DM Mono',monospace" },
              boxWidth: 10, padding: 8, filter: (i: any) => !i.text.startsWith('_') },
          },
          tooltip: {
            backgroundColor: cssVar('--chart-tooltip-bg'),
            titleColor: cssVar('--chart-tooltip-title'),
            bodyColor: cssVar('--chart-tooltip-body'),
            borderColor: cssVar('--chart-tooltip-border'),
            borderWidth: 1, padding: 8,
            callbacks: {
              label: (ctx: any) => {
                if (ctx.dataset.label.startsWith('_')) return undefined;
                const v = ctx.parsed.y;
                return ctx.dataset.yAxisID === 'yAct'
                  ? ` ${ctx.dataset.label}: ${v > 0.5 ? 'ON' : 'OFF'}`
                  : ` ${ctx.dataset.label}: ${v?.toFixed(4) ?? '—'}`;
              },
            },
          },
        },
        scales: {
          x: {
            offset: true,
            ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 8, maxRotation: 0 },
            grid: { color: grid }, border: { color: grid },
          },
          y: {
            display: hasY,
            ticks: { color: tick, font: { size: 9, family: "'DM Mono',monospace" }, maxTicksLimit: 5 },
            grid: { color: grid }, border: { color: grid },
            afterDataLimits: (axis: any) => applyAxisLimits(axis),
          },
          yAct: { display: false, min: -0.05, max: 1.2 },
        },
      },
    });
  }

  function rerender() {
    if (!chart || !readings.length) return;
    chart.data.datasets = buildDatasets();
    const hasY = buildDatasets().some((d: any) => d.yAxisID !== 'yAct');
    (chart.options.scales as any).y.display = hasY;
    chart.update('none');
  }

  function applyYAxis() {
    if (!chart) return;
    const sc = chart.options.scales as any;
    if (lockY && yMin !== '' && yMax !== '') {
      sc.y.min = parseFloat(yMin); sc.y.max = parseFloat(yMax);
      sc.y.afterDataLimits = undefined;
    } else {
      sc.y.min = undefined; sc.y.max = undefined;
      sc.y.afterDataLimits = (axis: any) => applyAxisLimits(axis);
    }
    chart.update('none');
  }

  $effect(() => {
    void preset; void customFrom; void customTo;
    if (preset === 'custom' && (!customFrom || !customTo)) return;
    load();
  });

  $effect(() => { if (canvas && readings.length && !chart) render(); });

  let obs: MutationObserver | null = null;
  onMount(() => {
    obs = new MutationObserver(() => {
      if (!chart) return;
      const tick = cssVar('--chart-tick'), grid = cssVar('--chart-grid');
      const sc = chart.options.scales as any;
      sc.x.ticks.color = tick; sc.x.grid.color = grid;
      sc.y.ticks.color = tick; sc.y.grid.color = grid;
      chart.update('none');
    });
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  });
  onDestroy(() => { chart?.destroy(); obs?.disconnect(); });
</script>

<div class="toolbar">
  <div class="time-row">
    <div class="preset-tabs">
      {#each PRESETS as p}
        <button class="tab" class:active={preset === p}
          onclick={() => { preset = p; if (p === 'custom') activateCustom(); }}
        >{p}</button>
      {/each}
    </div>
    {#if preset === 'custom'}
      <div class="custom-range">
        <input type="datetime-local" class="dt-in" bind:value={customFrom}
          oninput={() => { if (customFrom && customTo) load(); }} />
        <span class="sep">→</span>
        <input type="datetime-local" class="dt-in" bind:value={customTo}
          oninput={() => { if (customFrom && customTo) load(); }} />
      </div>
    {/if}
  </div>

  {#if availableVars.length > 1}
    <div class="vars-row">
      {#each Object.entries(
        availableVars.reduce((acc: Record<string, VarDef[]>, v) => {
          (acc[v.group] ??= []).push(v); return acc;
        }, {})
      ) as [group, gvars]}
        <div class="var-group">
          <span class="var-group-label">{group}</span>
          {#each gvars as v (v.key)}
            <label class="var-chip" class:active={v.enabled}
              style:--chip-color={v.color}>
              <input type="checkbox" bind:checked={v.enabled} onchange={rerender} />
              <span class="chip-dot" style:background={v.color}></span>
              {v.label}
            </label>
          {/each}
        </div>
      {/each}
    </div>
  {/if}

  <div class="axis-row">
    <button class="ctrl-btn" class:active={showAxisPanel}
      onclick={() => showAxisPanel = !showAxisPanel}>⊞ ejes</button>
    {#if showAxisPanel}
      <label class="axis-lbl">
        <input type="checkbox" bind:checked={lockY} onchange={applyYAxis}/> Y
      </label>
      <input class="axis-in" type="number" step="0.001" placeholder="mín"
        bind:value={yMin} oninput={applyYAxis} disabled={!lockY}/>
      <span class="sep">—</span>
      <input class="axis-in" type="number" step="0.001" placeholder="máx"
        bind:value={yMax} oninput={applyYAxis} disabled={!lockY}/>
      <button class="ctrl-btn" onclick={() => { lockY=false; yMin=''; yMax=''; applyYAxis(); }}>⌖</button>
    {/if}
  </div>
</div>

<div class="chart-wrap">
  {#if loading}
    <div class="sk"></div>
  {:else if empty}
    <div class="msg">sin lecturas en este rango</div>
  {:else}
    <canvas bind:this={canvas}></canvas>
  {/if}
</div>

<style>
  .toolbar { display:flex; flex-direction:column; gap:6px; margin-bottom:8px; }

  .time-row    { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
  .preset-tabs { display:flex; gap:2px; background:var(--bg-inset); border-radius:6px; padding:2px; }
  .tab {
    padding:calc(2px * var(--font-scale)) calc(8px * var(--font-scale));
    border:none; border-radius:4px; background:none; cursor:pointer;
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace;
    color:var(--text-muted); transition:background .12s;
  }
  .tab.active { background:var(--bg-surface); color:var(--text-primary); box-shadow:0 0 0 0.5px var(--border-default); }
  .tab:hover:not(.active) { background:var(--interactive-hover); }

  .custom-range { display:flex; align-items:center; gap:4px; }
  .dt-in {
    padding:calc(2px * var(--font-scale)) calc(5px * var(--font-scale));
    border:0.5px solid var(--border-default); border-radius:4px;
    background:var(--bg-surface); color:var(--text-primary);
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace;
  }
  .sep { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); }

  .vars-row      { display:flex; flex-wrap:wrap; gap:10px; }
  .var-group     { display:flex; align-items:center; flex-wrap:wrap; gap:4px; }
  .var-group-label {
    font-size:calc(9px * var(--font-scale)); color:var(--text-muted);
    font-family:'DM Mono',monospace; text-transform:uppercase; letter-spacing:0.04em; margin-right:2px;
  }
  .var-chip {
    display:inline-flex; align-items:center; gap:4px;
    padding:calc(2px * var(--font-scale)) calc(7px * var(--font-scale));
    border:0.5px solid var(--border-subtle); border-radius:10px;
    background:var(--bg-inset); cursor:pointer;
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace;
    color:var(--text-muted); transition:all .12s;
  }
  .var-chip input { display:none; }
  .var-chip.active { border-color:var(--chip-color, var(--border-default)); background:var(--bg-elevated); color:var(--text-primary); }
  .var-chip:hover  { background:var(--interactive-hover); }
  .chip-dot { width:7px; height:7px; border-radius:50%; flex-shrink:0; }

  .axis-row  { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
  .ctrl-btn  {
    padding:calc(2px * var(--font-scale)) calc(7px * var(--font-scale));
    border:0.5px solid var(--border-default); border-radius:5px; background:none; cursor:pointer;
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted);
  }
  .ctrl-btn.active { background:var(--bg-elevated); color:var(--text-primary); }
  .ctrl-btn:hover  { background:var(--interactive-hover); }
  .axis-lbl  {
    display:flex; align-items:center; gap:4px;
    font-size:calc(10px * var(--font-scale)); color:var(--text-muted);
    font-family:'DM Mono',monospace; cursor:pointer;
  }
  .axis-in {
    width:60px; padding:2px 5px; border:0.5px solid var(--border-default); border-radius:4px;
    background:var(--bg-surface); color:var(--text-primary);
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace;
  }
  .axis-in:disabled { opacity:.4; }

  .chart-wrap { height:calc(180px * var(--font-scale)); position:relative; }
  .chart-wrap canvas { width:100% !important; height:100% !important; }
  .sk {
    width:100%; height:100%; border-radius:4px;
    background:linear-gradient(90deg,var(--bg-inset) 25%,var(--bg-elevated) 50%,var(--bg-inset) 75%);
    background-size:200% 100%; animation:shimmer 1.4s infinite;
  }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }
  .msg {
    display:flex; align-items:center; justify-content:center; height:100%;
    font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace;
  }
</style>