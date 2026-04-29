<!-- src/lib/components/processes/ControlTab.svelte -->
<script lang="ts">
  import { processStore, canOperate, canAdmin, processState } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  let cmdError  = $state('');
  let cmdBusy   = $state<string | null>(null);   // key del botón activo

  const state    = $derived($processState);
  const lineas   = $derived(state ? Object.entries(state.estado_lineas).sort() : []);
  const kalman   = $derived(state?.kalman);
  const overrides = $derived(state?.overrides ?? {});

  // ── Helpers ────────────────────────────────────────────────────────────────

  function valveMode(linea: string): 'override-on' | 'override-off' | 'auto' {
    const ov = overrides[linea];
    if (ov === true)  return 'override-on';
    if (ov === false) return 'override-off';
    return 'auto';
  }

  function humedad(linea: string): string {
    const idx = parseInt(linea) - 1;
    const v = state?.calibrado?.[String(idx + 1)];
    if (v == null) return '—';
    return (v * 100).toFixed(1) + '%';
  }

  function humedadRaw(linea: string): string {
    const idx = parseInt(linea) - 1;
    const v = state?.campbell_crudo?.[String(idx + 1)];
    if (v == null) return '—';
    return (v * 100).toFixed(1) + '%';
  }

  function kalmanHat(linea: string): string {
    const idx = parseInt(linea) - 1;
    const v = kalman?.x_hat?.[idx];
    if (v == null) return '—';
    return (v * 100).toFixed(2) + '%';
  }

  function rango(linea: string): [number, number] | null {
    return state?.rangos_linea?.[linea] ?? null;
  }

  function gaugePercent(linea: string): number {
    const r = rango(linea);
    if (!r) return 0;
    const idx = parseInt(linea) - 1;
    const v = kalman?.x_hat?.[idx] ?? state?.calibrado?.[String(idx + 1)] ?? 0;
    const [min, max] = r;
    const range = (max - min) * 2;
    return Math.min(100, Math.max(0, ((v - (min - range * 0.2)) / (range * 1.4)) * 100));
  }

  function gaugeColor(linea: string): string {
    const r = rango(linea);
    if (!r) return '#8a9bb0';
    const idx = parseInt(linea) - 1;
    const v = kalman?.x_hat?.[idx] ?? state?.calibrado?.[String(idx + 1)] ?? 0;
    const [min, max] = r;
    if (v < min) return '#4a90d9';     // seco — regar
    if (v > max) return '#e07b54';     // húmedo — no regar
    return '#3da85a';                  // en rango
  }

  // ── Comandos ───────────────────────────────────────────────────────────────

  async function setOverride(linea: string, estado: boolean) {
    cmdBusy = `valve-${linea}-${estado}`; cmdError = '';
    try {
      await processStore.command(processId, { cmd: 'valve_override', linea, estado });
    } catch (e: any) { cmdError = e.message; }
    finally { cmdBusy = null; }
  }

  async function clearOverride(linea: string | null) {
    cmdBusy = linea ? `clear-${linea}` : 'clear-all'; cmdError = '';
    try {
      await processStore.command(processId, linea ? { cmd: 'clear_override', linea } : { cmd: 'clear_override' });
    } catch (e: any) { cmdError = e.message; }
    finally { cmdBusy = null; }
  }

  async function overrideAll(estado: boolean) {
    cmdBusy = `all-${estado}`; cmdError = '';
    try {
      await processStore.command(processId, { cmd: 'valve_override_all', estado });
    } catch (e: any) { cmdError = e.message; }
    finally { cmdBusy = null; }
  }
</script>

