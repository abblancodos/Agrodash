<!-- src/lib/components/processes/ConfigTab.svelte -->
<script lang="ts">
  import { SvelteFlow, Background, Controls, Handle, Position } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { processStore, canAdmin } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  // ── Config actual ─────────────────────────────────────────────────────────
  const proc   = $derived($processStore.process);
  const config = $derived(proc?.config ?? {});

  // Panel lateral
  let activePanel = $state<'source' | 'filter' | 'decision' | 'actuator' | null>(null);
  let saving      = $state(false);
  let saveMsg     = $state('');
  let saveMsgOk   = $state(true);

  // Formulario local — copia editable del config
  let draft = $state<any>({});

  $effect(() => {
    if (proc?.config && Object.keys(draft).length === 0) {
      draft = JSON.parse(JSON.stringify(proc.config));
    }
  });

  // ── Nodos fijos en línea ──────────────────────────────────────────────────
  const nodes = $derived([
    {
      id: 'source',
      type: 'pipelineNode',
      position: { x: 40, y: 120 },
      data: {
        label:    'Fuente',
        icon:     '⬡',
        sublabel: sourceLabel(config),
        color:    '#4a90d9',
        active:   activePanel === 'source',
        onClick:  () => openPanel('source'),
      },
    },
    {
      id: 'filter',
      type: 'pipelineNode',
      position: { x: 260, y: 120 },
      data: {
        label:    'Filtro',
        icon:     '〜',
        sublabel: filterLabel(config),
        color:    '#7c6fcd',
        active:   activePanel === 'filter',
        onClick:  () => openPanel('filter'),
      },
    },
    {
      id: 'decision',
      type: 'pipelineNode',
      position: { x: 480, y: 120 },
      data: {
        label:    'Decisión',
        icon:     '◇',
        sublabel: decisionLabel(config),
        color:    '#e07b54',
        active:   activePanel === 'decision',
        onClick:  () => openPanel('decision'),
      },
    },
    {
      id: 'actuator',
      type: 'pipelineNode',
      position: { x: 700, y: 120 },
      data: {
        label:    'Actuador',
        icon:     '⚡',
        sublabel: actuatorLabel(config),
        color:    '#3da85a',
        active:   activePanel === 'actuator',
        onClick:  () => openPanel('actuator'),
      },
    },
  ]);

  const edges = [
    { id: 'e1', source: 'source',   target: 'filter',   animated: true },
    { id: 'e2', source: 'filter',   target: 'decision',  animated: true },
    { id: 'e3', source: 'decision', target: 'actuator',  animated: true },
  ];

  // ── Labels de resumen ─────────────────────────────────────────────────────
  function sourceLabel(cfg: any) {
    const n = cfg?.source?.sensors?.length ?? 0;
    return n ? `${n} sensor${n !== 1 ? 'es' : ''}` : 'sin configurar';
  }
  function filterLabel(cfg: any) {
    const t = cfg?.filter?.type;
    return t ? `${t} · warmup ${cfg.filter.warmup_samples ?? '—'}` : 'sin configurar';
  }
  function decisionLabel(cfg: any) {
    const t = cfg?.decision?.type;
    if (!t) return 'sin configurar';
    if (t === 'mahalanobis') return `Mahalanobis · act ${cfg.decision.threshold_act}`;
    if (t === 'hysteresis')  return `Histéresis · ${cfg.decision.low}–${cfg.decision.high}`;
    return t;
  }
  function actuatorLabel(cfg: any) {
    const t = cfg?.actuator?.type;
    if (!t) return 'sin configurar';
    if (t === 'mqtt') return `MQTT · ${cfg.actuator.topic ?? '—'}`;
    if (t === 'http') return `HTTP · ${cfg.actuator.path_on ?? '—'}`;
    return t;
  }

  // ── Panel lateral ─────────────────────────────────────────────────────────
  function openPanel(panel: typeof activePanel) {
    activePanel = activePanel === panel ? null : panel;
  }

  // ── Guardar config ────────────────────────────────────────────────────────
  async function saveConfig() {
    saving = true; saveMsg = '';
    try {
      const res = await fetch(`${API}/api/v1/processes/${processId}`, {
        method: 'PATCH',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ config: draft }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      saveMsg = '✓ configuración guardada'; saveMsgOk = true;
      // Recargar el proceso para que los labels se actualicen
      await processStore.load(processId);
      draft = {};
    } catch (e: any) {
      saveMsg = e.message; saveMsgOk = false;
    } finally { saving = false; }
  }

  // ── Helpers de sensores disponibles ──────────────────────────────────────
  let availableSensors = $state<any[]>([]);
  $effect(() => {
    fetch(`${API}/api/v1/boxes`, { credentials: 'include' })
      .then(r => r.json())
      .then(boxes => {
        availableSensors = boxes.flatMap((b: any) =>
          (b.sensors ?? []).map((s: any) => ({
            id: s.id,
            label: `${b.name} · #${s.sensor_number} · ${s.sensor_type}`,
            box: b.name,
          }))
        );
      }).catch(() => {});
  });

  // helpers para draft
  function ensurePath(obj: any, ...keys: string[]) {
    let cur = obj;
    for (const k of keys) {
      if (!cur[k] || typeof cur[k] !== 'object') cur[k] = {};
      cur = cur[k];
    }
  }

  function addSensor() {
    if (!draft.source) draft.source = { type: 'postgres_sensor', sensors: [] };
    if (!draft.source.sensors) draft.source.sensors = [];
    draft.source.sensors = [...draft.source.sensors, { id: '', label: '' }];
  }

  function removeSensor(i: number) {
    draft.source.sensors = draft.source.sensors.filter((_: any, idx: number) => idx !== i);
  }
</script>

<div class="config-tab">
  <!-- Toolbar -->
  <div class="toolbar">
    <span class="toolbar-hint">Hacé click en un nodo para configurarlo</span>
    {#if saveMsg}
      <span class="save-msg" class:ok={saveMsgOk} class:err={!saveMsgOk}>{saveMsg}</span>
    {/if}
    {#if $canAdmin}
      <button class="btn-save" disabled={saving} onclick={saveConfig}>
        {saving ? 'guardando...' : 'guardar configuración'}
      </button>
    {/if}
  </div>

  <!-- Flow canvas + panel lateral -->
  <div class="canvas-wrap">
    <div class="flow-canvas" class:panel-open={activePanel !== null}>
      <SvelteFlow
        {nodes}
        {edges}
        fitView
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        nodeTypes={{ pipelineNode: PipelineNode }}
        defaultEdgeOptions={{ style: 'stroke: var(--border-default); stroke-width: 2;' }}
      >
        <Background gap={20} color="var(--border-subtle)" />
        <Controls showInteractive={false} />
      </SvelteFlow>
    </div>

    <!-- Panel lateral -->
    {#if activePanel !== null}
      <div class="side-panel">
        <div class="panel-head">
          <span class="panel-title">
            {{ source: 'Fuente', filter: 'Filtro', decision: 'Decisión', actuator: 'Actuador' }[activePanel]}
          </span>
          <button class="close-btn" onclick={() => activePanel = null}>✕</button>
        </div>

        <div class="panel-body">

          <!-- ── FUENTE ── -->
          {#if activePanel === 'source'}
            <div class="field">
              <label>tipo de fuente</label>
              <select bind:value={draft.source.type}>
                <option value="postgres_sensor">Sensor PostgreSQL</option>
              </select>
            </div>

            <div class="field">
              <label>sensores <span class="hint">(orden = índice del vector)</span></label>
              {#each (draft.source?.sensors ?? []) as sensor, i}
                <div class="sensor-row">
                  <select class="sensor-sel" bind:value={draft.source.sensors[i].id}
                    onchange={(e) => {
                      const found = availableSensors.find(s => s.id === (e.target as HTMLSelectElement).value);
                      if (found) draft.source.sensors[i].label = found.label;
                    }}>
                    <option value="">— elegir sensor —</option>
                    {#each availableSensors as s}
                      <option value={s.id}>{s.label}</option>
                    {/each}
                  </select>
                  <input class="label-in" bind:value={draft.source.sensors[i].label}
                    placeholder="etiqueta" />
                  <button class="btn-rm" onclick={() => removeSensor(i)}>✕</button>
                </div>
              {/each}
              <button class="btn-add-sensor" onclick={addSensor}>+ agregar sensor</button>
            </div>

          <!-- ── FILTRO ── -->
          {:else if activePanel === 'filter'}
            <div class="field">
              <label>tipo de filtro</label>
              <select bind:value={draft.filter.type}
                onchange={() => { draft.filter.warmup_samples ??= 10; }}>
                <option value="kalman">Kalman (recomendado)</option>
                <option value="moving_avg">Media móvil</option>
                <option value="ewma">EWMA</option>
                <option value="lowpass">Paso bajo (RC)</option>
                <option value="passthrough">Sin filtro</option>
              </select>
            </div>

            <div class="field">
              <label>warmup_samples <span class="hint">muestras antes de actuar</span></label>
              <input type="number" bind:value={draft.filter.warmup_samples} min="1" />
            </div>

            {#if draft.filter.type === 'kalman'}
              <div class="field-group">
                <div class="field">
                  <label>Q <span class="hint">varianza de proceso</span></label>
                  <input class="mono" bind:value={draft.filter.Q} placeholder="1e-5" />
                </div>
                <div class="field">
                  <label>R <span class="hint">varianza de medición</span></label>
                  <input class="mono" bind:value={draft.filter.R} placeholder="1e-3" />
                </div>
                <div class="field">
                  <label>P0 <span class="hint">covarianza inicial</span></label>
                  <input class="mono" bind:value={draft.filter.P0} placeholder="1.0" />
                </div>
                <div class="field">
                  <label>convergence_threshold</label>
                  <input class="mono" bind:value={draft.filter.convergence_threshold} placeholder="5e-4" />
                </div>
              </div>
            {:else if draft.filter.type === 'moving_avg'}
              <div class="field">
                <label>window_n <span class="hint">tamaño de la ventana</span></label>
                <input type="number" bind:value={draft.filter.window_n} min="2" placeholder="10" />
              </div>
            {:else if draft.filter.type === 'ewma'}
              <div class="field">
                <label>alpha <span class="hint">factor de suavizado [0,1]</span></label>
                <input class="mono" bind:value={draft.filter.alpha} placeholder="0.1" />
              </div>
            {:else if draft.filter.type === 'lowpass'}
              <div class="field">
                <label>tau_seconds <span class="hint">constante de tiempo RC</span></label>
                <input class="mono" bind:value={draft.filter.tau_seconds} placeholder="120" />
              </div>
            {/if}

          <!-- ── DECISIÓN ── -->
          {:else if activePanel === 'decision'}
            <div class="field">
              <label>método de decisión</label>
              <select bind:value={draft.decision.type}>
                <option value="mahalanobis">Mahalanobis (recomendado)</option>
                <option value="hysteresis">Histéresis escalar</option>
                <option value="sprt">SPRT (secuencial)</option>
              </select>
            </div>

            {#if draft.decision.type === 'mahalanobis'}
              <div class="field">
                <label>target <span class="hint">vector objetivo — un valor por sensor, separados por coma</span></label>
                <input class="mono" placeholder="0.082, 0.082, 0.082, 0.082, 0.082, 0.082"
                  value={(draft.decision.target ?? []).join(', ')}
                  oninput={(e) => {
                    draft.decision.target = (e.target as HTMLInputElement).value
                      .split(',').map(v => parseFloat(v.trim())).filter(v => !isNaN(v));
                  }} />
              </div>
              <div class="field-group">
                <div class="field">
                  <label>threshold_act <span class="hint">activar si d > este valor</span></label>
                  <input class="mono" type="number" step="0.1" bind:value={draft.decision.threshold_act} placeholder="2.5" />
                </div>
                <div class="field">
                  <label>threshold_deact <span class="hint">desactivar si d &lt; este valor</span></label>
                  <input class="mono" type="number" step="0.1" bind:value={draft.decision.threshold_deact} placeholder="1.0" />
                </div>
              </div>
              <div class="field checkbox-field">
                <input type="checkbox" id="use-P" bind:checked={draft.decision.use_kalman_P} />
                <label for="use-P">usar P del Kalman como métrica de incertidumbre</label>
              </div>
              {#if !draft.decision.use_kalman_P}
                <div class="field">
                  <label>sigma <span class="hint">desviación estándar asumida</span></label>
                  <input class="mono" bind:value={draft.decision.sigma} placeholder="0.01" />
                </div>
              {/if}

            {:else if draft.decision.type === 'hysteresis'}
              <div class="field">
                <label>reducción del vector</label>
                <select bind:value={draft.decision.reduction.type}>
                  <option value="mean">Media</option>
                  <option value="min">Mínimo</option>
                  <option value="max">Máximo</option>
                  <option value="weighted_by_p">Ponderado por P</option>
                  <option value="count_below">Conteo debajo de umbral</option>
                </select>
              </div>
              {#if draft.decision.reduction?.type === 'count_below'}
                <div class="field-group">
                  <div class="field">
                    <label>threshold</label>
                    <input class="mono" type="number" bind:value={draft.decision.reduction.threshold} />
                  </div>
                  <div class="field">
                    <label>min_count <span class="hint">mínimo de componentes</span></label>
                    <input type="number" bind:value={draft.decision.reduction.min_count} />
                  </div>
                </div>
              {/if}
              <div class="field-group">
                <div class="field">
                  <label>low</label>
                  <input class="mono" type="number" step="0.001" bind:value={draft.decision.low} />
                </div>
                <div class="field">
                  <label>high</label>
                  <input class="mono" type="number" step="0.001" bind:value={draft.decision.high} />
                </div>
              </div>

            {:else if draft.decision.type === 'sprt'}
              <div class="field-group">
                <div class="field">
                  <label>mu_H0 <span class="hint">media bajo H0 (húmedo)</span></label>
                  <input class="mono" type="number" step="0.001" bind:value={draft.decision.mu_H0} />
                </div>
                <div class="field">
                  <label>mu_H1 <span class="hint">media bajo H1 (seco)</span></label>
                  <input class="mono" type="number" step="0.001" bind:value={draft.decision.mu_H1} />
                </div>
                <div class="field">
                  <label>sigma</label>
                  <input class="mono" type="number" step="0.001" bind:value={draft.decision.sigma} />
                </div>
                <div class="field">
                  <label>alpha <span class="hint">prob. falsa alarma</span></label>
                  <input class="mono" type="number" step="0.01" bind:value={draft.decision.alpha} />
                </div>
                <div class="field">
                  <label>beta <span class="hint">prob. miss</span></label>
                  <input class="mono" type="number" step="0.01" bind:value={draft.decision.beta} />
                </div>
              </div>
            {/if}

          <!-- ── ACTUADOR ── -->
          {:else if activePanel === 'actuator'}
            <div class="field">
              <label>tipo de actuador</label>
              <select bind:value={draft.actuator.type}>
                <option value="mqtt">MQTT</option>
                <option value="http">HTTP webhook</option>
              </select>
            </div>

            {#if draft.actuator.type === 'mqtt'}
              <div class="field">
                <label>broker_url</label>
                <input class="mono" bind:value={draft.connections.mqtt.broker_url}
                  placeholder="mqtt://172.21.224.19:1883" />
              </div>
              <div class="field">
                <label>topic</label>
                <input class="mono" bind:value={draft.actuator.topic} placeholder="relay/control" />
              </div>
              <div class="field-group">
                <div class="field">
                  <label>payload ON</label>
                  <input class="mono" bind:value={draft.actuator.payload_on} placeholder="on,1" />
                </div>
                <div class="field">
                  <label>payload OFF</label>
                  <input class="mono" bind:value={draft.actuator.payload_off} placeholder="off,1" />
                </div>
              </div>
              <div class="field-group">
                <div class="field">
                  <label>username <span class="hint">opcional</span></label>
                  <input bind:value={draft.connections.mqtt.username} />
                </div>
                <div class="field">
                  <label>password <span class="hint">opcional</span></label>
                  <input type="password" bind:value={draft.connections.mqtt.password} />
                </div>
              </div>

            {:else if draft.actuator.type === 'http'}
              <div class="field">
                <label>base_url</label>
                <input class="mono" bind:value={draft.connections.http.base_url}
                  placeholder="http://host/api" />
              </div>
              <div class="field-group">
                <div class="field">
                  <label>path ON</label>
                  <input class="mono" bind:value={draft.actuator.path_on} placeholder="/relay/on" />
                </div>
                <div class="field">
                  <label>path OFF</label>
                  <input class="mono" bind:value={draft.actuator.path_off} placeholder="/relay/off" />
                </div>
              </div>
              <div class="field">
                <label>bearer_token <span class="hint">opcional</span></label>
                <input type="password" bind:value={draft.connections.http.bearer_token} />
              </div>
            {/if}
          {/if}

        </div>
      </div>
    {/if}
  </div>

  <!-- Loop interval -->
  <div class="loop-row">
    <label>Intervalo del loop</label>
    <input type="number" class="mono" bind:value={draft.loop_interval_seconds} min="5" />
    <span class="hint">segundos</span>
  </div>
</div>

<!-- Nodo custom inline (Svelte 5 component pasado como nodeTypes) -->
{#snippet PipelineNode({ data }: { data: any })}
  <div class="pipeline-node" class:active={data.active}
       style="--node-color:{data.color}"
       onclick={data.onClick}
       role="button" tabindex="0"
       onkeydown={(e) => e.key === 'Enter' && data.onClick()}>
    <Handle type="target" position={Position.Left} />
    <div class="node-icon">{data.icon}</div>
    <div class="node-label">{data.label}</div>
    <div class="node-sub">{data.sublabel}</div>
    <Handle type="source" position={Position.Right} />
  </div>
{/snippet}

<style>
  .config-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); height: 100%; }

  .toolbar { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .toolbar-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); flex: 1; }
  .save-msg { font-size: calc(12px * var(--font-scale)); }
  .save-msg.ok  { color: #3B6D11; }
  .save-msg.err { color: var(--error-color); }
  .btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-save:disabled { opacity: 0.5; }

  .canvas-wrap { display: flex; gap: 0; flex: 1; min-height: 320px; border: 0.5px solid var(--border-subtle); border-radius: 10px; overflow: hidden; }
  .flow-canvas { flex: 1; transition: flex .2s; }
  .flow-canvas.panel-open { flex: 0 0 60%; }

  /* Nodo custom */
  :global(.pipeline-node) {
    display: flex; flex-direction: column; align-items: center; gap: 4px;
    padding: 14px 20px; border-radius: 10px;
    background: var(--bg-surface); border: 2px solid var(--node-color);
    cursor: pointer; min-width: 120px; text-align: center;
    transition: all .15s; box-shadow: 0 2px 8px rgba(0,0,0,0.08);
  }
  :global(.pipeline-node:hover), :global(.pipeline-node.active) {
    background: color-mix(in srgb, var(--node-color) 12%, var(--bg-surface));
    box-shadow: 0 4px 16px rgba(0,0,0,0.14);
  }
  :global(.node-icon)  { font-size: 20px; }
  :global(.node-label) { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  :global(.node-sub)   { font-size: 10px; color: var(--text-muted); font-family: 'DM Mono', monospace; white-space: nowrap; }

  /* Panel lateral */
  .side-panel { width: 40%; border-left: 0.5px solid var(--border-subtle); display: flex; flex-direction: column; overflow-y: auto; background: var(--bg-surface); }
  .panel-head { display: flex; align-items: center; justify-content: space-between; padding: calc(12px * var(--font-scale)) calc(16px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); position: sticky; top: 0; background: var(--bg-surface); z-index: 1; }
  .panel-title { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; letter-spacing: .04em; }
  .close-btn { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 14px; }
  .panel-body { padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  /* Fields */
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .field input, .field select {
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 6px;
    background: var(--bg-elevated); color: var(--text-primary);
    font-size: calc(12px * var(--font-scale)); outline: none;
  }
  .field input:focus, .field select:focus { border-color: var(--text-primary); }
  .field-group { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .checkbox-field { flex-direction: row; align-items: center; gap: 8px; }
  .checkbox-field label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .mono { font-family: 'DM Mono', monospace !important; }
  .hint { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-weight: 400; }

  /* Sensor rows */
  .sensor-row { display: flex; gap: 6px; align-items: center; }
  .sensor-sel { flex: 1; }
  .label-in { width: 90px; flex-shrink: 0; }
  .btn-rm { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 12px; padding: 2px 4px; }
  .btn-add-sensor { align-self: flex-start; padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); margin-top: 4px; }
  .btn-add-sensor:hover { border-color: var(--text-muted); color: var(--text-primary); }

  /* Loop interval */
  .loop-row { display: flex; align-items: center; gap: 10px; padding: calc(8px * var(--font-scale)) 0; border-top: 0.5px solid var(--border-subtle); }
  .loop-row label { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }
  .loop-row input { width: 80px; padding: 4px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-elevated); color: var(--text-primary); font-family: 'DM Mono', monospace; font-size: calc(12px * var(--font-scale)); outline: none; }
</style>
