<!-- src/lib/components/processes/BlockPicker.svelte -->
<script lang="ts">
  let {
    onAdd,
  }: {
    onAdd: (type: string) => void;
  } = $props();

  const BLOCKS = [
    {
      category: 'Fuente',
      color: '#4a90d9',
      items: [
        { type: 'postgres_sensor', label: 'Sensor PostgreSQL', desc: 'Lee lecturas de la DB' },
      ]
    },
    {
      category: 'Filtro',
      color: '#7c6fcd',
      items: [
        { type: 'kalman',     label: 'Kalman',     desc: 'Filtro de Kalman escalar' },
        { type: 'moving_avg', label: 'Media móvil', desc: 'Promedio de ventana deslizante' },
        { type: 'ewma',       label: 'EWMA',        desc: 'Media móvil exponencial' },
        { type: 'lowpass',    label: 'Pasa-bajos',  desc: 'Filtro RC discreto' },
        { type: 'passthrough',label: 'Passthrough', desc: 'Sin filtrado' },
      ]
    },
    {
      category: 'Combinador',
      color: '#e8a838',
      items: [
        { type: 'concat',        label: 'Concat',         desc: 'Une vectores de múltiples entradas' },
        { type: 'weighted_mean', label: 'Media ponderada', desc: 'Promedio con pesos' },
      ]
    },
    {
      category: 'Decisor',
      color: '#e07b54',
      items: [
        { type: 'mahalanobis', label: 'Mahalanobis', desc: 'Distancia estadística multivariada' },
        { type: 'hysteresis',  label: 'Histéresis',  desc: 'Control on/off con banda muerta' },
        { type: 'sprt',        label: 'SPRT',         desc: 'Test secuencial de razón de probabilidad' },
      ]
    },
    {
      category: 'Actuador',
      color: '#3da85a',
      items: [
        { type: 'mqtt_actuator', label: 'MQTT',  desc: 'Publica en un topic MQTT' },
        { type: 'http_actuator', label: 'HTTP',  desc: 'Hace POST a un endpoint' },
      ]
    },
    {
      category: 'Utilidad',
      color: '#8a9bb0',
      items: [
        { type: 'logger',       label: 'Logger',       desc: 'Registra sin modificar la señal' },
        { type: 'select',       label: 'Select',       desc: 'Selecciona componentes del vector' },
        { type: 'linear_scale', label: 'Escala lineal', desc: 'y = a·x + b' },
      ]
    },
  ];
</script>

<div class="picker">
  <div class="picker-title">bloques</div>

  {#each BLOCKS as group (group.category)}
    <div class="group">
      <div class="group-label" style="color:{group.color}">{group.category}</div>
      {#each group.items as item (item.type)}
        <button
          class="block-btn"
          style="--nc:{group.color}"
          draggable="true"
          onclick={() => onAdd(item.type)}
          ondragstart={(e) => {
            e.dataTransfer?.setData('text/plain', item.type);
            e.dataTransfer && (e.dataTransfer.effectAllowed = 'copy');
          }}
          title={item.desc}
        >
          <span class="block-btn-icon">⠿</span>
          <div class="block-btn-text">
            <span class="block-btn-label">{item.label}</span>
            <span class="block-btn-desc">{item.desc}</span>
          </div>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
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

  .block-btn {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-left: 3px solid var(--nc);
    border-radius: 5px;
    background: var(--bg-surface);
    cursor: pointer;
    text-align: left;
    transition: background .1s, transform .1s;
  }
  .block-btn:hover {
    background: color-mix(in srgb, var(--nc) 6%, var(--bg-surface));
    transform: translateX(2px);
  }
  .block-btn:active { transform: translateX(4px); }
  .block-btn[draggable="true"] { cursor: grab; }
  .block-btn[draggable="true"]:active { cursor: grabbing; }
  .block-btn-icon { color: var(--text-muted); font-size: 12px; flex-shrink: 0; opacity: 0.5; }
  .block-btn-text { display: flex; flex-direction: column; gap: 1px; }

  .block-btn-label {
    font-size: calc(11px * var(--font-scale));
    font-weight: 500;
    color: var(--text-primary);
  }
  .block-btn-desc {
    font-size: calc(9px * var(--font-scale));
    color: var(--text-muted);
    line-height: 1.3;
  }
</style>