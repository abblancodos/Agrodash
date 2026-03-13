<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchReadings, sensorColor, normaliseSensorLabel, type Sensor, type Reading } from '$lib/api';
  // chartjs-adapter-date-fns is required for the time scale.
  // Install with: npm install chartjs-adapter-date-fns date-fns

  interface Props {
    sensors: Sensor[]; from: Date; to: Date; live?: boolean;
    onRangeChange?: (from: Date, to: Date) => void;
  }
  let { sensors, from, to, live = false, onRangeChange }: Props = $props();

  let canvas: HTMLCanvasElement;
  let chart: any = null;
  let loading = $state(true);
  let error = $state('');

  let visible = $state<Record<string, boolean>>(
    Object.fromEntries(sensors.map(s => [s.id, true]))
  );

  let menuOpen = $state(false);
  let menuEl: HTMLDivElement;
  let yMin = $state('');
  let yMax = $state('');
  let lockX = $state(false);
  let lockY = $state(false);

  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  let datasets: { sensor: Sensor; readings: Reading[] }[] = [];

  async function loadAll() {
    loading = true; error = '';
    try {
      datasets = await Promise.all(
        sensors.map(async s => ({ sensor: s, readings: await fetchReadings(s.id, s.type, from, to) }))
      );
      renderChart();
    } catch (e: any) { error = e.message ?? 'Error'; }
    finally { loading = false; }
  }

  // Raw parsed data stored as {x, y} objects for decimation to work correctly
  let rawDatasets: { sensor: Sensor; points: { x: number; y: number | null }[] }[] = [];

  // Custom crosshair plugin: finds the closest point per dataset by X value (timestamp),
  // independent of array index. This is necessary because datasets have different bucket
  // timestamps and `mode: 'index'` would misalign them with parsing: false + time scale.
  function makeCrosshairPlugin(cssVarFn: (n: string) => string) {
    return {
      id: 'agro-crosshair',
      afterDraw(chartInstance: any) {
        const { ctx, chartArea, scales, tooltip } = chartInstance;
        if ((chartInstance as any)._agroInteraction === false) return;
        if (!tooltip || !tooltip._active || !tooltip._active.length) return;

        const x = tooltip._active[0].element.x;
        if (x < chartArea.left || x > chartArea.right) return;

        ctx.save();
        ctx.beginPath();
        ctx.moveTo(x, chartArea.top);
        ctx.lineTo(x, chartArea.bottom);
        ctx.strokeStyle = cssVarFn('--border-strong');
        ctx.lineWidth = 1;
        ctx.setLineDash([3, 3]);
        ctx.stroke();
        ctx.restore();
      },
    };
  }

  async function renderChart() {
    if (!canvas) return;
    const [{ default: Chart }, { default: zoomPlugin }] = await Promise.all([
      import('chart.js/auto'),
      import('chartjs-plugin-zoom'),
      import('chartjs-adapter-date-fns'), // registers itself automatically
    ]);
    Chart.register(zoomPlugin);

    // Register crosshair once
    if (!Chart.registry.plugins.get('agro-crosshair')) {
      Chart.register(makeCrosshairPlugin(cssVar));
    }

    // Shared time axis — union of all buckets, sorted
    const allTimes = [...new Set(datasets.flatMap(d => d.readings.map(r => r.bucket)))].sort();

    const chartDatasets = datasets.map(({ sensor, readings }) => {
      const map = new Map(readings.map(r => [r.bucket, r.value]));
      const color = sensorColor(sensor.type);
      const n = allTimes.length;
      return {
        label: `${normaliseSensorLabel(sensor.type)} #${sensor.sensor_number}`,
        data: allTimes.map(t => map.get(t) ?? null),
        borderColor: color,
        backgroundColor: color + '18',
        borderWidth: 1.5,
        pointRadius: n > 500 ? 0 : n > 100 ? 1.5 : 3,
        pointHoverRadius: 5,
        pointHitRadius: 8,
        tension: 0,
        spanGaps: false,
        hidden: !visible[sensor.id],
        yAxisID: 'y',
      };
    });

    const labels = allTimes.map(t =>
      new Date(t).toLocaleString('es-CR', {
        day: '2-digit', month: '2-digit', year: '2-digit',
        hour: '2-digit', minute: '2-digit', second: '2-digit',
      })
    );

    if (chart) {
      chart.data.labels = labels;
      chart.data.datasets = chartDatasets;
      applyScaleOptions();
      chart.update('none');
      return;
    }

    chart = new Chart(canvas, {
      type: 'line',
      data: { labels, datasets: chartDatasets },
      options: buildOptions(),
    });
  }

  function buildOptions(): any {
    const grid = cssVar('--chart-grid');
    const tick = cssVar('--chart-tick');
    const mode = lockX ? 'y' : lockY ? 'x' : 'xy';
    return {
      responsive: true,
      maintainAspectRatio: false,
      animation: false,          // ← kill animation entirely; zoom would queue hundreds of frames
      parsing: false,            // data is pre-parsed as {x,y}
      normalized: true,          // data is already sorted and normalized
      interaction: { mode: 'index', intersect: false, axis: 'x' },
      plugins: {
        legend: { display: false },
        decimation: {
          enabled: true,
          algorithm: 'lttb',     // Largest-Triangle-Three-Buckets — best quality/perf tradeoff
          samples: 300,          // max points rendered per dataset at any zoom level
          threshold: 300,
        },
        tooltip: {
          animation: false,
          mode: 'index',
          intersect: false,
          axis: 'x',
          backgroundColor: cssVar('--chart-tooltip-bg'),
          titleColor: cssVar('--chart-tooltip-title'),
          bodyColor: cssVar('--chart-tooltip-body'),
          borderColor: cssVar('--chart-tooltip-border'),
          borderWidth: 1, padding: 10,
          callbacks: {
            title: (items: any[]) => {
              if (!items.length) return '';
              // label is already a formatted locale string
              return items[0].label ?? '';
            },
            label: (ctx: any) => {
              if (ctx.parsed.y == null) return null; // hide null entries from tooltip
              return ` ${ctx.dataset.label}: ${ctx.parsed.y.toFixed(3)}`;
            },
          },
        },
        zoom: {
          pan: {
            enabled: true,
            mode,
            threshold: 5,        // min px before pan activates — prevents accidental pan on click
          },
          zoom: {
            wheel: {
              enabled: true,
              speed: 0.08,       // slower zoom speed = less jumpy feel
            },
            pinch: { enabled: true },
            mode,
          },
        },
      },
      scales: {
        x: {
          ticks: {
            color: tick,
            font: { size: 9, family: "'DM Mono',monospace" },
            maxTicksLimit: 8,
            maxRotation: 0,
          },
          grid: { color: grid }, border: { color: grid },
        },
        y: {
          min: yMin !== '' ? parseFloat(yMin) : undefined,
          max: yMax !== '' ? parseFloat(yMax) : undefined,
          ticks: {
            color: tick,
            font: { size: 9, family: "'DM Mono',monospace" },
            maxTicksLimit: 6,
          },
          grid: { color: grid }, border: { color: grid },
        },
      },
    };
  }

  function applyScaleOptions() {
    if (!chart) return;
    chart.options.scales.y.min = yMin !== '' ? parseFloat(yMin) : undefined;
    chart.options.scales.y.max = yMax !== '' ? parseFloat(yMax) : undefined;
    const mode = lockX ? 'y' : lockY ? 'x' : 'xy';
    chart.options.plugins.zoom.pan.mode = mode;
    chart.options.plugins.zoom.zoom.mode = mode;
    chart.update('none');
  }

  function toggleSensor(id: string) {
    visible[id] = !visible[id];
    if (!chart) return;
    const idx = sensors.findIndex(s => s.id === id);
    if (idx !== -1) { chart.setDatasetVisibility(idx, visible[id]); chart.update('none'); }
  }

  let interactionEnabled = $state(true);

  function resetZoom() { chart?.resetZoom(); }

  function toggleInteraction() {
    interactionEnabled = !interactionEnabled;
    if (!chart) return;
    chart.options.plugins.tooltip.enabled = interactionEnabled;
    // disable crosshair by hiding its draw via a flag on the chart instance
    (chart as any)._agroInteraction = interactionEnabled;
    chart.update('none');
  }

  function handleOutside(e: MouseEvent) {
    if (menuOpen && menuEl && !menuEl.contains(e.target as Node)) menuOpen = false;
  }

  let liveInterval: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    loadAll();
    if (live) liveInterval = setInterval(loadAll, 15_000);
    return () => { if (liveInterval) clearInterval(liveInterval); };
  });
  onDestroy(() => { chart?.destroy(); if (liveInterval) clearInterval(liveInterval); });
