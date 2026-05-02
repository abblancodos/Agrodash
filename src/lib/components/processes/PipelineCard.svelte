<!-- src/lib/components/processes/PipelineCard.svelte -->
<script lang="ts">
  import LoggerRow    from './LoggerRow.svelte';
  import PipelineChart from './PipelineChart.svelte';
  import ActuatorRow  from './ActuatorRow.svelte';
  import { processStore, type ProcessReading } from '$lib/stores/process';

  const canOperate = $derived(['operator','admin'].includes(processStore.process?.user_role ?? ''));

  let {
    processId, pipeline, state: pipelineState, readings, rdLoading,
    timePreset, onSetPreset,
  }: {
    processId:  string;
    pipeline:   any;
    state:      any; // received as pipelineState internally
    readings:   ProcessReading[];
    rdLoading:  boolean;
    timePreset: string;
    onSetPreset: (label: string) => void;
  } = $props();

  const COLORS  = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];
  const PRESETS = ['1h','6h','24h','7d'];

  // ── Loggers del pipeline ───────────────────────────────────────────────────
  const loggers = $derived.by(() => {
    return (pipeline.nodes ?? [])
      .filter((n: any) => n.type === 'logger')
      .map((n: any, i: number) => {
        const upstreamId    = (pipeline.edges ?? []).find((e: any) => e.target === n.id)?.source;
        const upstreamState = upstreamId ? pipelineState?.node_states?.[upstreamId] : null;
        return {
          id:       n.id,
          tag:      n.tag || n.id,
          nodeType: upstreamState?.node_type ?? 'unknown',
          nodeData: upstreamState?.data ?? {},
          color:    COLORS[i % COLORS.length],
        };
      });
  });

  // ── Actuadores del pipeline ────────────────────────────────────────────────
  const actuators = $derived.by(() => {
    return Object.values(pipelineState?.node_states ?? {})
      .filter((n: any) => n.node_type === 'mqtt_actuator' || n.node_type === 'http_actuator')
      .map((n: any) => ({
        id:             n.node_id,
        type:           n.node_type,
        lastAction:     n.data?.last_action ?? null,
        totalOn:        n.data?.total_on ?? null,
        overrideActive: !!(pipelineState?.override_active),
      }));
  });

  // ── Header stats ───────────────────────────────────────────────────────────
  const isReady   = $derived(pipelineState?.is_ready ?? false);
  const cycle     = $derived(pipelineState?.cycle ?? 0);
  const isOn      = $derived(actuators.some((a: any) => a.lastAction === 'on'));
  const totalOnSec = $derived(actuators[0]?.totalOn ?? null);

  function fmtOn(s: number): string {
    if (s < 60)   return `${s.toFixed(0)}s`;
    if (s < 3600) return `${(s/60).toFixed(1)}m`;
    return `${(s/3600).toFixed(2)}h`;
  }

  // ── Logger expandido ───────────────────────────────────────────────────────
  let expandedTag = $state<string | null>(null);

  function toggleLogger(tag: string) {
    expandedTag = expandedTag === tag ? null : tag;
  }

  // ── Labels de sensores ─────────────────────────────────────────────────────
  const sensorLabels = $derived.by(() =>
    (pipeline.nodes ?? [])
      .find((n: any) => n.type === 'postgres_sensor')
      ?.sensors?.map((s: any) => s.label) ?? []
  );
</script>

