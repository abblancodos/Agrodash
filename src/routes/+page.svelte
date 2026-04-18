<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchBoxes, fetchTimeRange, fetchStats, type Box, type StatsResponse } from '$lib/api';
  import BoxCard from '$lib/components/BoxCard.svelte';
  import BoxSelector from '$lib/components/BoxSelector.svelte';
  import TypeSelector from '$lib/components/TypeSelector.svelte';
  import DateTimePicker from '$lib/components/DateTimePicker.svelte';
  import MultiSensorChart from '$lib/components/MultiSensorChart.svelte';
  import HelpPanel from '$lib/components/HelpPanel.svelte';

  // ── Estado global ─────────────────────────────────────────────────────────

  let boxes        = $state<Box[]>([]);
  let loading      = $state(true);
  let error        = $state('');

  let minDate      = $state<Date | undefined>(undefined);
  let maxDate      = $state<Date | undefined>(undefined);
  let fromDate     = $state(new Date(Date.now() - 24 * 60 * 60 * 1000));
  let toDate       = $state(new Date());

  let rangeLoading = $state(false);
  let rangeError   = $state(false);

  let selectedBoxIds = $state<Set<string>>(new Set());
  let activeTypes    = $state<Set<string>>(new Set());
  let search         = $state('');
  let live           = $state(false);
  let expandedBoxId  = $state<string | null>(null);

  // ── Stats ─────────────────────────────────────────────────────────────────

  let stats        = $state<StatsResponse | null>(null);
  let statsLoading = $state(false);
  let statsError   = $state('');

  // Polling de stats cada 60s (el worker actualiza cada 5 min, no tiene
  // sentido pedir más seguido)
  let statsInterval: ReturnType<typeof setInterval> | null = null;

  async function loadStats() {
    statsLoading = true; statsError = '';
    try {
      stats = await fetchStats();
    } catch (e: any) {
      statsError = e.message;
    } finally {
      statsLoading = false;
    }
  }

  // ── Modo de visualización ─────────────────────────────────────────────────

  type Mode = 'compacto' | 'cards' | 'graficas' | 'analisis';
  let mode = $state<Mode>('cards');

  const MODES: { id: Mode; label: string }[] = [
    { id: 'compacto', label: 'compacto' },
    { id: 'cards',    label: 'cards'    },
    { id: 'graficas', label: 'gráficas' },
    { id: 'analisis', label: 'análisis' },
  ];

  // ── Presets globales ──────────────────────────────────────────────────────

  const PRESETS = [
    { label: '1h',  hours: 1   },
    { label: '6h',  hours: 6   },
    { label: '24h', hours: 24  },
    { label: '7d',  hours: 168 },
    { label: '30d', hours: 720 },
  ];
  let activePreset = $state('24h');

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    toDate       = maxDate ?? new Date();
    fromDate     = new Date(toDate.getTime() - p.hours * 3_600_000);
  }

  async function loadTimeRange() {
    rangeLoading = true; rangeError = false;
    try {
      const { first, last } = await fetchTimeRange();
      minDate  = first; maxDate = last;
      toDate   = last;
      fromDate = new Date(last.getTime() - 24 * 60 * 60 * 1000);
    } catch { rangeError = true; }
    finally { rangeLoading = false; }
  }

  // ── Derived ───────────────────────────────────────────────────────────────

  const filteredBoxes = $derived(
    boxes.filter(b =>
      selectedBoxIds.has(b.id) &&
      (!search.trim() || b.name.toLowerCase().includes(search.toLowerCase()))
    )
  );

  /** Cajas ordenadas por anomaly_score más alto de sus sensores */
  const sortedBoxes = $derived(() => {
    if (!stats) return filteredBoxes;
    return [...filteredBoxes].sort((a, b) => {
      const scoreA = Math.max(...stats!.sensors
        .filter(s => s.box_id === a.id)
        .map(s => s.anomaly_score ?? 0));
      const scoreB = Math.max(...stats!.sensors
        .filter(s => s.box_id === b.id)
        .map(s => s.anomaly_score ?? 0));
      return scoreB - scoreA;
    });
  });

  const totalAnomalies = $derived(() =>
    stats?.sensors.filter(s => (s.anomaly_score ?? 0) >= 1.5).length ?? 0
  );

  // ── Mount ─────────────────────────────────────────────────────────────────

  onMount(async () => {
    await Promise.all([
      fetchBoxes().then(b => {
        boxes = b;
        selectedBoxIds = new Set(b.map(box => box.id));
        activeTypes = new Set(b.flatMap(box => box.sensors.map(s => s.type.toLowerCase())));
      }).catch(e => error = e.message),
      loadTimeRange(),
      loadStats(),
    ]);
    loading = false;

    // Polling de stats cada 60s
    statsInterval = setInterval(loadStats, 60_000);

    return () => { if (statsInterval) clearInterval(statsInterval); };
  });
