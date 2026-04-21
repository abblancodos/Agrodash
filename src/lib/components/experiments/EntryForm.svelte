<script lang="ts">
  import { experimentStore, experimentContext } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import TimestampWarning from './TimestampWarning.svelte';
  import CsvUploadStep from './CsvUploadStep.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  // Pasos disponibles del experimento
  const steps = $derived(
    $experimentStore.definitions.filter(d => d.type === 'step' || d.type === 'csv_schema')
  );

  let selectedStep = $state<string>('');
  $effect(() => { if (!selectedStep && steps.length > 0) selectedStep = steps[0].key; });
  let fieldValues  = $state<Record<string, string>>({});
  let note         = $state('');
  let soilId       = $state('');
  let loading      = $state(false);
  let error        = $state('');
  let csvDone      = $state(false);

  const step = $derived(steps.find(s => s.key === selectedStep));
  const isCsv = $derived(step?.type === 'csv_schema');

  // Campos del paso seleccionado
  const fields = $derived(() => {
    if (!step) return [];
    const payload = step.payload as any;
    return (payload.fields ?? []) as { key: string; label: string; unit?: string }[];
  });

  // Calcular preview de expresiones con los valores actuales del form
  const previewCtx = $derived(() => {
    const base = { ...$experimentContext };
    for (const [k, v] of Object.entries(fieldValues)) {
      const n = parseFloat(v);
      if (!isNaN(n)) base[k] = n;
    }
    return base;
  });

  // Expresiones que referencian los campos de este paso
  const relatedExprs = $derived(
    $experimentStore.definitions.filter(d => {
      if (d.type !== 'expression') return false;
      const formula = (d.payload as any).formula as string ?? '';
      return fields().some(f => formula.includes(f.key));
    })
  );

  function evalExpr(formula: string, ctx: Record<string, number>) {
    const keys = Object.keys(ctx).sort((a, b) => b.length - a.length);
    let expr = formula;
    for (const k of keys) expr = expr.replace(new RegExp(`\\b${k}\\b`, 'g'), String(ctx[k]));
    if (!/^[\d\s\+\-\*\/\.\(\)\^%]+$/.test(expr)) return null;
    try { return new Function(`return (${expr.replace(/\^/g, '**')})`)(); } catch { return null; }
  }

  async function submit() {
    if (isCsv && !csvDone) { error = 'Primero subí el CSV'; return; }
    loading = true; error = '';
    try {
      const token = auth.getToken();
      const data: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(fieldValues)) {
        const n = parseFloat(v);
        data[k] = isNaN(n) ? v : n;
      }
      // Guardar también los outputs de expresiones calculadas
      for (const expr of relatedExprs) {
        const val = evalExpr((expr.payload as any).formula, previewCtx());
        if (val !== null) data[expr.key] = val;
      }

      const res = await fetch(
        `${API}/api/v1/experiments/${$experimentStore.experiment?.id}/events`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
          body: JSON.stringify({
            step_key:   selectedStep,
            event_type: step?.type ?? 'measurement',
            soil_id:    soilId || null,
            data,
            note:       note || null,
          }),
        }
      );
      const result = await res.json();
      if (!res.ok) { error = result.error ?? 'Error'; return; }
      experimentStore.addEvent(result);
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
      <span class="modal-title">nueva entry</span>
      <button class="modal-close" onclick={onClose}>✕</button>
    </div>

    <div class="modal-body">
      <TimestampWarning />

      <!-- Selector de paso -->
      {#if steps.length > 1}
        <div class="field">
          <label class="field-label" for="step-select">tipo de evento</label>
          <select id="step-select" class="field-input" bind:value={selectedStep}>
            {#each steps as s}
              <option value={s.key}>{s.label}</option>
            {/each}
          </select>
        </div>
      {/if}

      <!-- CSV upload -->
      {#if isCsv}
        <CsvUploadStep
          stepKey={selectedStep}
          onUploaded={(rows, cols) => { csvDone = true; fieldValues = { _csv_rows: rows.length.toString() }; }}
        />

      {:else}
        <!-- Campos del paso -->
        {#each fields() as f}
          <div class="field">
            <label class="field-label" for="field-{f.key}">
              {f.label}
              {#if f.unit}<span class="field-unit">({f.unit})</span>{/if}
            </label>
            <input id="field-{f.key}" class="field-input mono" bind:value={fieldValues[f.key]}
                   placeholder="0.000" inputmode="decimal" />
          </div>
        {/each}

        <!-- Preview de expresiones calculadas -->
        {#if relatedExprs.length > 0 && Object.keys(fieldValues).length > 0}
          <div class="calc-preview">
            <div class="calc-label">valores calculados</div>
            {#each relatedExprs as expr}
              {@const val = evalExpr((expr.payload as any).formula, previewCtx())}
              <div class="calc-row">
                <span class="calc-key">{expr.label}</span>
                <span class="calc-val">
                  {val !== null ? val.toFixed(6).replace(/\.?0+$/, '') : '—'}
                  {(expr.payload as any).unit ?? ''}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      {/if}

      <div class="field">
        <label class="field-label" for="entry-note">nota (opcional)</label>
        <input id="entry-note" class="field-input" bind:value={note} placeholder="observaciones..." />
      </div>

      {#if error}<div class="error-box">{error}</div>{/if}
    </div>

    <div class="modal-foot">
      <button class="btn-cancel" onclick={onClose}>cancelar</button>
      <button class="btn-save" disabled={loading} onclick={submit}>
        {loading ? 'guardando...' : 'guardar entry'}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: center; padding: 20px; }
  .modal { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 12px; width: 100%; max-width: 400px; max-height: 90vh; overflow-y: auto; }
  .modal-head { display: flex; align-items: center; padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); gap: 8px; }
  .modal-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .modal-close { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .modal-body { padding: calc(16px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
  .modal-foot { padding: calc(12px * var(--font-scale)) calc(16px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); display: flex; justify-content: flex-end; gap: 8px; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .field-unit { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }
  .field-input { padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; font-size: calc(13px * var(--font-scale)); background: var(--bg-surface); color: var(--text-primary); outline: none; }
  .field-input.mono { font-family: 'DM Mono', monospace; }
  .calc-preview { background: var(--bg-elevated); border-radius: 6px; padding: calc(10px * var(--font-scale)); }
  .calc-label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; margin-bottom: 6px; }
  .calc-row { display: flex; justify-content: space-between; padding: 3px 0; }
  .calc-key { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .calc-val { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: #185FA5; font-weight: 500; }
  .error-box { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }
  .btn-cancel { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); background: none; border: none; cursor: pointer; padding: 6px 12px; }
  .btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
  .btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
</style>