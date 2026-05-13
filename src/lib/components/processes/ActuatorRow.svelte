<!-- src/lib/components/processes/ActuatorRow.svelte -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { processStore } from '$lib/stores/process';

  let {
    processId, pipelineId, actuatorId, actuatorType,
    lastAction, totalOn, overrideActive, canOperate = false,
    label = '', payloadOn = '',
  }: {
    processId: string; pipelineId: string; actuatorId: string;
    actuatorType: string; lastAction: string | null;
    totalOn: number | null; overrideActive: boolean; canOperate?: boolean;
    label?: string; payloadOn?: string;
  } = $props();

  // ── ACK state ─────────────────────────────────────────────────────────────
  type AckStage = 'idle' | 'mqtt' | 'lora' | 'done' | 'error';
  let ackStage  = $state<AckStage>('idle');
  let ackErrMsg = $state('');
  let ackRawMsg = $state('');
  let busy      = $state(false);
  let error     = $state('');
  let doneTimer: ReturnType<typeof setTimeout> | null = null;

  // Valve key: número extraído de payload_on (ej. "on,5" → "5")
  const valveKey = $derived.by(() => {
    const p = payloadOn ?? '';
    return p.includes(',') ? (p.split(',')[1]?.trim() ?? actuatorId) : actuatorId;
  });
  // Snapshot para onDestroy — por si los props se limpian antes del teardown
  let _valveKey = actuatorId;
  $effect(() => { _valveKey = valveKey; });

  // Nombre a mostrar
  const displayName = $derived.by(() => {
    if (label?.trim()) return label.trim();
    const p = payloadOn ?? '';
    if (p.includes(',')) return `Válvula ${p.split(',')[1]?.trim() ?? ''}`;
    return actuatorType ?? '';
  });

  function clearDoneTimer() {
    if (doneTimer) { clearTimeout(doneTimer); doneTimer = null; }
  }

  function unregister() {
    processStore.offMqttAck(_valveKey);
    processStore.offMqttAck(actuatorId);
  }

  function onAckMsg(raw: string) {
    ackRawMsg = raw;
    if (raw.startsWith('MQTT_RECIBIDO') || raw.startsWith('MQTT_OVERRIDE')) {
      ackStage = 'mqtt';
    } else if (raw.startsWith('LORA_ENVIANDO')) {
      ackStage = 'lora';
    } else if (raw.startsWith('CONCENTRADOR:')) {
      ackStage = 'done';
      clearDoneTimer(); busy = false; unregister();
      doneTimer = setTimeout(() => { ackStage = 'idle'; ackRawMsg = ''; }, 3000);
    } else if (raw.startsWith('LORA_ERROR') || raw.startsWith('CONCENTRADOR_ERROR')) {
      ackStage = 'error'; ackErrMsg = raw;
      clearDoneTimer(); busy = false; error = raw; unregister();
    }
  }

  function cancelTracking() {
    clearDoneTimer(); unregister();
    ackStage = 'idle'; ackErrMsg = ''; ackRawMsg = ''; busy = false;
  }

  async function send(action: 'on' | 'off' | 'clear') {
    if (busy) return;
    busy = true; error = '';
    clearDoneTimer(); ackStage = 'idle'; ackErrMsg = ''; ackRawMsg = '';

    try {
      if (action !== 'clear') {
        ackStage = 'mqtt'; // mostrar barra inmediatamente
        processStore.onMqttAck(valveKey, onAckMsg);
        if (valveKey !== actuatorId) processStore.onMqttAck(actuatorId, onAckMsg);
      }

      const cmd = action === 'clear'
        ? { type:'command', cmd:'ClearOverride', pipeline_id:pipelineId, actuator_id:actuatorId }
        : { type:'command', cmd:'Override', action, pipeline_id:pipelineId, actuator_id:actuatorId };

      const viaWs = processStore.wsSend(cmd);
      if (!viaWs) {
        const httpCmd = action === 'clear'
          ? { cmd:'ClearOverride', pipeline_id:pipelineId, actuator_id:actuatorId }
          : { cmd:'Override', action, pipeline_id:pipelineId, actuator_id:actuatorId };
        await processStore.command(processId, httpCmd);
      }

      if (action === 'clear') {
        await processStore.refreshPipelineState(processId, pipelineId);
        busy = false; ackStage = 'idle';
      }
    } catch (e: any) {
      error = e.message; ackStage = 'idle'; busy = false; unregister();
    }
  }

  onDestroy(() => { clearDoneTimer(); unregister(); });

  const isOn  = $derived(lastAction === 'on');
  const isOff = $derived(lastAction === 'off');

  function fmtOn(s: number): string {
    if (s < 60) return `${s.toFixed(0)}s`;
    if (s < 3600) return `${(s/60).toFixed(1)}m`;
    return `${(s/3600).toFixed(2)}h`;
  }

  // Etapa donde ocurrió el error
  type StageKey = 'mqtt' | 'lora' | 'done';
  const ORDER: StageKey[] = ['mqtt','lora','done'];
  const LABELS: Record<StageKey,string> = { mqtt:'Gateway', lora:'LoRa', done:'Relay' };

  const errorStage = $derived.by((): StageKey | null => {
    if (ackStage !== 'error') return null;
    if (ackErrMsg.startsWith('CONCENTRADOR_ERROR')) return 'done';
    if (ackErrMsg.startsWith('LORA_ERROR'))         return 'lora';
    return 'mqtt';
  });

  function stageStatus(key: StageKey): 'pending'|'active'|'done'|'error' {
    if (ackStage === 'idle') return 'pending';
    if (ackStage === 'error' && errorStage) {
      const ki = ORDER.indexOf(key), ei = ORDER.indexOf(errorStage);
      if (ki < ei) return 'done'; if (ki === ei) return 'error'; return 'pending';
    }
    const ci = ORDER.indexOf(ackStage as StageKey), ki = ORDER.indexOf(key);
    if (ki < ci) return 'done'; if (ki === ci) return 'active'; return 'pending';
  }
