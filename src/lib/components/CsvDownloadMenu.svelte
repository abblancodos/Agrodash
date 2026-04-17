<script lang="ts">
  import { fetchReadings, normaliseSensorLabel, sensorColor, type Box } from '$lib/api';
  import { downloading } from '$lib/stores/downloading';

  interface Props {
    box: Box;
    onclose: () => void;
  }

  let { box, onclose }: Props = $props();

  // ── Rango de tiempo ───────────────────────────────────────────────────────

  const PRESETS = [
    { label: '1 día',    hours: 24  },
    { label: '1 semana', hours: 168 },
    { label: '1 mes',    hours: 720 },
  ];

  let activePreset = $state<string | null>('1 día');

  function fmt(d: Date): string {
    return d.toISOString().slice(0, 16);
  }

  let toDate   = $state(fmt(new Date()));
  let fromDate = $state(fmt(new Date(Date.now() - 24 * 3_600_000)));

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    const now = new Date();
    toDate   = fmt(now);
    fromDate = fmt(new Date(now.getTime() - p.hours * 3_600_000));
  }

  // ── Resolución ────────────────────────────────────────────────────────────

  const RESOLUTION_STEPS  = [100, 300, 1000, 3000, 5000];
  const RESOLUTION_LABELS = ['100', '300', '1 000', '3 000', '5 000 (máx)'];
  let resolutionStep = $state(2); // default: 1000 puntos

  const points      = $derived(RESOLUTION_STEPS[resolutionStep]);
  const pointsLabel = $derived(RESOLUTION_LABELS[resolutionStep]);

  const resHint = $derived(() => {
    if (resolutionStep <= 1) return 'Rápido — bueno para vista general';
    if (resolutionStep === 2) return 'Balanceado — resolución media';
    if (resolutionStep === 3) return 'Alta resolución — puede tardar unos segundos';
    return 'Máxima resolución — todos los datos disponibles';
  });

  // ── Selección de sensores ─────────────────────────────────────────────────

  let selectedIds = $state<Set<string>>(new Set(box.sensors.map(s => s.id)));

  function toggleSensor(id: string) {
    const next = new Set(selectedIds);
    next.has(id) ? next.delete(id) : next.add(id);
    selectedIds = next;
  }

  function toggleAll() {
    selectedIds = selectedIds.size === box.sensors.length
      ? new Set()
      : new Set(box.sensors.map(s => s.id));
  }

  // ── Estimación de filas ───────────────────────────────────────────────────

  const estimatedRows = $derived(() => {
    const from = new Date(fromDate + ':00Z');
    const to   = new Date(toDate   + ':00Z');
    const hours = Math.max(0, (to.getTime() - from.getTime()) / 3_600_000);
    // asumimos 1 dato cada 5min como promedio = 12/h, acotado por points
    return Math.min(points, Math.round(hours * 12)).toLocaleString('es-CR');
  });

  // ── Descarga ──────────────────────────────────────────────────────────────

  let error = $state('');

  async function download() {
    if (!selectedIds.size) return;
    error = '';

    const from    = new Date(fromDate + ':00Z');
    const to      = new Date(toDate   + ':00Z');
    const sensors = box.sensors.filter(s => selectedIds.has(s.id));

    downloading.start(`${box.name} — preparando...`);

    try {
      const allReadings: { sensor: typeof sensors[0]; data: { bucket: string; value: number }[] }[] = [];

      for (let i = 0; i < sensors.length; i++) {
        const s = sensors[i];
        downloading.setProgress(
          Math.round((i / sensors.length) * 88),
          `${box.name} — ${normaliseSensorLabel(s.type)} #${s.sensor_number} (${i + 1}/${sensors.length})`
        );
        const data = await fetchReadings(s.id, s.type, from, to, points);
        allReadings.push({ sensor: s, data });
      }

      downloading.setProgress(92, `${box.name} — construyendo CSV...`);

      // Mapa timestamp → valores por sensor
      const tsMap = new Map<string, Record<string, number | null>>();
      for (const { sensor, data } of allReadings) {
        for (const r of data) {
          const ts = r.bucket.slice(0, 16).replace('T', ' ');
          if (!tsMap.has(ts)) tsMap.set(ts, {});
          tsMap.get(ts)![sensor.id] = r.value;
        }
      }

      const header = [
        'timestamp',
        ...sensors.map(s => `${normaliseSensorLabel(s.type)}_#${s.sensor_number}`),
      ].join(',');

      const rows = [...tsMap.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([ts, vals]) => [
          ts,
          ...sensors.map(s => {
            const v = vals[s.id];
            return v !== undefined && v !== null ? v.toFixed(4) : '';
          }),
        ].join(','));

      downloading.setProgress(98, `${box.name} — guardando archivo...`);

      const csv  = [header, ...rows].join('\n');
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `${box.name.toLowerCase().replace(/\s+/g, '_')}_${fromDate.slice(0, 10)}_${toDate.slice(0, 10)}_${points}pts.csv`;
      a.click();
      URL.revokeObjectURL(url);

      downloading.finish();
      setTimeout(onclose, 500);

    } catch (e: any) {
      downloading.cancel();
      error = e.message ?? 'Error desconocido';
    }
  }

  function cancel() {
    downloading.cancel();
    onclose();
  }
