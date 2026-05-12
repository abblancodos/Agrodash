<!-- src/lib/components/processes/ActuatorRow.svelte -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { processStore } from '$lib/stores/process';

  let {
    processId, pipelineId, actuatorId, actuatorType,
    lastAction, totalOn, overrideActive, canOperate = false,
    label = '',
  }: {
    processId: string; pipelineId: string; actuatorId: string;
    actuatorType: string; lastAction: string | null;
    totalOn: number | null; overrideActive: boolean; canOperate?: boolean;
    label?: string;
  } = $props();

  // ── MQTT ACK progress ──────────────────────────────────────────────────────
  // Los ACKs llegan por WebSocket via processStore.onMqttAck().
  // El store despacha el mensaje crudo de ack/valvula al handler registrado acá.

  type AckStage = 'idle' | 'mqtt' | 'lora' | 'done' | 'error';

  let ackStage  = $state<AckStage>('idle');
  let ackErrMsg = $state('');
  let busy      = $state(false);
  let error     = $state('');
  let doneTimer: ReturnType<typeof setTimeout> | null = null;

  function clearDoneTimer() {
    if (doneTimer) { clearTimeout(doneTimer); doneTimer = null; }
  }

  function onAckMsg(raw: string) {
    if (raw.startsWith('MQTT_RECIBIDO') || raw.startsWith('MQTT_OVERRIDE')) {
      ackStage = 'mqtt';
    } else if (raw.startsWith('LORA_ENVIANDO')) {
      ackStage = 'lora';
    } else if (raw.startsWith('CONCENTRADOR:')) {
      ackStage = 'done';
      onSuccess();
    } else if (raw.startsWith('LORA_ERROR') || raw.startsWith('CONCENTRADOR_ERROR')) {
      ackStage  = 'error';
      ackErrMsg = raw;
      onError(raw);
    }
  }

  function onSuccess() {
    clearDoneTimer();
    busy = false;
    processStore.offMqttAck(actuatorId);
    doneTimer = setTimeout(() => {
      ackStage = 'idle';
    }, 3000);
  }

  function onError(msg: string) {
    clearDoneTimer();
    busy  = false;
    error = msg;
    processStore.offMqttAck(actuatorId);
    doneTimer = setTimeout(() => {
      ackStage = 'idle';
    }, 6000);
  }

  // Timeout de seguridad: si en 18s no llega respuesta, liberar el lock.
  let safetyTimer: ReturnType<typeof setTimeout> | null = null;
  function clearSafety() {
    if (safetyTimer) { clearTimeout(safetyTimer); safetyTimer = null; }
  }

  async function send(action: 'on' | 'off' | 'clear') {
    if (busy) return;

    busy = true; error = '';
    clearDoneTimer(); clearSafety();
    ackStage = 'idle'; ackErrMsg = '';

    try {
      if (action !== 'clear') {
        // Registrar handler de ACKs antes de enviar el comando
        processStore.onMqttAck(actuatorId, onAckMsg);

        // Safety timeout: libera el lock si el concentrador no responde
        safetyTimer = setTimeout(() => {
          if (busy) {
            ackStage  = 'error';
            ackErrMsg = 'Timeout: sin respuesta del concentrador (18s)';
            onError(ackErrMsg);
          }
        }, 18_000);
      }

      const cmd = action === 'clear'
        ? { type: 'command', cmd: 'ClearOverride', pipeline_id: pipelineId, actuator_id: actuatorId }
        : { type: 'command', cmd: 'Override', action, pipeline_id: pipelineId, actuator_id: actuatorId };

      // Intentar por WS primero (más rápido, sin HTTP round-trip)
      const sentViaWs = processStore.wsSend(cmd);

      if (!sentViaWs) {
        // Fallback a HTTP si el WS no está conectado
        const httpCmd = action === 'clear'
          ? { cmd: 'ClearOverride', pipeline_id: pipelineId, actuator_id: actuatorId }
          : { cmd: 'Override', action, pipeline_id: pipelineId, actuator_id: actuatorId };
        await processStore.command(processId, httpCmd);
      }

      if (action === 'clear') {
        await processStore.refreshPipelineState(processId, pipelineId);
        busy = false;
      }
      // Para on/off: busy se libera cuando llegue el ACK (onSuccess/onError)
    } catch (e: any) {
      error = e.message;
      ackStage = 'idle';
      busy = false;
      processStore.offMqttAck(actuatorId);
      clearSafety();
    }
  }

  function fmtOn(s: number): string {
    if (s < 60) return `${s.toFixed(0)}s`;
    if (s < 3600) return `${(s/60).toFixed(1)}m`;
    return `${(s/3600).toFixed(2)}h`;
  }

  onDestroy(() => {
    clearDoneTimer(); clearSafety();
    processStore.offMqttAck(actuatorId);
  });

  const isOn  = $derived(lastAction === 'on');
  const isOff = $derived(lastAction === 'off');
  const displayName = $derived(label?.trim() || actuatorType);

  type StageStatus = 'pending' | 'active' | 'done' | 'error';
  const ORDER = ['mqtt', 'lora', 'done'] as const;

  function stageStatus(key: typeof ORDER[number]): StageStatus {
    if (ackStage === 'idle') return 'pending';
    if (ackStage === 'error') {
      const ki = ORDER.indexOf(key);
      if (ki === 0) return 'done';
      if (ki === 1) return 'error';
      return 'pending';
    }
    const ci = ORDER.indexOf(ackStage as typeof ORDER[number]);
    const ki = ORDER.indexOf(key);
    if (ki < ci)  return 'done';
    if (ki === ci) return 'active';
    return 'pending';
  }
