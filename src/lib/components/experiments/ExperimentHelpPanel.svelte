<!-- src/lib/components/experiments/ExperimentHelpPanel.svelte -->
<script lang="ts">
  import { preferences, FONT_SCALES, type FontScale } from '$lib/stores/preferences';

  let open      = $state(false);
  type Tab = 'ayuda' | 'fuente';
  let activeTab = $state<Tab>('ayuda');

  const SCALE_VALUES = [0.8, 0.9, 1.0, 1.1, 1.2, 1.35, 1.5];

  let sliderIdx = $state(
    (() => {
      const idx = SCALE_VALUES.indexOf($preferences.expFontScaleValue);
      return idx >= 0 ? idx : SCALE_VALUES.findIndex(v => v >= 1.2) ?? 4;
    })()
  );

  const previewScale = $derived(SCALE_VALUES[sliderIdx] ?? 1.2);
  const applied      = $derived($preferences.expFontScaleValue);

  function applyScale() {
    preferences.setExpFontScaleValue(SCALE_VALUES[sliderIdx]);
  }

  function reset() {
    sliderIdx = SCALE_VALUES.indexOf(1.2);
    preferences.setExpFontScaleValue(1.2);
  }
</script>

<button class="fab" onclick={() => { open = true; activeTab = 'ayuda'; }}
  title="Ayuda y preferencias del experimento" aria-label="Abrir panel de ayuda">
  <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4"
       stroke-linecap="round" stroke-linejoin="round" width="18" height="18">
    <path d="M10 17c-3.5 0-6-2.5-6-5.5V7a1 1 0 0 1 2 0v3"/>
    <path d="M6 7V4a1 1 0 0 1 2 0v3"/>
    <path d="M8 4V3a1 1 0 0 1 2 0v4"/>
    <path d="M10 3.5a1 1 0 0 1 2 0V7"/>
    <path d="M12 5a1 1 0 0 1 2 0v6.5c0 3-2.5 5.5-6 5.5"/>
  </svg>
</button>

