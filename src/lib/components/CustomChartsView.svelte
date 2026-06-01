<script lang="ts">
  import { onMount } from 'svelte';
  import type { Box, Sensor } from '$lib/api';
  import { normaliseSensorLabel, sensorColor } from '$lib/api';
  import MultiSensorChart from '$lib/components/MultiSensorChart.svelte';
  import DateTimePicker from '$lib/components/DateTimePicker.svelte';

  // ── Props ─────────────────────────────────────────────────────────────────
  interface Props { boxes: Box[]; live: boolean; }
  let { boxes, live }: Props = $props();

  // ── Types ─────────────────────────────────────────────────────────────────
  interface ChartSeries {
    sensorId:     string;
    sensorType:   string;
    boxId:        string;
    boxName:      string;
    sensorNumber: number;
  }

  interface ChartCard {
    id:      string;
    title:   string;
    preset:  string; // '1h' | '6h' | '24h' | '7d' | '30d' | 'custom'
    fromMs:  number;
    toMs:    number;
    series:  ChartSeries[];
    /** Puntos solicitados al API. 0 = sin límite (raw). Default 0. */
    points:  number;
    /** Tensión de curva: 0 = recta (raw feel), 0.4 = suave. Default 0. */
    tension: number;
  }

  // ── Constants ─────────────────────────────────────────────────────────────
  const PRESETS = [
    { label: '1h',  hours: 1   },
    { label: '6h',  hours: 6   },
    { label: '24h', hours: 24  },
    { label: '7d',  hours: 168 },
    { label: '30d', hours: 720 },
  ] as const;

  const LS_KEY = 'agrodash_custom_charts_v1';

  // ── State ─────────────────────────────────────────────────────────────────
  let cards        = $state<ChartCard[]>([]);
  let configOpenId = $state<string | null>(null);
  // Per-card: which box is selected in the "add series" picker
  let pickerBox    = $state<Record<string, string>>({});
  let ready        = $state(false);

  // ── Storage ───────────────────────────────────────────────────────────────
  function load() {
    try {
      const raw = localStorage.getItem(LS_KEY);
      if (!raw) return;
      const parsed: ChartCard[] = JSON.parse(raw);
      const now = Date.now();
      cards = parsed.map(c => {
        // backward-compat: cards guardadas antes de agregar points/tension
        const base: ChartCard = {
          ...c,
          points:  c.points  ?? 0,
          tension: c.tension ?? 0,
        };
        if (base.preset === 'custom') return base;
        const p = PRESETS.find(p => p.label === base.preset);
        return p ? { ...base, toMs: now, fromMs: now - p.hours * 3_600_000 } : base;
      });
    } catch { /* silently ignore malformed storage */ }
  }

  function save() {
    try { localStorage.setItem(LS_KEY, JSON.stringify(cards)); } catch {}
  }

  // ── Card CRUD ─────────────────────────────────────────────────────────────
  function addCard() {
    const now = Date.now();
    const card: ChartCard = {
      id:      Math.random().toString(36).slice(2, 10),
      title:   `Gráfico ${cards.length + 1}`,
      preset:  '24h',
      fromMs:  now - 24 * 3_600_000,
      toMs:    now,
      series:  [],
      points:  0,      // raw por defecto
      tension: 0,      // líneas rectas por defecto
    };
    cards = [...cards, card];
    configOpenId = card.id;
    save();
  }

  function removeCard(id: string) {
    cards = cards.filter(c => c.id !== id);
    if (configOpenId === id) configOpenId = null;
    save();
  }

  function patch(id: string, delta: Partial<ChartCard>) {
    cards = cards.map(c => c.id === id ? { ...c, ...delta } : c);
    save();
  }

  // ── Time ──────────────────────────────────────────────────────────────────
  function applyPreset(cardId: string, p: { label: string; hours: number }) {
    const now = Date.now();
    patch(cardId, { preset: p.label, toMs: now, fromMs: now - p.hours * 3_600_000 });
  }

  // ── Series ────────────────────────────────────────────────────────────────
  function toggleSeries(cardId: string, sensor: Sensor, box: Box) {
    const card = cards.find(c => c.id === cardId);
    if (!card) return;
    const has = card.series.some(s => s.sensorId === sensor.id);
    patch(cardId, {
      series: has
        ? card.series.filter(s => s.sensorId !== sensor.id)
        : [...card.series, {
            sensorId:     sensor.id,
            sensorType:   sensor.type,
            boxId:        box.id,
            boxName:      box.name,
            sensorNumber: sensor.sensor_number,
          }],
    });
  }

  // ── Helpers ───────────────────────────────────────────────────────────────
  function cardSensors(card: ChartCard): Sensor[] {
    return card.series.map(s => ({
      id:            s.sensorId,
      sensor_number: s.sensorNumber,
      type:          s.sensorType,
    }));
  }

  function fmtDate(ms: number): string {
    return new Date(ms).toLocaleDateString('es-CR', { day: '2-digit', month: 'short' });
  }

  function rangeLabel(card: ChartCard): string {
    if (card.preset !== 'custom') return card.preset;
    return `${fmtDate(card.fromMs)} → ${fmtDate(card.toMs)}`;
  }

  // ── Mount ─────────────────────────────────────────────────────────────────
  onMount(() => { load(); ready = true; });