</script>

<div class="act-row">
  <div class="act-info">
    <span class="act-name">{displayName}</span>
    <span class="act-state" class:on={isOn} class:off={isOff}>{lastAction?.toUpperCase() ?? '—'}</span>
    {#if totalOn != null}<span class="act-meta">total ON: {fmtOn(totalOn)}</span>{/if}
    {#if overrideActive && ackStage === 'idle'}<span class="act-badge">override</span>{/if}
  </div>

  {#if ackStage !== 'idle'}
    <div class="ack-pipeline" class:ack-has-error={ackStage === 'error'}>
      {#each ORDER as key, i (key)}
        {#if i > 0}
          <div class="ack-line"
            class:line-active={stageStatus(ORDER[i]) !== 'pending' && ackStage !== 'error'}
            class:line-error={ackStage === 'error' && i === 2}>
          </div>
        {/if}
        {@const ss = stageStatus(key)}
        <div class="ack-node node-{ss}">
          <div class="ack-dot">
            {#if ss === 'active'}<span class="ack-spinner"></span>
            {:else if ss === 'done'}✓
            {:else if ss === 'error'}✕
            {:else}·{/if}
          </div>
          <span class="ack-lbl">{key === 'mqtt' ? 'MQTT' : key === 'lora' ? 'LoRa' : 'OK'}</span>
        </div>
      {/each}
      {#if ackStage === 'error' && ackErrMsg}
        <span class="ack-errtxt">{ackErrMsg}</span>
      {/if}
    </div>
  {/if}

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
  .act-name  { font-size:calc(11px * var(--font-scale)); color:var(--text-secondary); font-family:'DM Mono',monospace; font-weight:500; }
  .act-state { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; color:var(--text-muted); }
  .act-state.on  { color:#3da85a; }
  .act-state.off { color:#e05454; }
  .act-meta  { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .act-badge { font-size:calc(9px * var(--font-scale)); padding:1px 6px; border-radius:10px; background:#FEF3C7; color:#92400E; font-family:'DM Mono',monospace; }
  .act-btns  { display:flex; gap:4px; }
  .tog       { padding:calc(3px * var(--font-scale)) calc(9px * var(--font-scale)); border:0.5px solid var(--border-default); border-radius:6px; background:none; cursor:pointer; font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); transition:background .1s,color .1s; }
  .tog:disabled { opacity:.4; cursor:not-allowed; }
  .tog--on.active   { background:#EAF3DE; color:#3B6D11; border-color:#3da85a55; }
  .tog--off.active  { background:#FCEBEB; color:#A32D2D; border-color:#e0545455; }
  .tog--auto.active { background:var(--bg-elevated); color:var(--text-primary); }
  .tog--on:not(:disabled):not(.active):hover   { background:#EAF3DE; color:#3da85a; }
  .tog--off:not(:disabled):not(.active):hover  { background:#FCEBEB; color:#e05454; }
  .tog--auto:not(:disabled):not(.active):hover { background:var(--interactive-hover); }
  .act-error { font-size:calc(11px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; }

  /* ── MQTT ACK pipeline ── */
  .ack-pipeline {
    display: flex; align-items: center; gap: 3px;
    padding: calc(4px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--bg-elevated); border-radius: 6px;
    border: 0.5px solid var(--border-subtle);
    animation: fadeIn .15s ease;
  }
  .ack-pipeline.ack-has-error { border-color: #e0545444; }
  @keyframes fadeIn { from { opacity:0; transform:translateY(2px); } to { opacity:1; transform:none; } }

  .ack-node { display:flex; flex-direction:column; align-items:center; gap:1px; min-width:26px; }
  .ack-dot  { font-size:calc(11px * var(--font-scale)); line-height:1; display:flex; align-items:center; justify-content:center; width:16px; height:16px; }
  .ack-lbl  { font-size:calc(8px * var(--font-scale)); font-family:'DM Mono',monospace; }

  .node-pending .ack-dot, .node-pending .ack-lbl { color:var(--text-muted); opacity:.35; }
  .node-active  .ack-dot, .node-active  .ack-lbl { color:#4a90d9; }
  .node-done    .ack-dot, .node-done    .ack-lbl { color:#3da85a; }
  .node-error   .ack-dot, .node-error   .ack-lbl { color:#e05454; }

  .ack-line { width:14px; height:1.5px; background:var(--border-subtle); flex-shrink:0; margin-bottom:10px; border-radius:2px; transition:background .3s; }
  .ack-line.line-active { background:#3da85a88; }
  .ack-line.line-error  { background:#e0545444; }

  .ack-errtxt { font-size:calc(9px * var(--font-scale)); color:#e05454; font-family:'DM Mono',monospace; max-width:160px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-left:6px; }

  .ack-spinner { display:inline-block; width:9px; height:9px; border:1.5px solid #4a90d933; border-top-color:#4a90d9; border-radius:50%; animation:spin .65s linear infinite; }
  @keyframes spin { to { transform:rotate(360deg); } }
</style>
