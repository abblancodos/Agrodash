<script lang="ts">
  import { fetchReadings, normaliseSensorLabel, sensorColor, type Box } from '$lib/api';
  import { downloading } from '$lib/stores/downloading';

  interface Props { box: Box; onclose: () => void; }
  let { box, onclose }: Props = $props();

  // ── Timezone ──────────────────────────────────────────────────────────────
  const userTz   = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzOffset = (() => {
    const off  = -new Date().getTimezoneOffset();
    const h    = Math.floor(Math.abs(off) / 60);
    const m    = Math.abs(off) % 60;
    const sign = off >= 0 ? '+' : '-';
    return m ? `UTC${sign}${h}:${String(m).padStart(2,'0')}` : `UTC${sign}${h}`;
  })();

  function bucketToLocal(bucket: string): string {
    return new Date(bucket + 'Z').toLocaleString('sv-SE', {
      timeZone: userTz, year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
    }).replace('T', ' ');
  }

  // ── Time range ────────────────────────────────────────────────────────────
  const PRESETS = [
    { label: '1h',   hours: 1   },
    { label: '24h',  hours: 24  },
    { label: '7d',   hours: 168 },
    { label: '1 mes',hours: 720 },
  ];
  let activePreset = $state<string | null>('24h');

  function fmt(d: Date) {
    return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
  }
  let toDate   = $state(fmt(new Date()));
  let fromDate = $state(fmt(new Date(Date.now() - 24 * 3_600_000)));

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    const now = new Date();
    toDate   = fmt(now);
    fromDate = fmt(new Date(now.getTime() - p.hours * 3_600_000));
  }

  // ── Resolution ────────────────────────────────────────────────────────────
  const RES_STEPS  = [100, 300, 1000, 3000, 5000];
  const RES_LABELS = ['100', '300', '1 000', '3 000', '5 000'];
  let resStep = $state(2);
  const points = $derived(RES_STEPS[resStep]);

  // ── Format mode ───────────────────────────────────────────────────────────
  // simple: current format   descriptive: VWC(v/v %)_SensorN
  let formatMode = $state<'simple' | 'descriptive'>('simple');

  // ── Download mode ────────────────────────────────────────────────────────
  let downloadMode = $state<'timeseries' | 'stats'>('timeseries');

  // ── Column editor (advanced) ──────────────────────────────────────────────
  let showAdvanced = $state(false);

  interface ColDef {
    sensorId:  string;
    enabled:   boolean;
    customLabel: string;
  }

  // Initialize column defs from box sensors
  let colDefs = $state<ColDef[]>(
    box.sensors.map(s => ({
      sensorId:    s.id,
      enabled:     true,
      customLabel: '',
    }))
  );

  // Stats columns available in summary mode
  interface StatCol { key: string; label: string; enabled: boolean; customLabel: string; }
  let statCols = $state<StatCol[]>([
    { key: 'mean',   label: 'promedio',           enabled: true,  customLabel: '' },
    { key: 'stddev', label: 'desv. estándar',     enabled: true,  customLabel: '' },
    { key: 'min',    label: 'mínimo',             enabled: true,  customLabel: '' },
    { key: 'max',    label: 'máximo',             enabled: true,  customLabel: '' },
    { key: 'count',  label: 'n (lecturas)',        enabled: true,  customLabel: '' },
    { key: 'range',  label: 'rango (max-min)',     enabled: false, customLabel: '' },
    { key: 'cv',     label: 'CV% (stddev/mean)',   enabled: false, customLabel: '' },
    { key: 'p25',    label: 'percentil 25',        enabled: false, customLabel: '' },
    { key: 'p75',    label: 'percentil 75',        enabled: false, customLabel: '' },
    { key: 'p95',    label: 'percentil 95',        enabled: false, customLabel: '' },
  ]);

  function statColLabel(sc: StatCol) {
    return sc.customLabel.trim() || sc.label;
  }

  function colLabel(col: ColDef): string {
    if (col.customLabel.trim()) return col.customLabel.trim();
    const s = box.sensors.find(x => x.id === col.sensorId)!;
    if (formatMode === 'descriptive') {
      return `${normaliseSensorLabel(s.type)}(v/v %)_Sensor${s.sensor_number}`;
    }
    return `${normaliseSensorLabel(s.type)}_#${s.sensor_number}`;
  }

  function moveCol(i: number, dir: -1 | 1) {
    const ni = i + dir;
    if (ni < 0 || ni >= colDefs.length) return;
    const next = [...colDefs];
    [next[i], next[ni]] = [next[ni], next[i]];
    colDefs = next;
  }

  const activeCols     = $derived(colDefs.filter(c => c.enabled));
  const activeStatCols = $derived(statCols.filter(c => c.enabled));

  // ── Estimated rows ────────────────────────────────────────────────────────
  const estimatedRows = $derived(() => {
    const from  = new Date(fromDate + ':00Z');
    const to    = new Date(toDate   + ':00Z');
    const hours = Math.max(0, (to.getTime() - from.getTime()) / 3_600_000);
    return Math.min(points, Math.round(hours * 12)).toLocaleString('es-CR');
  });

  // ── Download ──────────────────────────────────────────────────────────────
  let error = $state('');

  // ── Stats computation ────────────────────────────────────────────────────
  function computeStats(values: number[]): Record<string, number> {
    if (!values.length) return {};
    const n   = values.length;
    const sum = values.reduce((a, b) => a + b, 0);
    const mean = sum / n;
    const sorted = [...values].sort((a, b) => a - b);
    const variance = values.reduce((a, b) => a + (b - mean) ** 2, 0) / n;
    const stddev = Math.sqrt(variance);
    const min = sorted[0];
    const max = sorted[n - 1];
    const p = (pct: number) => {
      const i = (pct / 100) * (n - 1);
      const lo = Math.floor(i); const hi = Math.ceil(i);
      return sorted[lo] + (sorted[hi] - sorted[lo]) * (i - lo);
    };
    return {
      mean, stddev, min, max, count: n,
      range: max - min,
      cv: mean !== 0 ? (stddev / Math.abs(mean)) * 100 : 0,
      p25: p(25), p75: p(75), p95: p(95),
    };
  }

  async function downloadStats() {
    if (!activeCols.length) return;
    error = '';
    const from    = new Date(fromDate + ':00Z');
    const to      = new Date(toDate   + ':00Z');
    const sensors = activeCols.map(c => box.sensors.find(s => s.id === c.sensorId)!);

    downloading.start(`${box.name} — calculando estadísticas...`);
    try {
      const rows: string[] = [];
      const statHeaders = activeStatCols.map(sc => statColLabel(sc));
      const header = ['sensor', ...statHeaders].join(',');
      rows.push(header);

      for (let i = 0; i < activeCols.length; i++) {
        const col = activeCols[i];
        const s   = sensors[i];
        downloading.setProgress(
          Math.round((i / activeCols.length) * 90),
          `${box.name} — ${normaliseSensorLabel(s.type)} #${s.sensor_number} (${i+1}/${activeCols.length})`
        );
        // Fetch with max resolution for accurate stats
        const data = await fetchReadings(s.id, s.type, from, to, 5000);
        const values = data.map(r => r.value).filter(v => v !== null && !isNaN(v));
        const stats  = computeStats(values);
        const sensorName = colLabel(col);
        const statValues = activeStatCols.map(sc => {
          const v = stats[sc.key];
          return v !== undefined ? v.toFixed(sc.key === 'count' ? 0 : 4) : '';
        });
        rows.push([sensorName, ...statValues].join(','));
      }

      downloading.setProgress(97, `${box.name} — guardando...`);

      // Add metadata footer
      rows.push('');
      rows.push(`# generado: ${new Date().toLocaleString('es-CR', { timeZone: userTz })}`);
      rows.push(`# rango: ${fromDate} → ${toDate} (${userTz})`);
      rows.push(`# resolución: hasta 5000 pts/sensor`);

      const csv  = rows.join('\n');
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `${box.name.toLowerCase().replace(/\s+/g,'_')}_stats_${fromDate.slice(0,10)}_${toDate.slice(0,10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      downloading.finish();
      setTimeout(onclose, 500);
    } catch (e: any) {
      downloading.cancel();
      error = e.message ?? 'Error desconocido';
    }
  }

  async function download() {
    if (downloadMode === 'stats') { await downloadStats(); return; }
    if (!activeCols.length) return;
    error = '';
    const from    = new Date(fromDate + ':00Z');
    const to      = new Date(toDate   + ':00Z');
    const sensors = activeCols.map(c => box.sensors.find(s => s.id === c.sensorId)!);

    downloading.start(`${box.name} — preparando...`);
    try {
      const allReadings: { col: ColDef; data: { bucket: string; value: number }[] }[] = [];
      for (let i = 0; i < activeCols.length; i++) {
        const col = activeCols[i];
        const s   = sensors[i];
        downloading.setProgress(
          Math.round((i / activeCols.length) * 88),
          `${box.name} — ${normaliseSensorLabel(s.type)} #${s.sensor_number} (${i+1}/${activeCols.length})`
        );
        const data = await fetchReadings(s.id, s.type, from, to, points);
        allReadings.push({ col, data });
      }

      downloading.setProgress(92, `${box.name} — construyendo CSV...`);

      const tsMap = new Map<string, Record<string, number | null>>();
      for (const { col, data } of allReadings) {
        for (const r of data) {
          const ts = bucketToLocal(r.bucket);
          if (!tsMap.has(ts)) tsMap.set(ts, {});
          tsMap.get(ts)![col.sensorId] = r.value;
        }
      }

      const header = [
        `timestamp (${userTz}, ${tzOffset})`,
        ...activeCols.map(c => colLabel(c)),
      ].join(',');

      const rows = [...tsMap.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([ts, vals]) => [
          ts,
          ...activeCols.map(c => {
            const v = vals[c.sensorId];
            return v !== undefined && v !== null ? v.toFixed(4) : '';
          }),
        ].join(','));

      downloading.setProgress(98, `${box.name} — guardando...`);

      const csv  = [header, ...rows].join('\n');
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `${box.name.toLowerCase().replace(/\s+/g,'_')}_${fromDate.slice(0,10)}_${toDate.slice(0,10)}_${points}pts.csv`;
      a.click();
      URL.revokeObjectURL(url);
      downloading.finish();
      setTimeout(onclose, 500);
    } catch (e: any) {
      downloading.cancel();
      error = e.message ?? 'Error desconocido';
    }
  }

  function cancel() { downloading.cancel(); onclose(); }
