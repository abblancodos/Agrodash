<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { Box } from '$lib/api';
  import type { SensorStat, SensorCorrelation } from '$lib/api';
  import { fetchReadings, normaliseSensorLabel, sensorColor } from '$lib/api';
  import { relTime, relTimeClass, anomalyClass, formatValue } from '$lib/utils';
  import SensorChart from './SensorChart.svelte';
  import CsvDownloadMenu from './CsvDownloadMenu.svelte';

  // ── Props ──────────────────────────────────────────────────────────────────

  interface Props {
    box: Box;
    /** Stats pre-calculadas del worker — ya filtradas para esta caja */
    stats: SensorStat[];
    /** Correlaciones de esta caja (|r| >= threshold) */
    correlations: SensorCorrelation[];
    /** Rango global de tiempo (default inicial) */
    from: Date;
    to: Date;
    live: boolean;
  }

  let { box, stats, correlations, from, to, live }: Props = $props();

  // ── Estado local de tiempo por caja ───────────────────────────────────────

  const PRESETS = [
    { label: '1h',  hours: 1  },
    { label: '6h',  hours: 6  },
    { label: '24h', hours: 24 },
    { label: '7d',  hours: 168 },
    { label: '30d', hours: 720 },
  ];

  let activePreset = $state('24h');
  let localFrom    = $derived(from);
  let localTo      = $derived(to);

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    localTo      = new Date();
    localFrom    = new Date(localTo.getTime() - p.hours * 3_600_000);
  }

  // ── Lógica de sensores ─────────────────────────────────────────────────────

  /**
   * IDs de sensores que están correlacionados con OTRO sensor de la misma caja
   * en la misma variable. Estos se agrupan al final.
   */
  const correlatedSensorIds = $derived<Set<string>>(() => {
    const ids = new Set<string>();
    for (const c of correlations) {
      if (c.box_id === box.id) {
        ids.add(c.sensor_id_a);
        ids.add(c.sensor_id_b);
      }
    }
    return ids;
  });

  /**
   * Sensores de esta caja con sus stats, ordenados por anomaly_score DESC.
   * Se divide en dos grupos: los que tienen comportamiento único (primero)
   * y los que están correlacionados entre sí (al final, colapsados).
   */
  const sensorStats = $derived(() =>
    stats
      .filter(s => s.box_id === box.id)
      .sort((a, b) => (b.anomaly_score ?? -1) - (a.anomaly_score ?? -1))
  );

  const uniqueSensors = $derived(() =>
    sensorStats().filter(s => !correlatedSensorIds().has(s.sensor_id))
  );

  const corrSensors = $derived(() =>
    sensorStats().filter(s => correlatedSensorIds().has(s.sensor_id))
  );

  /**
   * Para la sección correlacionada, agrupa por (sensor_type) y toma rango.
   * Muestra una fila por tipo, con min–max de los valores actuales.
   */
  const corrGroups = $derived(() => {
    const groups = new Map<string, { type: string; sensors: SensorStat[]; pearsonR: number }>();
    for (const c of correlations) {
      if (c.box_id !== box.id) continue;
      const key = c.sensor_type;
      if (!groups.has(key)) {
        const sensorList = corrSensors().filter(
          s => s.sensor_type.toLowerCase() === key
        );
        if (sensorList.length) {
          groups.set(key, { type: key, sensors: sensorList, pearsonR: c.pearson_r });
        }
      }
    }
    return Array.from(groups.values());
  });

  // ── Anomaly score de la caja (peor sensor) ────────────────────────────────
  const boxScore = $derived(() =>
    Math.max(...sensorStats().map(s => s.anomaly_score ?? 0))
  );
  const boxAnomalyClass = $derived(() => anomalyClass(boxScore()));

  // ── Último dato de la caja (más reciente entre todos los sensores) ─────────
  const boxLastSeen = $derived(() => {
    const dates = sensorStats()
      .map(s => s.last_seen_at)
      .filter((d): d is string => d !== null);
    if (!dates.length) return null;
    return dates.reduce((a, b) => (a > b ? a : b));
  });

  // ── Live polling ──────────────────────────────────────────────────────────
  let liveInterval: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (live) {
      liveInterval = setInterval(() => { localTo = new Date(); }, 15_000);
    } else {
      if (liveInterval) clearInterval(liveInterval);
      liveInterval = null;
    }
    return () => { if (liveInterval) clearInterval(liveInterval); };
  });

  // ── Chart expand ─────────────────────────────────────────────────────────
  let expandedSensorId  = $state<string | null>(null);
  let expandedCorrType  = $state<string | null>(null);
  let csvOpen = $state(false);

  function toggleExpand(sensorId: string) {
    expandedSensorId = expandedSensorId === sensorId ? null : sensorId;
  }
