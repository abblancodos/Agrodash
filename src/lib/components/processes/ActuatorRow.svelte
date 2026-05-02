<!-- src/lib/components/processes/ActuatorRow.svelte -->
<script lang="ts">
  import { processStore } from '$lib/stores/process';

  let {
    processId, pipelineId, actuatorId, actuatorType,
    lastAction, totalOn, overrideActive, canOperate = false,
  }: {
    processId: string; pipelineId: string; actuatorId: string;
    actuatorType: string; lastAction: string | null;
    totalOn: number | null; overrideActive: boolean; canOperate?: boolean;
  } = $props();

  let busy = $state(false);
  let error = $state('');

  async function send(action: 'on' | 'off' | 'clear') {
    busy = true; error = '';
    try {
      const cmd = action === 'clear'
        ? { cmd: 'ClearOverride', pipeline_id: pipelineId, actuator_id: actuatorId }
        : { cmd: 'Override', action, pipeline_id: pipelineId, actuator_id: actuatorId };
      await processStore.command(processId, cmd);
      await processStore.refreshPipelineState(processId, pipelineId);
    } catch (e: any) { error = e.message; }
    finally { busy = false; }
  }

  function fmtOn(s: number): string {
    if (s < 60) return `${s.toFixed(0)}s`;
    if (s < 3600) return `${(s/60).toFixed(1)}m`;
    return `${(s/3600).toFixed(2)}h`;
  }

  const isOn  = $derived(lastAction === 'on');
  const isOff = $derived(lastAction === 'off');
</script>

<div class="act-row">
  <div class="act-info">
    <span class="act-type">{actuatorType}</span>
    <span class="act-state" class:on={isOn} class:off={isOff}>{lastAction?.toUpperCase() ?? '—'}</span>
    {#if totalOn != null}<span class="act-meta">total ON: {fmtOn(totalOn)}</span>{/if}
    {#if overrideActive}<span class="act-badge">override</span>{/if}
  </div>
  {#if canOperate}
    <div class="act-btns">
      <button class="tog tog--on"   class:active={isOn && overrideActive}  disabled={busy} onclick={() => send('on')}>ON</button>
      <button class="tog tog--off"  class:active={isOff && overrideActive} disabled={busy} onclick={() => send('off')}>OFF</button>
      <button class="tog tog--auto" class:active={!overrideActive}         disabled={busy} onclick={() => send('clear')}>↺ auto</button>
    </div>
  {/if}
  {#if error}<span class="act-error">{error}</span>{/if}
</div>

<style>
  .act-row   { display:flex; align-items:center; gap:10px; flex-wrap:wrap; padding:calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); background:var(--bg-inset); border-top:0.5px solid var(--border-subtle); }
  .act-info  { display:flex; align-items:center; gap:8px; flex:1; flex-wrap:wrap; min-width:0; }
  .act-type  { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .act-state { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; color:var(--text-muted); }
  .act-state.on  { color:#3da85a; }
  .act-state.off { color:#e05454; }
  .act-meta  { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .act-badge { font-size:calc(9px * var(--font-scale)); padding:1px 6px; border-radius:10px; background:#FEF3C7; color:#92400E; font-family:'DM Mono',monospace; }
  .act-btns  { display:flex; gap:4px; }
  .tog       { padding:calc(3px * var(--font-scale)) calc(9px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:6px; background:none; cursor:pointer; font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .tog:disabled { opacity:.4; cursor:default; }
  .tog--on.active   { background:#EAF3DE; color:#3B6D11; border-color:#3da85a55; }
  .tog--off.active  { background:#FCEBEB; color:#A32D2D; border-color:#e0545455; }
  .tog--auto.active { background:var(--bg-elevated); color:var(--text-primary); }
  .tog--on:not(:disabled):not(.active):hover   { background:#EAF3DE; color:#3da85a; }
  .tog--off:not(:disabled):not(.active):hover  { background:#FCEBEB; color:#e05454; }
  .tog--auto:not(:disabled):not(.active):hover { background:var(--interactive-hover); }
  .act-error { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }
</style>