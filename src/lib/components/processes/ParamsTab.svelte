<!-- src/lib/components/processes/ParamsTab.svelte -->
<script lang="ts">
  import { processStore, canAdmin, processState } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const state  = $derived($processState);
  const kalman = $derived(state?.kalman);

  let editQ    = $state('');
  let editR    = $state('');
  let saving   = $state<string | null>(null);
  let msg      = $state('');
  let msgOk    = $state(true);

  // Inicializar con valores actuales
  $effect(() => {
    if (kalman && !editQ) editQ = String(kalman.Q_base?.[0] ?? '');
    if (kalman && !editR) editR = String(kalman.R?.[0] ?? '');
  });

  async function saveParam(cmd: string, payload: Record<string, any>, label: string) {
    saving = cmd; msg = ''; msgOk = true;
    try {
      const res = await processStore.command(processId, { cmd, ...payload });
      if (res.ok) { msg = `${label} actualizado`; msgOk = true; }
      else { msg = res.error ?? 'Error'; msgOk = false; }
    } catch (e: any) { msg = e.message; msgOk = false; }
    finally { saving = null; }
  }

  // Thresholds editables
  let thresholds = $state<Record<string, [string, string]>>({});

  $effect(() => {
    if (state?.rangos_linea && Object.keys(thresholds).length === 0) {
      thresholds = Object.fromEntries(
        Object.entries(state.rangos_linea).map(([k, v]) => [k, [String(v[0]), String(v[1])]])
      );
    }
  });

  async function saveThreshold(linea: string) {
    const [minS, maxS] = thresholds[linea];
    const min = parseFloat(minS), max = parseFloat(maxS);
    if (isNaN(min) || isNaN(max)) { msg = 'Valores inválidos'; msgOk = false; return; }
    await saveParam('set_threshold', { linea, min, max }, `Umbral línea ${linea}`);
  }

  async function saveAllThresholds() {
    const rangos: Record<string, [number, number]> = {};
    for (const [k, [minS, maxS]] of Object.entries(thresholds)) {
      const min = parseFloat(minS), max = parseFloat(maxS);
      if (isNaN(min) || isNaN(max)) { msg = 'Hay valores inválidos'; msgOk = false; return; }
      rangos[k] = [min, max];
    }
    await saveParam('set_all_thresholds', { rangos }, 'Todos los umbrales');
  }
</script>