{#if open}
  <div class="overlay" onclick={() => open = false} role="presentation">
    <div class="modal" onclick={(e) => e.stopPropagation()}
         onkeydown={(e) => e.stopPropagation()}
         role="dialog" aria-modal="true" tabindex="-1">

      <div class="modal-head">
        <span class="modal-title">ayuda · experimentos</span>
        <button class="close-btn" onclick={() => open = false} aria-label="Cerrar">✕</button>
      </div>

      <div class="tabs">
        <button class="tab" class:active={activeTab === 'ayuda'}
          onclick={() => activeTab = 'ayuda'}>Guía</button>
        <button class="tab" class:active={activeTab === 'fuente'}
          onclick={() => activeTab = 'fuente'}>Fuente</button>
      </div>

      <div class="modal-body">

        {#if activeTab === 'ayuda'}

          <div class="term">
            <div class="term__name">Definitions</div>
            <div class="term__def">
              Describen la estructura del experimento. Hay cuatro tipos:
              <strong>constantes</strong> (valores fijos como masas o volúmenes),
              <strong>variables</strong> (campos que se registran en cada entry),
              <strong>expresiones</strong> (fórmulas calculadas automáticamente) y
              <strong>objetivos</strong> (condiciones de alerta sobre una variable o expresión).
            </div>
          </div>

          <div class="term">
            <div class="term__name">Entries</div>
            <div class="term__def">
              Cada fila de datos registrada en el experimento. Una entry puede contener valores
              para múltiples variables. Las columnas se configuran arrastrando desde el menú <code>+</code>
              en el encabezado de la tabla — arrastrá hacia la papelera para quitar una columna.
            </div>
          </div>

          <div class="term">
            <div class="term__name">Expresiones <span class="badge-expr">ƒ</span></div>
            <div class="term__def">
              Se calculan automáticamente para cada entry usando las constantes definidas y los
              valores registrados. Usá los keys de las constantes y variables en la fórmula,
              por ejemplo: <code>(masa_agua / V_suelo) * 100</code>.
              El resultado aparece en la columna con el badge <span class="badge-expr">ƒ</span>.
            </div>
          </div>

          <div class="term">
            <div class="term__name">Grupos</div>
            <div class="term__def">
              Permiten asociar constantes distintas a subconjuntos de entries — por ejemplo,
              distintas macetas con diferente masa de suelo. Las constantes de grupo sobreescriben
              las globales al calcular expresiones.
            </div>
          </div>

          <div class="term">
            <div class="term__name">Importar CSV</div>
            <div class="term__def">
              Usá <em>↑ importar CSV</em> para cargar datos en lote. El CSV debe tener columnas
              con los mismos keys que las variables del experimento. Las columnas extras se ignoran.
            </div>
          </div>

          <div class="footer">
            Los cambios en columnas y entries se guardan inmediatamente.
          </div>

        {:else if activeTab === 'fuente'}

          <div class="font-section">
            <div class="font-label">
              Tamaño de texto — experimentos
              <span class="font-pct">{Math.round(previewScale * 100)}%</span>
            </div>
            <div class="font-note">Independiente del tamaño del dashboard principal.</div>

            <div class="slider-row">
              <span class="slider-tick">A</span>
              <input type="range" min="0" max={SCALE_VALUES.length - 1} step="1"
                bind:value={sliderIdx} class="font-slider" />
              <span class="slider-tick large">A</span>
            </div>

            <div class="font-preview" style="--preview-scale: {previewScale}">
              <div class="preview-label">Vista previa</div>
              <div class="preview-block">
                <div class="preview-row header-row">
                  <span>timestamp</span>
                  <span>Iteración</span>
                  <span>θ actual <span class="badge-expr-sm">ƒ</span></span>
                  <span>Masa suelo</span>
                </div>
                <div class="preview-row">
                  <span class="muted">23-abr, 19:29</span>
                  <span>1</span>
                  <span class="expr-val">0.287</span>
                  <span>8.93</span>
                </div>
                <div class="preview-row">
                  <span class="muted">23-abr, 20:14</span>
                  <span>2</span>
                  <span class="expr-val">0.301</span>
                  <span>8.90</span>
                </div>
              </div>
            </div>

            <div class="font-actions">
              {#if Math.abs(previewScale - applied) > 0.01}
                <button class="apply-btn" onclick={applyScale}>
                  Aplicar {Math.round(previewScale * 100)}%
                </button>
              {:else}
                <span class="apply-applied">✓ Aplicado</span>
              {/if}
              <button class="reset-btn" onclick={reset}>Restablecer (120%)</button>
            </div>
          </div>

        {/if}

      </div>
    </div>
  </div>
{/if}

<style>
  .fab {
    position: fixed; bottom: 24px; right: 24px; z-index: 90;
    width: 40px; height: 40px; border-radius: 50%;
    border: 0.5px solid var(--border-default);
    background: var(--bg-surface); color: var(--text-muted);
    cursor: pointer; display: flex; align-items: center; justify-content: center;
    transition: all .15s;
  }
  .fab:hover { background: var(--interactive-hover); color: var(--text-secondary); border-color: var(--border-strong); }

  .overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.35);
    backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);
    display: flex; align-items: center; justify-content: center;
  }
  .modal {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 12px;
    width: 480px; max-width: calc(100vw - 32px);
    max-height: 88vh; overflow-y: auto;
    display: flex; flex-direction: column;
  }
  .modal-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: calc(13px * var(--font-scale)) calc(18px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    position: sticky; top: 0; background: var(--bg-surface); z-index: 1;
  }
  .modal-title {
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    color: var(--text-primary); letter-spacing: .04em; font-family: 'DM Mono', monospace;
  }
  .close-btn {
    width: 24px; height: 24px; border: none; background: transparent;
    color: var(--text-muted); font-size: calc(12px * var(--font-scale)); cursor: pointer;
    border-radius: 4px; display: flex; align-items: center; justify-content: center;
  }
  .close-btn:hover { background: var(--interactive-hover); }

  .tabs {
    display: flex; border-bottom: 0.5px solid var(--border-subtle);
    padding: 0 calc(18px * var(--font-scale));
  }
  .tab {
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    border: none; background: transparent; color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale));
    letter-spacing: .04em; cursor: pointer;
    border-bottom: 2px solid transparent; margin-bottom: -0.5px; transition: all .12s;
  }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }

  .modal-body {
    padding: calc(16px * var(--font-scale)) calc(18px * var(--font-scale));
    display: flex; flex-direction: column; gap: calc(18px * var(--font-scale));
  }

  .term { display: flex; flex-direction: column; gap: calc(5px * var(--font-scale)); }
  .term__name {
    font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
    display: flex; align-items: center; gap: 6px;
  }
  .term__def {
    font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.65;
  }
  .term__def strong { font-weight: 500; color: var(--text-primary); }
  .term__def code, .term__def em {
    font-family: 'DM Mono', monospace; font-style: normal;
    font-size: calc(11px * var(--font-scale)); color: var(--text-primary);
    background: var(--bg-elevated); padding: 1px 5px; border-radius: 3px;
  }
  .badge-expr {
    font-size: calc(10px * var(--font-scale)); padding: 1px 5px; border-radius: 20px;
    background: #EEEDFE; color: #3C3489;
  }
  .footer {
    font-size: calc(11px * var(--font-scale)); color: var(--text-muted);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
    border-top: 0.5px solid var(--border-subtle);
    padding-top: calc(12px * var(--font-scale));
  }

  /* Fuente tab */
  .font-section { display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }
  .font-label {
    display: flex; align-items: center; justify-content: space-between;
    font-size: calc(12px * var(--font-scale)); font-weight: 500;
    color: var(--text-primary); font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .font-note {
    font-size: calc(11px * var(--font-scale)); color: var(--text-muted); margin-top: -8px;
  }
  .font-pct {
    font-size: calc(12px * var(--font-scale)); color: var(--text-muted);
    background: var(--bg-elevated); padding: 2px 8px; border-radius: 4px;
  }
  .slider-row { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); }
  .slider-tick { font-family: 'DM Mono', monospace; color: var(--text-muted); user-select: none; }
  .slider-tick       { font-size: 11px; }
  .slider-tick.large { font-size: 18px; }
  .font-slider { flex: 1; }

  .font-preview {
    background: var(--bg-elevated); border: 0.5px solid var(--border-subtle);
    border-radius: 8px; overflow: hidden;
  }
  .preview-label {
    font-size: calc(10px * var(--font-scale)); color: var(--text-muted);
    font-family: 'DM Mono', monospace; letter-spacing: .06em;
    padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle); background: var(--bg-inset);
  }
  .preview-block {
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    display: flex; flex-direction: column;
  }
  .preview-row {
    display: grid; grid-template-columns: 120px 70px 90px 80px;
    gap: 8px; align-items: center;
    font-size: calc(12px * var(--preview-scale));
    font-family: 'DM Mono', monospace; color: var(--text-primary);
    padding: calc(4px * var(--font-scale)) 0;
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .preview-row:last-child { border-bottom: none; }
  .header-row { font-weight: 500; color: var(--text-secondary); font-size: calc(11px * var(--preview-scale)); }
  .muted { color: var(--text-muted); }
  .expr-val { color: #185FA5; }
  .badge-expr-sm {
    font-size: calc(9px * var(--preview-scale)); padding: 1px 4px; border-radius: 10px;
    background: #EEEDFE; color: #3C3489;
  }

  .font-actions { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); }
  .apply-btn {
    padding: calc(7px * var(--font-scale)) calc(18px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px; font-family: 'DM Mono', monospace;
    font-size: calc(12px * var(--font-scale)); font-weight: 500;
    letter-spacing: .04em; cursor: pointer;
  }
  .apply-btn:hover { opacity: .85; }
  .apply-applied {
    font-size: calc(12px * var(--font-scale)); color: var(--live-color);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .reset-btn {
    padding: calc(7px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 6px;
    background: transparent; color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale));
    cursor: pointer; letter-spacing: .04em;
  }
  .reset-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }

  @media (max-width: 640px) {
    .fab { bottom: 68px; right: 16px; }
    .modal { width: calc(100vw - 24px); max-height: 80vh; }
  }
</style>