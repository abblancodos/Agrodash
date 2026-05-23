<script lang="ts">
  import type { Box } from '$lib/api';
  import type { SensorStat, SensorCorrelation } from '$lib/api';
  import { normaliseSensorLabel, sensorColor } from '$lib/api';
  import { relTime, relTimeClass, anomalyClass, formatValue } from '$lib/utils';
  import { preferences, type SensorSort } from '$lib/stores/preferences';
  import SensorChart from './SensorChart.svelte';
  import CsvDownloadMenu from './CsvDownloadMenu.svelte';
  import DateTimePicker from './DateTimePicker.svelte';

  // ── Props ───────────────────────────────────────────────────────

  interface Props {
    box: Box;
    stats: SensorStat[];
    correlations: SensorCorrelation[];
    /** Rango global — usado como fallback si no hay prefs guardadas.... */
    from: Date;
    to: Date;
    live: boolean;
  }

  let { box, stats, correlations, from, to, live }: Props = $props();

  // ── Estado local de tiempo por caja (persistido) ──────────────────────────

  const PRESETS = [
    { label: '1h',  hours: 1   },
    { label: '6h',  hours: 6   },
    { label: '24h', hours: 24  },
    { label: '7d',  hours: 168 },
    { label: '30d', hours: 720 },
  ];

  function initTime() {
    const saved = preferences.getBox(box.id);
    if (saved.activePreset !== 'custom') {
      const preset = PRESETS.find(p => p.label === saved.activePreset);
      if (preset) {
        const t = new Date();
        return { preset: saved.activePreset, from: new Date(t.getTime() - preset.hours * 3_600_000), to: t };
      }
    }
    return { preset: saved.activePreset, from: new Date(saved.fromMs), to: new Date(saved.toMs) };
  }

  const _init = initTime();
  let activePreset = $state(_init.preset);
  let localFrom    = $state(_init.from);
  let localTo      = $state(_init.to);

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    localTo      = new Date();
    localFrom    = new Date(localTo.getTime() - p.hours * 3_600_000);
    preferences.setBox(box.id, { activePreset: p.label, fromMs: localFrom.getTime(), toMs: localTo.getTime() });
  }

  function onFromChange(d: Date) {
    localFrom    = d;
    activePreset = 'custom';
    preferences.setBox(box.id, { activePreset: 'custom', fromMs: d.getTime(), toMs: localTo.getTime() });
  }

  function onToChange(d: Date) {
    localTo      = d;
    activePreset = 'custom';
    preferences.setBox(box.id, { activePreset: 'custom', fromMs: localFrom.getTime(), toMs: d.getTime() });
  }

  // ── Lógica de sensores ─────────────────────────────────────────────────────

  const correlatedSensorIds = $derived(() => {
    const ids = new Set<string>();
    for (const c of correlations) {
      if (c.box_id === box.id) { ids.add(c.sensor_id_a); ids.add(c.sensor_id_b); }
    }
    return ids;
  });

  const sensorStats = $derived(() =>
    stats.filter(s => s.box_id === box.id)
         .sort((a, b) => (b.anomaly_score ?? -1) - (a.anomaly_score ?? -1))
  );

  const uniqueSensors = $derived(() =>
    sensorStats().filter(s => !correlatedSensorIds().has(s.sensor_id))
  );

  const corrSensors = $derived(() =>
    sensorStats().filter(s => correlatedSensorIds().has(s.sensor_id))
  );

  const corrGroups = $derived(() => {
    const groups = new Map<string, { type: string; sensors: SensorStat[]; pearsonR: number }>();
    for (const c of correlations) {
      if (c.box_id !== box.id) continue;
      const key = c.sensor_type;
      if (!groups.has(key)) {
        const sensorList = corrSensors().filter(s => s.sensor_type.toLowerCase() === key);
        if (sensorList.length) groups.set(key, { type: key, sensors: sensorList, pearsonR: c.pearson_r });
      }
    }
    return Array.from(groups.values());
  });

  const boxScore = $derived(() =>
    Math.max(0, ...sensorStats().map(s => s.anomaly_score ?? 0))
  );
  const boxAnomalyClass = $derived(() => anomalyClass(boxScore()));

  const boxLastSeen = $derived(() => {
    const dates = sensorStats().map(s => s.last_seen_at).filter((d): d is string => d !== null);
    if (!dates.length) return null;
    return dates.reduce((a, b) => (a > b ? a : b));
  });

  // ── Live polling ──────────────────────────────────────────────────────────
  let liveInterval: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (live) {
      liveInterval = setInterval(() => {
        localTo = new Date();
        if (activePreset !== 'custom') {
          const preset = PRESETS.find(p => p.label === activePreset);
          if (preset) localFrom = new Date(localTo.getTime() - preset.hours * 3_600_000);
        }
      }, 15_000);
    } else {
      if (liveInterval) clearInterval(liveInterval);
      liveInterval = null;
    }
    return () => { if (liveInterval) clearInterval(liveInterval); };
  });

  // ── Filtros de sensores (persistidos) ────────────────────────────────────

  const _boxPrefs = preferences.getBox(box.id);
  let filterOpen      = $state(false);
  let sensorSort      = $state<SensorSort>(_boxPrefs.sensorSort);
  let hideOlderThanH  = $state<number | null>(_boxPrefs.hideOlderThanH);
  let hideLowVariance = $state(_boxPrefs.hideLowVariance);

  $effect(() => {
    preferences.setBox(box.id, { sensorSort, hideOlderThanH, hideLowVariance });
  });

  function isLowVariance(s: SensorStat): boolean {
    if (!hideLowVariance) return false;
    const range = (s.max_24h ?? 0) - (s.min_24h ?? 0);
    const mean  = Math.abs(s.mean_24h ?? 0);
    if (mean === 0) return range === 0;
    return range / mean < 0.01;
  }

  function isOlderThan(s: SensorStat): boolean {
    if (hideOlderThanH === null) return false;
    if (!s.last_seen_at) return true;
    return (Date.now() - new Date(s.last_seen_at).getTime()) / 3_600_000 > hideOlderThanH;
  }

  function applySort(arr: SensorStat[]): SensorStat[] {
    return [...arr].sort((a, b) => {
      if (sensorSort === 'score') return (b.anomaly_score ?? -1) - (a.anomaly_score ?? -1);
      if (sensorSort === 'reciente') {
        const ta = a.last_seen_at ? new Date(a.last_seen_at).getTime() : 0;
        const tb = b.last_seen_at ? new Date(b.last_seen_at).getTime() : 0;
        return tb - ta;
      }
      return a.sensor_type.localeCompare(b.sensor_type);
    });
  }

  const visibleUnique = $derived(() =>
    applySort(uniqueSensors().filter(s => !isOlderThan(s) && !isLowVariance(s)))
  );

  const hiddenCount = $derived(() =>
    uniqueSensors().filter(s => isOlderThan(s) || isLowVariance(s)).length
  );

  // ── Chart expand ─────────────────────────────────────────────────────────
  let expandedSensorId = $state<string | null>(null);
  let hoveredSensorId  = $state<string | null>(null);
  let expandedCorrType = $state<string | null>(null);
  let csvOpen          = $state(false);

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

    <!-- Controles: CSV + filtro sensores + presets de tiempo -->
    <div class="card-head__controls">
      <button class="csv-btn" onclick={() => csvOpen = true} title="Descargar CSV">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="12" height="12"><path d="M2 10v2h10v-2M7 2v7M4 6l3 3 3-3"/></svg>
        CSV
      </button>
      <button class="filter-btn" class:active={filterOpen || hiddenCount() > 0 || sensorSort !== 'score'}
        onclick={() => filterOpen = !filterOpen} title="Filtrar sensores">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" width="12" height="12">
          <path d="M2 3h10M4 7h6M6 11h2"/>
        </svg>
        {#if hiddenCount() > 0}<span class="filter-count">{hiddenCount()}</span>{/if}
      </button>
      <div class="card-head__time">
        <div class="card-head__presets">
          {#each PRESETS as p}
            <button class="pbtn" class:active={activePreset === p.label}
              onclick={() => applyPreset(p)}>
              {p.label}
            </button>
          {/each}
        </div>
        <div class="card-head__pickers">
          <DateTimePicker bind:value={localFrom} max={localTo} label="DESDE"
            onchange={onFromChange} />
          <span class="picker-sep">→</span>
          <DateTimePicker bind:value={localTo}  min={localFrom} label="HASTA"
            onchange={onToChange} />
          {#if activePreset === 'custom'}
            <button class="picker-reset" title="Volver a 24h"
              onclick={() => applyPreset(PRESETS[2])}>↺</button>
          {/if}
        </div>
      </div>
    </div>
  </header>

  <!-- Panel de filtros de sensores -->
  {#if filterOpen}
    <div class="sensor-filter-panel">
      <div class="sfp-row">
        <span class="sfp-label">orden</span>
        <div class="sfp-pills">
          <button class="sfp-pill" class:active={sensorSort === 'score'} onclick={() => sensorSort = 'score'}>anomalía</button>
          <button class="sfp-pill" class:active={sensorSort === 'reciente'} onclick={() => sensorSort = 'reciente'}>más reciente</button>
          <button class="sfp-pill" class:active={sensorSort === 'alfa'} onclick={() => sensorSort = 'alfa'}>alfabético</button>
        </div>
      </div>
      <div class="sfp-row">
        <span class="sfp-label">ocultar sin datos hace más de</span>
        <div class="sfp-pills">
          {#each [null, 1, 6, 24, 168] as h}
            <button class="sfp-pill" class:active={hideOlderThanH === h} onclick={() => hideOlderThanH = h}>
              {h === null ? 'todo' : h === 168 ? '7d' : `${h}h`}
            </button>
          {/each}
        </div>
      </div>
      <div class="sfp-row">
        <span class="sfp-label">ocultar sin variación (&lt;1% del rango)</span>
        <button class="sfp-toggle" class:on={hideLowVariance} onclick={() => hideLowVariance = !hideLowVariance}>
          {hideLowVariance ? 'activado' : 'desactivado'}
        </button>
      </div>
      {#if hiddenCount() > 0}
        <div class="sfp-hidden-note">{hiddenCount()} sensor{hiddenCount() !== 1 ? 'es' : ''} oculto{hiddenCount() !== 1 ? 's' : ''}</div>
      {/if}
    </div>
  {/if}

  <!-- Cabecera de columnas -->
  <div class="sensor-cols-head">
    <span>sensor</span>
    <span>variable</span>
    <span>tendencia</span>
    <span class="align-right">valor</span>
    <span class="align-right">score</span>
    <span class="align-right">último dato</span>
  </div>

  <!-- Sensores únicos (filtrados y ordenados) ─────────────────────────────── -->
  {#each visibleUnique() as stat (stat.sensor_id)}
    {@const ac = anomalyClass(stat.anomaly_score)}
    {@const color = sensorColor(stat.sensor_type)}

    <div class="sensor-row" class:is-warn={ac === 'warn'} class:is-alert={ac === 'alert'}
         class:is-hovered={hoveredSensorId === stat.sensor_id}
         class:is-expanded={expandedSensorId === stat.sensor_id}
         role="button" tabindex="0"
         onmouseenter={() => hoveredSensorId = stat.sensor_id}
         onmouseleave={() => hoveredSensorId = null}
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
      <!-- Hint de expandir — visible en hover desktop -->
      <div class="s-expand-hint" aria-hidden="true">
        {#if expandedSensorId === stat.sensor_id}
          <span>▲ cerrar</span>
        {:else}
          <span>▼ expandir gráfico</span>
        {/if}
      </div>
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
      variables correlacionadas entre sensores de esta caja (r ≥ 0.90)
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
        {#each group.sensors as s (s.sensor_id)}
            <div class="sensor-row corr-sub-row"
                 role="button" tabindex="0"
                 class:is-expanded={expandedSensorId === s.sensor_id}
                 onmouseenter={() => hoveredSensorId = s.sensor_id}
                 onmouseleave={() => hoveredSensorId = null}
                 onclick={() => expandedSensorId = expandedSensorId === s.sensor_id ? null : s.sensor_id}
                 onkeydown={(e) => e.key === 'Enter' && (expandedSensorId = expandedSensorId === s.sensor_id ? null : s.sensor_id)}>
              <span class="s-num" style="color:var(--text-muted)">#{s.sensor_number}</span>
              <span class="s-type">{normaliseSensorLabel(group.type)}</span>
              <div class="s-spark">
                <SensorChart sensorId={s.sensor_id} sensorType={group.type}
                  from={localFrom} to={localTo} points={50} spark={true} {color} />
              </div>
              <span class="s-val align-right">{formatValue(s.last_value, group.type)}</span>
              <span class="align-right">
                {#if (s.anomaly_score ?? 0) >= 3}
                  <span class="badge badge-alert">{s.anomaly_score?.toFixed(1)}σ</span>
                {:else if (s.anomaly_score ?? 0) >= 1.5}
                  <span class="badge badge-warn">{s.anomaly_score?.toFixed(1)}σ</span>
                {/if}
              </span>
              <span class="align-right ago {relTimeClass(s.last_seen_at)}">{relTime(s.last_seen_at)}</span>
              <!-- Mobile -->
              <div class="s-mobile">
                <span class="s-mobile__name">
                  {normaliseSensorLabel(group.type)}
                  <span class="s-mobile__num">#{s.sensor_number}</span>
                </span>
                <span class="s-mobile__meta">
                  <span class="s-mobile__val">{formatValue(s.last_value, group.type)}</span>
                  <span class="ago {relTimeClass(s.last_seen_at)}">{relTime(s.last_seen_at)}</span>
                </span>
              </div>
            </div>
            {#if expandedSensorId === s.sensor_id}
              <div class="sensor-expanded">
                <SensorChart sensorId={s.sensor_id} sensorType={group.type}
                  from={localFrom} to={localTo} points={300} spark={false} {color} />
              </div>
            {/if}
        {/each}
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
  .card-head__time {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: calc(5px * var(--font-scale));
  }
  .card-head__presets { display: flex; gap: calc(3px * var(--font-scale)); flex-wrap: wrap; justify-content: flex-end; }
  .card-head__pickers {
    display: flex;
    align-items: center;
    gap: calc(5px * var(--font-scale));
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .picker-sep { color: var(--text-muted); font-size: calc(12px * var(--font-scale)); }
  .picker-reset {
    padding: calc(2px * var(--font-scale)) calc(6px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 4px;
    background: transparent; color: var(--text-muted);
    font-size: calc(12px * var(--font-scale)); cursor: pointer;
    transition: all .12s;
  }
  .picker-reset:hover { background: var(--interactive-hover); color: var(--text-secondary); }

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
  .sensor-cols-head {
    grid-template-columns: 56px 120px 1fr 90px 76px 100px;
  }
  .sensor-cols-head,
  .sensor-row {
    display: grid;
    grid-template-columns: 56px 120px 1fr 90px 76px 100px auto;
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
  .sensor-row.is-hovered { background: var(--interactive-hover); }
  .sensor-row.is-expanded { background: var(--bg-elevated); }

  /* Hint de expandir */
  .s-expand-hint {
    display: none;
    font-size: calc(10px * var(--font-scale));
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    letter-spacing: .04em;
    white-space: nowrap;
    align-self: center;
    opacity: 0;
    transition: opacity .15s;
  }
  .sensor-row.is-hovered .s-expand-hint {
    display: flex;
    opacity: 1;
  }
  .sensor-row.is-expanded .s-expand-hint {
    display: flex;
    opacity: 0.6;
  }
  .sensor-row.is-warn    { background: rgba(186,117,23,0.07); }
  .sensor-row.is-alert   { background: rgba(176,48,48,0.07); }
  .sensor-row.corr-sub-row {
    background: var(--bg-inset);
    cursor: pointer;
    border-left: 2px solid var(--border-subtle);
    padding-left: calc(18px * var(--font-scale));
  }
  .sensor-row.corr-sub-row:hover { background: color-mix(in srgb, var(--bg-inset) 70%, var(--interactive-hover) 30%); }
  .sensor-row.corr-sub-row.is-expanded { background: color-mix(in srgb, var(--bg-inset) 60%, var(--interactive-hover) 40%); }
  .sensor-row.corr-group {
    background: var(--bg-elevated);
    cursor: pointer;
  }
  .sensor-row.corr-group:hover { background: color-mix(in srgb, var(--bg-elevated) 80%, var(--interactive-hover) 20%); }

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
    .card-head__time { align-items: flex-start; }
    .card-head__pickers { display: none; }   /* ocultar pickers en mobile — presets bastan */
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

  /* ── Filter panel button ── */
  .filter-btn {
    display: flex; align-items: center; gap: 4px;
    padding: calc(3px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px; background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: calc(14px * var(--font-scale));
    cursor: pointer; transition: all .12s;
  }
  .filter-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
  .filter-btn.active { background: var(--interactive-hover); border-color: var(--text-muted); color: var(--text-secondary); }
  .filter-count {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; border-radius: 50%;
    background: rgba(186,117,23,0.25); color: #e8a838;
    font-size: calc(10px * var(--font-scale)); font-weight: 600; line-height: 1;
  }

  /* ── Sensor filter panel ── */
  .sensor-filter-panel {
    display: flex; flex-direction: column; gap: calc(8px * var(--font-scale));
    padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale));
    background: var(--bg-elevated);
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .sfp-row {
    display: flex; align-items: center; gap: calc(10px * var(--font-scale)); flex-wrap: wrap;
  }
  .sfp-label {
    font-size: calc(11px * var(--font-scale)); color: var(--text-muted);
    font-family: 'DM Mono', monospace; letter-spacing: .05em; min-width: 120px;
    flex-shrink: 0;
  }
  .sfp-pills { display: flex; gap: calc(4px * var(--font-scale)); flex-wrap: wrap; }
  .sfp-pill {
    padding: calc(2px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 4px;
    background: transparent; color: var(--text-secondary);
    font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale));
    cursor: pointer; transition: all .1s;
  }
  .sfp-pill:hover { background: var(--interactive-hover); }
  .sfp-pill.active { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }
  .sfp-toggle {
    padding: calc(2px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 4px;
    background: transparent; color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale));
    cursor: pointer; transition: all .1s;
  }
  .sfp-toggle.on { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }
  .sfp-hidden-note {
    font-size: calc(10px * var(--font-scale)); color: #e8a838;
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }

</style>