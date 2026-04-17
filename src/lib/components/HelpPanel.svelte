<script lang="ts">
  import { preferences, FONT_SCALES, type FontScale } from '$lib/stores/preferences';

  let open      = $state(false);
  type Tab = 'ayuda' | 'fuente' | 'idioma';
  let activeTab = $state<Tab>('ayuda');

  // Slider local — solo se aplica al hacer click en Aplicar
  const SCALE_VALUES = [0.8, 0.9, 1.0, 1.1, 1.2, 1.35, 1.5];
  let sliderIdx = $state(
    SCALE_VALUES.indexOf($preferences.fontScale in FONT_SCALES
      ? FONT_SCALES[$preferences.fontScale as FontScale]
      : 1.0)
  );
  // Si no coincide exactamente, default al índice 2 (1.0)
  $effect(() => {
    if (sliderIdx < 0) sliderIdx = 2;
  });

  const previewScale = $derived(SCALE_VALUES[sliderIdx] ?? 1.0);
  const applied      = $derived(FONT_SCALES[$preferences.fontScale]);

  function applyScale() {
    // Encontrar la FontScale más cercana al valor del slider
    const val = SCALE_VALUES[sliderIdx];
    const entry = Object.entries(FONT_SCALES).reduce((best, [k, v]) =>
      Math.abs(v - val) < Math.abs(FONT_SCALES[best as FontScale] - val) ? k : best
    , 'md' as string);
    preferences.setFontScaleValue(val);
  }
</script>

<!-- Botón flotante — ícono de mano -->
<button class="fab" onclick={() => { open = true; activeTab = 'ayuda'; }}
  title="Panel de ayuda y preferencias" aria-label="Abrir panel de ayuda">
  <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4"
       stroke-linecap="round" stroke-linejoin="round" width="18" height="18">
    <!-- Palma -->
    <path d="M10 17c-3.5 0-6-2.5-6-5.5V7a1 1 0 0 1 2 0v3"/>
    <!-- Dedo índice -->
    <path d="M6 7V4a1 1 0 0 1 2 0v3"/>
    <!-- Dedo medio -->
    <path d="M8 4V3a1 1 0 0 1 2 0v4"/>
    <!-- Dedo anular -->
    <path d="M10 3.5a1 1 0 0 1 2 0V7"/>
    <!-- Dedo meñique -->
    <path d="M12 5a1 1 0 0 1 2 0v6.5c0 3-2.5 5.5-6 5.5"/>
  </svg>
</button>