<div class="card" class:valve-on={isOn}>

  <!-- Header -->
  <div class="card-head">
    <div class="ch-left">
      <span class="pl-name">{pipeline.label}</span>
      {#if !isReady}
        <span class="badge-warmup">warmup</span>
      {:else}
        <span class="badge-ok">ciclo {cycle}</span>
      {/if}
      {#if isOn}
        <span class="badge-on">● riego</span>
      {/if}
      {#if totalOnSec != null}
        <span class="head-meta">total ON: {fmtOn(totalOnSec)}</span>
      {/if}
    </div>
    <div class="ch-right">
      <div class="preset-group">
        {#each PRESETS as p (p)}
          <button class="preset-btn" class:active={timePreset === p}
            onclick={() => onSetPreset(p)}>{p}</button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Columnas de tabla -->
  <div class="table-head">
    <span>señal</span>
    <span>nodo</span>
    <span>tendencia</span>
    <span style="text-align:right">valor</span>
    <span></span>
  </div>

  <!-- Filas de Logger -->
  {#if loggers.length === 0}
    <div class="no-loggers">Sin loggers configurados — agregá un nodo Logger al pipeline para visualizar señales</div>
  {:else}
    {#each loggers as l (l.id)}
      <LoggerRow
        tag={l.tag}
        nodeType={l.nodeType}
        nodeData={l.nodeData}
        {readings}
        color={l.color}
        expanded={expandedTag === l.tag}
        ontoggle={() => toggleLogger(l.tag)}
      />

      <!-- Chart expandido -->
      {#if expandedTag === l.tag}
        <div class="expanded-chart">
          <PipelineChart
            {processId}
            pipelineId={pipeline.id}
            labels={sensorLabels}
            hours={({'1h':1,'6h':6,'24h':24,'7d':168})[timePreset] ?? 6}
            loggerTag={l.tag}
            upstreamType={l.nodeType}
          />
        </div>
      {/if}
    {/each}
  {/if}

  <!-- Actuadores -->
  {#each actuators as act (act.id)}
    <ActuatorRow
      {processId}
      pipelineId={pipeline.id}
      actuatorId={act.id}
      actuatorType={act.type}
      lastAction={act.lastAction}
      totalOn={act.totalOn}
      overrideActive={act.overrideActive}
      {canOperate}
    />
  {/each}

</div>

<style>
  .card        { background:var(--bg-surface); border:0.5px solid var(--border-default); border-radius:10px; overflow:hidden; transition:border-color .15s; }
  .card.valve-on { border-color:#3da85a55; }

  .card-head   { display:flex; align-items:center; justify-content:space-between; padding:calc(11px * var(--font-scale)) calc(14px * var(--font-scale)); gap:8px; flex-wrap:wrap; }
  .ch-left     { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
  .ch-right    { display:flex; align-items:center; gap:6px; }
  .pl-name     { font-size:calc(14px * var(--font-scale)); font-weight:500; color:var(--text-primary); }
  .badge-warmup{ font-size:calc(10px * var(--font-scale)); padding:1px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .badge-ok    { font-size:calc(10px * var(--font-scale)); padding:1px 7px; border-radius:10px; background:var(--bg-inset); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .badge-on    { font-size:calc(10px * var(--font-scale)); padding:1px 7px; border-radius:10px; background:#EAF3DE; color:#3B6D11; font-family:'DM Mono',monospace; }
  .head-meta   { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }

  .preset-group { display:flex; gap:3px; }
  .preset-btn   { padding:calc(3px * var(--font-scale)) calc(8px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:5px; background:none; cursor:pointer; font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .preset-btn:hover  { background:var(--interactive-hover); }
  .preset-btn.active { background:var(--bg-elevated); color:var(--text-primary); border-color:var(--text-muted); }

  .table-head  { display:grid; grid-template-columns:120px 90px 1fr 80px 16px; gap:calc(8px * var(--font-scale)); padding:calc(5px * var(--font-scale)) calc(14px * var(--font-scale)); border-top:0.5px solid var(--border-subtle); background:var(--bg-inset); }
  .table-head span { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }

  .no-loggers  { padding:calc(16px * var(--font-scale)) calc(14px * var(--font-scale)); font-size:calc(12px * var(--font-scale)); color:var(--text-muted); font-style:italic; border-top:0.5px solid var(--border-subtle); }

  .expanded-chart { padding:calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-top:0.5px solid var(--border-subtle); background:var(--bg-elevated); }
</style>