</script>

<!-- Fila principal — siempre la misma altura -->
<div class="act-wrap">
  <div class="act-main">
    <div class="act-dot" class:on={isOn} class:off={isOff}></div>
    <span class="act-name">{displayName}</span>
    <span class="act-st" class:on={isOn} class:off={isOff}>
      {lastAction?.toUpperCase() ?? '—'}
    </span>
    {#if totalOn != null}
      <span class="act-total">{fmtOn(totalOn)}</span>
    {/if}
    {#if overrideActive && ackStage === 'idle'}
      <span class="act-badge">override</span>
    {/if}
    {#if canOperate}
      <div class="act-btns">
        <button class="tog" class:ton={isOn && overrideActive}  disabled={busy} onclick={() => send('on')}>ON</button>
        <button class="tog" class:toff={isOff && overrideActive} disabled={busy} onclick={() => send('off')}>OFF</button>
        <button class="tog" class:tauto={!overrideActive}        disabled={busy} onclick={() => send('clear')}>↺</button>
      </div>
    {/if}
  </div>

  <!-- Sub-fila ACK — solo aparece cuando hay evento -->
  {#if ackStage !== 'idle'}
    <div class="ack-bar" class:ack-err={ackStage === 'error'}>
      <div class="ack-stages">
        {#each ORDER as key, i (key)}
          {#if i > 0}
            {@const prev = stageStatus(ORDER[i-1])}
            <div class="aline" class:aline-done={prev==='done'} class:aline-err={ackStage==='error'&&stageStatus(key)==='error'}></div>
          {/if}
          {@const ss = stageStatus(key)}
          <div class="ast node-{ss}">
            <div class="adot">
              {#if ss==='active'}<span class="sp"></span>
              {:else if ss==='done'}✓
              {:else if ss==='error'}✕
              {:else}·{/if}
            </div>
            <span class="albl">{LABELS[key]}</span>
          </div>
        {/each}
      </div>
      {#if ackRawMsg}
        <span class="ack-msg" class:ack-msg-err={ackStage==='error'} class:ack-msg-ok={ackStage==='done'}>
          {ackRawMsg.length > 40 ? ackRawMsg.slice(0,40) + '…' : ackRawMsg}
        </span>
      {/if}
      <button class="ack-x" onclick={cancelTracking} title="Cerrar">×</button>
    </div>
  {/if}

  {#if error && ackStage === 'idle'}<div class="act-error">{error}</div>{/if}
</div>

<style>
  .act-wrap { border-top:0.5px solid var(--border-subtle); display:flex; flex-direction:column; }
  .act-wrap:first-child { border-top:none; }

  /* Fila principal */
  .act-main { display:flex; align-items:center; gap:6px; padding:calc(5px * var(--font-scale)) calc(11px * var(--font-scale)); min-height:calc(32px * var(--font-scale)); }
  .act-dot  { width:6px; height:6px; border-radius:50%; flex-shrink:0; background:var(--border-default); }
  .act-dot.on  { background:#3da85a; }
  .act-dot.off { background:#e05454; }
  .act-name  { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); min-width:58px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .act-st    { font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; color:var(--text-muted); }
  .act-st.on  { color:#3da85a; }
  .act-st.off { color:#e05454; }
  .act-total { font-size:calc(9px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .act-badge { font-size:calc(8px * var(--font-scale)); padding:1px 5px; border-radius:8px; background:#FEF3C7; color:#92400E; font-family:'DM Mono',monospace; }
  .act-btns  { display:flex; gap:2px; margin-left:auto; flex-shrink:0; }
  .tog       { padding:1px 6px; border:0.5px solid var(--border-default); border-radius:4px; background:none; cursor:pointer; font-size:calc(9px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); transition:background .1s; }
  .tog:disabled { opacity:.35; cursor:not-allowed; }
  .tog:hover:not(:disabled) { background:var(--interactive-hover); }
  .tog.ton   { background:#EAF3DE; color:#3B6D11; border-color:#3da85a55; }
  .tog.toff  { background:#FCEBEB; color:#A32D2D; border-color:#e0545455; }
  .tog.tauto { background:var(--bg-elevated); color:var(--text-primary); border-color:var(--border-default); }
  .act-error { font-size:calc(9px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; padding:2px calc(11px * var(--font-scale)); }

  /* Sub-fila ACK */
  .ack-bar {
    display:flex; align-items:center; gap:6px;
    padding:calc(4px * var(--font-scale)) calc(11px * var(--font-scale)) calc(5px * var(--font-scale)) calc(23px * var(--font-scale));
    background:var(--bg-inset);
    border-top:0.5px solid var(--border-subtle);
    animation:slideDown .12s ease;
  }
  .ack-bar.ack-err { background:#FCEBEB0a; }
  @keyframes slideDown { from { opacity:0; transform:translateY(-3px); } to { opacity:1; transform:none; } }

  .ack-stages { display:flex; align-items:center; gap:2px; flex-shrink:0; }
  .ast   { display:flex; flex-direction:column; align-items:center; gap:1px; min-width:20px; }
  .adot  { font-size:calc(9px * var(--font-scale)); height:13px; display:flex; align-items:center; justify-content:center; }
  .albl  { font-size:calc(7px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); line-height:1; }

  .node-pending .adot, .node-pending .albl { color:var(--text-muted); opacity:.3; }
  .node-active  .adot, .node-active  .albl { color:#4a90d9; }
  .node-done    .adot, .node-done    .albl { color:#3da85a; }
  .node-error   .adot, .node-error   .albl { color:#e05454; }

  .aline      { width:12px; height:1px; background:var(--border-subtle); margin-bottom:9px; border-radius:1px; transition:background .3s; }
  .aline-done { background:#3da85a88; }
  .aline-err  { background:#e0545444; }

  .ack-msg     { font-size:calc(8px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1; min-width:0; }
  .ack-msg-err { color:#e05454; }
  .ack-msg-ok  { color:#3da85a; }

  .ack-x { margin-left:auto; padding:0 3px; border:0.5px solid var(--border-subtle); border-radius:3px; background:none; cursor:pointer; color:var(--text-muted); font-size:calc(10px * var(--font-scale)); line-height:14px; flex-shrink:0; }
  .ack-x:hover { background:var(--bg-elevated); }

  .sp { display:inline-block; width:7px; height:7px; border:1.5px solid #4a90d933; border-top-color:#4a90d9; border-radius:50%; animation:spin .65s linear infinite; }
  @keyframes spin { to { transform:rotate(360deg); } }
</style>
