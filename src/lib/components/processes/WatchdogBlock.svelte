<!-- src/lib/components/processes/WatchdogBlock.svelte -->
<!-- svelte-ignore a11y_label_has_associated_control -->
<script lang="ts">
  let {
    node,
    color,
    isExpanded,
    canEdit,
    onexpand,
    onremove,
    onchange,
    onstartdrag,
    onresize,
    onconnectstart,
    onconnectend,
    portEls = {} as Record<string, HTMLElement | undefined>,
    onPortEls,
  }: {
    node: any;
    color: string;
    isExpanded: boolean;
    canEdit: boolean;
    onexpand: () => void;
    onremove: () => void;
    onchange: () => void;
    onstartdrag?: (e: MouseEvent) => void;
    onresize?: (e: MouseEvent) => void;
    onconnectstart?: (e: MouseEvent, nodeId: string, portName: string) => void;
    onconnectend?:   (e: MouseEvent, nodeId: string, portName: string) => void;
    portEls?: Record<string, HTMLElement | undefined>;
    onPortEls?: (els: Record<string, HTMLElement | undefined>) => void;
  } = $props();

  $effect(() => {
    if (Object.keys(portEls).length > 0) onPortEls?.(portEls);
  });

  const INPUTS  = ['act_in', 'mqtt_ret_in', 'sig_in'] as const;
  const OUTPUTS = ['act_out'] as const;

  const PORT_COLOR: Record<string, string> = {
    act_in:      '#e07b54',
    mqtt_ret_in: '#c084fc',
    sig_in:      '#4a90d9',
    act_out:     '#3da85a',
  };

  const MODE_LABEL: Record<string, string> = {
    good: 'Good', bad: 'Bad', ugly: 'Ugly',
  };

  function set(field: string, value: any) { node[field] = value; onchange(); }
</script>

<!--
  Layout: [ports-left] [.wd-block] [ports-right]
  Los puertos están FUERA del borde del bloque, igual que PipelineBlock.