</script>

<svelte:head><title>AgroDash</title></svelte:head>

<div class="layout">

  <!-- Sidebar de modos ──────────────────────────────────────────────────── -->
  <nav class="sidebar">
    <div class="sidebar__sep" style="margin-top:8px"></div>

    {#each MODES as m}
      <button
        class="mode-btn"
        class:active={mode === m.id}
        onclick={() => mode = m.id}
        title={m.label}
      >
        {#if m.id === 'compacto'}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <rect x="2" y="3" width="12" height="2"/><rect x="2" y="7" width="12" height="2"/><rect x="2" y="11" width="12" height="2"/>
          </svg>
        {:else if m.id === 'cards'}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <rect x="2" y="2" width="5" height="5" rx="1"/><rect x="9" y="2" width="5" height="5" rx="1"/>
            <rect x="2" y="9" width="5" height="5" rx="1"/><rect x="9" y="9" width="5" height="5" rx="1"/>
          </svg>
        {:else if m.id === 'graficas'}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="2,12 5,7 8,9 11,4 14,6"/><line x1="2" y1="14" x2="14" y2="14"/>
          </svg>
        {:else}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
            <circle cx="5" cy="5" r="1.5"/><circle cx="11" cy="4" r="1.5"/>
            <circle cx="8" cy="9" r="1.5"/><circle cx="4" cy="12" r="1.5"/><circle cx="12" cy="11" r="1.5"/>
          </svg>
        {/if}
      </button>
    {/each}
  </nav>

  <!-- Main ───────────────────────────────────────────────────────────────── -->
  <div class="main">

    <!-- Topbar -->
    <div class="topbar">
      <span class="topbar__title">{
        mode === 'compacto' ? 'compacto' :
        mode === 'cards'    ? 'cards' :
        mode === 'graficas' ? 'gráficas' :
        'análisis'
      }</span>
      <div class="vsep"></div>

      {#if mode !== 'cards'}
        <!-- Controles globales de tiempo (en cards cada caja tiene los suyos) -->
        <div class="presets">
          {#each PRESETS as p}
            <button class="pbtn" class:active={activePreset === p.label}
              onclick={() => applyPreset(p)}>{p.label}</button>
          {/each}
        </div>
        <DateTimePicker bind:value={fromDate} min={minDate} max={toDate}
          label="DESDE" onchange={() => activePreset = ''} />
        <span class="sep">→</span>
        <DateTimePicker bind:value={toDate} min={fromDate} max={maxDate}
          label="HASTA" onchange={() => activePreset = ''} />
        <button class="reload-btn" onclick={loadTimeRange} disabled={rangeLoading}>↺</button>
        <div class="vsep"></div>
      {/if}

      <button class="live-btn" class:active={live} onclick={() => live = !live}>
        <span class="live-dot"></span>
        {live ? 'LIVE ON' : 'LIVE OFF'}
      </button>

      <div class="spacer"></div>

      {#if totalAnomalies() > 0}
        <span class="badge badge-warn">{totalAnomalies()} anomalía{totalAnomalies() > 1 ? 's' : ''}</span>
      {/if}

      {#if !loading}
        <span class="badge badge-muted">
          {filteredBoxes.length} cajas · {filteredBoxes.reduce((a, b) => a + b.sensors.length, 0)} sensores
        </span>
      {/if}

      {#if !loading && boxes.length}
        <TypeSelector {boxes} selected={activeTypes} onchange={(s) => activeTypes = s} />
      {/if}
    </div>

    <!-- Box selector -->
    {#if !loading && boxes.length}
      <div class="box-sel-bar">
        <div class="controls__search">
          <span class="search-icon">⌕</span>
          <input class="search-input" type="text"
            placeholder="Filtrar cajas..." bind:value={search} />
        </div>
        <BoxSelector {boxes} selected={selectedBoxIds} onchange={(s) => selectedBoxIds = s} />
      </div>
    {/if}

    <!-- Contenido por modo -->
    <div class="content">

      {#if loading}
        {#each Array(3) as _}<div class="skeleton"></div>{/each}

      {:else if error}
        <div class="error-msg">Error: {error}</div>

      {:else if filteredBoxes.length === 0}
        <div class="empty">Seleccioná una o más cajas para ver los datos.</div>

      {:else if mode === 'compacto'}
        <!-- Vista compacta: tabla con una fila por caja -->
        <div class="compact-table">
          <div class="ct-head">
            <span>caja</span>
            <span>sensores</span>
            <span class="align-right">peor score</span>
            <span class="align-right">último dato</span>
            <span class="align-right">estado</span>
          </div>
          {#each sortedBoxes() as box (box.id)}
            {@const boxStats = stats?.sensors.filter(s => s.box_id === box.id) ?? []}
            {@const score = Math.max(...boxStats.map(s => s.anomaly_score ?? 0))}
            {@const lastSeen = boxStats.map(s => s.last_seen_at).filter(Boolean).reduce((a, b) => (a! > b! ? a : b), null as string | null)}
            {@const isExpanded = expandedBoxId === box.id}
            <div class="ct-row" class:expanded={isExpanded}
                 role="button" tabindex="0"
                 onclick={() => expandedBoxId = isExpanded ? null : box.id}
                 onkeydown={(e) => e.key === 'Enter' && (expandedBoxId = isExpanded ? null : box.id)}>
              <span class="ct-name">
                <span class="ct-chevron" class:open={isExpanded}>▶</span>
                {box.name}
              </span>
              <span>{box.sensors.length}</span>
              <span class="align-right">
                {#if score >= 1.5}
                  <span class="badge badge-warn">{score.toFixed(1)}σ</span>
                {:else}
                  <span class="badge badge-muted">—</span>
                {/if}
              </span>
              <span class="align-right ago
                {!lastSeen ? 'dead' : (Date.now() - new Date(lastSeen).getTime()) < 300_000 ? 'fresh' :
                 (Date.now() - new Date(lastSeen).getTime()) < 1_800_000 ? 'recent' :
                 (Date.now() - new Date(lastSeen).getTime()) < 86_400_000 ? 'stale' : 'dead'}">
                {#if lastSeen}
                  {#if (Date.now() - new Date(lastSeen).getTime()) < 60_000}hace {Math.floor((Date.now() - new Date(lastSeen).getTime()) / 1000)}s
                  {:else if (Date.now() - new Date(lastSeen).getTime()) < 3_600_000}hace {Math.round((Date.now() - new Date(lastSeen).getTime()) / 60_000)} min
                  {:else if (Date.now() - new Date(lastSeen).getTime()) < 86_400_000}hace {Math.round((Date.now() - new Date(lastSeen).getTime()) / 3_600_000)} h
                  {:else if (Date.now() - new Date(lastSeen).getTime()) < 2_592_000_000}hace {Math.round((Date.now() - new Date(lastSeen).getTime()) / 86_400_000)} días
                  {:else}hace {Math.round((Date.now() - new Date(lastSeen).getTime()) / 2_592_000_000)} meses{/if}
                {:else}sin datos{/if}
              </span>
              <span class="align-right">
                {#if score >= 3}
                  <span class="badge badge-alert">alerta</span>
                {:else if score >= 1.5}
                  <span class="badge badge-warn">anomalía</span>
                {:else}
                  <span class="badge badge-ok">normal</span>
                {/if}
              </span>
            </div>

            {#if isExpanded}
              <div class="ct-expand">
                <div class="ct-expand__cols">
                  <span class="ct-expand__h">sensor</span>
                  <span class="ct-expand__h">variable</span>
                  <span class="ct-expand__h align-right">último valor</span>
                  <span class="ct-expand__h align-right">media 24h</span>
                  <span class="ct-expand__h align-right">score</span>
                  <span class="ct-expand__h align-right">último dato</span>
                </div>
                {#each boxStats.sort((a, b) => (b.anomaly_score ?? 0) - (a.anomaly_score ?? 0)) as s (s.sensor_id)}
                  {@const ac = (s.anomaly_score ?? 0) >= 3 ? 'alert' : (s.anomaly_score ?? 0) >= 1.5 ? 'warn' : 'ok'}
                  {@const secsAgo = s.last_seen_at ? Math.floor((Date.now() - new Date(s.last_seen_at).getTime()) / 1000) : null}
                  {@const agoClass = !secsAgo ? 'dead' : secsAgo < 300 ? 'fresh' : secsAgo < 1800 ? 'recent' : secsAgo < 86400 ? 'stale' : 'dead'}
                  {@const agoText = !secsAgo ? 'sin datos' : secsAgo < 60 ? `hace ${secsAgo}s` : secsAgo < 3600 ? `hace ${Math.round(secsAgo/60)} min` : secsAgo < 86400 ? `hace ${Math.round(secsAgo/3600)} h` : secsAgo < 2592000 ? `hace ${Math.round(secsAgo/86400)} días` : `hace ${Math.round(secsAgo/2592000)} meses`}
                  <div class="ct-expand__row" class:row-warn={ac === 'warn'} class:row-alert={ac === 'alert'}>
                    <span class="ct-expand__cell muted">#{s.sensor_number}</span>
                    <span class="ct-expand__cell">{s.sensor_type}</span>
                    <span class="ct-expand__cell align-right" class:val-warn={ac !== 'ok'}>
                      {s.last_value !== null ? s.last_value.toFixed(4) : '—'}
                    </span>
                    <span class="ct-expand__cell align-right muted">
                      {s.mean_24h !== null ? s.mean_24h.toFixed(4) : '—'}
                    </span>
                    <span class="ct-expand__cell align-right">
                      {#if s.anomaly_score !== null}
                        <span class="badge badge-{ac === 'ok' ? 'muted' : ac}">{s.anomaly_score.toFixed(1)}σ</span>
                      {:else}—{/if}
                    </span>
                    <span class="ct-expand__cell align-right ago {agoClass}">{agoText}</span>
                  </div>
                {/each}
              </div>
            {/if}
          {/each}
        </div>

      {:else if mode === 'cards'}
        <!-- Vista cards: una BoxCard por caja -->
        <div class="cards-grid">
          {#each sortedBoxes() as box (box.id)}
            <BoxCard
              {box}
              stats={stats?.sensors.filter(s => s.box_id === box.id) ?? []}
              correlations={stats?.correlations.filter(c => c.box_id === box.id) ?? []}
              from={fromDate}
              to={toDate}
              {live}
            />
          {/each}
        </div>

      {:else if mode === 'graficas'}
        <!-- Vista gráficas: MultiSensorChart por tipo de variable -->
        <div class="charts-grid">
          {#each filteredBoxes as box (box.id)}
            {#each [...new Set(box.sensors.map(s => s.type))] as type}
              {#if activeTypes.has(type.toLowerCase())}
                <MultiSensorChart
                  sensors={box.sensors.filter(s => s.type === type)}
                  sensorType={type}
                  boxName={box.name}
                  from={fromDate}
                  to={toDate}
                  {live}
                />
              {/if}
            {/each}
          {/each}
        </div>

      {:else if mode === 'analisis'}
        <!-- Vista análisis: tabla de stats raw con pruning -->
        <div class="analysis">
          <p style="font-size:13px;color:var(--text-muted);margin-bottom:12px">
            Estadísticas pre-calculadas · última actualización del worker:
            {stats ? new Date(stats.computed_at).toLocaleTimeString('es-CR') : '—'}
          </p>
          <div class="data-table">
            <div class="dt-head">
              <span>caja</span><span>sensor</span><span>variable</span>
              <span class="align-right">último valor</span>
              <span class="align-right">media 24h</span>
              <span class="align-right">σ 24h</span>
              <span class="align-right">score</span>
              <span class="align-right">último dato</span>
            </div>
            {#each (stats?.sensors ?? []).filter(s => filteredBoxes.some(b => b.id === s.box_id)) as s (s.sensor_id)}
              <div class="dt-row" class:dt-warn={(s.anomaly_score ?? 0) >= 1.5}>
                <span>{s.box_name}</span>
                <span>#{s.sensor_number}</span>
                <span>{s.sensor_type}</span>
                <span class="align-right" style="font-weight:500">{s.last_value?.toFixed(4) ?? '—'}</span>
                <span class="align-right">{s.mean_24h?.toFixed(4) ?? '—'}</span>
                <span class="align-right">{s.stddev_24h?.toFixed(4) ?? '—'}</span>
                <span class="align-right">
                  {#if s.anomaly_score !== null}
                    <span class="badge badge-{s.anomaly_score >= 3 ? 'alert' : s.anomaly_score >= 1.5 ? 'warn' : 'muted'}">
                      {s.anomaly_score.toFixed(2)}σ
                    </span>
                  {:else}—{/if}
                </span>
                <span class="align-right ago
                  {!s.last_seen_at ? 'dead' :
                    (Date.now()-new Date(s.last_seen_at).getTime())<300000?'fresh':
                    (Date.now()-new Date(s.last_seen_at).getTime())<1800000?'recent':
                    (Date.now()-new Date(s.last_seen_at).getTime())<86400000?'stale':'dead'}">
                  {#if s.last_seen_at}
                    {#if (Date.now()-new Date(s.last_seen_at).getTime())<60000}hace {Math.floor((Date.now()-new Date(s.last_seen_at).getTime())/1000)}s
                    {:else if (Date.now()-new Date(s.last_seen_at).getTime())<3600000}hace {Math.round((Date.now()-new Date(s.last_seen_at).getTime())/60000)} min
                    {:else if (Date.now()-new Date(s.last_seen_at).getTime())<86400000}hace {Math.round((Date.now()-new Date(s.last_seen_at).getTime())/3600000)} h
                    {:else if (Date.now()-new Date(s.last_seen_at).getTime())<2592000000}hace {Math.round((Date.now()-new Date(s.last_seen_at).getTime())/86400000)} días
                    {:else}hace {Math.round((Date.now()-new Date(s.last_seen_at).getTime())/2592000000)} meses{/if}
                  {:else}sin datos{/if}
                </span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

    </div>
  </div>

  <!-- Bottom nav — solo visible en mobile via CSS -->
  <nav class="bottom-nav" aria-label="Navegación principal">
    {#each MODES as m}
      <button class="bnav-btn" class:active={mode === m.id}
        onclick={() => mode = m.id} title={m.label}>
        <div class="bnav-icon" class:has-anomaly={m.id === 'compacto' && totalAnomalies() > 0}>
          {#if m.id === 'compacto'}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
              <rect x="2" y="3" width="12" height="2"/><rect x="2" y="7" width="12" height="2"/><rect x="2" y="11" width="12" height="2"/>
            </svg>
          {:else if m.id === 'cards'}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
              <rect x="2" y="2" width="5" height="5" rx="1"/><rect x="9" y="2" width="5" height="5" rx="1"/>
              <rect x="2" y="9" width="5" height="5" rx="1"/><rect x="9" y="9" width="5" height="5" rx="1"/>
            </svg>
          {:else if m.id === 'graficas'}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="2,12 5,7 8,9 11,4 14,6"/><line x1="2" y1="14" x2="14" y2="14"/>
            </svg>
          {:else}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="5" cy="5" r="1.5"/><circle cx="11" cy="4" r="1.5"/>
              <circle cx="8" cy="9" r="1.5"/><circle cx="4" cy="12" r="1.5"/><circle cx="12" cy="11" r="1.5"/>
            </svg>
          {/if}
        </div>
        <span class="bnav-label">{m.label}</span>
      </button>
    {/each}
  </nav>

  <HelpPanel />
</div>

<style>
  .layout { display: flex; min-height: 100vh; background: var(--bg-base); }

  /* Sidebar */
  .sidebar {
    width: 40px;
    flex-shrink: 0;
    border-right: 0.5px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: calc(12px * var(--font-scale))0;
    gap: calc(2px * var(--font-scale));
    background: var(--bg-surface);
    position: sticky;
    top: 0;
    height: 100vh;
  }
  .sidebar__brand {
    font-size: calc(14px * var(--font-scale));
    font-weight: 500;
    color: var(--text-muted);
    letter-spacing: .1em;
    margin-bottom: 4px;
  }
  .sidebar__sep {
    width: 24px;
    height: 0.5px;
    background: var(--border-subtle);
    margin: 6px 0;
  }
  .mode-btn {
    width: 36px;
    height: 36px;
    border: 0.5px solid transparent;
    border-radius: 8px;
    background: transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    transition: all .12s;
  }
  .mode-btn svg { width: 15px; height: 15px; }
  .mode-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
  .mode-btn.active {
    background: var(--interactive-hover);
    border-color: var(--border-default);
    color: var(--text-primary);
  }
  .mode-label {
    font-size: calc(14px * var(--font-scale));
    letter-spacing: .06em;
    color: var(--text-muted);
    text-align: center;
    line-height: 1.2;
    margin-bottom: 4px;
  }

  /* Main */
  .main { flex: 1; display: flex; flex-direction: column; min-width: 0; }

  /* Topbar */
  .topbar {
    display: flex;
    align-items: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(9px * var(--font-scale)) calc(16px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    background: var(--bg-surface);
    flex-wrap: wrap;
  }
  .topbar__title { font-size: calc(14px * var(--font-scale)); font-weight: 500; letter-spacing: .08em; color: var(--text-primary); }
  .vsep { width: 0.5px; height: 16px; background: var(--border-subtle); flex-shrink: 0; }
  .spacer { flex: 1; }
  .sep { color: var(--text-muted); font-size: calc(14px * var(--font-scale)); }

  .presets { display: flex; gap: calc(3px * var(--font-scale)); }
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
  }
  .pbtn.active { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }

  .live-btn {
    display: flex;
    align-items: center;
    gap: calc(5px * var(--font-scale));
    padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    font-size: calc(14px * var(--font-scale));
    letter-spacing: .1em;
    cursor: pointer;
  }
  .live-btn.active { background: var(--live-bg, #eafaf1); border-color: var(--live-border, #82e0aa); color: var(--live-color, #1e8449); }
  .live-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .reload-btn {
    width: 26px; height: 26px; padding: 0;
    border: 0.5px solid var(--border-default);
    border-radius: 4px; background: transparent;
    color: var(--text-muted); font-size: calc(14px * var(--font-scale)); cursor: pointer;
  }

  /* Box selector bar */
  .box-sel-bar {
    display: flex;
    align-items: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(8px * var(--font-scale)) calc(16px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    background: var(--bg-surface);
    flex-wrap: wrap;
  }
  .controls__search { position: relative; }
  .search-icon { position: absolute; left: 8px; top: 50%; transform: translateY(-50%); color: var(--text-muted); font-size: calc(14px * var(--font-scale)); pointer-events: none; }
  .search-input {
    padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)) calc(5px * var(--font-scale)) calc(26px * var(--font-scale));
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    color: var(--text-primary);
    font-family: 'DM Mono', monospace;
    font-size: calc(14px * var(--font-scale));
    outline: none;
    width: 180px;
  }

  /* Content */
  .content { flex: 1; padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)) calc(48px * var(--font-scale)); background: var(--bg-base); }


  /* Compact expand */
  .ct-chevron { font-size: calc(14px * var(--font-scale)); color: var(--text-muted); transition: transform .15s; display: inline-block; margin-right: 4px; }
  .ct-chevron.open { transform: rotate(90deg); }
  .ct-row.expanded { background: var(--interactive-hover); }

  .ct-expand {
    border-bottom: 0.5px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .ct-expand__cols {
    display: grid;
    grid-template-columns: 56px 1fr 110px 110px 80px 110px;
    padding: calc(6px * var(--font-scale)) calc(16px * var(--font-scale));
    gap: calc(8px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .ct-expand__h {
    font-size: calc(14px * var(--font-scale)); color: var(--text-muted);
    letter-spacing: .07em; font-family: 'DM Mono', monospace;
  }
  .ct-expand__row {
    display: grid;
    grid-template-columns: 56px 1fr 110px 110px 80px 110px;
    padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); gap: calc(12px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    align-items: center;
  }
  .ct-expand__row:last-child { border-bottom: none; }
  .ct-expand__row.row-warn { background: var(--bg-surface)df5; }
  .ct-expand__row.row-alert { background: var(--bg-surface)5f5; }
  .ct-expand__cell { font-size: calc(14px * var(--font-scale)); color: var(--text-primary); }
  .ct-expand__cell.muted { color: var(--text-muted); }
  .val-warn { color: #e8a838 !important; font-weight: 500; }

  /* Compact table */
  .compact-table {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 10px;
    overflow: hidden;
  }
  .ct-head {
    display: grid;
    grid-template-columns: 1fr 70px 110px 130px 90px;
    padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale));
    background: var(--bg-elevated);
    border-bottom: 0.5px solid var(--border-subtle);
    font-size: calc(14px * var(--font-scale));
    color: var(--text-muted);
    letter-spacing: .07em;
    gap: calc(8px * var(--font-scale));
  }
  .ct-row {
    display: grid;
    grid-template-columns: 1fr 70px 110px 130px 90px;
    padding: calc(9px * var(--font-scale)) calc(16px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    align-items: center;
    gap: calc(8px * var(--font-scale));
    font-size: calc(14px * var(--font-scale));
    cursor: pointer;
    transition: background .1s;
  }
  .ct-row:last-child { border-bottom: none; }
  .ct-row:hover { background: var(--interactive-hover); }
  .ct-name { font-weight: 500; color: var(--text-primary); }
  .align-right { text-align: right; }

  /* Cards grid */
  .cards-grid { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }

  /* Charts grid */
  .charts-grid { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }

  /* Data table (análisis) */
  .data-table {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 10px;
    overflow: hidden;
  }
  .dt-head {
    display: grid;
    grid-template-columns: 80px 50px 90px repeat(5, 1fr);
    padding: calc(6px * var(--font-scale)) calc(14px * var(--font-scale));
    background: var(--bg-elevated);
    border-bottom: 0.5px solid var(--border-subtle);
    font-size: calc(14px * var(--font-scale));
    color: var(--text-muted);
    letter-spacing: .07em;
    gap: calc(6px * var(--font-scale));
  }
  .dt-row {
    display: grid;
    grid-template-columns: 80px 50px 90px repeat(5, 1fr);
    padding: calc(6px * var(--font-scale)) calc(14px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    font-size: calc(14px * var(--font-scale));
    color: var(--text-secondary);
    align-items: center;
    gap: calc(6px * var(--font-scale));
  }
  .dt-row:last-child { border-bottom: none; }
  .dt-row.dt-warn { background: var(--bg-surface)df5; }

  /* Badges */
  .badge { font-size: calc(14px * var(--font-scale)); padding: calc(2px * var(--font-scale)) calc(6px * var(--font-scale)); border-radius: 4px; letter-spacing: .04em; font-weight: 500; }
  .badge-muted  { background: var(--bg-elevated); color: var(--text-muted); }
  .badge-ok     { background: var(--live-bg); color: var(--live-color); }
  .badge-warn   { background: rgba(186,117,23,0.18); color: #e8a838; }
  .badge-alert  { background: var(--error-bg); color: var(--error-color); }

  /* Tiempo relativo */
  .ago        { font-size: calc(14px * var(--font-scale)); }
  .ago.fresh  { color: var(--live-color); }
  .ago.recent { color: var(--text-muted); }
  .ago.stale  { color: #e8a838; }
  .ago.dead   { color: var(--error-color); }

  /* Skeleton */
  .skeleton {
    height: 120px;
    border-radius: 10px;
    border: 0.5px solid var(--border-subtle);
    background: var(--bg-elevated);
    margin-bottom: 10px;
    animation: shimmer 1.5s infinite;
  }
  @keyframes shimmer { 0%,100%{opacity:.6}50%{opacity:1} }

  .error-msg { font-size: calc(14px * var(--font-scale)); color: #A32D2D; padding: calc(24px * var(--font-scale)); text-align: center; }
  .empty { font-size: calc(14px * var(--font-scale)); color: var(--text-muted); padding: calc(48px * var(--font-scale)); text-align: center; }



  /* ═══════════════════════════════════════════════════════
     MOBILE  (≤ 640px)
     ═══════════════════════════════════════════════════════ */

  /* Bottom nav — hidden on desktop */
  .bottom-nav {
    display: none;
  }

  @media (max-width: 640px) {

    /* Layout — sidebar oculto, main ocupa todo */
    .layout { flex-direction: column; }
    .sidebar { display: none; }
    .main { width: 100%; padding-bottom: 56px; }

    /* Topbar — más compacto, sin separadores ni fecha */
    .topbar { gap: 6px; padding: 8px 12px; flex-wrap: nowrap; overflow-x: auto; }
    .vsep { display: none; }
    .presets { display: none; }
    .reload-btn { display: none; }
    .topbar__title { font-size: 12px; min-width: max-content; }

    /* Box selector bar */
    .box-sel-bar { padding: 6px 12px; gap: 6px; }
    .search-input { width: 120px; font-size: 11px; }

    /* Content */
    .content { padding: 10px 10px 16px; }

    /* Compact table — quitar columna de peor score en mobile */
    .ct-head { grid-template-columns: 1fr 80px 100px !important; gap: 8px !important; padding: 6px 10px !important; }
    .ct-head span:nth-child(2) { display: none; }
    .ct-head span:nth-child(3) { display: none; }
    .ct-row { grid-template-columns: 1fr 80px 100px !important; gap: 8px !important; padding: 8px 10px !important; }
    .ct-row > span:nth-child(2) { display: none; }
    .ct-row > span:nth-child(3) { display: none; }

    /* Compact expand sub-table */
    .ct-expand__cols { grid-template-columns: 36px 1fr 68px 60px !important; gap: 6px !important; padding: 5px 10px !important; }
    .ct-expand__cols span:nth-child(4) { display: none; }
    .ct-expand__row { grid-template-columns: 36px 1fr 68px 60px !important; gap: 6px !important; padding: 6px 10px !important; }
    .ct-expand__row > span:nth-child(4) { display: none; }

    /* Data table análisis — scroll horizontal */
    .data-table { overflow-x: auto; }
    .dt-head { grid-template-columns: 64px 44px 80px repeat(4, 1fr) !important; gap: 4px !important; }
    .dt-row  { grid-template-columns: 64px 44px 80px repeat(4, 1fr) !important; gap: 4px !important; }

    /* Cards grid — sin sparklines en filas (BoxCard las oculta via su propio @media) */
    .cards-grid { gap: 8px; }

    /* Bottom nav — visible en mobile */
    .bottom-nav {
      display: flex;
      position: fixed;
      bottom: 0; left: 0; right: 0;
      background: var(--bg-surface);
      border-top: 0.5px solid var(--border-subtle);
      z-index: 80;
      height: 56px;
    }
    .bnav-btn {
      flex: 1; display: flex; flex-direction: column;
      align-items: center; justify-content: center;
      gap: 2px; border: none; background: transparent;
      color: var(--text-muted); cursor: pointer;
      padding: 6px 0 10px; transition: color .12s;
    }
    .bnav-btn.active { color: var(--text-primary); }
    .bnav-btn svg { width: 20px; height: 20px; }
    .bnav-label { font-size: 9px; font-family: 'DM Mono', monospace; letter-spacing: .04em; }
    .bnav-icon { position: relative; }
    .bnav-icon.has-anomaly::after {
      content: '';
      position: absolute; top: -2px; right: -6px;
      width: 6px; height: 6px;
      border-radius: 50%; background: #e8a838;
    }

    /* Anomaly banner — más compacto */
    .anomaly-bar { font-size: 11px; padding: 6px 10px; }

  }

</style>