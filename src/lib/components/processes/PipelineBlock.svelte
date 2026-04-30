<!-- src/lib/components/processes/PipelineBlock.svelte -->
<script lang="ts">
  let {
    node,
    color,
    category,
    isExpanded,
    availableSensors,
    canEdit,
    onexpand,
    onremove,
    onchange,
    onstartdrag,
  }: {
    node: any;
    color: string;
    category: string;
    isExpanded: boolean;
    availableSensors: { id: string; label: string }[];
    canEdit: boolean;
    onexpand: () => void;
    onremove: () => void;
    onchange: () => void;
    onstartdrag?: (e: MouseEvent) => void;
  } = $props();

  function set(field: string, value: any) {
    node[field] = value;
    onchange();
  }

  function fmtType(t: string) {
    return t.replace(/_/g, ' ');
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="block-inner" style="--nc:{color}">

  <!-- Header — drag handle + expand/remove -->
  <div class="block-header"
    ondblclick={onexpand}
    onmousedown={(e) => {
      // Only drag from header, not from buttons
      if ((e.target as HTMLElement).closest('button')) return;
      onstartdrag?.(e);
    }}
    style="cursor:grab"
  >
    <span class="drag-handle">⠿</span>
    <span class="cat">{category}</span>
    <span class="type-label">{fmtType(node.type)}</span>
    <div class="header-actions">
      <button class="btn-expand" onclick={onexpand} title={isExpanded ? 'colapsar' : 'expandir'}>
        {isExpanded ? '▲' : '▼'}
      </button>
      {#if canEdit}
        <button class="btn-remove" onclick={onremove} title="eliminar">✕</button>
      {/if}
    </div>
  </div>

  <!-- Expanded body -->
  {#if isExpanded}
    <div class="block-body">

      {#if node.type === 'postgres_sensor'}
        <div class="field-group">
          <div class="field-label">sensores</div>
          {#each (node.sensors ?? []) as s, i (i)}
            <div class="sensor-row">
              <select
                class="inp inp--select"
                value={s.id}
                onchange={(e) => {
                  const val = (e.target as HTMLSelectElement).value;
                  const found = availableSensors.find(x => x.id === val);
                  const sensors = [...(node.sensors ?? [])];
                  sensors[i] = { id: val, label: found?.label.split('·')[2]?.trim() ?? '' };
                  set('sensors', sensors);
                }}>
                <option value="">— elegir —</option>
                {#each availableSensors as opt (opt.id)}
                  <option value={opt.id}>{opt.label}</option>
                {/each}
              </select>
              <input class="inp inp--label" value={s.label}
                oninput={(e) => {
                  const sensors = [...(node.sensors ?? [])];
                  sensors[i] = { ...sensors[i], label: (e.target as HTMLInputElement).value };
                  set('sensors', sensors);
                }} placeholder="etiqueta" />
              {#if canEdit}
                <button class="btn-sm" onclick={() => set('sensors', (node.sensors ?? []).filter((_: any, j: number) => j !== i))}>✕</button>
              {/if}
            </div>
          {/each}
          {#if canEdit}
            <button class="btn-add" onclick={() => set('sensors', [...(node.sensors ?? []), { id: '', label: '' }])}>+ sensor</button>
          {/if}
        </div>

      {:else if node.type === 'kalman'}
        <div class="field-row">
          <div class="field">
            <label>Q</label>
            <input class="inp mono" value={node.Q ?? 1e-5}
              oninput={(e) => set('Q', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>R</label>
            <input class="inp mono" value={node.R ?? 1e-3}
              oninput={(e) => set('R', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>P₀</label>
            <input class="inp mono" value={node.P0 ?? 1.0}
              oninput={(e) => set('P0', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field">
          <label>convergence threshold</label>
          <input class="inp mono" value={node.convergence_threshold ?? 5e-4}
            oninput={(e) => set('convergence_threshold', parseFloat((e.target as HTMLInputElement).value))} />
        </div>

      {:else if node.type === 'moving_avg'}
        <div class="field-row">
          <div class="field">
            <label>ventana</label>
            <input class="inp mono" type="number" value={node.window_n ?? 10}
              oninput={(e) => set('window_n', parseInt((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      {:else if node.type === 'ewma'}
        <div class="field-row">
          <div class="field">
            <label>alpha</label>
            <input class="inp mono" value={node.alpha ?? 0.1}
              oninput={(e) => set('alpha', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      {:else if node.type === 'mahalanobis'}
        <div class="field">
          <label>target <span class="hint">separado por comas</span></label>
          <input class="inp mono"
            value={(node.target ?? []).join(', ')}
            oninput={(e) => set('target',
              (e.target as HTMLInputElement).value.split(',')
                .map((v: string) => parseFloat(v.trim()))
                .filter((v: number) => !isNaN(v))
            )} />
        </div>
        <div class="field-row">
          <div class="field">
            <label>threshold act</label>
            <input class="inp mono" type="number" step="0.1" value={node.threshold_act ?? 2.5}
              oninput={(e) => set('threshold_act', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>threshold deact</label>
            <input class="inp mono" type="number" step="0.1" value={node.threshold_deact ?? 1.0}
              oninput={(e) => set('threshold_deact', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field checkbox-field">
          <input type="checkbox" id="use-P-{node.id}" checked={node.use_kalman_P ?? true}
            onchange={(e) => set('use_kalman_P', (e.target as HTMLInputElement).checked)} />
          <label for="use-P-{node.id}">usar P del Kalman como Σ</label>
        </div>

      {:else if node.type === 'hysteresis'}
        <div class="field-row">
          <div class="field">
            <label>reducción</label>
            <select class="inp" value={node.reduction?.type ?? 'mean'}
              onchange={(e) => set('reduction', { type: (e.target as HTMLSelectElement).value })}>
              <option value="mean">media</option>
              <option value="min">mínimo</option>
              <option value="max">máximo</option>
              <option value="weighted_by_p">pond. P</option>
            </select>
          </div>
          <div class="field">
            <label>low</label>
            <input class="inp mono" type="number" step="0.001" value={node.low ?? 0.08}
              oninput={(e) => set('low', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>high</label>
            <input class="inp mono" type="number" step="0.001" value={node.high ?? 0.085}
              oninput={(e) => set('high', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      {:else if node.type === 'mqtt_actuator'}
        <div class="field">
          <label>topic</label>
          <input class="inp mono" value={node.topic ?? ''}
            oninput={(e) => set('topic', (e.target as HTMLInputElement).value)}
            placeholder="relay/control" />
        </div>
        <div class="field-row">
          <div class="field">
            <label>ON</label>
            <input class="inp mono" value={node.payload_on ?? 'on'}
              oninput={(e) => set('payload_on', (e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>OFF</label>
            <input class="inp mono" value={node.payload_off ?? 'off'}
              oninput={(e) => set('payload_off', (e.target as HTMLInputElement).value)} />
          </div>
        </div>
        <!-- Live switch -->
        <div class="actuator-switch">
          <span class="switch-label">override</span>
          <button class="sw-btn sw-on">ON</button>
          <button class="sw-btn sw-off">OFF</button>
          <button class="sw-btn sw-auto">↺ auto</button>
        </div>

      {:else if node.type === 'http_actuator'}
        <div class="field-row">
          <div class="field">
            <label>path ON</label>
            <input class="inp mono" value={node.path_on ?? '/on'}
              oninput={(e) => set('path_on', (e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>path OFF</label>
            <input class="inp mono" value={node.path_off ?? '/off'}
              oninput={(e) => set('path_off', (e.target as HTMLInputElement).value)} />
          </div>
        </div>

      {:else if node.type === 'logger'}
        <div class="field">
          <label>tag</label>
          <input class="inp mono" value={node.tag ?? ''}
            oninput={(e) => set('tag', (e.target as HTMLInputElement).value)}
            placeholder="debug" />
        </div>

      {:else}
        <div class="field">
          <span class="hint">tipo: {node.type}</span>
        </div>
      {/if}

    </div>
  {/if}

</div>

<style>
  .block-inner {
    background: var(--bg-surface);
    border: 2px solid var(--nc);
    border-radius: 10px;
    min-width: 180px;
    max-width: 260px;
    overflow: visible;
  }

  .block-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    cursor: pointer;
    border-radius: 8px 8px 0 0;
    background: color-mix(in srgb, var(--nc) 8%, var(--bg-surface));
  }
  .block-inner:not(.expanded) .block-header { border-radius: 8px; }

  .cat {
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: .08em;
    color: var(--nc);
    font-family: 'DM Mono', monospace;
    flex-shrink: 0;
  }
  .type-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .header-actions {
    display: flex;
    gap: 3px;
    flex-shrink: 0;
  }
  .btn-expand, .btn-remove {
    width: 18px;
    height: 18px;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 10px;
    color: var(--text-muted);
    border-radius: 3px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .drag-handle { font-size: 12px; color: var(--text-muted); opacity: 0.4; flex-shrink: 0; }
  .block-header:hover .drag-handle { opacity: 0.8; }
  .btn-expand:hover { background: var(--interactive-hover); }
  .btn-remove:hover { background: var(--error-bg); color: var(--error-color); }

  .block-body {
    padding: 8px 10px;
    cursor: default;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: 1px solid color-mix(in srgb, var(--nc) 20%, transparent);
  }

  .field { display: flex; flex-direction: column; gap: 2px; }
  .field-row { display: flex; gap: 6px; }
  .field-row .field { flex: 1; min-width: 0; }
  .field-group { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: 10px; color: var(--text-muted); font-weight: 500; }
  label { font-size: 10px; color: var(--text-muted); }
  .hint { font-size: 9px; color: var(--text-muted); font-weight: 400; }

  .inp {
    padding: 3px 6px;
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 11px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .inp:focus { border-color: var(--nc); }
  .inp--select { font-size: 10px; }
  .inp--label { width: 72px; flex-shrink: 0; }
  .mono { font-family: 'DM Mono', monospace; }

  .sensor-row { display: flex; gap: 4px; align-items: center; }
  .btn-sm { border: none; background: none; cursor: pointer; color: var(--text-muted); font-size: 11px; padding: 0 2px; flex-shrink: 0; }
  .btn-add { align-self: flex-start; font-size: 10px; color: var(--text-secondary); background: none; border: 0.5px dashed var(--border-default); border-radius: 4px; padding: 2px 6px; cursor: pointer; }

  .checkbox-field { flex-direction: row; align-items: center; gap: 6px; }
  .checkbox-field label { font-size: 11px; color: var(--text-secondary); }

  /* Actuator switch */
  .actuator-switch {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-top: 4px;
    border-top: 0.5px solid var(--border-subtle);
    margin-top: 2px;
  }
  .switch-label { font-size: 9px; color: var(--text-muted); font-family: 'DM Mono', monospace; flex: 1; }
  .sw-btn {
    padding: 2px 7px;
    border-radius: 4px;
    border: 0.5px solid var(--border-default);
    background: none;
    cursor: pointer;
    font-size: 10px;
    font-family: 'DM Mono', monospace;
  }
  .sw-on  { color: #3da85a; border-color: #3da85a44; }
  .sw-on:hover  { background: #EAF3DE; }
  .sw-off { color: #e05454; border-color: #e0545444; }
  .sw-off:hover { background: #FCEBEB; }
  .sw-auto:hover { background: var(--interactive-hover); }
</style>