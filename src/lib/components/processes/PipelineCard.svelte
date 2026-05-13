<!-- src/lib/components/processes/PipelineCard.svelte -->
<script lang="ts">
  import ActuatorRow   from './ActuatorRow.svelte';
  import PipelineChart from './PipelineChart.svelte';
  import SparkLine     from './SparkLine.svelte';
  import type { ProcessReading } from '$lib/stores/process';

  let {
    processId, pipeline, state: pipelineState,
    readings, rdLoading, timePreset, canOperate = false,
  }: {
    processId:  string;
    pipeline:   any;
    state:      any;
    readings:   ProcessReading[];
    rdLoading:  boolean;
    timePreset: string;
    canOperate: boolean;
  } = $props();

  const COLORS   = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];
  const hoursMap: Record<string,number> = { '1h':1,'6h':6,'24h':24,'7d':168 };

  // ── Loggers ───────────────────────────────────────────────────────────────
  const loggers = $derived.by(() =>
    (pipeline.nodes ?? [])
      .filter((n: any) => n.type === 'logger')
      .map((n: any, i: number) => {
        const edge       = (pipeline.edges ?? []).find((e: any) =>
          (e.to ?? e.target) === n.id || e.target === n.id);
        const upstreamId = edge?.from ?? edge?.source;
        const upstreamSt = upstreamId ? pipelineState?.node_states?.[upstreamId] : null;
        return {
          id:          n.id,
          tag:         n.tag || n.id,
          nodeType:    upstreamSt?.node_type ?? 'unknown',
          nodeData:    upstreamSt?.data ?? {},
          color:       COLORS[i % COLORS.length],
          upstreamId,
        };
      })
  );

  // ── Actuadores ────────────────────────────────────────────────────────────
  const ACTUATOR_TYPES = ['mqtt_actuator', 'http_actuator'];

  const actuators = $derived.by(() => {
    const configured = (pipeline.nodes ?? [])
      .filter((n: any) => n?.id && ACTUATOR_TYPES.includes(n.type));
    if (configured.length) {
      return configured.map((n: any) => {
        const st = pipelineState?.node_states?.[n.id] ?? null;
        return {
          id:             n.id,
          type:           n.type,
          label:          n.label?.trim() || '',
          payloadOn:      n.payload_on ?? '',
          lastAction:     st?.data?.last_action ?? null,
          totalOn:        st?.data?.total_on ?? null,
          overrideActive: !!(pipelineState?.override_active),
        };
      });
    }
    return Object.values(pipelineState?.node_states ?? {})
      .filter((n: any) => ACTUATOR_TYPES.includes(n.node_type))
      .map((n: any) => ({
        id: n.node_id, type: n.node_type, label: '', payloadOn: '',
        lastAction: n.data?.last_action ?? null, totalOn: n.data?.total_on ?? null,
        overrideActive: !!(pipelineState?.override_active),
      }));
  });

  // ── Status ────────────────────────────────────────────────────────────────
  const isReady = $derived(pipelineState?.is_ready ?? false);
  const cycle   = $derived(pipelineState?.cycle ?? 0);
  const hasOn   = $derived(actuators.some((a: any) => a.lastAction === 'on'));

  // ── Sensor expandido ──────────────────────────────────────────────────────
  let expandedId = $state<string | null>(null);
  function toggleLogger(id: string) {
    expandedId = expandedId === id ? null : id;
  }

  // ── Valor actual del sensor ───────────────────────────────────────────────
  function sensorValue(l: any): string {
    const d = l.nodeData;
    if (!d || l.nodeType === 'unknown') return '—';
    switch (l.nodeType) {
      case 'kalman': case 'ewma': case 'lowpass': case 'moving_avg':
        return (d.x?.[0] ?? d.y?.[0])?.toFixed(3) ?? '—';
      case 'mahalanobis': return d.last_d != null ? `d=${d.last_d.toFixed(2)}` : '—';
      case 'hysteresis':  return d.state  ? String(d.state).toUpperCase()  : '—';
      case 'sprt':        return d.last   ? String(d.last).toUpperCase()   : '—';
      case 'mqtt_actuator': case 'http_actuator':
        return d.last_action?.toUpperCase() ?? '—';
      default: return d.x?.[0]?.toFixed(3) ?? '—';
    }
  }

  function sensorColor(l: any): string {
    const d = l.nodeData; const t = l.nodeType;
    if (['hysteresis','sprt','mqtt_actuator','http_actuator'].includes(t)) {
      const st = d?.state ?? d?.last ?? d?.last_action;
      if (st === 'on' || st === 'On')   return '#3da85a';
      if (st === 'off' || st === 'Off') return '#e05454';
    }
    if (t === 'mahalanobis') {
      if (d?.last_d > 2.0) return '#e05454';
      if (d?.last_d > 0.8) return '#e8a838';
      return '#3da85a';
    }
    return l.color;
  }

  // ── Sparkline data ────────────────────────────────────────────────────────
  function sparkData(l: any): number[] {
    const t = l.nodeType;
    return readings.map((r: ProcessReading) => {
      if (['hysteresis','sprt','mqtt_actuator','http_actuator'].includes(t)) {
        const v = r.scope_values?.[l.tag];
        if (v === 'on')  return 1;
        if (v === 'off') return 0;
        return r.actuator === 'on' ? 1 : 0;
      }
      if (t === 'mahalanobis') {
        return r.scope_values?.[l.tag + ':d'] ?? r.filtered?.[0] ?? NaN;
      }
      return r.filtered?.[0] ?? r.raw?.[0] ?? NaN;
    }).filter((v: number) => isFinite(v));
  }

  const sensorLabels = $derived.by(() =>
    (pipeline.nodes ?? [])
      .find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? []
  );
