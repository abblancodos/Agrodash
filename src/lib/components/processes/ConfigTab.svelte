<!-- src/lib/components/processes/ConfigTab.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { SvelteFlow, Background, Controls, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import PipelineNode from './PipelineNode.svelte';
  import { processStore, canAdmin, type ProcessConfig } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const API = (import.meta as any).env?.VITE_API_BASE ?? '';
  const nodeTypes = { pipeline_node: PipelineNode };

  const proc = $derived($processStore.process);
  let draft     = $state<ProcessConfig | null>(null);
  let saving    = $state(false);
  let saveMsg   = $state('');
  let saveMsgOk = $state(true);
  let dirty     = $state(false);      // cambios sin guardar al servidor
  let activePl  = $state(0);
  let panelNode = $state<{plIdx: number; nodeId: string} | null>(null);
  let showRestoreBanner = $state(false);
  let localDraft: ProcessConfig | null = null;

  const STORAGE_KEY = () => `agrodash_process_draft_${processId}`;

  // ── Init: cargar draft desde servidor, luego chequear localStorage ────────
  $effect(() => {
    if (proc?.config && !draft) {
      const serverCfg: ProcessConfig = JSON.parse(JSON.stringify(proc.config));

      // Chequear si hay draft local más reciente
      try {
        const stored = localStorage.getItem(STORAGE_KEY());
        if (stored) {
          const parsed = JSON.parse(stored);
          localDraft = parsed.config;
          const localTs = new Date(parsed.ts);
          const serverTs = new Date(proc.updated_at ?? 0);
          if (localTs > serverTs) {
            showRestoreBanner = true;
            draft = serverCfg; // mostrar server version, ofrecemos restaurar
          } else {
            localStorage.removeItem(STORAGE_KEY());
            draft = serverCfg;
          }
        } else {
          draft = serverCfg;
        }
      } catch {
        draft = serverCfg;
      }
    }
  });

  function restoreLocal() {
    if (localDraft) { draft = localDraft; dirty = true; }
    showRestoreBanner = false;
  }

  function discardLocal() {
    localStorage.removeItem(STORAGE_KEY());
    showRestoreBanner = false;
    localDraft = null;
  }

  // ── Debounce: guardar en localStorage 2s después de cada cambio ───────────
  let saveLocalTimer: ReturnType<typeof setTimeout> | null = null;

  function schedulLocalSave() {
    dirty = true;
    if (saveLocalTimer) clearTimeout(saveLocalTimer);
    saveLocalTimer = setTimeout(() => {
      if (!draft) return;
      try {
        localStorage.setItem(STORAGE_KEY(), JSON.stringify({ config: draft, ts: new Date().toISOString() }));
      } catch {}
    }, 2000);
  }

  // ── Aviso beforeunload si hay cambios sin guardar ─────────────────────────
  function handleBeforeUnload(e: BeforeUnloadEvent) {
    if (dirty) {
      e.preventDefault();
      return '';
    }
  }

  onMount(() => window.addEventListener('beforeunload', handleBeforeUnload));
  onDestroy(() => {
    window.removeEventListener('beforeunload', handleBeforeUnload);
    if (saveLocalTimer) clearTimeout(saveLocalTimer);
  });

  // ── Sensores disponibles ──────────────────────────────────────────────────
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

  // ── Node metadata ─────────────────────────────────────────────────────────
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

  // ── Flow nodes/edges reactivos ────────────────────────────────────────────
  let flowNodes = $state<Node[]>([]);
  let flowEdges = $state<Edge[]>([]);

  $effect(() => {
    const pl = draft?.pipelines[activePl];
    if (!pl) { flowNodes = []; flowEdges = []; return; }

    flowNodes = pl.nodes.map((n, i) => ({
      id:       n.id,
      type:     'pipeline_node',
      position: pl.node_positions?.[n.id] ?? { x: 80 + i * 220, y: 120 },
      data: {
        nodeId:   n.id,
        label:    n.type.replace(/_/g, ' '),
        category: NODE_CATEGORY[n.type] ?? '',
        color:    NODE_COLORS[n.type] ?? '#8a9bb0',
        active:   panelNode?.nodeId === n.id,
        onClick:  () => openPanel(activePl, n.id),
      },
    }));

    flowEdges = (pl.edges ?? []).map(e => ({
      id:       e.id ?? `${e.from ?? (e as any).source}-${e.to ?? (e as any).target}`,
      source:   e.from ?? (e as any).source,
      target:   e.to   ?? (e as any).target,
      animated: true,
      deletable: true,
    }));
  });

  function openPanel(plIdx: number, nodeId: string) {
    panelNode = panelNode?.nodeId === nodeId ? null : { plIdx, nodeId };
    flowNodes = flowNodes.map(n => ({
      ...n,
      data: { ...n.data, active: n.id === nodeId && panelNode !== null },
    }));
  }

  // ── Drag stop: guardar posición ───────────────────────────────────────────
  function onNodeDragStop(event: CustomEvent) {
    const node = event.detail?.targetNode ?? event.detail?.node;
    if (!node || !draft) return;
    const pl = draft.pipelines[activePl];
    if (!pl.node_positions) pl.node_positions = {};
    pl.node_positions[node.id] = { x: node.position.x, y: node.position.y };
    schedulLocalSave();
  }

  // ── Connect: crear edge arrastrando ──────────────────────────────────────
  function onConnect(event: CustomEvent) {
    const conn = event.detail?.connection ?? event.detail;
    if (!conn || !draft) return;
    const pl = draft.pipelines[activePl];
    const newEdge = {
      id:     `${conn.source}-${conn.target}-${Date.now()}`,
      from:   conn.source, to: conn.target,
      source: conn.source, target: conn.target,
    };
    pl.edges = [...(pl.edges ?? []), newEdge];
    draft = { ...draft };
    schedulLocalSave();
  }

  // ── Delete nodos y edges (teclado + evento SvelteFlow) ────────────────────
  function onNodesDelete(event: CustomEvent) {
    const deleted: Node[] = event.detail?.nodes ?? [];
    if (!draft || !deleted.length) return;
    const pl = draft.pipelines[activePl];
    const ids = new Set(deleted.map(n => n.id));
    pl.nodes = pl.nodes.filter(n => !ids.has(n.id));
    pl.edges = (pl.edges ?? []).filter(e =>
      !ids.has(e.from ?? (e as any).source) && !ids.has(e.to ?? (e as any).target)
    );
    if (panelNode && ids.has(panelNode.nodeId)) panelNode = null;
    draft = { ...draft };
    schedulLocalSave();
  }

  function onEdgesDelete(event: CustomEvent) {
    const deleted: Edge[] = event.detail?.edges ?? [];
    if (!draft || !deleted.length) return;
    const pl = draft.pipelines[activePl];
    const ids = new Set(deleted.map(e => e.id));
    pl.edges = (pl.edges ?? []).filter(e => {
      const eid = e.id ?? `${e.from ?? (e as any).source}-${e.to ?? (e as any).target}`;
      return !ids.has(eid);
    });
    draft = { ...draft };
    schedulLocalSave();
  }

  // ── Add / remove nodes ────────────────────────────────────────────────────
  function addNode(type: string) {
    if (!draft) return;
    const pl  = draft.pipelines[activePl];
    const id  = `${type}_${Date.now()}`;
    const pos = { x: 80 + pl.nodes.length * 220, y: 120 };
    pl.nodes = [...pl.nodes, { id, type, ...defaultParams(type) }];
    if (!pl.node_positions) pl.node_positions = {};
    pl.node_positions[id] = pos;
    draft = { ...draft };
    schedulLocalSave();
  }

  function removeNode(plIdx: number, nodeId: string) {
    if (!draft) return;
    const pl = draft.pipelines[plIdx];
    pl.nodes = pl.nodes.filter(n => n.id !== nodeId);
    pl.edges = (pl.edges ?? []).filter(e =>
      (e.from ?? (e as any).source) !== nodeId &&
      (e.to   ?? (e as any).target) !== nodeId
    );
    draft = { ...draft };
    if (panelNode?.nodeId === nodeId) panelNode = null;
    schedulLocalSave();
  }

  function defaultParams(type: string): Record<string, any> {
    const d: Record<string, any> = {
      postgres_sensor: { sensors: [] },
      kalman:   { Q: 1e-5, R: 1e-3, P0: 1.0, convergence_threshold: 5e-4, warmup_samples: 10 },
      moving_avg: { window_n: 10, warmup_samples: 10 },
      ewma:     { alpha: 0.1, warmup_samples: 10 },
      lowpass:  { tau_seconds: 120, warmup_samples: 10 },
      mahalanobis: { target: [], threshold_act: 2.5, threshold_deact: 1.0, use_kalman_P: true },
      hysteresis:  { reduction: { type: 'mean' }, low: 0.08, high: 0.085, action_below_low: 'on', action_above_high: 'off' },
      mqtt_actuator: { connection: 'shared', topic: '', payload_on: 'on', payload_off: 'off' },
      http_actuator: { connection: 'shared', path_on: '/on', path_off: '/off' },
    };
    return d[type] ?? {};
  }

  // ── Panel node data ───────────────────────────────────────────────────────
  const panelNodeData = $derived(() => {
    if (!panelNode || !draft) return null;
    return draft.pipelines[panelNode.plIdx]?.nodes.find(n => n.id === panelNode!.nodeId) ?? null;
  });

  function updateNodeField(field: string, value: any) {
    if (!panelNode || !draft) return;
    const node = draft.pipelines[panelNode.plIdx].nodes.find(n => n.id === panelNode!.nodeId);
    if (!node) return;
    node[field] = value;
    draft = { ...draft };
    schedulLocalSave();
  }

  // ── Pipeline management ───────────────────────────────────────────────────
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
    draft = { ...draft };
    schedulLocalSave();
  }

  function removePipeline(idx: number) {
    if (!draft || draft.pipelines.length <= 1) return;
    draft.pipelines = draft.pipelines.filter((_, i) => i !== idx);
    activePl = Math.min(activePl, draft.pipelines.length - 1);
    draft = { ...draft };
    schedulLocalSave();
  }

  function movePipeline(idx: number, dir: -1 | 1) {
    if (!draft) return;
    const ni = idx + dir;
    if (ni < 0 || ni >= draft.pipelines.length) return;
    const pls = [...draft.pipelines];
    [pls[idx], pls[ni]] = [pls[ni], pls[idx]];
    draft.pipelines = pls;
    activePl = ni;
    draft = { ...draft };
    schedulLocalSave();
  }

  // ── Save to server ────────────────────────────────────────────────────────
  async function save() {
    if (!draft) return;
    saving = true; saveMsg = '';
    try {
      await processStore.saveConfig(processId, draft);
      dirty = false;
      localStorage.removeItem(STORAGE_KEY());
      saveMsg = '✓ guardado'; saveMsgOk = true;
    } catch (e: any) {
      saveMsg = e.message; saveMsgOk = false;
    } finally { saving = false; }
  }

