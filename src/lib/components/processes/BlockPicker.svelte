<!-- src/lib/components/processes/BlockPicker.svelte -->
<script lang="ts">
  let {
    onAdd,
  }: {
    onAdd: (type: string) => void;
  } = $props();

  let openInfo = $state<string | null>(null);

  interface PortDef { name: string; desc: string; }
  interface BlockDef {
    type:    string;
    label:   string;
    desc:    string;
    inputs:  PortDef[];
    outputs: PortDef[];
    params:  string[];
  }

  const BLOCKS: { category: string; color: string; items: BlockDef[] }[] = [
    {
      category: 'Fuente', color: '#4a90d9',
      items: [
        {
          type: 'postgres_sensor', label: 'Sensor PostgreSQL',
          desc: 'Lee lecturas de sensores desde la base de datos del lab. Cada sensor agrega una dimensión al vector de salida.',
          inputs:  [],
          outputs: [{ name: 'sig_out', desc: 'Vector de lecturas — una dimensión por sensor configurado' }],
          params:  ['sensors — lista de sensores (id + etiqueta)'],
        },
      ]
    },
    {
      category: 'Filtro', color: '#7c6fcd',
      items: [
        {
          type: 'kalman', label: 'Kalman',
          desc: 'Estimación óptima que balancea ruido de proceso Q y ruido de medición R. Genera también covarianza P usable por Mahalanobis.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de mediciones sin filtrar' }],
          outputs: [{ name: 'sig_out', desc: 'Vector estimado (covarianza P disponible internamente)' }],
          params:  ['Q — ruido de proceso (bajo = más suave)', 'R — ruido de medición (alto = más suave)', 'P₀ — covarianza inicial', 'warmup_samples'],
        },
        {
          type: 'moving_avg', label: 'Media móvil',
          desc: 'Promedio de las últimas N muestras. Simple pero introduce retardo de N/2 ciclos.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de entrada' }],
          outputs: [{ name: 'sig_out', desc: 'Vector promediado en ventana deslizante' }],
          params:  ['window_n — tamaño de la ventana', 'warmup_samples'],
        },
        {
          type: 'ewma', label: 'EWMA',
          desc: 'Media exponencial. Alpha cercano a 0 = muy suave, cercano a 1 = rápido pero ruidoso.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de entrada' }],
          outputs: [{ name: 'sig_out', desc: 'Vector suavizado exponencialmente' }],
          params:  ['alpha — peso de muestra nueva (0.01–0.99)', 'warmup_samples'],
        },
        {
          type: 'lowpass', label: 'Pasa-bajos',
          desc: 'Filtro RC discreto parametrizado por constante de tiempo τ. La señal tarda ~3τ segundos en seguir un escalón.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de entrada' }],
          outputs: [{ name: 'sig_out', desc: 'Vector filtrado' }],
          params:  ['tau_seconds — constante de tiempo en segundos', 'warmup_samples'],
        },
        {
          type: 'passthrough', label: 'Passthrough',
          desc: 'Pasa la señal sin modificar. Útil para ramificar o debug.',
          inputs:  [{ name: 'sig_in', desc: 'Cualquier señal' }],
          outputs: [{ name: 'sig_out', desc: 'Misma señal sin cambios' }],
          params:  [],
        },
      ]
    },
    {
      category: 'Decisor', color: '#e07b54',
      items: [
        {
          type: 'mahalanobis', label: 'Mahalanobis',
          desc: 'Distancia estadística multivariada al vector objetivo. Usa la covarianza P del Kalman para ser robusto a correlaciones entre sensores.',
          inputs:  [{ name: 'sig_in', desc: 'Vector filtrado — idealmente de Kalman para aprovechar su P' }],
          outputs: [{ name: 'act_out', desc: 'ON si distancia > threshold_act · OFF si < threshold_deact' }],
          params:  ['target — vector objetivo', 'threshold_act / threshold_deact', 'use_kalman_P'],
        },
        {
          type: 'hysteresis', label: 'Histéresis',
          desc: 'ON/OFF con banda muerta. Activa bajo "low", desactiva sobre "high". Evita oscilaciones con señales ruidosas.',
          inputs:  [{ name: 'sig_in', desc: 'Vector — se reduce a escalar según reducción configurada' }],
          outputs: [{ name: 'act_out', desc: 'ON / OFF según umbrales low y high' }],
          params:  ['reducción (media/min/max)', 'low — umbral de activación', 'high — umbral de desactivación'],
        },
        {
          type: 'sprt', label: 'SPRT',
          desc: 'Test secuencial: acumula evidencia muestral antes de decidir entre H₀ (normal) y H₁ (anómalo). Más robusto que histéresis para señal gaussiana.',
          inputs:  [{ name: 'sig_in', desc: 'Vector — se reduce a escalar' }],
          outputs: [{ name: 'act_out', desc: 'ON (H₁ aceptada) / OFF (H₀ aceptada)' }],
          params:  ['μ H₀ / μ H₁ — medias de cada hipótesis', 'σ — desviación estándar', 'α / β — tasas de error'],
        },
      ]
    },
    {
      category: 'Watchdog', color: '#c084fc',
      items: [
        {
          type: 'mqtt_subscriber', label: 'MQTT Subscriber',
          desc: 'Suscribe a un topic MQTT y lee el estado reportado por el actuador. Alimenta el puerto de feedback del Watchdog.',
          inputs:  [],
          outputs: [{ name: 'mqtt_ret_out', desc: 'Acción leída: ON si payload = payload_on, OFF si = payload_off' }],
          params:  ['topic', 'payload_on / payload_off', 'stale_after_secs — Hold si no llega mensaje'],
        },
        {
          type: 'watchdog', label: 'Watchdog',
          desc: 'Verifica que el actuador responda. Bloquea o pide confirmación si detecta fallo. Modos: Good (feedback MQTT), Bad (tendencia), Ugly (manual).',
          inputs:  [
            { name: 'act_in',      desc: 'Decisión del decisor — conectar desde Histéresis/Mahalanobis/SPRT' },
            { name: 'mqtt_ret_in', desc: 'Feedback del actuador — conectar desde MQTT Subscriber (modo Good)' },
            { name: 'sig_in',      desc: 'Señal del sensor — para verificar tendencia (modo Bad)' },
          ],
          outputs: [{ name: 'act_out', desc: 'Acción verificada → actuador. Hold si bloqueado o pendiente confirmación' }],
          params:  ['modo — Good / Bad / Ugly', 'actuator_id — se toma del nodo conectado', 'timeout (s)', 'max_retries'],
        },
      ]
    },
    {
      category: 'Actuador', color: '#3da85a',
      items: [
        {
          type: 'actuator', label: 'Actuator',
          desc: 'Nodo unificado: decisor + actuador. Recibe señal filtrada, decide ON/OFF y publica por MQTT o HTTP. Soporta etapas de confirmación y coherence check.',
          inputs:  [{ name: 'sig_in', desc: 'Signal::Vector del filtro upstream' }],
          outputs: [{ name: 'act_out', desc: 'Signal::Action — la acción decidida' }],
          params:  ['decision — hysteresis / mahalanobis / sprt', 'output — mqtt / http con conexión propia', 'stages — etapas de confirmación (opcional)', 'coherence — check sensor↔actuación (opcional)'],
        },
        {
          type: 'mqtt_actuator', label: 'MQTT Actuator',
          desc: 'Publica payload ON/OFF en un topic MQTT. Conectar después del Watchdog si usás verificación.',
          inputs:  [{ name: 'act_in', desc: 'ON → publica payload_on · OFF → publica payload_off · Hold → no publica' }],
          outputs: [],
          params:  ['topic', 'payload_on / payload_off', 'conexión — en shared connections'],
        },
        {
          type: 'http_actuator', label: 'HTTP Actuator',
          desc: 'Hace POST a path_on en ON y a path_off en OFF.',
          inputs:  [{ name: 'act_in', desc: 'Acción ON/OFF' }],
          outputs: [],
          params:  ['path_on / path_off', 'base_url — en shared connections'],
        },
      ]
    },
    {
      category: 'Combinador', color: '#e8a838',
      items: [
        {
          type: 'concat', label: 'Concat',
          desc: 'Une vectores de múltiples fuentes en uno solo. Útil para combinar sensores antes del filtro.',
          inputs:  [{ name: 'sig_in×N', desc: 'N vectores — se concatenan en orden de los edges' }],
          outputs: [{ name: 'sig_out', desc: 'Vector unificado con la suma de dimensiones de entrada' }],
          params:  [],
        },
        {
          type: 'weighted_mean', label: 'Media ponderada',
          desc: 'Promedio pesado de las entradas. Los pesos deben sumar 1.0.',
          inputs:  [{ name: 'sig_in×N', desc: 'N vectores — se tratan como vector concatenado' }],
          outputs: [{ name: 'sig_out', desc: 'Vector promediado con pesos por componente' }],
          params:  ['weights — pesos por componente (deben sumar 1.0)'],
        },
      ]
    },
    {
      category: 'Utilidad', color: '#8a9bb0',
      items: [
        {
          type: 'logger', label: 'Logger',
          desc: 'Registra la señal en process_readings. El Monitor Tab usa estos registros para gráficas.',
          inputs:  [{ name: 'sig_in', desc: 'Cualquier señal' }],
          outputs: [{ name: 'sig_out', desc: 'Misma señal (passthrough)' }],
          params:  ['tag — nombre en el monitor'],
        },
        {
          type: 'select', label: 'Select',
          desc: 'Extrae componentes del vector por índice base 0.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de entrada' }],
          outputs: [{ name: 'sig_out', desc: 'Sub-vector con los componentes seleccionados' }],
          params:  ['indices — ej: [0, 2] de un vector dim=3 → dim=2'],
        },
        {
          type: 'linear_scale', label: 'Escala lineal',
          desc: 'Transforma la señal: y = a·x + b por componente.',
          inputs:  [{ name: 'sig_in', desc: 'Vector de entrada' }],
          outputs: [{ name: 'sig_out', desc: 'Vector transformado: a·x + b por componente' }],
          params:  ['a — escala por componente', 'b — offset por componente'],
        },
      ]
    },
  ];

  function toggleInfo(type: string) {
    openInfo = openInfo === type ? null : type;
  }