</script>

<!-- ═══════════════════════════════════════════════════════════════════════════
     ROOT
     ═══════════════════════════════════════════════════════════════════════════ -->
<div class="ccv">

  <!-- Toolbar ──────────────────────────────────────────────────────────────── -->
  <div class="ccv__toolbar">
    <button class="ccv__add" onclick={addCard}>
      <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
        <line x1="6" y1="1" x2="6" y2="11"/>
        <line x1="1" y1="6" x2="11" y2="6"/>
      </svg>
      Nueva gráfica
    </button>
    {#if cards.length > 0}
      <span class="ccv__count">
        {cards.length} gráfica{cards.length !== 1 ? 's' : ''} · guardadas en este navegador
      </span>
    {/if}
  </div>

  <!-- Empty state ──────────────────────────────────────────────────────────── -->
  {#if ready && cards.length === 0}
    <div class="ccv__empty">
      <svg class="ccv__empty-icon" viewBox="0 0 48 48" fill="none" stroke="currentColor"
        stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="4 36 16 20 24 28 34 12 44 18"/>
        <line x1="4" y1="42" x2="44" y2="42"/>
        <line x1="24" y1="6" x2="24" y2="2"/>
        <line x1="38" y1="10" x2="41" y2="7"/>
      </svg>
      <p class="ccv__empty-h">Sin gráficas personalizadas</p>
      <p class="ccv__empty-sub">
        Creá gráficas y elegí qué sensores y variables comparar.<br>
        Se guardan automáticamente en este navegador.
      </p>
      <button class="ccv__empty-cta" onclick={addCard}>
        + Nueva gráfica
      </button>
    </div>
  {/if}

  <!-- Card list ────────────────────────────────────────────────────────────── -->
  <div class="ccv__list">
    {#each cards as card (card.id)}
      {@const isOpen     = configOpenId === card.id}
      {@const boxSel     = pickerBox[card.id]}
      {@const selBox     = boxSel ? boxes.find(b => b.id === boxSel) : null}
      {@const sensors    = cardSensors(card)}
      {@const hasSeries  = card.series.length > 0}

      <div class="cc" class:cc--open={isOpen}>

        <!-- ── Header ─────────────────────────────────────────────────────── -->
        <div class="cc__head">

          <!-- Title: transparent input, looks like text until focused -->
          <input
            class="cc__title"
            value={card.title}
            spellcheck={false}
            aria-label="Nombre de la gráfica"
            oninput={(e) => patch(card.id, { title: (e.target as HTMLInputElement).value })}
          />

          <!-- Series chips: colored, scrollable, progressively disclosed -->
          {#if hasSeries}
            <div class="cc__chips" aria-label="Series activas">
              {#each card.series as s (s.sensorId)}
                {@const col = sensorColor(s.sensorType)}
                <span class="cc__chip" style="--c:{col}"
                  title="{s.boxName} · {normaliseSensorLabel(s.sensorType)} #{s.sensorNumber}">
                  <span class="cc__chip-dot"></span>
                  <span class="cc__chip-txt">
                    {s.boxName}<span class="cc__chip-sep">·</span>{normaliseSensorLabel(s.sensorType)}<span class="cc__chip-num">#{s.sensorNumber}</span>
                  </span>
                </span>
              {/each}
            </div>
          {:else}
            <span class="cc__no-series">sin series</span>
          {/if}

          <!-- Right cluster: range + config + delete -->
          <div class="cc__actions">
            <span class="cc__range" title="Rango de tiempo">{rangeLabel(card)}</span>

            <!-- Config toggle -->
            <button class="cc__iconbtn" class:cc__iconbtn--on={isOpen}
              title={isOpen ? 'Cerrar configuración' : 'Configurar'}
              onclick={() => configOpenId = isOpen ? null : card.id}>
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
                <circle cx="8" cy="8" r="2.5"/>
                <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2
                         M3.6 3.6l1.4 1.4M11 11l1.4 1.4
                         M3.6 12.4l1.4-1.4M11 5l1.4-1.4"/>
              </svg>
            </button>

            <!-- Delete -->
            <button class="cc__iconbtn cc__iconbtn--del" title="Eliminar gráfica"
              onclick={() => removeCard(card.id)}>
              <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <line x1="1" y1="1" x2="11" y2="11"/>
                <line x1="11" y1="1" x2="1" y2="11"/>
              </svg>
            </button>
          </div>
        </div>

        <!-- ── Config panel (progressive disclosure) ──────────────────────── -->
        {#if isOpen}
          <div class="cc__cfg">

            <!-- Section A: Tiempo ── -->
            <div class="cc__cfg-section">
              <span class="cc__cfg-lbl">TIEMPO</span>
              <div class="cc__cfg-row cc__cfg-row--time">
                <!-- Presets -->
                <div class="cc__presets">
                  {#each PRESETS as p}
                    <button class="cc__pill" class:active={card.preset === p.label}
                      onclick={() => applyPreset(card.id, p)}>{p.label}</button>
                  {/each}
                </div>
                <!-- Custom pickers -->
                <span class="cc__vsep"></span>
                <DateTimePicker
                  value={new Date(card.fromMs)}
                  label="DESDE"
                  onchange={(d) => patch(card.id, { preset: 'custom', fromMs: d.getTime() })}
                />
                <span class="cc__arrow">→</span>
                <DateTimePicker
                  value={new Date(card.toMs)}
                  label="HASTA"
                  onchange={(d) => patch(card.id, { preset: 'custom', toMs: d.getTime() })}
                />
              </div>
            </div>

            <!-- Section B: Series ── -->
            <div class="cc__cfg-section">
              <span class="cc__cfg-lbl">
                SERIES
                {#if card.series.length > 0}
                  <span class="cc__cfg-count">{card.series.length}</span>
                {/if}
              </span>

              <!-- Active series list: remove-on-click × button -->
              {#if card.series.length > 0}
                <div class="cc__series">
                  {#each card.series as s (s.sensorId)}
                    {@const col = sensorColor(s.sensorType)}
                    <div class="cc__serie" style="--c:{col}">
                      <span class="cc__serie-dot"></span>
                      <span class="cc__serie-info">
                        <span class="cc__serie-box">{s.boxName}</span>
                        <span class="cc__serie-type">
                          {normaliseSensorLabel(s.sensorType)}
                          <span class="cc__serie-num">#{s.sensorNumber}</span>
                        </span>
                      </span>
                      <button class="cc__serie-rm"
                        title="Quitar serie"
                        onclick={() => patch(card.id, {
                          series: card.series.filter(x => x.sensorId !== s.sensorId)
                        })}>✕</button>
                    </div>
                  {/each}
                </div>
              {/if}

              <!-- Sensor picker: 2-step (caja → sensor) ── -->
              <div class="cc__picker">
                <span class="cc__picker-lbl">Agregar</span>

                <!-- Step 1 ── Select box -->
                <div class="cc__picker-boxes">
                  {#each boxes as box (box.id)}
                    <button class="cc__pill cc__pill--box"
                      class:active={boxSel === box.id}
                      onclick={() => {
                        pickerBox = { ...pickerBox,
                          [card.id]: boxSel === box.id ? '' : box.id };
                      }}>
                      {box.name}
                    </button>
                  {/each}
                </div>

                <!-- Step 2 ── Select sensors (shown only when a box is selected) -->
                {#if selBox}
                  <div class="cc__picker-sensors">
                    {#each selBox.sensors as sensor (sensor.id)}
                      {@const col     = sensorColor(sensor.type)}
                      {@const checked = card.series.some(s => s.sensorId === sensor.id)}
                      <button class="cc__sensor" class:checked style="--c:{col}"
                        onclick={() => toggleSeries(card.id, sensor, selBox)}>
                        <span class="cc__sensor-dot"></span>
                        <span class="cc__sensor-label">
                          {normaliseSensorLabel(sensor.type)}
                          <span class="cc__sensor-num">#{sensor.sensor_number}</span>
                        </span>
                        {#if checked}
                          <span class="cc__sensor-check" aria-hidden="true">✓</span>
                        {/if}
                      </button>
                    {/each}
                  </div>
                {:else if boxes.length > 0}
                  <p class="cc__picker-hint">Seleccioná una caja para ver sus sensores</p>
                {/if}
              </div>

            </div>

            <!-- Section C: Datos ── -->
            <div class="cc__cfg-section">
              <span class="cc__cfg-lbl">DATOS</span>
              <div class="cc__cfg-row cc__cfg-row--data">

                <!-- Puntos (resolución) -->
                <div class="cc__data-group">
                  <span class="cc__data-lbl">puntos</span>
                  <div class="cc__presets">
                    {#each [
                      { label: '300',  val: 300  },
                      { label: '1k',   val: 1000 },
                      { label: '3k',   val: 3000 },
                      { label: 'raw',  val: 0    },
                    ] as opt}
                      <button class="cc__pill"
                        class:active={card.points === opt.val}
                        title={opt.val === 0 ? 'Sin agregación — todos los puntos del rango' : `${opt.val} puntos (time-bucket)`}
                        onclick={() => patch(card.id, { points: opt.val })}>
                        {opt.label}
                      </button>
                    {/each}
                    <!-- Campo numérico libre -->
                    <input
                      class="cc__pts-input"
                      type="number" min="10" max="9999" step="50"
                      value={card.points || ''}
                      placeholder="…"
                      title="Número de puntos exacto"
                      oninput={(e) => {
                        const v = parseInt((e.target as HTMLInputElement).value);
                        if (!isNaN(v) && v >= 0) patch(card.id, { points: v });
                      }}
                    />
                  </div>
                </div>

                <span class="cc__vsep"></span>

                <!-- Interpolación (tensión de curva) -->
                <div class="cc__data-group">
                  <span class="cc__data-lbl">interpolación</span>
                  <div class="cc__presets">
                    {#each [
                      { label: 'recta',  val: 0,   title: 'Sin interpolación — segmentos lineales' },
                      { label: 'suave',  val: 0.35, title: 'Spline cúbico suave' },
                    ] as opt}
                      <button class="cc__pill"
                        class:active={card.tension === opt.val}
                        title={opt.title}
                        onclick={() => patch(card.id, { tension: opt.val })}>
                        {opt.label}
                      </button>
                    {/each}
                  </div>
                </div>

              </div>
            </div>
          </div>
        {/if}

        <!-- ── Chart area ──────────────────────────────────────────────────── -->
        <div class="cc__chart">
          {#if !hasSeries}
            <div class="cc__chart-empty">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" opacity=".4">
                <polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>
              </svg>
              <span>Sin series seleccionadas</span>
              <button onclick={() => configOpenId = card.id}>
                ⚙ Configurar
              </button>
            </div>
          {:else}
            <MultiSensorChart
              sensors={cardSensors(card)}
              from={new Date(card.fromMs)}
              to={new Date(card.toMs)}
              {live}
              points={card.points}
              tension={card.tension}
            />
          {/if}
        </div>

      </div>
    {/each}
  </div>

</div>

<style>
  /* ── Root ─────────────────────────────────────────────────────────────── */
  .ccv {
    display: flex;
    flex-direction: column;
    gap: calc(10px * var(--font-scale));
  }

  /* ── Toolbar ──────────────────────────────────────────────────────────── */
  .ccv__toolbar {
    display: flex;
    align-items: center;
    gap: calc(10px * var(--font-scale));
    flex-wrap: wrap;
  }

  .ccv__add {
    display: inline-flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
    height: 28px;
    padding: 0 calc(12px * var(--font-scale));
    background: var(--accent-bg);
    color: var(--accent-text);
    border: none;
    border-radius: 5px;
    font-family: 'DM Mono', monospace;
    font-size: calc(11px * var(--font-scale));
    letter-spacing: .05em;
    cursor: pointer;
    transition: opacity .12s;
  }
  .ccv__add:hover { opacity: .85; }

  .ccv__count {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    letter-spacing: .05em;
    color: var(--text-muted);
  }

  /* ── Empty state ──────────────────────────────────────────────────────── */
  .ccv__empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: calc(10px * var(--font-scale));
    padding: calc(64px * var(--font-scale)) calc(24px * var(--font-scale));
    text-align: center;
  }
  .ccv__empty-icon {
    width: calc(48px * var(--font-scale));
    height: calc(48px * var(--font-scale));
    color: var(--text-muted);
    opacity: .4;
  }
  .ccv__empty-h {
    font-size: calc(15px * var(--font-scale));
    font-weight: 500;
    color: var(--text-primary);
    margin: 0;
  }
  .ccv__empty-sub {
    font-size: calc(13px * var(--font-scale));
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }
  .ccv__empty-cta {
    margin-top: calc(6px * var(--font-scale));
    padding: calc(8px * var(--font-scale)) calc(20px * var(--font-scale));
    background: var(--accent-bg);
    color: var(--accent-text);
    border: none;
    border-radius: 6px;
    font-family: 'DM Mono', monospace;
    font-size: calc(12px * var(--font-scale));
    letter-spacing: .05em;
    cursor: pointer;
    transition: opacity .12s;
  }
  .ccv__empty-cta:hover { opacity: .85; }

  /* ── Card list ────────────────────────────────────────────────────────── */
  .ccv__list {
    display: flex;
    flex-direction: column;
    gap: calc(12px * var(--font-scale));
  }

  /* ── Card ─────────────────────────────────────────────────────────────── */
  .cc {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 10px;
    overflow: hidden;
    transition: border-color .15s;
  }
  .cc--open { border-color: var(--border-strong, var(--border-default)); }

  /* ── Card header ──────────────────────────────────────────────────────── */
  .cc__head {
    display: flex;
    align-items: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(9px * var(--font-scale)) calc(14px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    min-height: calc(42px * var(--font-scale));
    flex-wrap: wrap;
  }

  /* Inline-editable title: invisible border until focused */
  .cc__title {
    background: transparent;
    border: none;
    border-bottom: 1px solid transparent;
    color: var(--text-primary);
    font-size: calc(13px * var(--font-scale));
    font-weight: 500;
    font-family: inherit;
    outline: none;
    padding: 1px 2px;
    min-width: 60px;
    max-width: 200px;
    flex-shrink: 0;
    transition: border-color .15s;
    border-radius: 0;
  }
  .cc__title:focus { border-bottom-color: var(--border-default); }
  .cc__title:hover:not(:focus) { border-bottom-color: var(--border-subtle); }

  /* Chips strip: scrollable horizontally, flex-1 so it fills space */
  .cc__chips {
    display: flex;
    align-items: center;
    gap: calc(4px * var(--font-scale));
    flex: 1;
    overflow-x: auto;
    scrollbar-width: none;
    min-width: 0;
  }
  .cc__chips::-webkit-scrollbar { display: none; }

  .cc__no-series {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    color: var(--text-muted);
    letter-spacing: .05em;
    flex: 1;
  }

  /* Individual chip */
  .cc__chip {
    display: inline-flex;
    align-items: center;
    gap: calc(4px * var(--font-scale));
    padding: calc(2px * var(--font-scale)) calc(7px * var(--font-scale)) calc(2px * var(--font-scale)) calc(5px * var(--font-scale));
    background: color-mix(in srgb, var(--c) 10%, var(--bg-elevated));
    border: 0.5px solid color-mix(in srgb, var(--c) 30%, transparent);
    border-radius: 3px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .cc__chip-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--c);
    flex-shrink: 0;
  }
  .cc__chip-txt {
    font-family: 'DM Mono', monospace;
    font-size: calc(9px * var(--font-scale));
    letter-spacing: .04em;
    color: color-mix(in srgb, var(--c) 75%, var(--text-primary));
    display: flex;
    align-items: center;
    gap: calc(3px * var(--font-scale));
  }
  .cc__chip-sep { opacity: .5; }
  .cc__chip-num { opacity: .65; }

  /* Right cluster */
  .cc__actions {
    display: flex;
    align-items: center;
    gap: calc(5px * var(--font-scale));
    flex-shrink: 0;
    margin-left: auto;
  }

  .cc__range {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    letter-spacing: .05em;
    color: var(--text-muted);
    padding: calc(2px * var(--font-scale)) calc(7px * var(--font-scale));
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 3px;
    white-space: nowrap;
  }

  .cc__iconbtn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 0.5px solid var(--border-default);
    border-radius: 5px;
    color: var(--text-muted);
    cursor: pointer;
    transition: all .12s;
    flex-shrink: 0;
  }
  .cc__iconbtn:hover { background: var(--interactive-hover); color: var(--text-secondary); }
  .cc__iconbtn--on {
    background: var(--accent-bg);
    color: var(--accent-text);
    border-color: transparent;
  }
  .cc__iconbtn--del:hover {
    background: var(--error-bg);
    color: var(--error-color);
    border-color: transparent;
  }

  /* ── Config panel ─────────────────────────────────────────────────────── */
  .cc__cfg {
    background: var(--bg-elevated);
    border-bottom: 0.5px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .cc__cfg-section {
    display: flex;
    flex-direction: column;
    gap: calc(8px * var(--font-scale));
    padding: calc(11px * var(--font-scale)) calc(14px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .cc__cfg-section:last-child { border-bottom: none; }

  /* Section label — DM Mono uppercase, tight tracking */
  .cc__cfg-lbl {
    font-family: 'DM Mono', monospace;
    font-size: calc(9px * var(--font-scale));
    letter-spacing: .12em;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
  }
  .cc__cfg-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    background: var(--accent-bg);
    color: var(--accent-text);
    border-radius: 8px;
    font-size: calc(9px * var(--font-scale));
    padding: 0 4px;
    letter-spacing: 0;
  }

  /* Time row: presets + vsep + pickers */
  .cc__cfg-row--time {
    display: flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
    flex-wrap: wrap;
  }

  .cc__presets {
    display: flex;
    gap: calc(3px * var(--font-scale));
  }

  .cc__pill {
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    letter-spacing: .04em;
    cursor: pointer;
    white-space: nowrap;
    transition: all .1s;
  }
  .cc__pill:hover { background: var(--interactive-hover); color: var(--text-secondary); }
  .cc__pill.active { background: var(--accent-bg); color: var(--accent-text); border-color: transparent; }

  .cc__pill--box { height: 26px; font-size: calc(10px * var(--font-scale)); color: var(--text-secondary); }

  .cc__vsep { width: 0.5px; height: 14px; background: var(--border-subtle); flex-shrink: 0; }
  .cc__arrow { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }

  /* Make DateTimePicker match our pill height */
  .cc__cfg-row--time :global(.dtp__trigger) {
    height: 24px;
    font-size: calc(10px * var(--font-scale));
    padding: 0 calc(7px * var(--font-scale));
  }

  /* ── Active series list ──────────────────────────────────────────────── */
  .cc__series {
    display: flex;
    flex-direction: column;
    gap: calc(3px * var(--font-scale));
    margin-bottom: calc(4px * var(--font-scale));
  }

  .cc__serie {
    display: flex;
    align-items: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(5px * var(--font-scale)) calc(9px * var(--font-scale));
    background: color-mix(in srgb, var(--c) 6%, var(--bg-surface));
    border: 0.5px solid color-mix(in srgb, var(--c) 20%, transparent);
    border-radius: 5px;
  }
  .cc__serie-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
    flex-shrink: 0;
  }
  .cc__serie-info {
    display: flex;
    align-items: center;
    gap: calc(6px * var(--font-scale));
    flex: 1;
    min-width: 0;
    flex-wrap: wrap;
  }
  .cc__serie-box {
    font-size: calc(10px * var(--font-scale));
    font-weight: 500;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .cc__serie-type {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .cc__serie-num { opacity: .7; }
  .cc__serie-rm {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: calc(10px * var(--font-scale));
    cursor: pointer;
    padding: calc(2px * var(--font-scale)) calc(4px * var(--font-scale));
    border-radius: 3px;
    line-height: 1;
    transition: all .1s;
    flex-shrink: 0;
  }
  .cc__serie-rm:hover { background: var(--error-bg); color: var(--error-color); }

  /* ── Sensor picker ────────────────────────────────────────────────────── */
  .cc__picker {
    display: flex;
    flex-direction: column;
    gap: calc(7px * var(--font-scale));
    padding-top: calc(6px * var(--font-scale));
    border-top: 0.5px solid var(--border-subtle);
  }

  .cc__picker-lbl {
    font-family: 'DM Mono', monospace;
    font-size: calc(9px * var(--font-scale));
    letter-spacing: .1em;
    color: var(--text-muted);
  }

  /* Step 1: box pills — horizontal scroll on narrow screens */
  .cc__picker-boxes {
    display: flex;
    flex-wrap: wrap;
    gap: calc(4px * var(--font-scale));
  }

  /* Step 2: sensor grid — 3 or more columns */
  .cc__picker-sensors {
    display: flex;
    flex-wrap: wrap;
    gap: calc(4px * var(--font-scale));
  }

  /* Individual sensor toggle button */
  .cc__sensor {
    display: inline-flex;
    align-items: center;
    gap: calc(5px * var(--font-scale));
    height: 26px;
    padding: 0 calc(9px * var(--font-scale)) 0 calc(7px * var(--font-scale));
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    cursor: pointer;
    transition: all .1s;
    white-space: nowrap;
  }
  .cc__sensor:hover {
    background: color-mix(in srgb, var(--c) 8%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--c) 35%, transparent);
  }
  .cc__sensor.checked {
    background: color-mix(in srgb, var(--c) 12%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--c) 45%, transparent);
  }
  .cc__sensor-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
    flex-shrink: 0;
    opacity: .7;
  }
  .cc__sensor.checked .cc__sensor-dot { opacity: 1; }
  .cc__sensor-label {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: calc(3px * var(--font-scale));
  }
  .cc__sensor-num { color: var(--text-muted); }
  .cc__sensor-check {
    font-size: calc(9px * var(--font-scale));
    color: var(--accent-text);
    font-weight: 600;
    margin-left: calc(2px * var(--font-scale));
  }

  .cc__picker-hint {
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    color: var(--text-muted);
    letter-spacing: .04em;
    margin: 0;
  }

  /* ── Chart area ──────────────────────────────────────────────────────── */
  .cc__chart {
    padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)) calc(8px * var(--font-scale));
  }

  /* Empty chart placeholder */
  .cc__chart-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: calc(8px * var(--font-scale));
    height: calc(180px * var(--font-scale));
    color: var(--text-muted);
    font-size: calc(12px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    letter-spacing: .04em;
  }
  .cc__chart-empty button {
    padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale));
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    color: var(--text-secondary);
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    cursor: pointer;
    letter-spacing: .05em;
    transition: all .12s;
  }
  .cc__chart-empty button:hover { background: var(--interactive-hover); color: var(--text-primary); }

  /* ── Sección DATOS (puntos + interpolación) ────────────────────────────── */
  .cc__cfg-row--data {
    display: flex;
    align-items: flex-start;
    gap: calc(10px * var(--font-scale));
    flex-wrap: wrap;
  }

  .cc__data-group {
    display: flex;
    flex-direction: column;
    gap: calc(5px * var(--font-scale));
  }

  .cc__data-lbl {
    font-family: 'DM Mono', monospace;
    font-size: calc(9px * var(--font-scale));
    letter-spacing: .08em;
    color: var(--text-muted);
  }

  /* Input numérico libre para puntos: ancho fijo, misma altura que cc__pill */
  .cc__pts-input {
    height: 24px;
    width: calc(56px * var(--font-scale));
    padding: 0 calc(6px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: var(--bg-inset, var(--bg-elevated));
    color: var(--text-secondary);
    font-family: 'DM Mono', monospace;
    font-size: calc(10px * var(--font-scale));
    outline: none;
    appearance: textfield;
    -moz-appearance: textfield;
    transition: border-color .12s;
  }
  .cc__pts-input::-webkit-inner-spin-button { -webkit-appearance: none; }
  .cc__pts-input:focus { border-color: var(--interactive-focus, var(--border-strong)); }
  .cc__pts-input::placeholder { color: var(--text-muted); }

  /* ── Mobile ──────────────────────────────────────────────────────────── */
  @media (max-width: 640px) {
    .cc__head { gap: calc(6px * var(--font-scale)); padding: calc(8px * var(--font-scale)) calc(10px * var(--font-scale)); }
    .cc__title { font-size: calc(12px * var(--font-scale)); max-width: 130px; }
    .cc__cfg-section { padding: calc(10px * var(--font-scale)) calc(10px * var(--font-scale)); }
    .cc__cfg-row--time { gap: calc(5px * var(--font-scale)); }
    .cc__range { display: none; } /* range visible in header chips on mobile */
    .cc__chart { padding: calc(8px * var(--font-scale)) calc(10px * var(--font-scale)); }
    .cc__picker-sensors { gap: calc(3px * var(--font-scale)); }
  }
</style>