<div class="ctrl">
  {#if !state}
    <div class="no-state">
      <div class="spinner"></div>
      <span>esperando datos del control...</span>
    </div>

  {:else}

    <!-- Kalman header -->
    <div class="kalman-bar">
      <div class="kalman-info">
        <span class="kalman-label">Filtro Kalman</span>
        <span class="kalman-badge" class:conv={kalman?.convergido}>
          {kalman?.convergido ? '✓ convergido' : '⟳ convergiendo'}
        </span>
        {#if kalman?.n_updates != null}
          <span class="kalman-meta">{kalman.n_updates} ciclos</span>
        {/if}
      </div>
      {#if $canOperate}
        <div class="bulk-actions">
          <button class="btn-bulk btn-bulk--on"
            disabled={!!cmdBusy}
            onclick={() => overrideAll(true)}>
            ▶ todas ON
          </button>
          <button class="btn-bulk btn-bulk--off"
            disabled={!!cmdBusy}
            onclick={() => overrideAll(false)}>
            ■ todas OFF
          </button>
          <button class="btn-bulk"
            disabled={!!cmdBusy}
            onclick={() => clearOverride(null)}>
            ↺ modo auto
          </button>
        </div>
      {/if}
    </div>

    {#if cmdError}
      <div class="cmd-err">{cmdError}</div>
    {/if}

    <!-- Grid de líneas -->
    <div class="lineas-grid">
      {#each lineas as [linea, estado]}
        {@const mode    = valveMode(linea)}
        {@const r       = rango(linea)}
        {@const pct     = gaugePercent(linea)}
        {@const color   = gaugeColor(linea)}
        {@const valOn   = estado === 'on'}

        <div class="linea-card" class:valve-on={valOn} class:override={mode !== 'auto'}>

          <div class="linea-head">
            <span class="linea-num">Línea {linea}</span>
            <span class="mode-badge mode-badge--{mode}">
              {mode === 'auto' ? 'auto' : mode === 'override-on' ? 'override ON' : 'override OFF'}
            </span>
          </div>

          <!-- Gauge de humedad -->
          <div class="gauge-wrap">
            <div class="gauge-track">
              {#if r}
                <!-- Zona objetivo -->
                {@const trackRange = (r[1] - r[0]) * 2 * 1.4}
                {@const minPx = ((r[0] - (r[0] - trackRange * 0.2)) / (trackRange)) * 100}
                {@const maxPx = ((r[1] - (r[0] - trackRange * 0.2)) / (trackRange)) * 100}
                <div class="gauge-target"
                  style="left:{minPx}%; width:{maxPx - minPx}%">
                </div>
              {/if}
              <div class="gauge-fill" style="width:{pct}%; background:{color}"></div>
            </div>
          </div>

          <!-- Valores -->
          <div class="linea-vals">
            <div class="val-item">
              <span class="val-label">Kalman</span>
              <span class="val-num" style="color:{color}">{kalmanHat(linea)}</span>
            </div>
            <div class="val-item">
              <span class="val-label">calibrado</span>
              <span class="val-num">{humedad(linea)}</span>
            </div>
            <div class="val-item">
              <span class="val-label">crudo</span>
              <span class="val-num muted">{humedadRaw(linea)}</span>
            </div>
            {#if r}
              <div class="val-item">
                <span class="val-label">rango</span>
                <span class="val-num muted">{(r[0]*100).toFixed(1)}–{(r[1]*100).toFixed(1)}%</span>
              </div>
            {/if}
          </div>

          <!-- Válvula -->
          <div class="valve-row">
            <div class="valve-indicator" class:on={valOn}>
              <span class="valve-dot"></span>
              <span class="valve-label">{valOn ? 'ABIERTA' : 'CERRADA'}</span>
            </div>
            {#if $canOperate}
              <div class="valve-btns">
                {#if mode !== 'auto'}
                  <button class="vbtn vbtn--auto"
                    disabled={cmdBusy === `clear-${linea}`}
                    onclick={() => clearOverride(linea)}>
                    ↺ auto
                  </button>
                {/if}
                <button class="vbtn vbtn--on"
                  class:active={mode === 'override-on'}
                  disabled={!!cmdBusy}
                  onclick={() => setOverride(linea, true)}>
                  ON
                </button>
                <button class="vbtn vbtn--off"
                  class:active={mode === 'override-off'}
                  disabled={!!cmdBusy}
                  onclick={() => setOverride(linea, false)}>
                  OFF
                </button>
              </div>
            {/if}
          </div>

        </div>
      {/each}
    </div>

  {/if}
</div>

<style>
  .ctrl { display: flex; flex-direction: column; gap: calc(16px * var(--font-scale)); }

  .no-state { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 40px 0; }
  .spinner { width: 16px; height: 16px; border: 2px solid var(--border-subtle); border-top-color: var(--text-muted); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Kalman bar */
  .kalman-bar { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); background: var(--bg-elevated); border-radius: 8px; border: 0.5px solid var(--border-subtle); }
  .kalman-info { display: flex; align-items: center; gap: 10px; }
  .kalman-label { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; letter-spacing: .04em; }
  .kalman-badge { font-size: calc(11px * var(--font-scale)); padding: 2px 8px; border-radius: 10px; background: var(--bg-inset); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .kalman-badge.conv { background: #EAF3DE; color: #3B6D11; }
  .kalman-meta { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }

  .bulk-actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .btn-bulk { padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); font-family: 'DM Mono', monospace; }
  .btn-bulk:hover { background: var(--interactive-hover); }
  .btn-bulk:disabled { opacity: 0.4; cursor: default; }
  .btn-bulk--on  { border-color: #3da85a44; color: #3da85a; }
  .btn-bulk--off { border-color: #e0545444; color: #e05454; }

  .cmd-err { background: var(--error-bg); color: var(--error-color); padding: 8px 12px; border-radius: 6px; font-size: calc(12px * var(--font-scale)); }

  /* Grid de líneas */
  .lineas-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: calc(12px * var(--font-scale)); }

  .linea-card { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); padding: calc(14px * var(--font-scale)); background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; transition: border-color .15s; }
  .linea-card.valve-on  { border-color: #3da85a66; background: #EAF3DE0A; }
  .linea-card.override  { border-style: dashed; }

  .linea-head { display: flex; align-items: center; justify-content: space-between; }
  .linea-num  { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; }
  .mode-badge { font-size: calc(10px * var(--font-scale)); padding: 1px 7px; border-radius: 10px; font-family: 'DM Mono', monospace; }
  .mode-badge--auto         { background: var(--bg-elevated); color: var(--text-muted); }
  .mode-badge--override-on  { background: #EAF3DE; color: #3B6D11; }
  .mode-badge--override-off { background: #FCEBEB; color: #A32D2D; }

  /* Gauge */
  .gauge-wrap { padding: 0 2px; }
  .gauge-track { height: 8px; background: var(--bg-elevated); border-radius: 4px; position: relative; overflow: hidden; }
  .gauge-target { position: absolute; top: 0; bottom: 0; background: rgba(74,154,98,0.2); }
  .gauge-fill   { height: 100%; border-radius: 4px; transition: width .4s ease, background .4s; }

  /* Valores */
  .linea-vals { display: grid; grid-template-columns: 1fr 1fr; gap: 4px calc(12px * var(--font-scale)); }
  .val-item { display: flex; flex-direction: column; gap: 1px; }
  .val-label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); letter-spacing: .04em; }
  .val-num { font-size: calc(13px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; color: var(--text-primary); }
  .val-num.muted { color: var(--text-muted); font-weight: 400; }

  /* Válvula */
  .valve-row { display: flex; align-items: center; justify-content: space-between; padding-top: calc(6px * var(--font-scale)); border-top: 0.5px solid var(--border-subtle); }
  .valve-indicator { display: flex; align-items: center; gap: 6px; }
  .valve-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--border-default); transition: background .2s; }
  .valve-indicator.on .valve-dot { background: #3da85a; box-shadow: 0 0 6px #3da85a88; }
  .valve-label { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); }
  .valve-indicator.on .valve-label { color: #3da85a; }

  .valve-btns { display: flex; gap: 4px; }
  .vbtn { padding: calc(4px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); transition: all .1s; }
  .vbtn:hover { background: var(--interactive-hover); color: var(--text-primary); }
  .vbtn:disabled { opacity: 0.35; cursor: default; }
  .vbtn--on.active  { background: #EAF3DE; color: #3B6D11; border-color: #3da85a44; }
  .vbtn--off.active { background: #FCEBEB; color: #A32D2D; border-color: #e0545444; }
  .vbtn--auto { color: var(--text-muted); }

  @media (max-width: 640px) {
    .lineas-grid { grid-template-columns: 1fr; }
    .kalman-bar { flex-direction: column; align-items: flex-start; }
  }
</style>