<div class="params">
  {#if msg}
    <div class="msg" class:msg--ok={msgOk} class:msg--err={!msgOk}>{msg}</div>
  {/if}

  <!-- Kalman -->
  <section class="section">
    <div class="section-head">Filtro Kalman</div>
    <div class="section-desc">
      Q controla la varianza de proceso (qué tan rápido puede cambiar la humedad).
      R es la varianza de medición (ruido del sensor). Valores más pequeños = más confianza.
    </div>

    <div class="param-grid">
      <div class="param-row">
        <div class="param-info">
          <span class="param-name">Q base</span>
          <span class="param-current">{kalman?.Q_base?.[0] != null ? kalman.Q_base[0].toExponential(2) : '—'}</span>
        </div>
        <div class="param-input">
          <input class="mono" bind:value={editQ} placeholder="1e-5" disabled={!$canAdmin} />
          {#if $canAdmin}
            <button class="btn-apply" disabled={saving === 'set_kalman_Q'}
              onclick={() => saveParam('set_kalman_Q', { Q: parseFloat(editQ) }, 'Q')}>
              {saving === 'set_kalman_Q' ? '...' : 'aplicar'}
            </button>
          {/if}
        </div>
      </div>

      <div class="param-row">
        <div class="param-info">
          <span class="param-name">R (ruido)</span>
          <span class="param-current">{kalman?.R?.[0] != null ? kalman.R[0].toExponential(2) : '—'}</span>
        </div>
        <div class="param-input">
          <input class="mono" bind:value={editR} placeholder="1e-3" disabled={!$canAdmin} />
          {#if $canAdmin}
            <button class="btn-apply" disabled={saving === 'set_kalman_R'}
              onclick={() => saveParam('set_kalman_R', { R: parseFloat(editR) }, 'R')}>
              {saving === 'set_kalman_R' ? '...' : 'aplicar'}
            </button>
          {/if}
        </div>
      </div>

      {#if kalman?.P}
        <div class="kalman-p">
          <span class="param-name">P actual (covarianza)</span>
          <div class="p-vals">
            {#each kalman.P as p, i}
              <span class="p-val">{p.toExponential(2)}</span>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </section>

  <!-- Umbrales por línea -->
  <section class="section">
    <div class="section-head-row">
      <span class="section-head">Umbrales de humedad</span>
      {#if $canAdmin}
        <button class="btn-save-all" disabled={!!saving}
          onclick={saveAllThresholds}>
          guardar todos
        </button>
      {/if}
    </div>
    <div class="section-desc">
      Rango objetivo de VWC (m³/m³) por línea. Si está por debajo del mínimo se riega, por encima del máximo se corta.
    </div>

    <div class="thresh-grid">
      {#each Object.entries(thresholds).sort() as [linea, [minV, maxV]]}
        <div class="thresh-row">
          <span class="thresh-label">Línea {linea}</span>
          <div class="thresh-inputs">
            <div class="thresh-field">
              <label>mín</label>
              <input class="mono" bind:value={thresholds[linea][0]} disabled={!$canAdmin} />
            </div>
            <span class="thresh-sep">–</span>
            <div class="thresh-field">
              <label>máx</label>
              <input class="mono" bind:value={thresholds[linea][1]} disabled={!$canAdmin} />
            </div>
            {#if $canAdmin}
              <button class="btn-apply-sm"
                disabled={saving === `set_threshold_${linea}`}
                onclick={() => saveThreshold(linea)}>
                ✓
              </button>
            {/if}
          </div>
          {#if state?.rangos_linea?.[linea]}
            {@const [curMin, curMax] = state.rangos_linea[linea]}
            <span class="thresh-current">{(curMin*100).toFixed(1)}–{(curMax*100).toFixed(1)} %</span>
          {/if}
        </div>
      {/each}
    </div>
  </section>

  {#if !$canAdmin}
    <div class="read-only-note">Solo los administradores pueden modificar parámetros.</div>
  {/if}
</div>

<style>
  .params { display: flex; flex-direction: column; gap: calc(24px * var(--font-scale)); }

  .msg { padding: 8px 12px; border-radius: 6px; font-size: calc(12px * var(--font-scale)); }
  .msg--ok  { background: #EAF3DE; color: #3B6D11; }
  .msg--err { background: var(--error-bg); color: var(--error-color); }

  .section { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
  .section-head { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-muted); letter-spacing: .06em; text-transform: uppercase; font-family: 'DM Mono', monospace; }
  .section-head-row { display: flex; align-items: center; justify-content: space-between; }
  .section-desc { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); line-height: 1.6; }

  .param-grid { display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); }
  .param-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; flex-wrap: wrap; }
  .param-info { display: flex; flex-direction: column; gap: 2px; }
  .param-name { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .param-current { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .param-input { display: flex; align-items: center; gap: 6px; }
  .param-input input { width: 100px; padding: 5px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); outline: none; }
  .param-input input:focus { border-color: var(--text-primary); }
  .param-input input:disabled { opacity: 0.5; }

  .kalman-p { display: flex; flex-direction: column; gap: 6px; padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; }
  .p-vals { display: flex; flex-wrap: wrap; gap: 6px; }
  .p-val { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); background: var(--bg-inset); padding: 2px 6px; border-radius: 4px; }

  .btn-apply { padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); }
  .btn-apply:hover { background: var(--interactive-hover); }
  .btn-apply:disabled { opacity: 0.4; cursor: default; }

  .btn-save-all { padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: var(--text-primary); color: var(--bg-surface); cursor: pointer; font-size: calc(12px * var(--font-scale)); }
  .btn-save-all:disabled { opacity: 0.5; cursor: default; }

  .thresh-grid { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .thresh-row { display: flex; align-items: center; gap: calc(12px * var(--font-scale)); padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; flex-wrap: wrap; }
  .thresh-label { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; min-width: 60px; }
  .thresh-inputs { display: flex; align-items: center; gap: 6px; }
  .thresh-field { display: flex; flex-direction: column; gap: 2px; }
  .thresh-field label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); }
  .thresh-field input { width: 70px; padding: 4px 6px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; outline: none; }
  .thresh-field input:disabled { opacity: 0.5; }
  .thresh-sep { color: var(--text-muted); font-size: 14px; }
  .thresh-current { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-left: auto; }
  .btn-apply-sm { width: 28px; height: 28px; border: 0.5px solid var(--border-default); border-radius: 4px; background: none; cursor: pointer; color: #3B6D11; font-size: 13px; }
  .btn-apply-sm:hover { background: #EAF3DE; }
  .btn-apply-sm:disabled { opacity: 0.4; cursor: default; }

  .mono { font-family: 'DM Mono', monospace !important; }
  .read-only-note { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); text-align: center; padding: 12px; }
</style>