</script>

<div class="picker">
  <div class="picker-title">bloques</div>

  {#each BLOCKS as group (group.category)}
    <div class="group">
      <div class="group-label" style="color:{group.color}">{group.category}</div>
      {#each group.items as item (item.type)}
        <div class="block-wrap">
          <!-- Main row: draggable + info toggle -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="block-btn"
            class:info-open={openInfo === item.type}
            style="--nc:{group.color}"
            draggable="true"
            ondragstart={(e) => {
              e.dataTransfer?.setData('text/plain', item.type);
              e.dataTransfer && (e.dataTransfer.effectAllowed = 'copy');
            }}
          >
            <span class="drag-handle">⠿</span>
            <div class="block-btn-text">
              <span class="block-btn-label">{item.label}</span>
              <span class="block-btn-desc">{item.desc.split('.')[0]}.</span>
            </div>
            <div class="block-btn-actions">
              <button
                class="btn-add-canvas"
                onclick={() => onAdd(item.type)}
                title="agregar al canvas"
              >+</button>
              <button
                class="btn-info"
                class:open={openInfo === item.type}
                onclick={() => toggleInfo(item.type)}
                title="más información"
              >{openInfo === item.type ? '▲' : '▼'}</button>
            </div>
          </div>

          <!-- Accordion -->
          {#if openInfo === item.type}
            <div class="info-panel" style="--nc:{group.color}">
              <p class="info-desc">{item.desc}</p>

              <div class="ports-grid">
                <div class="port-section">
                  <div class="port-section-title">entradas</div>
                  {#if item.inputs.length === 0}
                    <div class="port-none">ninguna — nodo fuente</div>
                  {:else}
                    {#each item.inputs as port (port.name)}
                      <div class="port-row">
                        <span class="port-name port-name--in">{port.name}</span>
                        <span class="port-desc">{port.desc}</span>
                      </div>
                    {/each}
                  {/if}
                </div>

                <div class="port-section">
                  <div class="port-section-title">salidas</div>
                  {#if item.outputs.length === 0}
                    <div class="port-none">ninguna — nodo terminal</div>
                  {:else}
                    {#each item.outputs as port (port.name)}
                      <div class="port-row">
                        <span class="port-name port-name--out">{port.name}</span>
                        <span class="port-desc">{port.desc}</span>
                      </div>
                    {/each}
                  {/if}
                </div>
              </div>

              {#if item.params.length > 0}
                <div class="port-section">
                  <div class="port-section-title">parámetros</div>
                  {#each item.params as p (p)}
                    <div class="param-row">· {p}</div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: calc(12px * var(--font-scale));
    overflow-y: auto;
    height: 100%;
    background: var(--bg-elevated);
    border-right: 0.5px solid var(--border-subtle);
  }
  .picker-title {
    font-size: calc(10px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: .08em;
    padding-bottom: 4px;
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .group { display: flex; flex-direction: column; gap: 3px; }
  .group-label {
    font-size: calc(9px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    text-transform: uppercase;
    letter-spacing: .08em;
    font-weight: 600;
    padding: 2px 0;
  }
  .block-wrap { display: flex; flex-direction: column; }

  .block-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: calc(5px * var(--font-scale)) calc(6px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-left: 3px solid var(--nc);
    border-radius: 5px;
    background: var(--bg-surface);
    cursor: grab;
    transition: background .1s;
    user-select: none;
  }
  .block-btn.info-open { border-radius: 5px 5px 0 0; border-bottom-color: transparent; }
  .block-btn:hover { background: color-mix(in srgb, var(--nc) 6%, var(--bg-surface)); }
  .block-btn:active { cursor: grabbing; }

  .drag-handle { font-size: 12px; color: var(--text-muted); opacity: 0.35; flex-shrink: 0; }
  .block-btn:hover .drag-handle { opacity: 0.7; }

  .block-btn-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .block-btn-label {
    font-size: calc(11px * var(--font-scale));
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .block-btn-desc {
    font-size: calc(9px * var(--font-scale));
    color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .block-btn-actions { display: flex; gap: 2px; flex-shrink: 0; }
  .btn-add-canvas, .btn-info {
    width: 18px; height: 18px;
    border: 0.5px solid var(--border-default);
    border-radius: 3px;
    background: var(--bg-elevated);
    cursor: pointer;
    font-size: 10px;
    color: var(--text-muted);
    display: flex; align-items: center; justify-content: center;
    padding: 0;
  }
  .btn-add-canvas:hover { background: color-mix(in srgb, var(--nc) 15%, var(--bg-elevated)); color: var(--nc); border-color: var(--nc); }
  .btn-info:hover, .btn-info.open { background: var(--interactive-hover); }

  /* Accordion */
  .info-panel {
    border: 0.5px solid color-mix(in srgb, var(--nc) 30%, var(--border-subtle));
    border-top: none;
    border-radius: 0 0 5px 5px;
    background: color-mix(in srgb, var(--nc) 4%, var(--bg-surface));
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .info-desc {
    font-size: calc(10px * var(--font-scale));
    color: var(--text-secondary);
    line-height: 1.5;
    margin: 0;
  }
  .ports-grid {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .port-section { display: flex; flex-direction: column; gap: 4px; }
  .port-section-title {
    font-size: calc(8px * var(--font-scale));
    text-transform: uppercase;
    letter-spacing: .08em;
    color: var(--text-muted);
    font-family: 'DM Mono', monospace;
    font-weight: 600;
    border-bottom: 0.5px solid var(--border-subtle);
    padding-bottom: 2px;
  }
  .port-row { display: flex; align-items: baseline; gap: 6px; flex-wrap: wrap; }
  .port-name {
    font-size: calc(9px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    font-weight: 600;
    flex-shrink: 0;
    padding: 1px 5px;
    border-radius: 3px;
  }
  .port-name--in  { background: #EFF6FF; color: #1D4ED8; }
  .port-name--out { background: #F0FDF4; color: #166534; }
  .port-desc {
    font-size: calc(9px * var(--font-scale));
    color: var(--text-muted);
    line-height: 1.4;
    flex: 1;
  }
  .port-none { font-size: calc(9px * var(--font-scale)); color: var(--text-muted); font-style: italic; }
  .param-row { font-size: calc(9px * var(--font-scale)); color: var(--text-secondary); line-height: 1.5; }
</style>