</script>

<div class="overlay" onclick={cancel} role="presentation">
  <div class="panel" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}
       role="dialog" aria-modal="true" tabindex="-1" aria-label="Descargar CSV de {box.name}">

    <!-- Header -->
    <div class="panel-head">
      <div class="head-left">
        <span class="panel-icon">⬇</span>
        <span class="panel-title">CSV — {box.name}</span>
      </div>
      <button class="close-btn" onclick={cancel} aria-label="Cerrar">✕</button>
    </div>

    <!-- ── Modo de descarga ──────────────────────────────────────────────── -->
    <div class="section section--mode">
      <button class="mode-btn" class:active={downloadMode === 'timeseries'}
        onclick={() => downloadMode = 'timeseries'}>
        <span class="mode-icon">📈</span>
        <div class="mode-text">
          <span class="mode-label">serie temporal</span>
          <span class="mode-desc">una fila por timestamp</span>
        </div>
      </button>
      <button class="mode-btn" class:active={downloadMode === 'stats'}
        onclick={() => downloadMode = 'stats'}>
        <span class="mode-icon">📊</span>
        <div class="mode-text">
          <span class="mode-label">resumen estadístico</span>
          <span class="mode-desc">una fila por sensor</span>
        </div>
      </button>
    </div>

    <!-- ── Sección 1: Rango de tiempo ──────────────────────────────────── -->
    <div class="section">
      <div class="section-label">rango de tiempo</div>

      <div class="presets">
        {#each PRESETS as p (p.label)}
          <button class="pbtn" class:active={activePreset === p.label}
            onclick={() => applyPreset(p)}>{p.label}</button>
        {/each}
      </div>

      <div class="date-row">
        <div class="date-field">
          <label for="csv-from" class="field-label">desde</label>
          <input id="csv-from" type="datetime-local" bind:value={fromDate}
            oninput={() => activePreset = null} class="date-input" />
        </div>
        <span class="date-sep">→</span>
        <div class="date-field">
          <label for="csv-to" class="field-label">hasta</label>
          <input id="csv-to" type="datetime-local" bind:value={toDate}
            oninput={() => activePreset = null} class="date-input" />
        </div>
      </div>

      <div class="tz-note">
        <span>🕐</span>
        <span>{userTz} ({tzOffset})</span>
      </div>
    </div>

    <!-- ── Sección 2: Resolución (solo en serie temporal) ─────────────────── -->
    {#if downloadMode === 'timeseries'}
    <div class="section">
      <div class="section-label">
        resolución
        <span class="badge">{RES_LABELS[resStep]} pts/sensor</span>
      </div>
      <div class="res-row">
        <span class="res-tick">100</span>
        <input type="range" min="0" max="4" step="1" bind:value={resStep} class="res-slider" />
        <span class="res-tick">5k</span>
      </div>
    </div>

    {/if}

    <!-- ── Sección 3: Formato de columnas ──────────────────────────────── -->
    <div class="section">
      <div class="section-label">formato de columnas</div>
      <div class="format-toggle">
        <button class="ftbtn" class:active={formatMode === 'simple'}
          onclick={() => formatMode = 'simple'}>
          <span class="ft-label">simple</span>
          <span class="ft-eg">humedad_#1</span>
        </button>
        <button class="ftbtn" class:active={formatMode === 'descriptive'}
          onclick={() => formatMode = 'descriptive'}>
          <span class="ft-label">descriptivo</span>
          <span class="ft-eg">VWC(v/v %)_Sensor1</span>
        </button>
      </div>
    </div>

    <!-- ── Sección 4: Sensores + advanced ──────────────────────────────── -->
    <div class="section">
      <div class="section-label">
        columnas
        <span class="badge">{activeCols.length}/{colDefs.length} activas</span>
        <button class="advanced-toggle" onclick={() => showAdvanced = !showAdvanced}>
          {showAdvanced ? 'básico' : 'avanzado ↓'}
        </button>
      </div>

      {#if !showAdvanced}
        <!-- Vista básica: checkboxes simples -->
        <div class="sensor-list">
          {#each colDefs as col, i (col.sensorId)}
            {@const s = box.sensors.find(x => x.id === col.sensorId)!}
            <label class="sensor-item">
              <input type="checkbox" bind:checked={col.enabled} />
              <span class="sensor-dot" style="background:{sensorColor(s.type)}"></span>
              <span class="sensor-name">
                {normaliseSensorLabel(s.type)}
                <span class="sensor-num">#{s.sensor_number}</span>
              </span>
              <span class="col-preview">{colLabel(col)}</span>
            </label>
          {/each}
        </div>

      {:else}
        <!-- Vista avanzada: sensores -->
        <div class="adv-note">Sensores — reordenár y renombrar columnas</div>
        <div class="adv-list">
          {#each colDefs as col, i (col.sensorId)}
            {@const s = box.sensors.find(x => x.id === col.sensorId)!}
            <div class="adv-row" class:disabled={!col.enabled}>
              <input type="checkbox" bind:checked={col.enabled} class="adv-check" />
              <span class="sensor-dot" style="background:{sensorColor(s.type)}"></span>
              <span class="adv-default">{normaliseSensorLabel(s.type)} #{s.sensor_number}</span>
              <input class="adv-label-input" bind:value={col.customLabel} placeholder={colLabel(col)} />
              <div class="adv-order">
                <button class="ord-btn" onclick={() => moveCol(i, -1)} disabled={i === 0}>↑</button>
                <button class="ord-btn" onclick={() => moveCol(i, 1)} disabled={i === colDefs.length - 1}>↓</button>
              </div>
            </div>
          {/each}
        </div>

        {#if downloadMode === 'stats'}
          <!-- Stats columns selector -->
          <div class="adv-note" style="margin-top:8px">Columnas estadísticas</div>
          <div class="adv-list">
            {#each statCols as sc, i (sc.key)}
              <div class="adv-row" class:disabled={!sc.enabled}>
                <input type="checkbox" bind:checked={sc.enabled} class="adv-check" />
                <span class="adv-default">{sc.label}</span>
                <input class="adv-label-input" bind:value={sc.customLabel} placeholder={sc.label} />
              </div>
            {/each}
          </div>
        {/if}

        <!-- Preview del header -->
        <div class="header-preview">
          <span class="hp-label">header:</span>
          {#if downloadMode === 'timeseries'}
            <span class="hp-val">timestamp, {activeCols.map(c => colLabel(c)).join(', ')}</span>
          {:else}
            <span class="hp-val">sensor, {activeStatCols.map(sc => statColLabel(sc)).join(', ')}</span>
          {/if}
        </div>
      {/if}
    </div>

    <!-- ── Preview + error + botón ─────────────────────────────────────── -->
    <div class="bottom">
      <div class="preview-row">
        <span>{activeCols.length} sensor{activeCols.length !== 1 ? 'es' : ''}</span>
        <span class="sep">·</span>
        {#if downloadMode === 'timeseries'}
          <span>~{estimatedRows()} filas</span>
          <span class="sep">·</span>
          <span>{RES_LABELS[resStep]} pts/sensor</span>
        {:else}
          <span>{activeCols.length} filas</span>
          <span class="sep">·</span>
          <span>{activeStatCols.length} estadísticas</span>
        {/if}
      </div>

      {#if error}
        <div class="error-msg">{error}</div>
      {/if}

      <button class="download-btn" onclick={download} disabled={activeCols.length === 0}>
        {downloadMode === 'timeseries' ? '⬇ Descargar serie temporal' : '⬇ Descargar resumen estadístico'}
      </button>
    </div>

  </div>
</div>

{#if $downloading.active}
  <div class="fullscreen-overlay" role="status" aria-live="polite">
    <div class="spinner-card">
      <div class="spinner"></div>
      <p class="spinner-label">{$downloading.label}</p>
      <div class="progress-track">
        <div class="progress-fill" style="width:{$downloading.progress}%"></div>
      </div>
      <span class="progress-pct">{$downloading.progress}%</span>
      <button class="cancel-btn" onclick={cancel}>cancelar</button>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.28);
    backdrop-filter: blur(2px);
    display: flex; align-items: center; justify-content: center;
  }
  .panel {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 14px;
    width: 460px; max-width: calc(100vw - 32px);
    max-height: 88vh; overflow-y: auto;
    display: flex; flex-direction: column;
  }

  /* Header */
  .panel-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)) calc(12px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    position: sticky; top: 0; background: var(--bg-surface); z-index: 1;
  }
  .head-left { display: flex; align-items: center; gap: 8px; }
  .panel-icon { font-size: calc(14px * var(--font-scale)); color: var(--text-muted); }
  .panel-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; }
  .close-btn { width: 24px; height: 24px; border: none; background: transparent; color: var(--text-muted); font-size: 14px; cursor: pointer; border-radius: 4px; }
  .close-btn:hover { background: var(--interactive-hover); }

  /* Mode selector */
  .section--mode { flex-direction: row; gap: 8px; }
  .mode-btn {
    flex: 1; display: flex; align-items: center; gap: 10px;
    padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 8px;
    background: transparent; cursor: pointer; transition: all .12s; text-align: left;
  }
  .mode-btn:hover { background: var(--interactive-hover); }
  .mode-btn.active { border-color: var(--text-primary); background: var(--bg-elevated); }
  .mode-icon { font-size: 18px; flex-shrink: 0; }
  .mode-text { display: flex; flex-direction: column; gap: 1px; }
  .mode-label { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; }
  .mode-desc  { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); }

  /* Sections */
  .section {
    padding: calc(12px * var(--font-scale)) calc(16px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    display: flex; flex-direction: column; gap: calc(8px * var(--font-scale));
  }
  .section-label {
    font-size: calc(11px * var(--font-scale));
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    letter-spacing: .06em;
    text-transform: uppercase;
    display: flex; align-items: center; gap: 8px;
  }
  .badge {
    font-size: calc(11px * var(--font-scale));
    background: var(--bg-elevated);
    color: var(--text-primary);
    padding: 1px 7px; border-radius: 4px;
    font-weight: 500; letter-spacing: .02em;
  }

  /* Presets */
  .presets { display: flex; gap: 5px; }
  .pbtn {
    padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 5px;
    background: transparent; color: var(--text-secondary);
    font-family: 'DM Mono', monospace; font-size: calc(13px * var(--font-scale));
    cursor: pointer; transition: all .1s;
  }
  .pbtn:hover { background: var(--interactive-hover); }
  .pbtn.active { background: var(--text-primary); color: var(--bg-surface); border-color: transparent; }

  /* Dates */
  .date-row { display: flex; align-items: flex-end; gap: 8px; flex-wrap: wrap; }
  .date-field { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 150px; }
  .field-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .date-input {
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 5px;
    background: var(--bg-elevated); color: var(--text-primary);
    font-family: 'DM Mono', monospace; font-size: calc(13px * var(--font-scale)); outline: none; width: 100%;
  }
  .date-input:focus { border-color: var(--text-primary); }
  .date-sep { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding-bottom: 8px; }
  .tz-note { display: flex; gap: 5px; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }

  /* Resolution */
  .res-row { display: flex; align-items: center; gap: 10px; }
  .res-tick { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; min-width: 24px; }
  .res-slider { flex: 1; }

  /* Format toggle */
  .format-toggle { display: flex; gap: 6px; }
  .ftbtn {
    flex: 1; display: flex; flex-direction: column; align-items: flex-start; gap: 2px;
    padding: calc(8px * var(--font-scale)) calc(10px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 6px;
    background: transparent; cursor: pointer; transition: all .1s; text-align: left;
  }
  .ftbtn:hover { background: var(--interactive-hover); }
  .ftbtn.active { border-color: var(--text-primary); background: var(--bg-elevated); }
  .ft-label { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; }
  .ft-eg { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }

  /* Advanced toggle */
  .advanced-toggle {
    margin-left: auto; font-size: calc(11px * var(--font-scale));
    font-family: 'DM Mono', monospace; color: var(--text-muted);
    border: none; background: transparent; cursor: pointer; padding: 0;
    text-decoration: underline;
  }

  /* Basic sensor list */
  .sensor-list { display: flex; flex-direction: column; gap: 1px; max-height: 160px; overflow-y: auto; }
  .sensor-item {
    display: flex; align-items: center; gap: 8px;
    padding: calc(5px * var(--font-scale)) calc(6px * var(--font-scale));
    border-radius: 5px; cursor: pointer; font-size: calc(13px * var(--font-scale));
    color: var(--text-primary); transition: background .1s;
  }
  .sensor-item:hover { background: var(--interactive-hover); }
  .sensor-item input[type="checkbox"] { width: 13px; height: 13px; cursor: pointer; }
  .sensor-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .sensor-name { flex: 1; }
  .sensor-num { color: var(--text-muted); font-size: calc(12px * var(--font-scale)); margin-left: 3px; }
  .col-preview { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; text-align: right; }

  /* Advanced list */
  .adv-note { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .adv-list { display: flex; flex-direction: column; gap: 4px; max-height: 200px; overflow-y: auto; }
  .adv-row {
    display: flex; align-items: center; gap: 6px;
    padding: calc(5px * var(--font-scale)) calc(4px * var(--font-scale));
    border-radius: 5px; transition: background .1s;
  }
  .adv-row:hover { background: var(--interactive-hover); }
  .adv-row.disabled { opacity: 0.4; }
  .adv-check { width: 13px; height: 13px; flex-shrink: 0; }
  .adv-default { font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); white-space: nowrap; flex-shrink: 0; min-width: 100px; }
  .adv-label-input {
    flex: 1; padding: 2px 6px;
    border: 0.5px solid var(--border-default); border-radius: 4px;
    background: var(--bg-elevated); color: var(--text-primary);
    font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; outline: none;
  }
  .adv-label-input:focus { border-color: var(--text-primary); }
  .adv-order { display: flex; flex-direction: column; gap: 1px; flex-shrink: 0; }
  .ord-btn { width: 16px; height: 14px; border: none; background: none; cursor: pointer; font-size: 9px; color: var(--text-muted); padding: 0; line-height: 1; }
  .ord-btn:hover:not(:disabled) { color: var(--text-primary); }
  .ord-btn:disabled { opacity: 0.2; }

  .header-preview {
    display: flex; gap: 6px; align-items: flex-start;
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale));
    background: var(--bg-inset); border-radius: 5px;
    font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace;
  }
  .hp-label { color: var(--text-muted); flex-shrink: 0; }
  .hp-val { color: var(--text-secondary); word-break: break-all; }

  /* Bottom */
  .bottom { padding: calc(12px * var(--font-scale)) calc(16px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); }
  .preview-row {
    display: flex; align-items: center; gap: 6px;
    padding: calc(6px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--bg-elevated); border: 0.5px solid var(--border-subtle);
    border-radius: 5px; font-size: calc(12px * var(--font-scale));
    color: var(--text-muted); font-family: 'DM Mono', monospace;
  }
  .sep { color: var(--border-default); }
  .error-msg {
    padding: calc(6px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--error-bg); border: 0.5px solid #F09595;
    border-radius: 5px; font-size: calc(13px * var(--font-scale)); color: #A32D2D;
  }
  .download-btn {
    padding: calc(10px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px;
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale));
    font-weight: 500; letter-spacing: .04em; cursor: pointer; transition: opacity .15s;
  }
  .download-btn:hover:not(:disabled) { opacity: .85; }
  .download-btn:disabled { opacity: .4; cursor: not-allowed; }

  /* Fullscreen overlay */
  .fullscreen-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0,0,0,0.6); backdrop-filter: blur(5px);
    display: flex; align-items: center; justify-content: center;
  }
  .spinner-card {
    display: flex; flex-direction: column; align-items: center; gap: calc(14px * var(--font-scale));
    padding: calc(32px * var(--font-scale)) calc(40px * var(--font-scale));
    background: var(--bg-surface); border: 0.5px solid var(--border-default);
    border-radius: 14px; min-width: 280px; max-width: calc(100vw - 48px);
  }
  .spinner { width: 38px; height: 38px; border: 3px solid var(--border-subtle); border-top-color: var(--text-primary); border-radius: 50%; animation: spin .75s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .spinner-label { font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); text-align: center; max-width: 240px; line-height: 1.6; margin: 0; }
  .progress-track { width: 100%; height: 3px; background: var(--border-subtle); border-radius: 2px; overflow: hidden; }
  .progress-fill { height: 100%; background: var(--text-primary); border-radius: 2px; transition: width .25s ease; }
  .progress-pct { font-size: calc(13px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); }
  .cancel-btn { padding: calc(5px * var(--font-scale)) calc(18px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 5px; background: transparent; color: var(--text-muted); font-family: 'DM Mono', monospace; font-size: calc(13px * var(--font-scale)); cursor: pointer; }
  .cancel-btn:hover { background: var(--interactive-hover); }

  @media (max-width: 640px) {
    .panel { width: calc(100vw - 20px); max-height: 85vh; }
    .date-row { flex-direction: column; }
    .date-sep { display: none; }
    .format-toggle { flex-direction: column; }
  }
</style>