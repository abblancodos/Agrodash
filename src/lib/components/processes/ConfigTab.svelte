<!-- src/lib/components/processes/ConfigTab.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { processStore, type ProcessConfig } from '$lib/stores/process';
  import NodeCanvas from './NodeCanvas.svelte';
  import BlockPicker from './BlockPicker.svelte';
  import ProcessCollaborators from './ProcessCollaborators.svelte';

  let { processId }: { processId: string } = $props();

  const API = (import.meta as any).env?.VITE_API_BASE ?? '';

  // ── State — plain variables, no reactivity ────────────────────────────────
  let draft     = $state<ProcessConfig | null>(null);
  let activePl  = $state(0);
  let saving    = $state(false);
  let saveMsg   = $state('');
  let saveMsgOk = $state(true);
  let dirty     = $state(false);
  let showRestoreBanner = $state(false);
  let localDraft: ProcessConfig | null = null;
  let availableSensors = $state<{id:string; label:string}[]>([]);

  const STORAGE_KEY = () => `agrodash_cfg_${processId}`;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Node metadata for defaults ────────────────────────────────────────────
  function defaultParams(type: string): Record<string, any> {
    const d: Record<string, any> = {
      postgres_sensor: { sensors: [] },
      kalman:   { Q: 1e-5, R: 1e-3, P0: 1.0, convergence_threshold: 5e-4, warmup_samples: 10 },
      moving_avg: { window_n: 10, warmup_samples: 10 },
      ewma:     { alpha: 0.1, warmup_samples: 10 },
      lowpass:  { tau_seconds: 120, warmup_samples: 10 },
      passthrough: {},
      concat:   {},
      weighted_mean: { weights: [] },
      mahalanobis: { target: [], threshold_act: 2.5, threshold_deact: 1.0, use_kalman_P: true },
      hysteresis: { reduction: { type: 'mean' }, low: 0.08, high: 0.085, action_below_low: 'on', action_above_high: 'off' },
      sprt: { mu_H0: 0.0, mu_H1: 1.0, sigma: 0.1, alpha: 0.05, beta: 0.05, reduction: { type: 'mean' }, reset_on_action: true },
      mqtt_actuator: { connection: 'shared', topic: '', payload_on: 'on', payload_off: 'off', retain: false },
      http_actuator: { connection: 'shared', path_on: '/on', path_off: '/off' },
      logger:   { tag: '' },
      select:   { indices: [] },
      linear_scale: { a: 1.0, b: 0.0 },
    };
    return d[type] ?? {};
  }

  // ── Init — once, in onMount ───────────────────────────────────────────────
  onMount(async () => {
    window.addEventListener('beforeunload', handleBeforeUnload);

    // Load available sensors
    fetch(`${API}/api/v1/boxes`, { credentials: 'include' })
      .then(r => r.json())
      .then((boxes: any[]) => {
        availableSensors = boxes.flatMap(b =>
          (b.sensors ?? []).map((s: any) => ({
            id: s.id,
            label: `${b.name} · #${s.sensor_number} · ${s.type}`,
          }))
        );
      }).catch(() => {});

    // Load draft from store (proc already loaded by parent)
    const proc = processStore.process;
    if (proc?.config) {
      const serverCfg: ProcessConfig = JSON.parse(JSON.stringify(proc.config));
      try {
        const stored = localStorage.getItem(STORAGE_KEY());
        if (stored) {
          const parsed = JSON.parse(stored);
          localDraft = parsed.config;
          const localTs  = new Date(parsed.ts);
          const serverTs = new Date((proc as any).updated_at ?? 0);
          if (localTs > serverTs) {
            showRestoreBanner = true;
            draft = serverCfg;
          } else {
            localStorage.removeItem(STORAGE_KEY());
            draft = serverCfg;
          }
        } else {
          draft = serverCfg;
        }
      } catch { draft = serverCfg; }
    } else {
      draft = { version: 1, pipelines: [] };
    }
  });

  onDestroy(() => {
    window.removeEventListener('beforeunload', handleBeforeUnload);
    if (saveTimer) clearTimeout(saveTimer);
  });

  function handleBeforeUnload(e: BeforeUnloadEvent) {
    if (dirty) { e.preventDefault(); return ''; }
  }

  // ── Local autosave ────────────────────────────────────────────────────────
  function markDirty() {
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      if (!draft) return;
      try { localStorage.setItem(STORAGE_KEY(), JSON.stringify({ config: draft, ts: new Date().toISOString() })); }
      catch {}
    }, 2000);
  }

  function restoreLocal() {
    if (localDraft) { draft = JSON.parse(JSON.stringify(localDraft)); dirty = true; }
    showRestoreBanner = false;
  }
  function discardLocal() {
    localStorage.removeItem(STORAGE_KEY());
    showRestoreBanner = false;
  }

  // ── Pipeline management ───────────────────────────────────────────────────
  function addPipeline() {
    if (!draft) return;
    const id = `pipeline_${Date.now()}`;
    draft.pipelines = [...draft.pipelines, {
      id, label: `Pipeline ${draft.pipelines.length + 1}`,
      loop_interval_seconds: 60,
      nodes: [], edges: [], node_positions: {},
      connections: undefined,
    }];
    activePl = draft.pipelines.length - 1;
    markDirty();
  }

  function removePipeline(idx: number) {
    if (!draft) return;
    const pl = draft.pipelines[idx];
    const msg = draft.pipelines.length <= 1
      ? `¿Eliminar el único pipeline "${pl.label}"? El proceso quedará sin pipelines.`
      : `¿Eliminar "${pl.label}"?`;
    if (!confirm(msg)) return;
    draft.pipelines = draft.pipelines.filter((_, i) => i !== idx);
    activePl = Math.max(0, Math.min(activePl, draft.pipelines.length - 1));
    markDirty();
  }

  function movePipeline(idx: number, dir: -1 | 1) {
    if (!draft) return;
    const ni = idx + dir;
    if (ni < 0 || ni >= draft.pipelines.length) return;
    const pls = [...draft.pipelines];
    [pls[idx], pls[ni]] = [pls[ni], pls[idx]];
    draft.pipelines = pls;
    activePl = ni;
    markDirty();
  }

  // ── Add node ──────────────────────────────────────────────────────────────
  function addNode(type: string) {
    if (!draft) return;
    const pl = draft.pipelines[activePl];
    const id = `${type}_${Date.now()}`;
    const n  = pl.nodes?.length ?? 0;
    pl.nodes = [...(pl.nodes ?? []), { id, type, ...defaultParams(type) }];
    if (!pl.node_positions) pl.node_positions = {};
    pl.node_positions[id] = { x: 60 + n * 260, y: 100 };
    draft = { ...draft }; // trigger Svelte to re-render canvas
    markDirty();
  }

  // ── Pipeline validation errors from agent logs ───────────────────────────
  let pipelineErrors = $state<Record<string, string>>({});

  async function checkAgentErrors() {
    // Leer logs recientes del proceso y mostrar errores de validación del agente
    // El agente escribe "Pipeline '...' tiene actuadores pero..." en level=error
    try {
      const r = await fetch(`${API}/api/v1/processes/${processId}/logs?limit=20&level=error`, {
        credentials: 'include',
      });
      if (!r.ok) return;
      const data = await r.json();
      const errs: Record<string, string> = {};
      for (const log of (data.logs ?? [])) {
        // Buscar errores de validación del agente que mencionan pipeline_id
        const msg: string = log.message ?? '';
        if (msg.includes('Pipeline') && (msg.includes('actuadores') || msg.includes('vacío') || msg.includes('decisor'))) {
          // Extraer pipeline_id del source o del mensaje
          const source: string = log.source ?? '';
          const pl = draft?.pipelines.find(p => source.includes(p.id) || msg.includes(p.id));
          if (pl) errs[pl.id] = msg;
        }
      }
      pipelineErrors = errs;
    } catch {}
  }

  // ── Save to server ────────────────────────────────────────────────────────
  async function save() {
    if (!draft) return;
    saving = true; saveMsg = '';
    try {
      await processStore.saveConfig(processId, draft);
      dirty = false;
      if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
      localStorage.removeItem(STORAGE_KEY());
      saveMsg = '✓ guardado'; saveMsgOk = true;
      // Si el proceso está running, verificar si el agente rechazó algún pipeline
      const proc = processStore.process;
      if (proc?.status === 'running') {
        setTimeout(checkAgentErrors, 4000); // esperar que el agente intente arrancar
      }
    } catch (e: any) {
      saveMsg = e.message; saveMsgOk = false;
    } finally { saving = false; }
  }
