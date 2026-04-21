<script lang="ts">
  import type { ExperimentEvent } from '$lib/stores/experiment';
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import TimestampWarning from './TimestampWarning.svelte';

  let { event, onClose }: { event: ExperimentEvent; onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let step    = $state(1);  // 1=advertencia, 2=motivo, 3=datos, 4=confirmar
  let reason  = $state('');
  let newData = $state<Record<string, string>>({});
  let note    = $state('');
  let loading = $state(false);
  let error   = $state('');

  // Pre-llenar con los datos originales
  $effect(() => {
    const d = event.data as Record<string, unknown>;
    newData = Object.fromEntries(
      Object.entries(d)
        .filter(([, v]) => typeof v === 'number' || typeof v === 'string')
        .map(([k, v]) => [k, String(v)])
    );
  });

  function dataPreview(data: Record<string, unknown>) {
    return Object.entries(data)
      .filter(([,v]) => typeof v === 'number' || typeof v === 'string')
      .map(([k,v]) => `${k}: ${v}`)
      .join(', ');
  }

  async function submit() {
    loading = true; error = '';
    try {
      const token = auth.getToken();
      const parsedData: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(newData)) {
        const n = parseFloat(v);
        parsedData[k] = isNaN(n) ? v : n;
      }
      const res = await fetch(
        `${API}/api/v1/experiments/${event.experiment_id}/events/${event.id}/correct`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
          body: JSON.stringify({
            step_key:          event.step_key,
            event_type:        event.event_type,
            soil_id:           event.soil_id,
            iteration:         event.iteration,
            data:              parsedData,
            note:              note || null,
            correction_reason: reason,
          }),
        }
      );
      const data = await res.json();
      if (!res.ok) { error = data.error ?? 'Error'; return; }
      experimentStore.applyCorrection(event.id, data);
      onClose();
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

<div class="overlay" onclick={onClose} role="presentation">
  <div class="modal" onclick={(e) => e.stopPropagation()}
       onkeydown={(e) => e.stopPropagation()}
       role="dialog" aria-modal="true" tabindex="-1">

    <div class="modal-head">
      <span class="modal-title">corregir entry — {event.step_key}</span>
      <span class="modal-badge">irreversible</span>
      <button class="modal-close" onclick={onClose}>✕</button>
    </div>

    <div class="modal-body">
      <!-- Paso 1: Advertencia -->
      {#if step === 1}
        <div class="warn-box">
          <div class="warn-icon">!</div>
          <div class="warn-text">
            <strong>Esta acción quedará registrada.</strong> La entry original
            quedará visible como anulada. No se puede deshacer.
          </div>
        </div>
        <div class="orig-card">
          <div class="orig-label">entry original · {new Date(event.recorded_at).toLocaleString('es-CR')}</div>
          <div class="orig-val">{dataPreview(event.data)}</div>
        </div>
        <button class="btn-next" onclick={() => step = 2}>entiendo, continuar →</button>

      <!-- Paso 2: Motivo -->
      {:else if step === 2}
        <div class="field">
          <label class="field-label">motivo de la corrección <span class="req">*</span></label>
          <textarea class="field-input" rows="3" bind:value={reason}
                    placeholder="Explicá por qué se corrige esta entry (mín. 10 caracteres)"></textarea>
          {#if reason.length > 0 && reason.length < 10}
            <span class="field-hint">mínimo 10 caracteres ({reason.length}/10)</span>
          {/if}
        </div>
        <div class="btn-row">
          <button class="btn-back" onclick={() => step = 1}>← volver</button>
          <button class="btn-next" disabled={reason.trim().length < 10}
                  onclick={() => step = 3}>continuar →</button>
        </div>

      <!-- Paso 3: Datos corregidos -->
      {:else if step === 3}
        <TimestampWarning />
        <div class="orig-card">
          <div class="orig-label">valores originales (tachados)</div>
          {#each Object.entries(event.data) as [k, v]}
            {#if typeof v === 'number' || typeof v === 'string'}
              <div class="orig-row">
                <span class="orig-key">{k}</span>
                <span class="orig-val-strike">{v}</span>
              </div>
            {/if}
          {/each}
        </div>
        {#each Object.entries(newData) as [k]}
          <div class="field">
            <label class="field-label">{k}</label>
            <input class="field-input mono" bind:value={newData[k]} />
          </div>
        {/each}
        <div class="field">
          <label class="field-label">nota adicional (opcional)</label>
          <input class="field-input" bind:value={note} placeholder="ej: sensor re-estabilizado" />
        </div>
        <div class="btn-row">
          <button class="btn-back" onclick={() => step = 2}>← volver</button>
          <button class="btn-next" onclick={() => step = 4}>continuar →</button>
        </div>

      <!-- Paso 4: Confirmación final -->
      {:else}
        <div class="danger-box">
          Esta acción es irreversible. La entry original quedará anulada y se creará
          una corrección. El historial de cambios quedará visible para todos los colaboradores.
        </div>
        {#if error}<div class="error-box">{error}</div>{/if}
        <div class="btn-row">
          <button class="btn-back" onclick={() => step = 3}>← volver</button>
          <button class="btn-danger" disabled={loading} onclick={submit}>
            {loading ? 'guardando...' : 'confirmar corrección'}
          </button>
        </div>
      {/if}
    </div>

    <!-- Indicador de pasos -->
    <div class="steps-indicator">
      {#each [1,2,3,4] as s}
        <div class="step-dot" class:active={s === step} class:done={s < step}></div>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: center; padding: 20px; }
  .modal { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 12px; width: 100%; max-width: 420px; overflow: hidden; }
  .modal-head { display: flex; align-items: center; gap: 8px; padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .modal-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .modal-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px; background: #FCEBEB; color: #A32D2D; }
  .modal-close { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .modal-body { padding: calc(16px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
  .warn-box { display: flex; gap: 10px; background: #FAEEDA; border: 0.5px solid #BA7517; border-radius: 6px; padding: calc(12px * var(--font-scale)); }
  .warn-icon { width: 20px; height: 20px; border-radius: 50%; background: #BA7517; color: #fff; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 500; flex-shrink: 0; }
  .warn-text { font-size: calc(12px * var(--font-scale)); color: #633806; line-height: 1.5; }
  .warn-text strong { color: #412402; }
  .danger-box { background: #FCEBEB; border: 0.5px solid rgba(162,45,45,0.4); border-radius: 6px; padding: calc(12px * var(--font-scale)); font-size: calc(12px * var(--font-scale)); color: #791F1F; line-height: 1.5; }
  .error-box { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }
  .orig-card { background: var(--bg-elevated); border-radius: 6px; padding: calc(10px * var(--font-scale)); }
  .orig-label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; margin-bottom: 6px; }
  .orig-val { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); }
  .orig-row { display: flex; justify-content: space-between; padding: 2px 0; }
  .orig-key { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .orig-val-strike { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; text-decoration: line-through; color: var(--text-muted); }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .field-hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .req { color: #A32D2D; }
  .field-input { padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; font-size: calc(13px * var(--font-scale)); background: var(--bg-surface); color: var(--text-primary); outline: none; }
  .field-input.mono { font-family: 'DM Mono', monospace; }
  textarea.field-input { resize: vertical; font-family: inherit; }
  .btn-row { display: flex; justify-content: space-between; align-items: center; margin-top: 4px; }
  .btn-back { background: none; border: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }
  .btn-next { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
  .btn-next:disabled { opacity: 0.4; cursor: not-allowed; }
  .btn-danger { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: #A32D2D; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
  .btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }
  .steps-indicator { display: flex; justify-content: center; gap: 6px; padding: calc(12px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); }
  .step-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--border-default); transition: all .2s; }
  .step-dot.active { background: var(--text-primary); transform: scale(1.3); }
  .step-dot.done { background: #639922; }
</style>