</script>

<div class="config-tab">

  <!-- Restore banner -->
  {#if showRestoreBanner && localDraft}
    <div class="restore-banner">
      <span class="rb-icon">💾</span>
      <span>Hay cambios locales sin guardar al servidor desde antes. ¿Restaurar?</span>
      <button class="rb-btn rb-btn--yes" onclick={restoreLocal}>Restaurar</button>
      <button class="rb-btn rb-btn--no" onclick={discardLocal}>Descartar</button>
    </div>
  {/if}

  <!-- Toolbar -->
  <div class="top-toolbar">
    <div class="add-node-row">
      <span class="add-label">agregar nodo</span>
      {#each ['postgres_sensor','kalman','moving_avg','mahalanobis','hysteresis','mqtt_actuator','http_actuator','logger','select'] as type (type)}
        <button class="add-btn"
          style="border-color:{NODE_COLORS[type]}44; color:{NODE_COLORS[type]}"
          onclick={() => addNode(type)}>
          + {type.replace(/_/g, ' ')}
        </button>
      {/each}
    </div>
    <div class="save-row">
      {#if dirty}
        <span class="local-unsaved">● cambios sin guardar</span>
      {/if}
      {#if saveMsg}
        <span class="save-msg" class:ok={saveMsgOk} class:err={!saveMsgOk}>{saveMsg}</span>
      {/if}
      {#if $canAdmin}
        <button class="btn-save" class:btn-save--dirty={dirty} disabled={saving} onclick={save}>
          {saving ? 'guardando...' : dirty ? '⬆ guardar' : 'guardar'}
        </button>
      {/if}
    </div>
  </div>

  <!-- Pipeline tabs -->
  <div class="pl-tabs">
    {#each (draft?.pipelines ?? []) as pl, i (pl.id)}
      <div class="pl-tab-wrap">
        <button class="pl-tab" class:active={activePl === i}
          onclick={() => { activePl = i; panelNode = null; }}>
          {pl.label}
        </button>
        {#if activePl === i && $canAdmin}
          <div class="pl-tab-actions">
            <button class="tact" onclick={() => movePipeline(i, -1)} disabled={i === 0} title="subir">↑</button>
            <button class="tact" onclick={() => movePipeline(i, 1)}
              disabled={i === (draft?.pipelines.length ?? 1) - 1} title="bajar">↓</button>
            <button class="tact tact--del" onclick={() => removePipeline(i)} title="eliminar pipeline">✕</button>
          </div>
        {/if}
      </div>
    {/each}
    {#if $canAdmin}
      <button class="pl-tab-add" onclick={addPipeline}>+ pipeline</button>
    {/if}
  </div>

  {#if !draft || draft.pipelines.length === 0}
    <div class="empty-canvas">
      No hay pipelines. Hacé click en <strong>+ pipeline</strong> para crear uno.
    </div>
  {:else}
    {@const pl = draft.pipelines[activePl]}

    {#if $canAdmin}
      <div class="pl-name-row">
        <input class="pl-name-input" bind:value={pl.label}
          oninput={schedulLocalSave} placeholder="Nombre del pipeline" />
        <span class="pl-sep">·</span>
        <label class="pl-interval-label" for="pl-interval">intervalo</label>
        <input id="pl-interval" class="pl-interval-input mono" type="number" min="5"
          bind:value={pl.loop_interval_seconds} oninput={schedulLocalSave} />
        <span class="pl-interval-unit">s</span>
      </div>
    {/if}

    <!-- Delete hint -->
    <div class="delete-hint">
      <span>Seleccioná un nodo o edge y presioná <kbd>Delete</kbd> o <kbd>Backspace</kbd> para borrar · También podés hacer click en un nodo para editarlo</span>
    </div>

    <div class="canvas-wrap" class:panel-open={panelNode !== null}>
      <div class="flow-canvas">
        <SvelteFlow
          nodes={flowNodes}
          edges={flowEdges}
          {nodeTypes}
          fitView
          nodesDraggable={$canAdmin}
          nodesConnectable={$canAdmin}
          deleteKey={['Delete', 'Backspace']}
          on:nodedragstop={onNodeDragStop}
          on:connect={onConnect}
          on:nodesdelete={onNodesDelete}
          on:edgesdelete={onEdgesDelete}
        >
          <Background gap={24} color="var(--border-subtle, #e5e7eb)" />
          <Controls showInteractive={false} />
        </SvelteFlow>
      </div>

      <!-- Side panel -->
      {#if panelNode !== null}
        {@const nd = panelNodeData()}
        {#if nd}
          <div class="side-panel">
            <div class="sp-head">
              <div>
                <span class="sp-type">{nd.type.replace(/_/g, ' ')}</span>
                <span class="sp-id">{nd.id}</span>
              </div>
              <div class="sp-actions">
                {#if $canAdmin}
                  <button class="sp-del" onclick={() => removeNode(panelNode!.plIdx, nd.id)}
                    title="Eliminar nodo (también podés seleccionarlo y presionar Delete)">🗑</button>
                {/if}
                <button class="sp-close" onclick={() => panelNode = null}>✕</button>
              </div>
            </div>

            <div class="sp-body">

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
                          sensors[si] = { id: (e.target as HTMLSelectElement).value, label: found?.label.split('·')[2]?.trim() ?? '' };
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
                        }} placeholder="etiqueta" />
                      <button class="btn-rm" onclick={() => updateNodeField('sensors', (nd.sensors ?? []).filter((_: any, j: number) => j !== si))}>✕</button>
                    </div>
                  {/each}
                  <button class="btn-add-sensor" onclick={() => updateNodeField('sensors', [...(nd.sensors ?? []), { id: '', label: '' }])}>+ sensor</button>
                </div>

              {:else if nd.type === 'kalman'}
                {#each [['Q','varianza proceso'],['R','varianza medición'],['P0','cov. inicial'],['convergence_threshold','umbral convergencia']] as [f, h] (f)}
                  <div class="sp-field">
                    <label for="kf-{f}">{f} <span class="sp-hint">{h}</span></label>
                    <input id="kf-{f}" class="mono" value={nd[f]}
                      oninput={(e) => updateNodeField(f, parseFloat((e.target as HTMLInputElement).value))} />
                  </div>
                {/each}
                <div class="sp-field">
                  <label for="kf-warmup">warmup_samples</label>
                  <input id="kf-warmup" type="number" value={nd.warmup_samples}
                    oninput={(e) => updateNodeField('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
                </div>

              {:else if nd.type === 'moving_avg'}
                <div class="sp-field">
                  <label for="ma-win">window_n</label>
                  <input id="ma-win" type="number" value={nd.window_n}
                    oninput={(e) => updateNodeField('window_n', parseInt((e.target as HTMLInputElement).value))} />
                </div>
                <div class="sp-field">
                  <label for="ma-warm">warmup_samples</label>
                  <input id="ma-warm" type="number" value={nd.warmup_samples}
                    oninput={(e) => updateNodeField('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
                </div>

              {:else if nd.type === 'mahalanobis'}
                <div class="sp-field">
                  <label for="mah-target">target <span class="sp-hint">valores separados por coma</span></label>
                  <input id="mah-target" class="mono"
                    value={(nd.target ?? []).join(', ')}
                    oninput={(e) => updateNodeField('target',
                      (e.target as HTMLInputElement).value.split(',').map((v: string) => parseFloat(v.trim())).filter((v: number) => !isNaN(v))
                    )} />
                </div>
                <div class="sp-field-row">
                  <div class="sp-field">
                    <label for="mah-act">threshold_act</label>
                    <input id="mah-act" class="mono" type="number" step="0.1" value={nd.threshold_act}
                      oninput={(e) => updateNodeField('threshold_act', parseFloat((e.target as HTMLInputElement).value))} />
                  </div>
                  <div class="sp-field">
                    <label for="mah-deact">threshold_deact</label>
                    <input id="mah-deact" class="mono" type="number" step="0.1" value={nd.threshold_deact}
                      oninput={(e) => updateNodeField('threshold_deact', parseFloat((e.target as HTMLInputElement).value))} />
                  </div>
                </div>
                <div class="sp-field sp-checkbox">
                  <input type="checkbox" id="use-P" checked={nd.use_kalman_P}
                    onchange={(e) => updateNodeField('use_kalman_P', (e.target as HTMLInputElement).checked)} />
                  <label for="use-P">usar P del Kalman como Σ</label>
                </div>

              {:else if nd.type === 'hysteresis'}
                <div class="sp-field">
                  <label for="hyst-red">reducción</label>
                  <select id="hyst-red" value={nd.reduction?.type ?? 'mean'}
                    onchange={(e) => updateNodeField('reduction', { type: (e.target as HTMLSelectElement).value })}>
                    <option value="mean">media</option>
                    <option value="min">mínimo</option>
                    <option value="max">máximo</option>
                    <option value="weighted_by_p">ponderado por P</option>
                  </select>
                </div>
                <div class="sp-field-row">
                  <div class="sp-field">
                    <label for="hyst-low">low</label>
                    <input id="hyst-low" class="mono" type="number" step="0.001" value={nd.low}
                      oninput={(e) => updateNodeField('low', parseFloat((e.target as HTMLInputElement).value))} />
                  </div>
                  <div class="sp-field">
                    <label for="hyst-high">high</label>
                    <input id="hyst-high" class="mono" type="number" step="0.001" value={nd.high}
                      oninput={(e) => updateNodeField('high', parseFloat((e.target as HTMLInputElement).value))} />
                  </div>
                </div>

              {:else if nd.type === 'mqtt_actuator'}
                <div class="sp-field">
                  <label for="mqtt-conn">conexión</label>
                  <select id="mqtt-conn" value={nd.connection ?? 'shared'}
                    onchange={(e) => updateNodeField('connection', (e.target as HTMLSelectElement).value)}>
                    <option value="shared">shared (proceso)</option>
                  </select>
                </div>
                <div class="sp-field">
                  <label for="mqtt-topic">topic</label>
                  <input id="mqtt-topic" class="mono" value={nd.topic ?? ''}
                    oninput={(e) => updateNodeField('topic', (e.target as HTMLInputElement).value)}
                    placeholder="relay/control" />
                </div>
                <div class="sp-field-row">
                  <div class="sp-field">
                    <label for="mqtt-on">payload ON</label>
                    <input id="mqtt-on" class="mono" value={nd.payload_on ?? ''}
                      oninput={(e) => updateNodeField('payload_on', (e.target as HTMLInputElement).value)} />
                  </div>
                  <div class="sp-field">
                    <label for="mqtt-off">payload OFF</label>
                    <input id="mqtt-off" class="mono" value={nd.payload_off ?? ''}
                      oninput={(e) => updateNodeField('payload_off', (e.target as HTMLInputElement).value)} />
                  </div>
                </div>

              {:else if nd.type === 'http_actuator'}
                <div class="sp-field-row">
                  <div class="sp-field">
                    <label for="http-on">path ON</label>
                    <input id="http-on" class="mono" value={nd.path_on ?? ''}
                      oninput={(e) => updateNodeField('path_on', (e.target as HTMLInputElement).value)} />
                  </div>
                  <div class="sp-field">
                    <label for="http-off">path OFF</label>
                    <input id="http-off" class="mono" value={nd.path_off ?? ''}
                      oninput={(e) => updateNodeField('path_off', (e.target as HTMLInputElement).value)} />
                  </div>
                </div>

              {:else}
                <div class="sp-note">Tipo <strong>{nd.type}</strong></div>
                <div class="sp-field">
                  <label for="json-edit">JSON</label>
                  <textarea id="json-edit" class="json-edit" rows="8"
                    value={JSON.stringify(nd, null, 2)}
                    onchange={(e) => {
                      try {
                        const parsed = JSON.parse((e.target as HTMLTextAreaElement).value);
                        const pl2 = draft!.pipelines[panelNode!.plIdx];
                        const idx2 = pl2.nodes.findIndex(n => n.id === panelNode!.nodeId);
                        if (idx2 >= 0) { pl2.nodes[idx2] = parsed; draft = { ...draft! }; schedulLocalSave(); }
                      } catch {}
                    }}></textarea>
                </div>
              {/if}

            </div>
          </div>
        {/if}
      {/if}
    </div>

    <!-- Shared connections -->
    {#if $canAdmin && draft}
      <div class="shared-conn">
        <div class="shared-conn-head">conexiones compartidas del proceso</div>
        <div class="shared-fields">
          <div class="sp-field">
            <label for="mqtt-broker">MQTT broker URL</label>
            <input id="mqtt-broker" class="mono"
              value={draft.shared_connections?.mqtt?.broker_url ?? ''}
              oninput={(e) => {
                if (!draft!.shared_connections) draft!.shared_connections = {};
                if (!draft!.shared_connections.mqtt) draft!.shared_connections.mqtt = { broker_url: '', client_id: 'agrodash-{process_id}' };
                draft!.shared_connections.mqtt.broker_url = (e.target as HTMLInputElement).value;
                draft = { ...draft! }; schedulLocalSave();
              }}
              placeholder="mqtt://172.21.224.19:1883" />
          </div>
          <div class="sp-field">
            <label for="mqtt-clientid">client_id</label>
            <input id="mqtt-clientid" class="mono"
              value={draft.shared_connections?.mqtt?.client_id ?? 'agrodash-{process_id}'}
              oninput={(e) => {
                if (!draft!.shared_connections?.mqtt) return;
                draft!.shared_connections.mqtt.client_id = (e.target as HTMLInputElement).value;
                draft = { ...draft! }; schedulLocalSave();
              }} />
          </div>
        </div>
      </div>
    {/if}
  {/if}

</div>

<style>
  .config-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  /* Restore banner */
  .restore-banner { display: flex; align-items: center; gap: 10px; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); background: #FEF3C7; border: 0.5px solid #D97706; border-radius: 8px; font-size: calc(13px * var(--font-scale)); flex-wrap: wrap; }
  .rb-icon { font-size: 16px; flex-shrink: 0; }
  .rb-btn { padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border-radius: 6px; cursor: pointer; font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; border: 0.5px solid; }
  .rb-btn--yes { background: #92400E; color: white; border-color: #92400E; }
  .rb-btn--no  { background: none; color: #92400E; border-color: #D97706; }

  /* Toolbar */
  .top-toolbar { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .add-node-row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .add-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .add-btn { padding: calc(3px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .add-btn:hover { background: var(--interactive-hover); }
  .save-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .local-unsaved { font-size: calc(11px * var(--font-scale)); color: #D97706; font-family: 'DM Mono', monospace; }
  .save-msg { font-size: calc(12px * var(--font-scale)); }
  .save-msg.ok { color: #3B6D11; } .save-msg.err { color: var(--error-color); }
  .btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; transition: background .15s; }
  .btn-save--dirty { background: #92400E; }
  .btn-save:disabled { opacity: 0.5; }

  /* Pipeline tabs */
  .pl-tabs { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; border-bottom: 0.5px solid var(--border-subtle); }
  .pl-tab-wrap { display: flex; align-items: center; }
  .pl-tab { padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: none; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); border-bottom: 2px solid transparent; margin-bottom: -0.5px; font-family: 'DM Mono', monospace; }
  .pl-tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }
  .pl-tab-actions { display: flex; gap: 2px; padding: 0 4px; }
  .tact { width: 18px; height: 18px; border: none; background: none; cursor: pointer; font-size: 10px; color: var(--text-muted); border-radius: 3px; padding: 0; display: flex; align-items: center; justify-content: center; }
  .tact:hover { background: var(--interactive-hover); }
  .tact--del:hover { background: var(--error-bg); color: var(--error-color); }
  .tact:disabled { opacity: 0.3; cursor: default; }
  .pl-tab-add { padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-bottom: 4px; }
  .pl-tab-add:hover { color: var(--text-primary); }

  .pl-name-row { display: flex; align-items: center; gap: 8px; padding: calc(4px * var(--font-scale)) 0; }
  .pl-name-input { border: none; border-bottom: 0.5px solid var(--border-default); background: none; font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); outline: none; width: 200px; font-family: 'DM Mono', monospace; padding: 2px 4px; }
  .pl-sep { color: var(--border-default); }
  .pl-interval-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .pl-interval-input { width: 60px; border: none; border-bottom: 0.5px solid var(--border-default); background: none; font-size: calc(13px * var(--font-scale)); color: var(--text-primary); outline: none; padding: 2px 4px; }
  .pl-interval-unit { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }

  .delete-hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .delete-hint kbd { background: var(--bg-elevated); border: 0.5px solid var(--border-default); border-radius: 3px; padding: 1px 4px; font-family: 'DM Mono', monospace; font-size: calc(10px * var(--font-scale)); }

  .empty-canvas { color: var(--text-muted); font-size: calc(13px * var(--font-scale)); text-align: center; padding: 60px 0; line-height: 1.8; }

  .canvas-wrap { display: flex; border: 0.5px solid var(--border-subtle); border-radius: 10px; overflow: hidden; height: 480px; }
  .canvas-wrap.panel-open .flow-canvas { flex: 0 0 60%; }
  .flow-canvas { flex: 1; }

  /* Side panel */
  .side-panel { width: 40%; border-left: 0.5px solid var(--border-subtle); display: flex; flex-direction: column; overflow-y: auto; background: var(--bg-surface); }
  .sp-head { display: flex; align-items: center; justify-content: space-between; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); position: sticky; top: 0; background: var(--bg-surface); z-index: 1; }
  .sp-type { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; display: block; }
  .sp-id { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .sp-actions { display: flex; gap: 6px; }
  .sp-del { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); } .sp-del:hover { color: var(--error-color); }
  .sp-close { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 14px; }
  .sp-body { padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }
  .sp-section { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .sp-section-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-weight: 500; }
  .sp-hint { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-weight: 400; }
  .sp-note { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .sp-field { display: flex; flex-direction: column; gap: 3px; }
  .sp-field label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .sp-field input, .sp-field select, .sp-field textarea { padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 5px; background: var(--bg-elevated); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); outline: none; }
  .sp-field input:focus, .sp-field select:focus { border-color: var(--text-primary); }
  .sp-field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .sp-checkbox { flex-direction: row; align-items: center; gap: 8px; }
  .sp-checkbox label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .mono { font-family: 'DM Mono', monospace !important; }
  .json-edit { font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale)); resize: vertical; }
  .sensor-row { display: flex; gap: 6px; align-items: center; }
  .sensor-sel { flex: 1; font-size: calc(11px * var(--font-scale)) !important; }
  .sensor-label-in { width: 80px; font-size: calc(11px * var(--font-scale)) !important; }
  .btn-rm { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 12px; }
  .btn-add-sensor { align-self: flex-start; padding: calc(4px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 4px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); margin-top: 2px; }
  .shared-conn { background: var(--bg-elevated); border: 0.5px solid var(--border-subtle); border-radius: 8px; padding: calc(12px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }
  .shared-conn-head { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-muted); font-family: 'DM Mono', monospace; letter-spacing: .05em; }
  .shared-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }

  @media (max-width: 768px) {
    .canvas-wrap { flex-direction: column; height: auto; }
    .canvas-wrap .flow-canvas { height: 320px; flex: none; width: 100%; }
    .canvas-wrap.panel-open .flow-canvas { flex: none; }
    .side-panel { width: 100%; border-left: none; border-top: 0.5px solid var(--border-subtle); }
    .shared-fields { grid-template-columns: 1fr; }
  }
</style>