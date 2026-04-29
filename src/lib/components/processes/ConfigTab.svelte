<!-- src/lib/components/processes/ConfigTab.svelte -->
<script lang="ts">
  import { SvelteFlow, Background, Controls, Handle, Position, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { processStore, canAdmin, type PipelineConfig, type ProcessConfig } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const API = (import.meta as any).env?.VITE_API_BASE ?? '';

  const proc   = $derived($processStore.process);
  let draft    = $state<ProcessConfig | null>(null);
  let saving   = $state(false);
  let saveMsg  = $state('');
  let saveMsgOk = $state(true);

  // Pipeline activo (tab seleccionado)
  let activePl = $state(0);

  // Panel lateral
  let panelNode = $state<{plIdx: number; nodeId: string} | null>(null);

  // Inicializar draft desde config
  $effect(() => {
    if (proc?.config && !draft) {
      draft = JSON.parse(JSON.stringify(proc.config));
    }
  });

  // ── Sensores disponibles para el selector ─────────────────────────────────
  let availableSensors = $state<{id: string; label: string}[]>([]);
  $effect(() => {
    fetch(`${API}/api/v1/boxes`, { credentials: 'include' })
      .then(r => r.json())
      .then((boxes: any[]) => {
        availableSensors = boxes.flatMap(b =>
          (b.sensors ?? []).map((s: any) => ({
            id:    s.id,
            label: `${b.name} · #${s.sensor_number} · ${s.type}`,
          }))
        );
      }).catch(() => {});
  });

  // ── Nodos y edges del pipeline activo para Svelteflow ─────────────────────
  const NODE_TYPES_ORDERED = ['postgres_sensor','kalman','moving_avg','ewma','lowpass','passthrough','concat','weighted_mean','mahalanobis','hysteresis','sprt','mqtt_actuator','http_actuator','logger','select','linear_scale'];

  const NODE_COLORS: Record<string, string> = {
    postgres_sensor: '#4a90d9', kalman: '#7c6fcd', moving_avg: '#7c6fcd',
    ewma: '#7c6fcd', lowpass: '#7c6fcd', passthrough: '#8a9bb0',
    concat: '#e8a838', weighted_mean: '#e8a838',
    mahalanobis: '#e07b54', hysteresis: '#e07b54', sprt: '#e07b54',
    mqtt_actuator: '#3da85a', http_actuator: '#3da85a',
    logger: '#8a9bb0', select: '#8a9bb0', linear_scale: '#8a9bb0',
  };

  const NODE_CATEGORY: Record<string, string> = {
    postgres_sensor: 'fuente',
    kalman: 'filtro', moving_avg: 'filtro', ewma: 'filtro', lowpass: 'filtro', passthrough: 'filtro',
    concat: 'combinador', weighted_mean: 'combinador',
    mahalanobis: 'decisor', hysteresis: 'decisor', sprt: 'decisor',
    mqtt_actuator: 'actuador', http_actuator: 'actuador',
    logger: 'util', select: 'util', linear_scale: 'util',
  };

  function getFlowNodes(plIdx: number): Node[] {
    const pl = draft?.pipelines[plIdx];
    if (!pl) return [];
    return pl.nodes.map((n, i) => ({
      id:       n.id,
      type:     'pipeline_node',
      position: pl.node_positions?.[n.id] ?? { x: i * 220, y: 100 },
      data:     { node: n, plIdx, color: NODE_COLORS[n.type] ?? '#8a9bb0' },
    }));
  }

  function getFlowEdges(plIdx: number): Edge[] {
    const pl = draft?.pipelines[plIdx];
    if (!pl) return [];
    return pl.edges.map(e => ({
      id:       e.id,
      source:   e.from ?? e.source,
      target:   e.to   ?? e.target,
      animated: true,
      style:    'stroke: var(--border-default); stroke-width: 2;',
    }));
  }

  // Actualizar posiciones cuando el usuario mueve nodos
  function onNodeDragStop(event: any, plIdx: number) {
    if (!draft) return;
    const node = event.detail?.node ?? event.node;
    if (!node) return;
    if (!draft.pipelines[plIdx].node_positions) {
      draft.pipelines[plIdx].node_positions = {};
    }
    draft.pipelines[plIdx].node_positions[node.id] = node.position;
  }

  // ── Agregar / quitar pipelines ────────────────────────────────────────────
  function addPipeline() {
    if (!draft) return;
    const id = `pipeline_${Date.now()}`;
    draft.pipelines = [...draft.pipelines, {
      id, label: `Pipeline ${draft.pipelines.length + 1}`,
      loop_interval_seconds: 60,
      connections: { mqtt: null, http: null },
      nodes: [], edges: [], node_positions: {},
    }];
    activePl = draft.pipelines.length - 1;
  }

  function removePipeline(idx: number) {
    if (!draft || draft.pipelines.length <= 1) return;
    draft.pipelines = draft.pipelines.filter((_, i) => i !== idx);
    activePl = Math.min(activePl, draft.pipelines.length - 1);
  }

  function movePipeline(idx: number, dir: -1 | 1) {
    if (!draft) return;
    const newIdx = idx + dir;
    if (newIdx < 0 || newIdx >= draft.pipelines.length) return;
    const pls = [...draft.pipelines];
    [pls[idx], pls[newIdx]] = [pls[newIdx], pls[idx]];
    draft.pipelines = pls;
    activePl = newIdx;
  }

  // ── Agregar nodo al pipeline activo ──────────────────────────────────────
  function addNode(type: string) {
    if (!draft) return;
    const pl  = draft.pipelines[activePl];
    const id  = `${type}_${Date.now()}`;
    const pos = { x: pl.nodes.length * 200, y: 100 };
    pl.nodes = [...pl.nodes, { id, type, ...defaultParams(type) }];
    pl.node_positions = { ...pl.node_positions, [id]: pos };
    draft = { ...draft };
  }

  function removeNode(plIdx: number, nodeId: string) {
    if (!draft) return;
    const pl = draft.pipelines[plIdx];
    pl.nodes = pl.nodes.filter(n => n.id !== nodeId);
    pl.edges = pl.edges.filter(e => (e.from ?? e.source) !== nodeId && (e.to ?? e.target) !== nodeId);
    draft = { ...draft };
    if (panelNode?.nodeId === nodeId) panelNode = null;
  }

  function defaultParams(type: string): Record<string, any> {
    const defaults: Record<string, any> = {
      postgres_sensor: { sensors: [] },
      kalman:          { Q: 1e-5, R: 1e-3, P0: 1.0, convergence_threshold: 5e-4, warmup_samples: 10 },
      moving_avg:      { window_n: 10, warmup_samples: 10 },
      ewma:            { alpha: 0.1, warmup_samples: 10 },
      lowpass:         { tau_seconds: 120, warmup_samples: 10 },
      mahalanobis:     { target: [], threshold_act: 2.5, threshold_deact: 1.0, use_kalman_P: true },
      hysteresis:      { reduction: { type: 'mean' }, low: 0.08, high: 0.085, action_below_low: 'on', action_above_high: 'off' },
      mqtt_actuator:   { connection: 'shared', topic: '', payload_on: 'on', payload_off: 'off' },
      http_actuator:   { connection: 'shared', path_on: '/on', path_off: '/off' },
    };
    return defaults[type] ?? {};
  }

  // ── Panel de edición de nodo ──────────────────────────────────────────────
  function openPanel(plIdx: number, nodeId: string) {
    panelNode = panelNode?.nodeId === nodeId ? null : { plIdx, nodeId };
  }

  const panelNodeData = $derived(() => {
    if (!panelNode || !draft) return null;
    return draft.pipelines[panelNode.plIdx]?.nodes.find(n => n.id === panelNode!.nodeId) ?? null;
  });

  function updateNodeField(field: string, value: any) {
    if (!panelNode || !draft) return;
    const pl   = draft.pipelines[panelNode.plIdx];
    const node = pl.nodes.find(n => n.id === panelNode!.nodeId);
    if (!node) return;
    node[field] = value;
    draft = { ...draft };
  }

  // ── Guardar ───────────────────────────────────────────────────────────────
  async function save() {
    if (!draft) return;
    saving = true; saveMsg = '';
    try {
      await processStore.saveConfig(processId, draft);
      saveMsg = '✓ guardado'; saveMsgOk = true;
    } catch (e: any) {
      saveMsg = e.message; saveMsgOk = false;
    } finally { saving = false; }
  }
</script>

<div class="config-tab">

  <!-- Toolbar superior -->
  <div class="top-toolbar">
    <div class="add-node-row">
      <span class="add-label">agregar nodo</span>
      {#each ['postgres_sensor','kalman','moving_avg','mahalanobis','hysteresis','mqtt_actuator','http_actuator','logger','select'] as type}
        <button class="add-btn" style="border-color:{NODE_COLORS[type]}44; color:{NODE_COLORS[type]}"
          onclick={() => addNode(type)}>
          + {type.replace('_', ' ')}
        </button>
      {/each}
    </div>
    <div class="save-row">
      {#if saveMsg}
        <span class="save-msg" class:ok={saveMsgOk} class:err={!saveMsgOk}>{saveMsg}</span>
      {/if}
      {#if $canAdmin}
        <button class="btn-save" disabled={saving} onclick={save}>
          {saving ? 'guardando...' : 'guardar'}
        </button>
      {/if}
    </div>
  </div>

  <!-- Tabs de pipelines -->
  <div class="pl-tabs">
    {#each (draft?.pipelines ?? []) as pl, i (pl.id)}
      <div class="pl-tab-wrap">
        <button class="pl-tab" class:active={activePl === i}
          onclick={() => activePl = i}>
          {pl.label}
        </button>
        {#if activePl === i && $canAdmin}
          <div class="pl-tab-actions">
            <button class="tact" onclick={() => movePipeline(i, -1)} disabled={i === 0} title="subir">↑</button>
            <button class="tact" onclick={() => movePipeline(i, 1)} disabled={i === (draft?.pipelines.length ?? 1) - 1} title="bajar">↓</button>
            <button class="tact tact--del" onclick={() => removePipeline(i)} title="eliminar">✕</button>
          </div>
        {/if}
      </div>
    {/each}
    {#if $canAdmin}
      <button class="pl-tab-add" onclick={addPipeline}>+ pipeline</button>
    {/if}
  </div>

  {#if draft?.pipelines.length === 0}
    <div class="empty-canvas">
      No hay pipelines. Hacé click en <strong>+ pipeline</strong> para crear uno.
    </div>

  {:else if draft}
    {@const pl = draft.pipelines[activePl]}
    <div class="canvas-wrap" class:panel-open={panelNode !== null}>

      <!-- Nombre del pipeline editable -->
      {#if $canAdmin}
        <div class="pl-name-row">
          <input class="pl-name-input" bind:value={pl.label} placeholder="Nombre del pipeline" />
          <span class="pl-interval-label">intervalo</span>
          <input class="pl-interval-input" type="number" min="5"
            bind:value={pl.loop_interval_seconds} />
          <span class="pl-interval-unit">s</span>
        </div>
      {/if}

      <!-- Svelteflow canvas -->
      <div class="flow-canvas">
        <SvelteFlow
          nodes={getFlowNodes(activePl)}
          edges={getFlowEdges(activePl)}
          fitView
          nodesDraggable={$canAdmin}
          nodesConnectable={false}
          elementsSelectable={true}
          on:nodedragstop={(e) => onNodeDragStop(e, activePl)}
        >
          <Background gap={24} color="var(--border-subtle)" />
          <Controls showInteractive={false} />

          {#snippet nodeTypes()}
            {#each getFlowNodes(activePl) as flowNode (flowNode.id)}
              {@const n     = flowNode.data.node}
              {@const color = flowNode.data.color}
              {@const isActive = panelNode?.nodeId === n.id}
              <div
                class="flow-node"
                class:flow-node--active={isActive}
                style="--nc:{color}"
                onclick={() => openPanel(activePl, n.id)}
                onkeydown={(e) => e.key === 'Enter' && openPanel(activePl, n.id)}
                role="button" tabindex="0"
              >
                <Handle type="target" position={Position.Left} />
                <div class="fn-category">{NODE_CATEGORY[n.type] ?? ''}</div>
                <div class="fn-type">{n.type.replace(/_/g, ' ')}</div>
                <div class="fn-id">{n.id}</div>
                <Handle type="source" position={Position.Right} />
              </div>
            {/each}
          {/snippet}
        </SvelteFlow>
      </div>

      <!-- Panel lateral de edición de nodo -->
      {#if panelNode !== null}
        {@const nd = panelNodeData()}
        {#if nd}
          <div class="side-panel">
            <div class="sp-head">
              <div>
                <span class="sp-type">{nd.type.replace(/_/g, ' ')}</span>
                <span class="sp-id">{nd.id}</span>
              </div>
              <div class="sp-head-actions">
                {#if $canAdmin}
                  <button class="sp-del" onclick={() => removeNode(panelNode!.plIdx, nd.id)} title="Eliminar nodo">🗑</button>
                {/if}
                <button class="sp-close" onclick={() => panelNode = null}>✕</button>
              </div>
            </div>

            <div class="sp-body">

              <!-- ── FUENTE ── -->
              {#if nd.type === 'postgres_sensor'}
                <div class="sp-section">
                  <div class="sp-section-label">sensores <span class="sp-hint">(orden = índice del vector)</span></div>
                  {#each (nd.sensors ?? []) as sensor, si (si)}
                    <div class="sensor-row">
                      <select class="sensor-sel"
                        value={sensor.id}
                        onchange={(e) => {
                          const found = availableSensors.find(s => s.id === (e.target as HTMLSelectElement).value);
                          const sensors = [...(nd.sensors ?? [])];
                          sensors[si] = { id: (e.target as HTMLSelectElement).value, label: found?.label ?? '' };
                          updateNodeField('sensors', sensors);
                        }}>
                        <option value="">— elegir sensor —</option>
                        {#each availableSensors as s (s.id)}
                          <option value={s.id}>{s.label}</option>
                        {/each}
                      </select>
                      <input class="sensor-label-in" value={sensor.label}
                        oninput={(e) => {
                          const sensors = [...(nd.sensors ?? [])];
                          sensors[si] = { ...sensors[si], label: (e.target as HTMLInputElement).value };
                          updateNodeField('sensors', sensors);
                        }}
                        placeholder="etiqueta" />
                      <button class="btn-rm" onclick={() => {
                        const sensors = (nd.sensors ?? []).filter((_: any, j: number) => j !== si);
                        updateNodeField('sensors', sensors);
                      }}>✕</button>
                    </div>
                  {/each}
                  <button class="btn-add-sensor" onclick={() => {
                    updateNodeField('sensors', [...(nd.sensors ?? []), { id: '', label: '' }]);
                  }}>+ agregar sensor</button>
                </div>

              <!-- ── KALMAN ── -->
              {:else if nd.type === 'kalman'}
                <div class="sp-section">
                  {#each [['Q','varianza de proceso'],['R','varianza de medición'],['P0','covarianza inicial'],['convergence_threshold','umbral de convergencia']] as [field, hint]}
                    <div class="sp-field">
                      <label>{field} <span class="sp-hint">{hint}</span></label>
                      <input class="mono" value={nd[field]} oninput={(e) => updateNodeField(field, parseFloat((e.target as HTMLInputElement).value))} />
                    </div>
                  {/each}
                  <div class="sp-field">
                    <label>warmup_samples</label>
                    <input type="number" value={nd.warmup_samples} oninput={(e) => updateNodeField('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
                  </div>
                </div>

              <!-- ── MOVING AVG ── -->
              {:else if nd.type === 'moving_avg'}
                <div class="sp-section">
                  <div class="sp-field"><label>window_n</label>
                    <input type="number" value={nd.window_n} oninput={(e) => updateNodeField('window_n', parseInt((e.target as HTMLInputElement).value))} /></div>
                  <div class="sp-field"><label>warmup_samples</label>
                    <input type="number" value={nd.warmup_samples} oninput={(e) => updateNodeField('warmup_samples', parseInt((e.target as HTMLInputElement).value))} /></div>
                </div>

              <!-- ── EWMA ── -->
              {:else if nd.type === 'ewma'}
                <div class="sp-section">
                  <div class="sp-field"><label>alpha <span class="sp-hint">[0,1]</span></label>
                    <input class="mono" value={nd.alpha} oninput={(e) => updateNodeField('alpha', parseFloat((e.target as HTMLInputElement).value))} /></div>
                  <div class="sp-field"><label>warmup_samples</label>
                    <input type="number" value={nd.warmup_samples} oninput={(e) => updateNodeField('warmup_samples', parseInt((e.target as HTMLInputElement).value))} /></div>
                </div>

              <!-- ── MAHALANOBIS ── -->
              {:else if nd.type === 'mahalanobis'}
                <div class="sp-section">
                  <div class="sp-field">
                    <label>target <span class="sp-hint">valores separados por coma</span></label>
                    <input class="mono"
                      value={(nd.target ?? []).join(', ')}
                      oninput={(e) => updateNodeField('target',
                        (e.target as HTMLInputElement).value.split(',').map(v => parseFloat(v.trim())).filter(v => !isNaN(v))
                      )} />
                  </div>
                  <div class="sp-field-row">
                    <div class="sp-field">
                      <label>threshold_act</label>
                      <input class="mono" type="number" step="0.1" value={nd.threshold_act} oninput={(e) => updateNodeField('threshold_act', parseFloat((e.target as HTMLInputElement).value))} />
                    </div>
                    <div class="sp-field">
                      <label>threshold_deact</label>
                      <input class="mono" type="number" step="0.1" value={nd.threshold_deact} oninput={(e) => updateNodeField('threshold_deact', parseFloat((e.target as HTMLInputElement).value))} />
                    </div>
                  </div>
                  <div class="sp-field sp-checkbox">
                    <input type="checkbox" id="use-P" checked={nd.use_kalman_P} onchange={(e) => updateNodeField('use_kalman_P', (e.target as HTMLInputElement).checked)} />
                    <label for="use-P">usar P del Kalman como Σ</label>
                  </div>
                </div>

              <!-- ── HYSTERESIS ── -->
              {:else if nd.type === 'hysteresis'}
                <div class="sp-section">
                  <div class="sp-field">
                    <label>reducción</label>
                    <select value={nd.reduction?.type ?? 'mean'} onchange={(e) => updateNodeField('reduction', { type: (e.target as HTMLSelectElement).value })}>
                      <option value="mean">media</option>
                      <option value="min">mínimo</option>
                      <option value="max">máximo</option>
                      <option value="weighted_by_p">ponderado por P</option>
                    </select>
                  </div>
                  <div class="sp-field-row">
                    <div class="sp-field"><label>low</label>
                      <input class="mono" type="number" step="0.001" value={nd.low} oninput={(e) => updateNodeField('low', parseFloat((e.target as HTMLInputElement).value))} /></div>
                    <div class="sp-field"><label>high</label>
                      <input class="mono" type="number" step="0.001" value={nd.high} oninput={(e) => updateNodeField('high', parseFloat((e.target as HTMLInputElement).value))} /></div>
                  </div>
                </div>

              <!-- ── MQTT ACTUADOR ── -->
              {:else if nd.type === 'mqtt_actuator'}
                <div class="sp-section">
                  <div class="sp-field">
                    <label>conexión</label>
                    <select value={nd.connection ?? 'shared'} onchange={(e) => updateNodeField('connection', (e.target as HTMLSelectElement).value)}>
                      <option value="shared">shared (proceso)</option>
                      {#each (draft?.pipelines ?? []) as opl}
                        {#if opl.id !== pl.id}
                          <option value="pipeline:{opl.id}">pipeline: {opl.label}</option>
                        {/if}
                      {/each}
                    </select>
                  </div>
                  {#if nd.connection === 'shared' || !nd.connection}
                    <div class="sp-note">Usando conexiones compartidas del proceso. Configurar en "conexiones compartidas" abajo.</div>
                  {/if}
                  <div class="sp-field"><label>topic</label>
                    <input class="mono" value={nd.topic ?? ''} oninput={(e) => updateNodeField('topic', (e.target as HTMLInputElement).value)} placeholder="relay/control" /></div>
                  <div class="sp-field-row">
                    <div class="sp-field"><label>payload ON</label>
                      <input class="mono" value={nd.payload_on ?? ''} oninput={(e) => updateNodeField('payload_on', (e.target as HTMLInputElement).value)} /></div>
                    <div class="sp-field"><label>payload OFF</label>
                      <input class="mono" value={nd.payload_off ?? ''} oninput={(e) => updateNodeField('payload_off', (e.target as HTMLInputElement).value)} /></div>
                  </div>
                </div>

              <!-- ── HTTP ACTUADOR ── -->
              {:else if nd.type === 'http_actuator'}
                <div class="sp-section">
                  <div class="sp-field-row">
                    <div class="sp-field"><label>path ON</label>
                      <input class="mono" value={nd.path_on ?? ''} oninput={(e) => updateNodeField('path_on', (e.target as HTMLInputElement).value)} /></div>
                    <div class="sp-field"><label>path OFF</label>
                      <input class="mono" value={nd.path_off ?? ''} oninput={(e) => updateNodeField('path_off', (e.target as HTMLInputElement).value)} /></div>
                  </div>
                </div>

              {:else}
                <div class="sp-section">
                  <div class="sp-note">Nodo de tipo <strong>{nd.type}</strong>. Edición en JSON:</div>
                  <textarea class="json-edit" rows="8"
                    value={JSON.stringify(nd, null, 2)}
                    onchange={(e) => {
                      try {
                        const parsed = JSON.parse((e.target as HTMLTextAreaElement).value);
                        const pl2 = draft!.pipelines[panelNode!.plIdx];
                        const idx2 = pl2.nodes.findIndex(n => n.id === panelNode!.nodeId);
                        if (idx2 >= 0) { pl2.nodes[idx2] = parsed; draft = { ...draft! }; }
                      } catch {}
                    }}></textarea>
                </div>
              {/if}

            </div>
          </div>
        {/if}
      {/if}
    </div>

    <!-- Conexiones compartidas del proceso -->
    {#if $canAdmin && draft}
      <div class="shared-conn">
        <div class="shared-conn-head">conexiones compartidas del proceso</div>
        <div class="shared-fields">
          <div class="sp-field">
            <label>MQTT broker URL</label>
            <input class="mono"
              value={draft.shared_connections?.mqtt?.broker_url ?? ''}
              oninput={(e) => {
                if (!draft!.shared_connections) draft!.shared_connections = {};
                if (!draft!.shared_connections.mqtt) draft!.shared_connections.mqtt = { broker_url: '', client_id: 'agrodash-{process_id}' };
                draft!.shared_connections.mqtt.broker_url = (e.target as HTMLInputElement).value;
                draft = { ...draft! };
              }}
              placeholder="mqtt://172.21.224.19:1883" />
          </div>
          <div class="sp-field">
            <label>client_id</label>
            <input class="mono"
              value={draft.shared_connections?.mqtt?.client_id ?? 'agrodash-{process_id}'}
              oninput={(e) => {
                if (!draft!.shared_connections?.mqtt) return;
                draft!.shared_connections.mqtt.client_id = (e.target as HTMLInputElement).value;
                draft = { ...draft! };
              }} />
          </div>
        </div>
      </div>
    {/if}
  {/if}

</div>

<style>
  .config-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  /* Toolbar */
  .top-toolbar { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .add-node-row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .add-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .add-btn { padding: calc(3px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .add-btn:hover { background: var(--interactive-hover); }
  .save-row { display: flex; align-items: center; gap: 8px; }
  .save-msg { font-size: calc(12px * var(--font-scale)); }
  .save-msg.ok  { color: #3B6D11; }
  .save-msg.err { color: var(--error-color); }
  .btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-save:disabled { opacity: 0.5; }

  /* Pipeline tabs */
  .pl-tabs { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; border-bottom: 0.5px solid var(--border-subtle); padding-bottom: 0; }
  .pl-tab-wrap { display: flex; align-items: center; }
  .pl-tab { padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: none; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); border-bottom: 2px solid transparent; margin-bottom: -0.5px; font-family: 'DM Mono', monospace; }
  .pl-tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }
  .pl-tab-actions { display: flex; gap: 2px; padding: 0 4px; }
  .tact { width: 18px; height: 18px; border: none; background: none; cursor: pointer; font-size: 10px; color: var(--text-muted); border-radius: 3px; padding: 0; display: flex; align-items: center; justify-content: center; }
  .tact:hover { background: var(--interactive-hover); }
  .tact--del:hover { background: var(--error-bg); color: var(--error-color); }
  .tact:disabled { opacity: 0.3; cursor: default; }
  .pl-tab-add { padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-bottom: 4px; }
  .pl-tab-add:hover { border-color: var(--text-muted); color: var(--text-primary); }

  /* Canvas area */
  .empty-canvas { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); text-align: center; padding: 60px 0; line-height: 1.8; }
  .canvas-wrap { display: flex; gap: 0; border: 0.5px solid var(--border-subtle); border-radius: 10px; overflow: hidden; height: 480px; }
  .canvas-wrap.panel-open .flow-canvas { flex: 0 0 60%; }
  .flow-canvas { flex: 1; }

  /* Pipeline name / interval */
  .pl-name-row { position: absolute; top: 8px; left: 8px; z-index: 10; display: flex; align-items: center; gap: 8px; background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 6px; padding: 4px 8px; }
  .pl-name-input { border: none; background: none; font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); outline: none; width: 180px; font-family: 'DM Mono', monospace; }
  .pl-interval-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .pl-interval-input { width: 60px; border: none; background: none; font-size: calc(13px * var(--font-scale)); color: var(--text-primary); outline: none; font-family: 'DM Mono', monospace; }
  .pl-interval-unit { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }

  /* Flow nodes */
  :global(.flow-node) {
    display: flex; flex-direction: column; align-items: center; gap: 3px;
    padding: 10px 16px; border-radius: 8px;
    background: var(--bg-surface); border: 2px solid var(--nc);
    cursor: pointer; min-width: 120px; text-align: center;
    transition: all .15s; box-shadow: 0 2px 8px rgba(0,0,0,0.06);
    position: relative;
  }
  :global(.flow-node:hover), :global(.flow-node--active) {
    background: color-mix(in srgb, var(--nc) 10%, var(--bg-surface));
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
  }
  :global(.fn-category) { font-size: 9px; color: var(--nc); text-transform: uppercase; letter-spacing: .08em; font-family: 'DM Mono', monospace; }
  :global(.fn-type) { font-size: 12px; font-weight: 600; color: var(--text-primary); }
  :global(.fn-id) { font-size: 9px; color: var(--text-muted); font-family: 'DM Mono', monospace; }

  /* Side panel */
  .side-panel { width: 40%; border-left: 0.5px solid var(--border-subtle); display: flex; flex-direction: column; overflow-y: auto; background: var(--bg-surface); }
  .sp-head { display: flex; align-items: center; justify-content: space-between; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); position: sticky; top: 0; background: var(--bg-surface); z-index: 1; }
  .sp-type { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; display: block; }
  .sp-id { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .sp-head-actions { display: flex; gap: 6px; align-items: center; }
  .sp-del { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .sp-del:hover { color: var(--error-color); }
  .sp-close { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 14px; }
  .sp-body { padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }

  .sp-section { display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); }
  .sp-section-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-weight: 500; letter-spacing: .04em; }
  .sp-hint { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-weight: 400; }
  .sp-note { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }
  .sp-field { display: flex; flex-direction: column; gap: 3px; }
  .sp-field label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .sp-field input, .sp-field select, .sp-field textarea {
    padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 5px;
    background: var(--bg-elevated); color: var(--text-primary);
    font-size: calc(12px * var(--font-scale)); outline: none;
  }
  .sp-field input:focus, .sp-field select:focus { border-color: var(--text-primary); }
  .sp-field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .sp-checkbox { flex-direction: row; align-items: center; gap: 8px; }
  .sp-checkbox label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .mono { font-family: 'DM Mono', monospace !important; }
  .json-edit { font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale)); resize: vertical; }

  /* Sensor rows */
  .sensor-row { display: flex; gap: 6px; align-items: center; }
  .sensor-sel { flex: 1; font-size: calc(11px * var(--font-scale)) !important; }
  .sensor-label-in { width: 80px; flex-shrink: 0; font-size: calc(11px * var(--font-scale)) !important; }
  .btn-rm { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 12px; }
  .btn-add-sensor { align-self: flex-start; padding: calc(4px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); margin-top: 2px; }
  .btn-add-sensor:hover { color: var(--text-primary); }

  /* Shared connections */
  .shared-conn { background: var(--bg-elevated); border: 0.5px solid var(--border-subtle); border-radius: 8px; padding: calc(12px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }
  .shared-conn-head { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-muted); font-family: 'DM Mono', monospace; letter-spacing: .05em; }
  .shared-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }

  @media (max-width: 768px) {
    .canvas-wrap { height: 360px; flex-direction: column; }
    .canvas-wrap.panel-open .flow-canvas { flex: 0 0 50%; }
    .side-panel { width: 100%; height: 50%; border-left: none; border-top: 0.5px solid var(--border-subtle); }
    .shared-fields { grid-template-columns: 1fr; }
  }
</style>