-->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="wd-outer" style="--nc:{color}">

  <!-- ── Left ports ── -->
  <div class="port-col port-col-left">
    {#each INPUTS as pname (pname)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="port-dot"
        style="--pc:{PORT_COLOR[pname]}"
        title={pname}
        bind:this={portEls[pname]}
        onmouseup={(e) => { e.stopPropagation(); onconnectend?.(e, node.id, pname); }}
      ></div>
    {/each}
  </div>

  <!-- ── Main block ── -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="wd-block"
    class:expanded={isExpanded}
    onmousedown={(e) => {
      if ((e.target as HTMLElement).closest('button, .port-dot, input, select')) return;
      onstartdrag?.(e);
    }}
  >
    <!-- Header -->
    <div class="wd-header" ondblclick={onexpand}>
      <span class="drag-handle">⠿</span>
      <span class="wd-cat">watchdog</span>
      <span class="wd-name">{MODE_LABEL[node.mode?.level ?? 'good']}</span>
      <div class="wd-actions">
        <button onclick={onexpand}>{isExpanded ? '▲' : '▼'}</button>
        {#if canEdit}<button onclick={onremove}>✕</button>{/if}
      </div>
    </div>

    <!-- Collapsed: port name pills -->
    {#if !isExpanded}
      <div class="port-pills">
        <div class="pills-left">
          {#each INPUTS as p (p)}<span class="pill pill-in">{p}</span>{/each}
        </div>
        <div class="pills-right">
          {#each OUTPUTS as p (p)}<span class="pill pill-out">{p}</span>{/each}
        </div>
      </div>
    {/if}

    <!-- Expanded: config -->
    {#if isExpanded}
      <div class="wd-body">
        <div class="row">
          <div class="field">
            <label>modo</label>
            <select class="inp" value={node.mode?.level ?? 'good'}
              onchange={(e) => {
                const l = (e.target as HTMLSelectElement).value;
                if (l === 'good') set('mode', { level: 'good' });
                if (l === 'bad')  set('mode', { level: 'bad', expected_on_trend: 'ascending', expected_off_trend: 'descending', min_change_pct: 0.05, component: null });
                if (l === 'ugly') set('mode', { level: 'ugly', notify_message: null });
              }}>
              <option value="good">Good — feedback MQTT</option>
              <option value="bad">Bad — tendencia</option>
              <option value="ugly">Ugly — confirmación manual</option>
            </select>
          </div>
        </div>
        <div class="row">
          <div class="field">
            <label>timeout (s)</label>
            <input class="inp mono" type="number" min="0" value={node.action_timeout_secs ?? 30}
              oninput={(e) => set('action_timeout_secs', parseInt((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>max retries</label>
            <input class="inp mono" type="number" min="0" value={node.max_retries ?? 3}
              oninput={(e) => set('max_retries', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

        {#if node.mode?.level === 'bad'}
          <div class="subpanel">
            <div class="row">
              <div class="field">
                <label>tendencia ON</label>
                <select class="inp" value={node.mode.expected_on_trend ?? 'ascending'}
                  onchange={(e) => set('mode', { ...node.mode, expected_on_trend: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ ascendente</option>
                  <option value="descending">↓ descendente</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
              <div class="field">
                <label>tendencia OFF</label>
                <select class="inp" value={node.mode.expected_off_trend ?? 'descending'}
                  onchange={(e) => set('mode', { ...node.mode, expected_off_trend: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ ascendente</option>
                  <option value="descending">↓ descendente</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
            </div>
            <div class="row">
              <div class="field">
                <label>cambio mín %</label>
                <input class="inp mono" type="number" step="0.01" min="0.01" max="1"
                  value={node.mode.min_change_pct ?? 0.05}
                  oninput={(e) => set('mode', { ...node.mode, min_change_pct: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>componente</label>
                <input class="inp mono" type="number" min="0"
                  value={node.mode.component ?? ''}
                  oninput={(e) => {
                    const v = (e.target as HTMLInputElement).value;
                    set('mode', { ...node.mode, component: v === '' ? null : parseInt(v) });
                  }} placeholder="L2" />
              </div>
            </div>
          </div>
        {/if}

        {#if node.mode?.level === 'ugly'}
          <div class="field">
            <label>mensaje</label>
            <input class="inp" value={node.mode.notify_message ?? ''}
              oninput={(e) => set('mode', { ...node.mode, notify_message: (e.target as HTMLInputElement).value || null })}
              placeholder="Verificar presión manual" />
          </div>
        {/if}

        <div class="hint">
          {#if node.mode?.level === 'bad'}
            Verifica que la señal cambie en la dirección esperada al actuar.
          {:else if node.mode?.level === 'ugly'}
            Pide confirmación manual antes de actuar.
          {:else}
            Conectar mqtt_ret_in desde el MQTT Subscriber correspondiente.
          {/if}
        </div>

        {#if onresize}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="resize-handle" onmousedown={(e) => { e.stopPropagation(); onresize?.(e); }}>⌟</div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- ── Right ports ── -->
  <div class="port-col port-col-right">
    {#each OUTPUTS as pname (pname)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="port-dot"
        style="--pc:{PORT_COLOR[pname]}"
        title={pname}
        bind:this={portEls[pname]}
        onmousedown={(e) => { e.stopPropagation(); onconnectstart?.(e, node.id, pname); }}
      ></div>
    {/each}
  </div>

</div>

<style>
  .wd-outer {
    display: flex;
    flex-direction: row;
    align-items: center;
  }

  /* Port columns — outside the border */
  .port-col {
    display: flex;
    flex-direction: column;
    justify-content: space-around;
    align-items: center;
    padding: 10px 0;
    gap: 10px;
    flex-shrink: 0;
  }

  .port-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--pc);
    background: var(--bg-surface);
    flex-shrink: 0;
    transition: background .1s;
  }
  .port-col-left  .port-dot { cursor: crosshair; }
  .port-col-right .port-dot { cursor: cell; }
  .port-dot:hover { background: var(--pc); }

  /* Main block */
  .wd-block {
    flex: 1;
    display: flex;
    flex-direction: column;
    border: 2px solid var(--nc);
    border-radius: 10px;
    background: var(--bg-surface);
    min-width: 180px;
    overflow: hidden;
    cursor: grab;
    user-select: none;
    position: relative;
  }
  .wd-block:active { cursor: grabbing; }
  .wd-block.expanded { cursor: default; }

  /* Header — matches PipelineBlock exactly */
  .wd-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--nc) 8%, var(--bg-surface));
    border-radius: 8px 8px 0 0;
    cursor: grab;
    flex-shrink: 0;
  }
  .wd-block:not(.expanded) .wd-header { border-radius: 8px; }
  .drag-handle { font-size: 12px; color: var(--text-muted); opacity: 0.4; flex-shrink: 0; }
  .wd-header:hover .drag-handle { opacity: 0.8; }
  .wd-cat {
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: .08em;
    color: var(--nc);
    font-family: 'DM Mono', monospace;
    flex-shrink: 0;
  }
  .wd-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
  }
  .wd-actions { display: flex; gap: 3px; flex-shrink: 0; }
  .wd-actions button {
    width: 18px; height: 18px;
    border: none; background: none;
    cursor: pointer; font-size: 10px;
    color: var(--text-muted);
    border-radius: 3px; padding: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .wd-actions button:first-child:hover { background: var(--interactive-hover); }
  .wd-actions button:last-child:hover  { background: var(--error-bg); color: var(--error-color); }

  /* Port name pills — collapsed only */
  .port-pills {
    display: flex;
    justify-content: space-between;
    padding: 4px 8px 6px;
    border-top: 0.5px solid color-mix(in srgb, var(--nc) 15%, transparent);
    background: color-mix(in srgb, var(--nc) 4%, var(--bg-surface));
  }
  .pills-left, .pills-right { display: flex; flex-direction: column; gap: 2px; }
  .pills-right { align-items: flex-end; }
  .pill {
    font-size: 7.5px;
    font-family: 'DM Mono', monospace;
    padding: 1px 4px;
    border-radius: 2px;
    line-height: 1.3;
  }
  .pill-in  { background: #EFF6FF; color: #1D4ED8; }
  .pill-out { background: #F0FDF4; color: #166534; }

  /* Expanded body */
  .wd-body {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: 1px solid color-mix(in srgb, var(--nc) 20%, transparent);
    overflow-y: auto;
    position: relative;
  }
  .row { display: flex; gap: 6px; }
  .field { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
  label { font-size: 10px; color: var(--text-muted); }
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
  .mono { font-family: 'DM Mono', monospace; }
  .subpanel {
    background: color-mix(in srgb, var(--nc) 5%, var(--bg-elevated));
    border: 0.5px solid color-mix(in srgb, var(--nc) 20%, transparent);
    border-radius: 6px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hint {
    font-size: 9px;
    color: var(--text-muted);
    line-height: 1.5;
    padding: 5px 7px;
    background: var(--bg-elevated);
    border-left: 2px solid color-mix(in srgb, var(--nc) 40%, transparent);
    border-radius: 0 4px 4px 0;
  }
  .resize-handle {
    position: absolute;
    bottom: 2px; right: 4px;
    font-size: 14px;
    color: var(--text-muted);
    opacity: 0.4;
    cursor: nwse-resize;
    user-select: none;
  }
  .resize-handle:hover { opacity: 0.9; }
</style>