</script>

<article class="box-card" class:has-warn={boxAnomalyClass() === 'warn'}
                          class:has-alert={boxAnomalyClass() === 'alert'}>

  <!-- Header ────────────────────────────────────────────────────────────── -->
  <header class="card-head">
    <div class="card-head__info">
      <div class="card-head__title">
        {box.name}
        {#if boxAnomalyClass() !== 'normal'}
          <span class="badge badge-{boxAnomalyClass()}">
            {boxScore().toFixed(1)}σ
          </span>
        {/if}
      </div>
      <div class="card-head__sub">
        {box.sensors.length} sensores
        · <span class="ago {relTimeClass(boxLastSeen())}">{relTime(boxLastSeen())}</span>
      </div>
    </div>

    <!-- Controles agrupados: CSV + presets de tiempo -->
    <div class="card-head__controls">
      <button class="csv-btn" onclick={() => csvOpen = true} title="Descargar CSV">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="12" height="12"><path d="M2 10v2h10v-2M7 2v7M4 6l3 3 3-3"/></svg>
        CSV
      </button>
      <div class="card-head__presets">
        {#each PRESETS as p}
          <button class="pbtn" class:active={activePreset === p.label}
            onclick={() => applyPreset(p)}>
            {p.label}
          </button>
        {/each}
      </div>
    </div>
  </header>

  <!-- Cabecera de columnas -->
  <div class="sensor-cols-head">
    <span>sensor</span>
    <span>variable</span>
    <span>tendencia</span>
    <span class="align-right">valor</span>
    <span class="align-right">score</span>
    <span class="align-right">último dato</span>
  </div>

  <!-- Sensores únicos (ordenados por anomaly score) ──────────────────────── -->
  {#each uniqueSensors() as stat (stat.sensor_id)}
    {@const ac = anomalyClass(stat.anomaly_score)}
    {@const color = sensorColor(stat.sensor_type)}

    <div class="sensor-row" class:is-warn={ac === 'warn'} class:is-alert={ac === 'alert'}
         role="button" tabindex="0"
         onclick={() => toggleExpand(stat.sensor_id)}
         onkeydown={(e) => e.key === 'Enter' && toggleExpand(stat.sensor_id)}>
      <span class="s-num">#{stat.sensor_number}</span>
      <span class="s-type">{normaliseSensorLabel(stat.sensor_type)}</span>
      <div class="s-spark">
        <SensorChart
          sensorId={stat.sensor_id}
          sensorType={stat.sensor_type}
          from={localFrom}
          to={localTo}
          points={50}
          spark={true}
          {color}
        />
      </div>
      <span class="s-val align-right" class:warn={ac !== 'normal'}>
        {formatValue(stat.last_value, stat.sensor_type)}
      </span>
      <span class="align-right">
        {#if stat.anomaly_score !== null}
          <span class="badge badge-{ac}">{stat.anomaly_score.toFixed(1)}σ</span>
        {:else}
          <span class="badge badge-muted">—</span>
        {/if}
      </span>
      <span class="align-right ago {relTimeClass(stat.last_seen_at)}">
        {relTime(stat.last_seen_at)}
      </span>
      <!-- Fila compacta solo visible en mobile -->
      <div class="s-mobile">
        <span class="s-mobile__name" class:warn={ac !== 'normal'}>
          {normaliseSensorLabel(stat.sensor_type)}
          <span class="s-mobile__num">#{stat.sensor_number}</span>
        </span>
        <span class="s-mobile__meta">
          <span class="s-mobile__val" class:warn={ac !== 'normal'}>
            {formatValue(stat.last_value, stat.sensor_type)}
          </span>
          {#if stat.anomaly_score !== null}
            <span class="badge badge-{ac}">{stat.anomaly_score.toFixed(1)}σ</span>
          {:else}
            <span class="s-mobile__dash">—</span>
          {/if}
          <span class="ago {relTimeClass(stat.last_seen_at)}">
            {relTime(stat.last_seen_at)}
          </span>
        </span>
      </div>
    </div>

    <!-- Gráfica expandida al hacer click -->
    {#if expandedSensorId === stat.sensor_id}
      <div class="sensor-expanded">
        <SensorChart
          sensorId={stat.sensor_id}
          sensorType={stat.sensor_type}
          from={localFrom}
          to={localTo}
          points={300}
          spark={false}
          {color}
        />
      </div>
    {/if}
  {/each}

  <!-- Sección correlacionada ──────────────────────────────────────────────── -->
  {#if corrGroups().length > 0}
    <div class="corr-label">
      variables correlacionadas entre sensores de esta caja (r ≥ 0.85)
    </div>

    {#each corrGroups() as group}
      {@const vals = group.sensors.map(s => s.last_value).filter((v): v is number => v !== null)}
      {@const minVal = vals.length ? Math.min(...vals) : null}
      {@const maxVal = vals.length ? Math.max(...vals) : null}
      {@const lastSeen = group.sensors
        .map(s => s.last_seen_at)
        .filter((d): d is string => d !== null)
        .reduce((a, b) => (a > b ? a : b), '')}
      {@const color = sensorColor(group.type)}
      {@const isPerfect = group.pearsonR >= 0.999}
      {@const corrExpanded = expandedCorrType === group.type}

      <div class="sensor-row corr-group"
           role="button" tabindex="0"
           onclick={() => expandedCorrType = corrExpanded ? null : group.type}
           onkeydown={(e) => e.key === 'Enter' && (expandedCorrType = corrExpanded ? null : group.type)}>
        <span class="s-num" style="color: var(--text-muted)">
          <span class="ct-chevron" class:open={corrExpanded}>▶</span>
        </span>
        <span class="s-type">{normaliseSensorLabel(group.type)}</span>
        <div class="s-spark">
          <SensorChart
            sensorId={group.sensors[0].sensor_id}
            sensorType={group.type}
            from={localFrom}
            to={localTo}
            points={50}
            spark={true}
            {color}
          />
        </div>
        <span class="s-val align-right">
          {#if minVal !== null && maxVal !== null && minVal !== maxVal}
            {formatValue(minVal, group.type)}–{formatValue(maxVal, group.type)}
          {:else if minVal !== null}
            {formatValue(minVal, group.type)}
          {:else}—{/if}
        </span>
        <span class="align-right">
          <span class="badge badge-info">r={group.pearsonR.toFixed(2)}</span>
        </span>
        <span class="align-right ago {relTimeClass(lastSeen || null)}">
          {relTime(lastSeen || null)}
        </span>
        <!-- Mobile layout para correlacionadas -->
        <div class="s-mobile">
          <span class="s-mobile__name">
            <span class="ct-chevron" class:open={corrExpanded}>▶</span>
            {normaliseSensorLabel(group.type)}
            <span class="s-mobile__num">todos</span>
          </span>
          <span class="s-mobile__meta">
            <span class="badge badge-info">r={group.pearsonR.toFixed(2)}</span>
            {#if minVal !== null && maxVal !== null && minVal !== maxVal}
              <span>{formatValue(minVal, group.type)}–{formatValue(maxVal, group.type)}</span>
            {:else if minVal !== null}
              <span>{formatValue(minVal, group.type)}</span>
            {/if}
            <span class="ago {relTimeClass(lastSeen || null)}">{relTime(lastSeen || null)}</span>
          </span>
        </div>
      </div>

      {#if corrExpanded}
        {#if isPerfect}
          <!-- r≈1: una sola gráfica -->
          <div class="sensor-expanded">
            <SensorChart
              sensorId={group.sensors[0].sensor_id}
              sensorType={group.type}
              from={localFrom}
              to={localTo}
              points={300}
              spark={false}
              {color}
            />
          </div>
        {:else}
          <!-- r 0.85–0.99: mínimo y máximo como filas separadas -->
          {@const sorted = [...group.sensors].sort((a, b) => (a.last_value ?? 0) - (b.last_value ?? 0))}
          {#each [sorted[0], sorted[sorted.length - 1]] as s, i}
            <div class="sensor-row corr-sub-row">
              <span class="s-num" style="color:var(--text-muted)">#{s.sensor_number}</span>
              <span class="s-type">{i === 0 ? 'mín' : 'máx'}</span>
              <div class="s-spark">
                <SensorChart sensorId={s.sensor_id} sensorType={group.type}
                  from={localFrom} to={localTo} points={50} spark={true} {color} />
              </div>
              <span class="s-val align-right">{formatValue(s.last_value, group.type)}</span>
              <span class="align-right"></span>
              <span class="align-right ago {relTimeClass(s.last_seen_at)}">{relTime(s.last_seen_at)}</span>
              <!-- Mobile -->
              <div class="s-mobile">
                <span class="s-mobile__name">
                  {normaliseSensorLabel(group.type)}
                  <span class="s-mobile__num">{i === 0 ? 'mín' : 'máx'} · #{s.sensor_number}</span>
                </span>
                <span class="s-mobile__meta">
                  <span class="s-mobile__val">{formatValue(s.last_value, group.type)}</span>
                  <span class="ago {relTimeClass(s.last_seen_at)}">{relTime(s.last_seen_at)}</span>
                </span>
              </div>
            </div>
            <div class="sensor-expanded">
              <SensorChart sensorId={s.sensor_id} sensorType={group.type}
                from={localFrom} to={localTo} points={300} spark={false} {color} />
            </div>
          {/each}
        {/if}
      {/if}
    {/each}
  {/if}

{#if csvOpen}
    <CsvDownloadMenu {box} onclose={() => csvOpen = false} />
  {/if}

</article>

<style>
  .box-card {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 10px;
    overflow: hidden;
  }
  .box-card.has-warn  { border-color: rgba(186,117,23,0.5); }
  .box-card.has-alert { border-color: rgba(176,48,48,0.4); }

  /* Header */
  .card-head {
    display: flex;
    align-items: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .card-head__info { flex: 1; min-width: 0; }
  .card-head__controls {
    display: flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
    flex-shrink: 0;
  }
  .card-head__title {
    font-size: calc(14px * var(--font-scale));
    font-weight: 500;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
  }
  .card-head__sub {
    font-size: calc(14px * var(--font-scale));
    color: var(--text-muted);
    margin-top: 2px;
  }
  .card-head__presets { display: flex; gap: calc(3px * var(--font-scale)); }

  /* Preset buttons */
  .pbtn {
    padding: calc(3px * var(--font-scale)) calc(7px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-family: 'DM Mono', monospace;
    font-size: calc(14px * var(--font-scale));
    cursor: pointer;
    letter-spacing: .04em;
    transition: all .12s;
  }
  .pbtn:hover  { background: var(--interactive-hover); }
  .pbtn.active { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }

  /* Columnas */
  .sensor-cols-head,
  .sensor-row {
    display: grid;
    grid-template-columns: 56px 120px 1fr 90px 76px 100px;
    gap: calc(14px * var(--font-scale));
    align-items: center;
    padding: calc(8px * var(--font-scale)) calc(16px * var(--font-scale));
    font-size: calc(14px * var(--font-scale));
  }
  .sensor-cols-head {
    background: var(--bg-inset);
    border-bottom: 0.5px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: calc(14px * var(--font-scale));
    letter-spacing: .07em;
  }
  .sensor-row {
    border-bottom: 0.5px solid var(--border-subtle);
    cursor: pointer;
    transition: background .1s;
  }
  .sensor-row:last-child { border-bottom: none; }
  .sensor-row:hover      { background: var(--interactive-hover); }
  .sensor-row.is-warn    { background: rgba(186,117,23,0.07); }
  .sensor-row.is-alert   { background: rgba(176,48,48,0.07); }
  .sensor-row.corr-sub-row { background: var(--bg-inset); }
  .sensor-row.corr-group {
    background: var(--bg-elevated);
    cursor: default;
  }
  .sensor-row.corr-group:hover { background: var(--bg-elevated); }

  .s-num  { font-size: calc(14px * var(--font-scale)); color: var(--text-muted); }
  .s-type { font-size: calc(14px * var(--font-scale)); color: var(--text-secondary); }
  .s-spark { height: 28px; }
  .s-val  { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .s-val.warn { color: #e8a838; }
  .align-right { text-align: right; }

  /* Tiempo relativo */
  .ago        { font-size: calc(14px * var(--font-scale)); }
  .ago.fresh  { color: var(--live-color); }
  .ago.recent { color: var(--text-muted); }
  .ago.stale  { color: #e8a838; }
  .ago.dead   { color: var(--error-color); }

  /* Correlación label */
  .corr-label {
    font-size: calc(14px * var(--font-scale));
    color: var(--text-muted);
    letter-spacing: .06em;
    padding: calc(5px * var(--font-scale)) calc(14px * var(--font-scale)) calc(3px * var(--font-scale));
    background: var(--bg-elevated);
    border-top: 0.5px solid var(--border-subtle);
    border-bottom: 0.5px solid var(--border-subtle);
  }

  /* Gráfica expandida */
  .sensor-expanded {
    padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    background: var(--bg-surface);
  }

  .csv-btn {
    display: flex;
    align-items: center;
    gap: calc(4px * var(--font-scale));
    padding: calc(3px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    font-size: calc(14px * var(--font-scale));
    letter-spacing: .06em;
    cursor: pointer;
    transition: all .12s;
  }
  .csv-btn:hover {
    background: var(--interactive-hover);
    color: var(--text-secondary);
  }


  .ct-chevron { font-size: calc(8px * var(--font-scale)); color: var(--text-muted); transition: transform .15s; display: inline-block; }
  .ct-chevron.open { transform: rotate(90deg); }


  /* Mobile row — hidden on desktop */
  .s-mobile { display: none; }

  /* Badges */
  .badge {
    font-size: calc(14px * var(--font-scale));
    padding: calc(2px * var(--font-scale)) calc(6px * var(--font-scale));
    border-radius: 4px;
    letter-spacing: .04em;
    font-weight: 500;
    white-space: nowrap;
  }
  .badge-normal { background: var(--bg-elevated); color: var(--text-muted); }
  .badge-muted  { background: var(--interactive-bg); color: var(--text-muted); }
  .badge-warn   { background: rgba(186,117,23,0.15); color: #e8a838; }
  .badge-alert  { background: var(--error-bg); color: var(--error-color); }
  .badge-ok     { background: var(--live-bg); color: var(--live-color); }
  .badge-info   { background: rgba(74,154,98,0.12); color: var(--tb-accent); }

  /* ── Mobile ──────────────────────────────────────────────────────────── */
  @media (max-width: 640px) {

    /* ── Card header: título en primera línea, controles en segunda ── */
    .card-head {
      flex-direction: column;
      align-items: flex-start;
      gap: calc(6px * var(--font-scale));
      padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale));
    }
    .card-head__info { width: 100%; }
    .card-head__controls { width: 100%; }
    .card-head__title { font-size: calc(13px * var(--font-scale)); }
    .card-head__sub { font-size: calc(11px * var(--font-scale)); }

    /* ── Ocultar cabecera de columnas — en mobile son autoevidentes ── */
    .sensor-cols-head { display: none; }

    /* ── Sparklines ocultas ── */
    .s-spark { display: none !important; }

    /* ── Filas de sensor: ocultar columnas desktop, mostrar mobile div ── */
    .sensor-row {
      display: block !important;
      padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)) !important;
      position: relative;
    }
    /* Ocultar todos los elementos del grid desktop */
    .sensor-row .s-num,
    .sensor-row .s-type,
    .sensor-row .s-val,
    .sensor-row .align-right,
    .sensor-row .s-spark { display: none !important; }

    /* Mostrar solo el div mobile */
    .s-mobile { display: flex; flex-direction: column; gap: calc(3px * var(--font-scale)); }
    .s-mobile__name {
      font-size: calc(13px * var(--font-scale));
      font-weight: 500;
      color: var(--text-primary);
      display: flex;
      align-items: baseline;
      gap: 6px;
    }
    .s-mobile__name.warn { color: #e8a838; }
    .s-mobile__num {
      font-size: calc(10px * var(--font-scale));
      color: var(--text-muted);
      font-weight: 400;
    }
    .s-mobile__meta {
      display: flex;
      align-items: center;
      gap: calc(8px * var(--font-scale));
      font-size: calc(12px * var(--font-scale));
      color: var(--text-secondary);
    }
    .s-mobile__val {
      font-size: calc(13px * var(--font-scale));
      font-weight: 500;
      color: var(--text-primary);
    }
    .s-mobile__val.warn { color: #e8a838; }
    .s-mobile__dash { color: var(--text-muted); }

    /* ── Gráfica expandida ── */
    .sensor-expanded {
      padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    }
    .sensor-expanded :global(.sc__body) {
      height: 140px !important;
    }

    /* ── Etiqueta de correlación ── */
    .corr-label { font-size: calc(10px * var(--font-scale)); }
  }

</style>