</script>

<svelte:window onclick={handleOutside} />

<div class="msc">
  <div class="msc__topbar">
    <div class="msc__toggles">
      {#each sensors as s (s.id)}
        {@const color = sensorColor(s.type)}
        <button class="toggle" class:off={!visible[s.id]} style="--c:{color}"
          onclick={() => toggleSensor(s.id)}>
          <span class="toggle__dot"></span>
          <span class="toggle__label">{normaliseSensorLabel(s.type)} #{s.sensor_number}</span>
        </button>
      {/each}
    </div>

    <div class="msc__actions">
      <!-- Interaction toggle -->
      <button
        class="action-btn"
        class:action-btn--on={interactionEnabled}
        onclick={toggleInteraction}
        title={interactionEnabled ? 'Desactivar tooltip/crosshair' : 'Activar tooltip/crosshair'}
      >
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
          <circle cx="12" cy="12" r="3"/>
          {#if !interactionEnabled}
            <line x1="2" y1="2" x2="22" y2="22"/>
          {/if}
        </svg>
      </button>

      <!-- Reset zoom -->
      <button class="action-btn" onclick={resetZoom} title="Resetear zoom">↺</button>

      <!-- ⋯ options menu -->
      <div class="msc__menu-wrap" bind:this={menuEl}>
        <button class="menu-btn" onclick={() => menuOpen = !menuOpen}>⋯</button>

        {#if menuOpen}
          <div class="menu">
            <div class="menu__title">OPCIONES DE GRÁFICO</div>

            <div class="menu__section">
              <span class="menu__label">Rango Y</span>
              <div class="menu__row">
                <input class="menu__input" type="number" placeholder="min"
                  bind:value={yMin} oninput={applyScaleOptions} />
                <span class="menu__sep">–</span>
                <input class="menu__input" type="number" placeholder="max"
                  bind:value={yMax} oninput={applyScaleOptions} />
              </div>
            </div>

            <div class="menu__section">
              <span class="menu__label">Bloquear zoom/pan</span>
              <label class="menu__check">
                <input type="checkbox" bind:checked={lockX}
                  onchange={() => { if(lockX) lockY=false; applyScaleOptions(); }} />
                Solo vertical (bloquear X)
              </label>
              <label class="menu__check">
                <input type="checkbox" bind:checked={lockY}
                  onchange={() => { if(lockY) lockX=false; applyScaleOptions(); }} />
                Solo horizontal (bloquear Y)
              </label>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="msc__chart">
    {#if loading && !chart}<div class="msc__skeleton"></div>
    {:else if error}<div class="msc__error">{error}</div>
    {:else}<canvas bind:this={canvas}></canvas>{/if}
  </div>

  <div class="msc__hint">scroll para zoom · arrastrar para pan</div>
</div>

<style>
  .msc { display:flex; flex-direction:column; gap:8px; }

  .msc__topbar { display:flex; align-items:flex-start; justify-content:space-between; gap:8px; }
  .msc__toggles { display:flex; flex-wrap:wrap; gap:5px; flex:1; }

  .toggle {
    display:flex; align-items:center; gap:5px; padding:3px 8px 3px 6px;
    background:color-mix(in srgb, var(--c) 8%, var(--bg-elevated));
    border:1px solid color-mix(in srgb, var(--c) 28%, transparent);
    border-radius:3px; cursor:pointer; transition:all .12s;
  }
  .toggle.off { background:var(--bg-elevated); border-color:var(--border-subtle); }
  .toggle.off .toggle__dot { background:var(--border-default) !important; }
  .toggle.off .toggle__label { color:var(--text-faint); }
  .toggle__dot { width:6px; height:6px; border-radius:50%; background:var(--c); flex-shrink:0; }
  .toggle__label { font-family:'DM Mono',monospace; font-size:9px; letter-spacing:.05em; color:color-mix(in srgb, var(--c) 80%, var(--text-primary)); white-space:nowrap; }

  /* Action buttons (reset + interaction toggle) */
  .msc__actions { display:flex; align-items:center; gap:4px; flex-shrink:0; }

  .action-btn {
    display:flex; align-items:center; justify-content:center;
    width:26px; height:26px;
    background:var(--interactive-bg); border:1px solid var(--border-default);
    border-radius:4px; color:var(--text-faint); font-size:13px;
    cursor:pointer; transition:all .12s; padding:0; line-height:1;
  }
  .action-btn:hover { background:var(--interactive-hover); color:var(--text-secondary); border-color:var(--border-strong); }
  .action-btn--on { color:var(--text-secondary); border-color:var(--border-strong); }

  /* Menu */
  .msc__menu-wrap { position:relative; }
  .menu-btn { background:var(--interactive-bg); border:1px solid var(--border-default); border-radius:4px; color:var(--text-muted); font-size:16px; cursor:pointer; padding:2px 8px; line-height:1.4; transition:all .12s; letter-spacing:.05em; }
  .menu-btn:hover { background:var(--interactive-hover); color:var(--text-secondary); }

  .menu {
    position:absolute; right:0; top:calc(100% + 6px); z-index:100;
    background:var(--bg-overlay); border:1px solid var(--border-default);
    border-radius:6px; padding:14px; width:220px;
    box-shadow:0 8px 24px rgba(0,0,0,.15);
    display:flex; flex-direction:column; gap:12px;
  }
  .menu__title { font-family:'DM Mono',monospace; font-size:8.5px; letter-spacing:.14em; color:var(--text-faint); padding-bottom:8px; border-bottom:1px solid var(--border-subtle); }
  .menu__section { display:flex; flex-direction:column; gap:6px; }
  .menu__label { font-family:'DM Mono',monospace; font-size:9px; letter-spacing:.08em; color:var(--text-muted); text-transform:uppercase; }
  .menu__row { display:flex; align-items:center; gap:6px; }
  .menu__input { width:70px; padding:5px 7px; background:var(--bg-inset); border:1px solid var(--border-subtle); border-radius:3px; color:var(--text-secondary); font-family:'DM Mono',monospace; font-size:11px; outline:none; transition:border-color .12s; -moz-appearance:textfield; }
  .menu__input::-webkit-inner-spin-button { -webkit-appearance:none; }
  .menu__input:focus { border-color:var(--interactive-focus); }
  .menu__input::placeholder { color:var(--text-faint); }
  .menu__sep { color:var(--text-faint); font-size:11px; }
  .menu__check { display:flex; align-items:center; gap:7px; font-family:'DM Mono',monospace; font-size:10px; color:var(--text-muted); cursor:pointer; }
  .menu__check input[type=checkbox] { accent-color:var(--text-secondary); cursor:pointer; }
  .menu__reset { background:var(--interactive-bg); border:1px solid var(--border-default); border-radius:3px; color:var(--text-muted); font-family:'DM Mono',monospace; font-size:10px; letter-spacing:.06em; padding:7px; cursor:pointer; width:100%; transition:all .12s; }
  .menu__reset:hover { background:var(--interactive-hover); color:var(--text-secondary); }

  .msc__chart { height:260px; position:relative; }
  canvas { width:100% !important; height:100% !important; }

  .msc__skeleton { width:100%; height:100%; border-radius:3px; background:linear-gradient(90deg,var(--skeleton-from) 25%,var(--skeleton-to) 50%,var(--skeleton-from) 75%); background-size:200% 100%; animation:shimmer 1.4s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  .msc__error { display:flex; align-items:center; justify-content:center; height:100%; font-family:'DM Mono',monospace; font-size:10px; color:var(--error-color); }

  .msc__hint { font-family:'DM Mono',monospace; font-size:8.5px; color:var(--text-faint); letter-spacing:.06em; text-align:right; }
</style>