</script>

<div class="config-tab">

  <!-- Restore banner -->
  {#if showRestoreBanner}
    <div class="restore-banner">
      <span>💾 Hay cambios locales sin guardar al servidor.</span>
      <button class="rb-btn rb-yes" onclick={restoreLocal}>Restaurar</button>
      <button class="rb-btn rb-no"  onclick={discardLocal}>Descartar</button>
    </div>
  {/if}

  <!-- Top bar: pipeline tabs + save -->
  <div class="top-bar">
    <div class="pl-tabs">
      {#each (draft?.pipelines ?? []) as pl, i (pl.id)}
        <div class="pl-tab-wrap">
          <button class="pl-tab" class:active={activePl === i}
            onclick={() => { activePl = i; }}>
            {pl.label}
          </button>
          {#if activePl === i && processStore.canAdmin}
            <button class="tact" onclick={() => movePipeline(i, -1)} disabled={i === 0}>↑</button>
            <button class="tact" onclick={() => movePipeline(i, 1)} disabled={i === (draft?.pipelines.length ?? 1) - 1}>↓</button>
            <button class="tact tact--del" onclick={() => removePipeline(i)}>✕</button>
          {/if}
        </div>
      {/each}
      {#if processStore.canAdmin}
        <button class="pl-tab-add" onclick={addPipeline}>+ pipeline</button>
      {/if}
    </div>

    <div class="save-bar">
      {#if dirty}<span class="unsaved">● sin guardar</span>{/if}
      {#if saveMsg}<span class="save-msg" class:ok={saveMsgOk} class:err={!saveMsgOk}>{saveMsg}</span>{/if}
      {#if processStore.canAdmin}
        <button class="btn-save" class:btn-dirty={dirty} disabled={saving} onclick={save}>
          {saving ? 'guardando...' : dirty ? '⬆ guardar' : 'guardar'}
        </button>
      {/if}
    </div>
  </div>

  <!-- Pipeline interval -->
  {#if draft && draft.pipelines.length > 0}
    {@const pl = draft.pipelines[activePl]}
    <div class="pl-meta">
      <input class="pl-name" bind:value={pl.label} oninput={markDirty} />
      <span class="pl-sep">·</span>
      <label for="pl-interval" class="pl-label">intervalo</label>
      <input id="pl-interval" class="pl-interval mono" type="number" min="5"
        bind:value={pl.loop_interval_seconds} oninput={markDirty} />
      <span class="pl-label">s</span>
    </div>
  {/if}

  <!-- Pipeline validation errors -->
  {#if draft && draft.pipelines.length > 0}
    {@const pl = draft.pipelines[activePl]}
    {#if pipelineErrors[pl.id]}
      <div class="pl-error">
        <span class="pl-error-icon">⚠</span>
        <span>{pipelineErrors[pl.id]}</span>
        <button class="pl-error-close" onclick={() => {
          const e = { ...pipelineErrors };
          delete e[pl.id];
          pipelineErrors = e;
        }}>✕</button>
      </div>
    {/if}
  {/if}

  <!-- Main area: picker + canvas -->
  <div class="main-area">
    {#if processStore.canAdmin}
      <div class="picker-col">
        <BlockPicker onAdd={addNode} />
      </div>
    {/if}

    <div class="canvas-col">
      {#if draft && draft.pipelines.length > 0}
        {#key activePl}
          <NodeCanvas
            pipeline={draft.pipelines[activePl]}
            {availableSensors}
            canEdit={processStore.canAdmin}
            onchange={markDirty}
          />
        {/key}
      {:else}
        <div class="empty">
          <p>No hay pipelines. Hacé click en <strong>+ pipeline</strong> para crear uno.</p>
        </div>
      {/if}
    </div>
  </div>

  <!-- Collaborators -->
  <ProcessCollaborators {processId} />

  <!-- Shared MQTT connection -->
  {#if draft && processStore.canAdmin}
    <div class="shared-conn">
      <div class="shared-title">conexión compartida — MQTT</div>
      <div class="shared-fields">
        <div class="sf">
          <label for="mqtt-broker">broker URL</label>
          <input id="mqtt-broker" class="inp mono"
            value={draft.shared_connections?.mqtt?.broker_url ?? ''}
            oninput={(e) => {
              if (!draft!.shared_connections) draft!.shared_connections = {};
              if (!draft!.shared_connections.mqtt) draft!.shared_connections.mqtt = { broker_url: '', client_id: 'agrodash' };
              draft!.shared_connections.mqtt.broker_url = (e.target as HTMLInputElement).value;
              markDirty();
            }} placeholder="mqtt://172.21.224.19:1883" />
        </div>
        <div class="sf">
          <label for="mqtt-client">client_id</label>
          <input id="mqtt-client" class="inp mono"
            value={draft.shared_connections?.mqtt?.client_id ?? 'agrodash-{process_id}'}
            oninput={(e) => {
              if (!draft!.shared_connections?.mqtt) return;
              draft!.shared_connections.mqtt.client_id = (e.target as HTMLInputElement).value;
              markDirty();
            }} />
        </div>
      </div>
    </div>
  {/if}

</div>

<style>
  .config-tab { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); height: 100%; }

  .restore-banner { display: flex; align-items: center; gap: 10px; padding: 8px 14px; background: #FEF3C7; border: 0.5px solid #D97706; border-radius: 8px; font-size: calc(13px * var(--font-scale)); flex-wrap: wrap; }
  .rb-btn { padding: 4px 12px; border-radius: 5px; cursor: pointer; font-size: calc(12px * var(--font-scale)); border: 0.5px solid; }
  .rb-yes { background: #92400E; color: white; border-color: #92400E; }
  .rb-no  { background: none; color: #92400E; border-color: #D97706; }

  .top-bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .pl-tabs { display: flex; align-items: center; gap: 3px; flex-wrap: wrap; }
  .pl-tab-wrap { display: flex; align-items: center; }
  .pl-tab { padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale)); border: none; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); border-bottom: 2px solid transparent; font-family: 'DM Mono', monospace; }
  .pl-tab.active { color: var(--text-primary); border-bottom-color: var(--text-primary); }
  .tact { width: 18px; height: 18px; border: none; background: none; cursor: pointer; font-size: 10px; color: var(--text-muted); border-radius: 3px; }
  .tact:hover { background: var(--interactive-hover); }
  .tact--del:hover { background: var(--error-bg); color: var(--error-color); }
  .tact:disabled { opacity: 0.3; }
  .pl-tab-add { padding: 4px 10px; border: 0.5px dashed var(--border-default); border-radius: 5px; background: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .pl-tab-add:hover { color: var(--text-primary); }

  .save-bar { display: flex; align-items: center; gap: 8px; }
  .unsaved { font-size: calc(11px * var(--font-scale)); color: #D97706; font-family: 'DM Mono', monospace; }
  .save-msg { font-size: calc(12px * var(--font-scale)); }
  .save-msg.ok { color: #3B6D11; } .save-msg.err { color: var(--error-color); }
  .btn-save { padding: calc(6px * var(--font-scale)) calc(14px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-dirty { background: #92400E; }
  .btn-save:disabled { opacity: 0.5; }

  .pl-meta { display: flex; align-items: center; gap: 8px; }
  .pl-name { border: none; border-bottom: 0.5px solid var(--border-default); background: none; font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); outline: none; width: 200px; font-family: 'DM Mono', monospace; padding: 2px 4px; }
  .pl-sep { color: var(--border-default); }
  .pl-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .pl-interval { width: 60px; border: none; border-bottom: 0.5px solid var(--border-default); background: none; font-size: calc(13px * var(--font-scale)); color: var(--text-primary); outline: none; padding: 2px 4px; }
  .mono { font-family: 'DM Mono', monospace; }

  .pl-error { display:flex; align-items:flex-start; gap:8px; padding:8px 12px; background:#FEF3C7; border:0.5px solid #D97706; border-radius:6px; font-size:calc(12px * var(--font-scale)); color:#92400E; }
  .pl-error-icon { flex-shrink:0; font-size:14px; }
  .pl-error span { flex:1; line-height:1.4; }
  .pl-error-close { background:none; border:none; cursor:pointer; color:#92400E; font-size:12px; flex-shrink:0; padding:0 2px; }
  .pl-error-close:hover { opacity:0.7; }

  .main-area { display: flex; flex: 1; min-height: 500px; border: 0.5px solid var(--border-subtle); border-radius: 10px; overflow: hidden; }
  .picker-col { width: 180px; flex-shrink: 0; }
  .canvas-col { flex: 1; position: relative; }

  .empty { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); text-align: center; padding: 40px; }

  .shared-conn { background: var(--bg-elevated); border: 0.5px solid var(--border-subtle); border-radius: 8px; padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); display: flex; flex-direction: column; gap: 8px; }
  .shared-title { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-muted); letter-spacing: .05em; }
  .shared-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .sf { display: flex; flex-direction: column; gap: 3px; }
  .sf label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .inp { padding: calc(5px * var(--font-scale)) calc(8px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 5px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); outline: none; }
  .inp:focus { border-color: var(--text-primary); }
</style>