<!-- Modal -->
{#if open}
  <div class="overlay" onclick={() => open = false} role="presentation">
    <div class="modal" onclick={(e) => e.stopPropagation()}
         role="dialog" aria-modal="true" tabindex="-1">

      <div class="modal-head">
        <span class="modal-title">Panel de ayuda</span>
        <button class="close-btn" onclick={() => open = false} aria-label="Cerrar">✕</button>
      </div>

      <!-- Tabs -->
      <div class="tabs">
        <button class="tab" class:active={activeTab === 'ayuda'}
          onclick={() => activeTab = 'ayuda'}>Indicadores</button>
        <button class="tab" class:active={activeTab === 'fuente'}
          onclick={() => activeTab = 'fuente'}>Fuente</button>
        <button class="tab disabled" disabled title="Próximamente">
          Idioma — Español
        </button>
      </div>

      <div class="modal-body">

        <!-- ── Tab: Indicadores ────────────────────────────────────── -->
        {#if activeTab === 'ayuda'}

          <div class="term">
            <div class="term__name">Score σ (sigma)</div>
            <div class="term__def">
              Desviaciones estándar que se aleja el último valor de la media de las últimas 24 horas.
              <strong>0σ</strong> = exactamente en la media.
              <strong>1.5σ</strong> = inusual.
              <strong>3σ+</strong> = anomalía clara (ocurre en menos del 0.3% de los casos).
            </div>
            <div class="term__formula">score = |valor_actual − media_24h| / σ_24h</div>
          </div>

          <div class="term">
            <div class="term__name">r (correlación de Pearson)</div>
            <div class="term__def">
              Qué tan parecido se comportan dos sensores para la misma variable.
              De <strong>−1</strong> (opuestos) a <strong>+1</strong> (idénticos).
              Con <strong>r ≥ 0.85</strong> los sensores se agrupan al final de la card — información redundante.
            </div>
            <div class="term__formula">r = Σ(xᵢ−x̄)(yᵢ−ȳ) / √[Σ(xᵢ−x̄)² · Σ(yᵢ−ȳ)²]</div>
          </div>

          <div class="term">
            <div class="term__name">Colores de estado</div>
            <div class="color-rows">
              <div class="color-row">
                <span class="pill pill-ok">normal</span>
                <span>Score &lt; 1.5σ — rango habitual.</span>
              </div>
              <div class="color-row">
                <span class="pill pill-warn">anomalía</span>
                <span>Score 1.5–3σ — conviene revisarlo.</span>
              </div>
              <div class="color-row">
                <span class="pill pill-alert">alerta</span>
                <span>Score &gt; 3σ — muy fuera de lo normal.</span>
              </div>
              <div class="color-row">
                <span class="pill pill-info">r = 0.xx</span>
                <span>Correlación alta — variable redundante agrupada.</span>
              </div>
            </div>
          </div>

          <div class="term">
            <div class="term__name">Último dato</div>
            <div class="term__def">
              <span class="c-fresh">Verde</span> &lt; 5 min ·
              <span class="c-recent">gris</span> &lt; 30 min ·
              <span class="c-stale">ámbar</span> &lt; 24 h ·
              <span class="c-dead">rojo</span> ≥ 24 h (posiblemente desconectado).
            </div>
          </div>

          <div class="term">
            <div class="term__name">Media 24h y rate of change</div>
            <div class="term__def">
              La media es el promedio de las últimas 24 horas — referencia del score σ.
              El rate of change es la diferencia con el valor de hace una hora: detecta tendencias rápidas antes de que el score suba.
            </div>
          </div>

          <div class="footer">
            Estadísticas recalculadas automáticamente cada 5 minutos.
          </div>

        <!-- ── Tab: Fuente ──────────────────────────────────────────── -->
        {:else if activeTab === 'fuente'}

          <div class="font-section">
            <div class="font-label">
              Tamaño de texto
              <span class="font-pct">{Math.round(previewScale * 100)}%</span>
            </div>

            <div class="slider-row">
              <span class="slider-tick">A</span>
              <input type="range" min="0" max={SCALE_VALUES.length - 1} step="1"
                bind:value={sliderIdx} class="font-slider" />
              <span class="slider-tick large">A</span>
            </div>

            <!-- Preview vivo -->
            <div class="font-preview" style="--preview-scale: {previewScale}">
              <div class="preview-label">Vista previa</div>
              <div class="preview-block">
                <div class="preview-name">Caja S
                  <span class="preview-badge warn">2.4σ</span>
                </div>
                <div class="preview-row">
                  <span class="preview-muted">#5</span>
                  <span>Humedad</span>
                  <span class="preview-val">0.19</span>
                  <span class="preview-ago fresh">hace 3 min</span>
                </div>
                <div class="preview-row">
                  <span class="preview-muted">#5</span>
                  <span>EC</span>
                  <span class="preview-val warn">0.1240</span>
                  <span class="preview-ago stale">hace 18 min</span>
                </div>
                <div class="preview-row">
                  <span class="preview-muted">#2</span>
                  <span>Temperatura</span>
                  <span class="preview-val">21.74</span>
                  <span class="preview-ago dead">hace 2 meses</span>
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
              <button class="reset-btn" onclick={() => { sliderIdx = 2; preferences.setFontScaleValue(1.0); }}>
                Restablecer
              </button>
            </div>
          </div>

        {/if}

      </div>
    </div>
  </div>
{/if}

<style>
  /* ── FAB ─────────────────────────────────────────────────────────────── */
  .fab {
    position: fixed; bottom: 24px; right: 24px; z-index: 90;
    width: 40px; height: 40px; border-radius: 50%;
    border: 0.5px solid var(--border-default);
    background: var(--bg-surface); color: var(--text-muted);
    cursor: pointer; display: flex; align-items: center; justify-content: center;
    transition: all .15s;
  }
  .fab:hover { background: var(--interactive-hover); color: var(--text-secondary); border-color: var(--border-strong); }

  /* ── Overlay + Modal ─────────────────────────────────────────────────── */
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
    transition: background .1s;
  }
  .close-btn:hover { background: var(--interactive-hover); }

  /* ── Tabs ────────────────────────────────────────────────────────────── */
  .tabs {
    display: flex; gap: 0;
    border-bottom: 0.5px solid var(--border-subtle);
    padding: 0 calc(18px * var(--font-scale));
  }
  .tab {
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    border: none; background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    font-size: calc(11px * var(--font-scale));
    letter-spacing: .04em; cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -0.5px;
    transition: all .12s;
  }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }
  .tab.disabled { color: var(--text-faint); cursor: not-allowed; font-style: italic; }

  /* ── Body ────────────────────────────────────────────────────────────── */
  .modal-body {
    padding: calc(16px * var(--font-scale)) calc(18px * var(--font-scale));
    display: flex; flex-direction: column;
    gap: calc(18px * var(--font-scale));
  }

  /* ── Indicadores ─────────────────────────────────────────────────────── */
  .term { display: flex; flex-direction: column; gap: calc(5px * var(--font-scale)); }
  .term__name {
    font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .term__def { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.65; }
  .term__def strong { font-weight: 500; color: var(--text-primary); }
  .term__formula {
    font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace;
    color: var(--text-muted); background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 5px; padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale));
    letter-spacing: .04em;
  }
  .color-rows { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); margin-top: calc(4px * var(--font-scale)); }
  .color-row {
    display: flex; align-items: center; gap: calc(10px * var(--font-scale));
    font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.5;
  }
  .pill {
    font-size: calc(10px * var(--font-scale)); font-weight: 500;
    padding: calc(3px * var(--font-scale)) calc(8px * var(--font-scale));
    border-radius: 4px; letter-spacing: .04em; white-space: nowrap; flex-shrink: 0;
    min-width: 68px; text-align: center; font-family: 'DM Mono', monospace;
  }
  .pill-ok    { background: var(--live-bg);             color: var(--live-color); }
  .pill-warn  { background: rgba(186,117,23,0.15);      color: #e8a838; }
  .pill-alert { background: var(--error-bg);            color: var(--error-color); }
  .pill-info  { background: rgba(74,154,98,0.12);       color: var(--tb-accent); }
  .c-fresh  { color: var(--live-color);  font-weight: 500; }
  .c-recent { color: var(--text-secondary); font-weight: 500; }
  .c-stale  { color: #e8a838; font-weight: 500; }
  .c-dead   { color: var(--error-color); font-weight: 500; }
  .footer {
    font-size: calc(11px * var(--font-scale)); color: var(--text-muted);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
    border-top: 0.5px solid var(--border-subtle);
    padding-top: calc(12px * var(--font-scale));
  }

  /* ── Fuente tab ──────────────────────────────────────────────────────── */
  .font-section { display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }

  .font-label {
    display: flex; align-items: center; justify-content: space-between;
    font-size: calc(12px * var(--font-scale)); font-weight: 500;
    color: var(--text-primary); font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .font-pct {
    font-size: calc(12px * var(--font-scale)); color: var(--text-muted);
    background: var(--bg-elevated); padding: calc(2px * var(--font-scale)) calc(8px * var(--font-scale)); border-radius: 4px;
  }

  .slider-row {
    display: flex; align-items: center; gap: calc(10px * var(--font-scale));
  }
  .slider-tick { font-family: 'DM Mono', monospace; color: var(--text-muted); user-select: none; }
  .slider-tick       { font-size: 11px; }
  .slider-tick.large { font-size: 18px; }
  .font-slider { flex: 1; }

  /* Preview box — usa su propio multiplicador local, independiente del global */
  .font-preview {
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 8px;
    overflow: hidden;
  }
  .preview-label {
    font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace;
    letter-spacing: .06em; padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    background: var(--bg-inset);
  }
  .preview-block { padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(4px * var(--font-scale)); }
  .preview-name {
    font-size: calc(13px * var(--preview-scale));
    font-weight: 500; color: var(--text-primary);
    font-family: 'DM Mono', monospace;
    display: flex; align-items: center; gap: calc(8px * var(--font-scale));
    margin-bottom: 4px;
  }
  .preview-badge {
    font-size: calc(10px * var(--preview-scale));
    padding: calc(2px * var(--font-scale)) calc(6px * var(--font-scale)); border-radius: 4px; font-weight: 500;
  }
  .preview-badge.warn { background: rgba(186,117,23,0.18); color: #e8a838; }
  .preview-row {
    display: grid; grid-template-columns: 28px 1fr 70px 90px;
    gap: calc(8px * var(--font-scale)); align-items: center;
    font-size: calc(12px * var(--preview-scale));
    font-family: 'DM Mono', monospace;
    color: var(--text-primary);
    padding: calc(3px * var(--font-scale)) 0;
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .preview-row:last-child { border-bottom: none; }
  .preview-muted { color: var(--text-muted); }
  .preview-val   { text-align: right; font-weight: 500; }
  .preview-val.warn { color: #e8a838; }
  .preview-ago   { font-size: calc(10px * var(--preview-scale)); text-align: right; }
  .preview-ago.fresh  { color: var(--live-color); }
  .preview-ago.stale  { color: #e8a838; }
  .preview-ago.dead   { color: var(--error-color); }

  .font-actions { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); }
  .apply-btn {
    padding: calc(7px * var(--font-scale)) calc(18px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px;
    font-family: 'DM Mono', monospace;
    font-size: calc(12px * var(--font-scale)); font-weight: 500;
    letter-spacing: .04em; cursor: pointer; transition: opacity .15s;
  }
  .apply-btn:hover { opacity: .85; }
  .apply-applied {
    font-size: calc(12px * var(--font-scale)); color: var(--live-color);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .reset-btn {
    padding: calc(7px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 6px; background: transparent;
    color: var(--text-muted); font-family: 'DM Mono', monospace;
    font-size: calc(11px * var(--font-scale)); cursor: pointer;
    letter-spacing: .04em; transition: all .12s;
  }
  .reset-btn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
</style>