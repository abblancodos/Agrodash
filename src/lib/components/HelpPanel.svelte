<script lang="ts">
  import { preferences, FONT_SCALES, type FontScale } from '$lib/stores/preferences';
  let open = $state(false);
</script>

<!-- Botón flotante -->
<button
  class="fab"
  onclick={() => open = true}
  title="Qué significan los indicadores"
  aria-label="Abrir panel de ayuda"
>
  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
       stroke-linecap="round" stroke-linejoin="round" width="18" height="18">
    <circle cx="8" cy="8" r="6"/>
    <path d="M8 11v-1"/>
    <path d="M8 8c0-1.5 2-1.5 2-3a2 2 0 0 0-4 0"/>
  </svg>
</button>

<!-- Modal -->
{#if open}
  <div class="overlay" onclick={() => open = false} role="presentation">
    <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">

      <div class="modal-head">
        <span class="modal-title">Indicadores del dashboard</span>
        <button class="close-btn" onclick={() => open = false} aria-label="Cerrar">✕</button>
      </div>

      <div class="modal-body">

        <div class="term">
          <div class="term__name">Score σ (sigma)</div>
          <div class="term__def">
            Desviaciones estándar que se aleja el último valor de la media de las últimas 24 horas.
            Un score de <strong>0σ</strong> significa que el valor está exactamente en la media.
            <strong>1.5σ</strong> empieza a ser inusual. <strong>3σ</strong> o más es una anomalía
            clara — ocurre en menos del 0.3% de los casos en una distribución normal.
          </div>
          <div class="term__formula">score = |valor_actual − media_24h| / σ_24h</div>
        </div>

        <div class="term">
          <div class="term__name">r (correlación de Pearson)</div>
          <div class="term__def">
            Mide qué tan parecido es el comportamiento de dos sensores para la misma variable.
            Va de <strong>−1</strong> (totalmente opuestos) a <strong>+1</strong> (idénticos).
            Los sensores con <strong>r ≥ 0.85</strong> se agrupan al final de la card porque
            dan información redundante — si uno sube, el otro también.
          </div>
          <div class="term__formula">r = Σ(xᵢ−x̄)(yᵢ−ȳ) / √[Σ(xᵢ−x̄)² · Σ(yᵢ−ȳ)²]</div>
        </div>

        <div class="term">
          <div class="term__name">Colores de estado</div>
          <div class="term__def">Nivel de atención que requiere cada sensor o caja:</div>
          <div class="color-rows">
            <div class="color-row">
              <span class="pill pill-ok">normal</span>
              <span>Score &lt; 1.5σ — opera dentro de su rango habitual.</span>
            </div>
            <div class="color-row">
              <span class="pill pill-warn">anomalía</span>
              <span>Score 1.5–3σ — se alejó de la media más de lo usual. Conviene revisarlo.</span>
            </div>
            <div class="color-row">
              <span class="pill pill-alert">alerta</span>
              <span>Score &gt; 3σ — muy fuera de lo normal (&lt; 0.3% de las lecturas).</span>
            </div>
            <div class="color-row">
              <span class="pill pill-info">r = 0.xx</span>
              <span>Correlación alta entre sensores — variable redundante, agrupada al final.</span>
            </div>
          </div>
        </div>

        <div class="term">
          <div class="term__name">Último dato</div>
          <div class="term__def">
            Tiempo desde la última lectura registrada.
            <span class="c-fresh">Verde</span> &lt; 5 min ·
            <span class="c-recent">gris</span> &lt; 30 min ·
            <span class="c-stale">ámbar</span> &lt; 24 h ·
            <span class="c-dead">rojo</span> ≥ 24 h (sensor posiblemente desconectado).
          </div>
        </div>

        <div class="term">
          <div class="term__name">Media 24h</div>
          <div class="term__def">
            Promedio de todas las lecturas del sensor en las últimas 24 horas.
            Es la referencia que usa el cálculo del score σ.
          </div>
        </div>

        <div class="term">
          <div class="term__name">Rate of change</div>
          <div class="term__def">
            Diferencia entre el valor actual y el valor de hace una hora.
            Útil para detectar tendencias rápidas aunque el score σ todavía no sea alto.
          </div>
        </div>


        <div class="term">
          <div class="term__name">Tamaño de texto</div>
          <div class="scale-btns">
            {#each (['sm', 'md', 'lg'] as FontScale[]) as scale}
              <button
                class="scale-btn"
                class:active={$preferences.fontScale === scale}
                onclick={() => preferences.setFontScale(scale)}
              >
                {scale === 'sm' ? 'pequeño' : scale === 'md' ? 'normal' : 'grande'}
              </button>
            {/each}
          </div>
        </div>

        <div class="footer">
          Las estadísticas se recalculan automáticamente cada 5 minutos.
        </div>

      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Botón flotante ──────────────────────────────────────────────────── */
  .fab {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 90;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    border: 0.5px solid var(--border-default);
    background: var(--bg-surface);
    color: var(--text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all .15s;
  }
  .fab:hover {
    background: var(--interactive-hover);
    color: var(--text-secondary);
    border-color: var(--border-strong);
  }

  /* ── Overlay ─────────────────────────────────────────────────────────── */
  .overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.3);
    backdrop-filter: blur(2px);
    -webkit-backdrop-filter: blur(2px);
    display: flex; align-items: center; justify-content: center;
  }

  /* ── Modal ───────────────────────────────────────────────────────────── */
  .modal {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 12px;
    width: 500px;
    max-width: calc(100vw - 32px);
    max-height: 88vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .modal-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px 12px;
    border-bottom: 0.5px solid var(--border-subtle);
    position: sticky; top: 0;
    background: var(--bg-surface); z-index: 1;
  }
  .modal-title {
    font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary);
    letter-spacing: .04em; font-family: 'DM Mono', monospace;
  }
  .close-btn {
    width: 24px; height: 24px; border: none; background: transparent;
    color: var(--text-muted); font-size: calc(14px * var(--font-scale)); cursor: pointer;
    border-radius: 4px; display: flex; align-items: center; justify-content: center;
    transition: background .1s;
  }
  .close-btn:hover { background: var(--interactive-hover); }

  /* ── Contenido ───────────────────────────────────────────────────────── */
  .modal-body {
    padding: 16px 18px;
    display: flex; flex-direction: column; gap: 20px;
  }
  .term { display: flex; flex-direction: column; gap: 5px; }
  .term__name {
    font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
  }
  .term__def {
    font-size: calc(14px * var(--font-scale)); color: var(--text-secondary); line-height: 1.65;
  }
  .term__def strong { font-weight: 500; color: var(--text-primary); }
  .term__formula {
    font-size: calc(14px * var(--font-scale)); font-family: 'DM Mono', monospace;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 5px; padding: 5px 10px; letter-spacing: .04em;
  }

  /* ── Colores ─────────────────────────────────────────────────────────── */
  .color-rows { display: flex; flex-direction: column; gap: 7px; margin-top: 6px; }
  .color-row {
    display: flex; align-items: center; gap: 10px;
    font-size: calc(14px * var(--font-scale)); color: var(--text-secondary); line-height: 1.5;
  }
  .pill {
    font-size: calc(14px * var(--font-scale)); font-weight: 500; padding: 3px 8px; border-radius: 4px;
    letter-spacing: .04em; white-space: nowrap; flex-shrink: 0;
    min-width: 64px; text-align: center; font-family: 'DM Mono', monospace;
  }
  .pill-ok    { background: var(--live-bg);  color: var(--live-color); }
  .pill-warn  { background: rgba(186,117,23,0.15); color: #e8a838; }
  .pill-alert { background: var(--error-bg); color: var(--error-color); }
  .pill-info  { background: rgba(74,154,98,0.12);  color: var(--tb-accent); }

  /* ── Tiempo relativo inline ──────────────────────────────────────────── */
  .c-fresh  { color: #3B6D11; font-weight: 500; }
  .c-recent { color: var(--text-secondary); font-weight: 500; }
  .c-stale  { color: #854F0B; font-weight: 500; }
  .c-dead   { color: #A32D2D; font-weight: 500; }

  /* ── Footer ──────────────────────────────────────────────────────────── */
  .footer {
    font-size: calc(14px * var(--font-scale)); color: var(--text-muted);
    font-family: 'DM Mono', monospace; letter-spacing: .04em;
    border-top: 0.5px solid var(--border-subtle);
    padding-top: 12px;
  }

  .scale-btns { display: flex; gap: 6px; margin-top: 6px; }
  .scale-btn {
    padding: calc(5px * var(--font-scale)) calc(14px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 5px;
    background: transparent;
    color: var(--text-secondary);
    font-family: 'DM Mono', monospace;
    font-size: calc(11px * var(--font-scale));
    cursor: pointer;
    letter-spacing: .04em;
    transition: all .12s;
  }
  .scale-btn:hover { background: var(--interactive-hover); }
  .scale-btn.active {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent-text);
  }

</style>