</script>

<div class="card" class:card-on={hasOn}>

  <!-- Header -->
  <div class="card-head">
    <span class="pl-name">{pipeline.label}</span>
    {#if !isReady}
      <span class="badge b-warm">warmup</span>
    {:else}
      <span class="badge b-ok">ciclo {cycle}</span>
    {/if}
    {#if hasOn}<span class="badge b-on">● riego</span>{/if}
  </div>

  <!-- Sensores en columnas -->
  {#if loggers.length > 0}
    <div class="sensor-strip">
      {#each loggers as l, i (l.id)}
        {#if i > 0}<div class="s-divider"></div>{/if}
        <button
          class="sensor-col"
          class:s-expanded={expandedId === l.id}
          onclick={() => toggleLogger(l.id)}
        >
          <div class="sc-top">
            <span class="sc-tag">{l.tag}</span>
            <span class="sc-val" style="color:{sensorColor(l)}">{sensorValue(l)}</span>
          </div>
          <div class="sc-spark">
            <SparkLine data={sparkData(l)} color={l.color} height={22} />
          </div>
          <span class="sc-chevron" class:open={expandedId === l.id}>▶</span>
        </button>
      {/each}
    </div>

    <!-- Gráfico expandido -->
    {#if expandedId !== null}
      {@const lg = loggers.find(l => l.id === expandedId)}
      {#if lg}
        <div class="chart-panel">
          <PipelineChart
            {processId}
            pipelineId={pipeline.id}
            labels={sensorLabels}
            hours={hoursMap[timePreset] ?? 6}
            loggerTag={lg.tag}
            upstreamType={lg.nodeType}
          />
        </div>
      {/if}
    {/if}
  {/if}

  <!-- Actuadores -->
  {#if actuators.length > 0}
    <div class="act-section">
      {#each actuators as act (act.id)}
        <ActuatorRow
          {processId}
          pipelineId={pipeline.id}
          actuatorId={act.id}
          actuatorType={act.type}
          label={act.label}
          payloadOn={act.payloadOn}
          lastAction={act.lastAction}
          totalOn={act.totalOn}
          overrideActive={act.overrideActive}
          {canOperate}
        />
      {/each}
    </div>
  {/if}

</div>

<style>
  .card    { background:var(--bg-surface); border:0.5px solid var(--border-default); border-radius:10px; overflow:hidden; display:flex; flex-direction:column; }
  .card-on { border-color:#3da85a55; }

  /* Header */
  .card-head { display:flex; align-items:center; gap:6px; padding:calc(7px * var(--font-scale)) calc(11px * var(--font-scale)); flex-wrap:wrap; }
  .pl-name   { font-size:calc(12px * var(--font-scale)); font-weight:500; color:var(--text-primary); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; flex:1; min-width:0; }
  .badge     { font-size:calc(9px * var(--font-scale)); padding:1px 6px; border-radius:8px; font-family:'DM Mono',monospace; flex-shrink:0; }
  .b-ok      { background:var(--bg-inset); color:var(--text-muted); }
  .b-warm    { background:var(--bg-inset); color:var(--text-muted); font-style:italic; }
  .b-on      { background:#EAF3DE; color:#3B6D11; }

  /* Sensor strip */
  .sensor-strip  { display:flex; border-top:0.5px solid var(--border-subtle); }
  .s-divider     { width:0.5px; background:var(--border-subtle); flex-shrink:0; }

  .sensor-col  {
    flex:1; min-width:0; padding:calc(5px * var(--font-scale)) calc(8px * var(--font-scale)) calc(3px * var(--font-scale));
    background:none; border:none; cursor:pointer; text-align:left;
    display:flex; flex-direction:column; gap:1px;
    transition:background .1s; position:relative;
  }
  .sensor-col:hover    { background:var(--interactive-hover); }
  .sensor-col.s-expanded { background:var(--bg-elevated); }

  .sc-top  { display:flex; align-items:baseline; justify-content:space-between; gap:3px; }
  .sc-tag  { font-size:calc(9px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:55%; }
  .sc-val  { font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; white-space:nowrap; flex-shrink:0; }
  .sc-spark { height:calc(22px * var(--font-scale)); }
  .sc-chevron { position:absolute; bottom:2px; right:5px; font-size:7px; color:var(--text-muted); opacity:.5; transition:transform .15s; }
  .sc-chevron.open { transform:rotate(90deg); }

  /* Chart panel */
  .chart-panel { border-top:0.5px solid var(--border-subtle); padding:calc(10px * var(--font-scale)) calc(11px * var(--font-scale)); background:var(--bg-elevated); animation:sd .15s ease; }
  @keyframes sd { from { opacity:0; transform:translateY(-3px); } to { opacity:1; transform:none; } }

  /* Actuadores */
  .act-section { border-top:0.5px solid var(--border-subtle); }
</style>