</script>

<!-- Panel semi-transparente -->
<div class="overlay" onclick={cancel} role="presentation">
  <div class="panel" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}
       role="dialog" aria-modal="true" tabindex="-1"
       aria-label="Descargar CSV de {box.name}">

    <div class="panel-head">
      <span class="panel-title">Descargar CSV — {box.name}</span>
      <button class="close-btn" onclick={cancel} aria-label="Cerrar">✕</button>
    </div>

    <!-- Presets de tiempo -->
    <div class="section-label">rango de tiempo</div>
    <div class="presets">
      {#each PRESETS as p}
        <button class="pbtn" class:active={activePreset === p.label}
          onclick={() => applyPreset(p)}>{p.label}</button>
      {/each}
    </div>

    <div class="date-row">
      <div class="date-field">
        <label class="field-label" for="csv-from">desde</label>
        <input id="csv-from" type="datetime-local" bind:value={fromDate}
          oninput={() => activePreset = null} class="date-input" />
      </div>
      <span class="date-sep">→</span>
      <div class="date-field">
        <label class="field-label" for="csv-to">hasta</label>
        <input id="csv-to" type="datetime-local" bind:value={toDate}
          oninput={() => activePreset = null} class="date-input" />
      </div>
    </div>

    <!-- Resolución -->
    <div class="section-label">
      resolución
      <span class="res-badge">{pointsLabel} pts/sensor</span>
    </div>
    <div class="res-row">
      <span class="res-tick">100</span>
      <input type="range" min="0" max="4" step="1" bind:value={resolutionStep} class="res-slider" />
      <span class="res-tick">5k</span>
    </div>
    <div class="res-hint">{resHint()}</div>

    <!-- Sensores -->
    <div class="section-label">
      sensores ({selectedIds.size}/{box.sensors.length})
      <button class="toggle-all" onclick={toggleAll}>
        {selectedIds.size === box.sensors.length ? 'quitar todos' : 'todos'}
      </button>
    </div>
    <div class="sensor-list">
      {#each box.sensors as sensor (sensor.id)}
        <label class="sensor-item">
          <input type="checkbox"
            checked={selectedIds.has(sensor.id)}
            onchange={() => toggleSensor(sensor.id)} />
          <span class="sensor-dot" style="background:{sensorColor(sensor.type)}"></span>
          <span class="sensor-name">
            {normaliseSensorLabel(sensor.type)}
            <span class="sensor-num">#{sensor.sensor_number}</span>
          </span>
        </label>
      {/each}
    </div>

    <!-- Preview -->
    <div class="preview">
      <span>{selectedIds.size} sensor{selectedIds.size !== 1 ? 'es' : ''}</span>
      <span class="sep">·</span>
      <span>~{estimatedRows()} filas</span>
      <span class="sep">·</span>
      <span>{pointsLabel} pts/sensor</span>
    </div>

    {#if error}
      <div class="error-msg">{error}</div>
    {/if}

    <button class="download-btn" onclick={download} disabled={selectedIds.size === 0}>
      Descargar CSV
    </button>
  </div>
</div>

<!-- Overlay pantalla completa durante la descarga -->
{#if $downloading.active}
  <div class="fullscreen-overlay" role="status" aria-live="polite" aria-label={$downloading.label}>
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
  /* ── Panel ───────────────────────────────────────────────────────────── */
  .overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.3);
    backdrop-filter: blur(2px);
    -webkit-backdrop-filter: blur(2px);
    display: flex; align-items: center; justify-content: center;
  }
  .panel {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 12px;
    width: 440px; max-width: calc(100vw - 32px);
    max-height: 88vh; overflow-y: auto;
    display: flex; flex-direction: column;
  }
  .panel-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)) calc(12px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    position: sticky; top: 0;
    background: var(--bg-surface); z-index: 1;
  }
  .panel-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); letter-spacing: .04em; font-family: 'DM Mono', monospace; }
  .close-btn {
    width: 24px; height: 24px; border: none; background: transparent;
    color: var(--text-muted); font-size: calc(14px * var(--font-scale)); cursor: pointer;
    border-radius: 4px; display: flex; align-items: center; justify-content: center;
    transition: background .1s;
  }
  .close-btn:hover { background: var(--interactive-hover); }

  .section-label {
    padding: 10px 16px 5px; font-size: calc(14px * var(--font-scale)); letter-spacing: .08em;
    color: var(--text-muted); display: flex; align-items: center; gap: calc(8px * var(--font-scale));
    font-family: 'DM Mono', monospace;
  }
  .toggle-all {
    font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted);
    border: none; background: transparent; cursor: pointer; padding: 0;
    text-decoration: underline; letter-spacing: .04em;
  }

  /* ── Presets ─────────────────────────────────────────────────────────── */
  .presets { display: flex; gap: calc(6px * var(--font-scale)); padding: 0 16px 10px; }
  .pbtn {
    padding: calc(5px * var(--font-scale)) calc(13px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 5px;
    background: transparent; color: var(--text-secondary);
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale));
    cursor: pointer; letter-spacing: .04em; transition: all .12s;
  }
  .pbtn:hover { background: var(--interactive-hover); }
  .pbtn.active { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }

  /* ── Fechas ──────────────────────────────────────────────────────────── */
  .date-row { display: flex; align-items: flex-end; gap: calc(8px * var(--font-scale)); padding: 0 16px 12px; flex-wrap: wrap; }
  .date-field { display: flex; flex-direction: column; gap: calc(3px * var(--font-scale)); flex: 1; min-width: 160px; }
  .field-label { font-size: calc(14px * var(--font-scale)); letter-spacing: .07em; color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .date-input {
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale)); width: 100%;
    border: 0.5px solid var(--border-default); border-radius: 5px;
    background: var(--bg-elevated); color: var(--text-primary);
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale)); outline: none;
  }
  .date-input:focus { border-color: var(--accent-border); }
  .date-sep { color: var(--text-muted); font-size: calc(14px * var(--font-scale)); padding-bottom: 7px; }

  /* ── Resolución ──────────────────────────────────────────────────────── */
  .res-badge {
    font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary);
    background: var(--bg-elevated);
    padding: calc(2px * var(--font-scale)) calc(7px * var(--font-scale)); border-radius: 4px; letter-spacing: .02em;
  }
  .res-row { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); padding: calc(2px * var(--font-scale)) calc(16px * var(--font-scale)) calc(4px * var(--font-scale)); }
  .res-tick { font-size: calc(14px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; min-width: 22px; }
  .res-slider { flex: 1; }
  .res-hint { padding: 0 16px 12px; font-size: calc(14px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; letter-spacing: .04em; }

  /* ── Sensores ────────────────────────────────────────────────────────── */
  .sensor-list { display: flex; flex-direction: column; gap: calc(1px * var(--font-scale)); padding: 0 16px 12px; max-height: 150px; overflow-y: auto; }
  .sensor-item {
    display: flex; align-items: center; gap: calc(8px * var(--font-scale)); padding: calc(5px * var(--font-scale)) calc(8px * var(--font-scale));
    border-radius: 5px; cursor: pointer; font-size: calc(14px * var(--font-scale));
    color: var(--text-primary); transition: background .1s;
  }
  .sensor-item:hover { background: var(--interactive-hover); }
  .sensor-item input[type="checkbox"] { width: 13px; height: 13px; cursor: pointer; accent-color: var(--accent-text); }
  .sensor-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .sensor-name { flex: 1; }
  .sensor-num { color: var(--text-muted); font-size: calc(14px * var(--font-scale)); margin-left: 4px; }

  /* ── Preview ─────────────────────────────────────────────────────────── */
  .preview {
    margin: 0 16px 10px; padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 5px; font-size: calc(14px * var(--font-scale)); color: var(--text-muted);
    letter-spacing: .04em; font-family: 'DM Mono', monospace;
    display: flex; align-items: center; gap: calc(6px * var(--font-scale));
  }
  .sep { color: var(--border-default, #ccc); }

  .error-msg {
    margin: 0 16px 8px; padding: calc(6px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--error-bg); border: 0.5px solid #F09595;
    border-radius: 5px; font-size: calc(14px * var(--font-scale)); color: #A32D2D;
  }

  .download-btn {
    margin: 0 16px 16px; padding: calc(9px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px;
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale)); font-weight: 500;
    letter-spacing: .06em; cursor: pointer; transition: opacity .15s;
  }
  .download-btn:hover:not(:disabled) { opacity: .85; }
  .download-btn:disabled { opacity: .4; cursor: not-allowed; }

  /* ── Fullscreen overlay ──────────────────────────────────────────────── */
  .fullscreen-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(5px);
    -webkit-backdrop-filter: blur(5px);
    display: flex; align-items: center; justify-content: center;
    pointer-events: all;
  }

  .spinner-card {
    display: flex; flex-direction: column; align-items: center; gap: calc(14px * var(--font-scale));
    padding: calc(32px * var(--font-scale)) calc(40px * var(--font-scale));
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 14px;
    min-width: 280px; max-width: calc(100vw - 48px);
  }

  .spinner {
    width: 38px; height: 38px;
    border: 3px solid var(--border-subtle);
    border-top-color: var(--text-primary);
    border-radius: 50%;
    animation: spin .75s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .spinner-label {
    font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace;
    color: var(--text-secondary); letter-spacing: .04em;
    text-align: center; max-width: 240px; line-height: 1.6;
    margin: 0;
  }

  .progress-track {
    width: 100%; height: 3px;
    background: var(--border-subtle);
    border-radius: 2px; overflow: hidden;
  }
  .progress-fill {
    height: 100%; background: var(--text-primary);
    border-radius: 2px; transition: width .25s ease;
  }

  .progress-pct {
    font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace;
    color: var(--text-muted); letter-spacing: .06em;
  }

  .cancel-btn {
    margin-top: 2px; padding: calc(5px * var(--font-scale)) calc(18px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 5px; background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale));
    cursor: pointer; letter-spacing: .06em; transition: all .12s;
  }
  .cancel-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
</style>