<!-- src/lib/components/processes/WatchdogBlock.svelte -->
<!--
  Bloque especial para el nodo Watchdog con layout de 5 puntos:
  - 3 inputs a la izquierda: act_in, mqtt_ret_in, sig_in
  - nombre + estado en el centro
  - 1 output a la derecha: act_out

  Los puertos se exponen como bind:portEls para que NodeCanvas
  pueda calcular las coordenadas exactas de cada puerto.
-->
<script lang="ts">
  import { onMount } from 'svelte';

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



  // Notify parent when port elements are mounted
  $effect(() => {
    if (Object.keys(portEls).length > 0) {
      onPortEls?.(portEls);
    }
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
    good:  'Good',
    bad:   'Bad',
    ugly:  'Ugly',
  };

  function set(field: string, value: any) {
    node[field] = value;
    onchange();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="wd-block"
  class:expanded={isExpanded}
  style="--nc:{color}"
  ondblclick={onexpand}
  onmousedown={(e) => {
    if ((e.target as HTMLElement).closest('button, .port-wd')) return;
    onstartdrag?.(e);
  }}
>
  <!-- Left ports -->
  <div class="ports-left">
    {#each INPUTS as pname (pname)}
      <div class="port-wd-wrap">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="port-wd port-wd-in"
          style="--pc:{PORT_COLOR[pname]}"
          title={pname}
          bind:this={portEls[pname]}

          onmouseup={(e) => onconnectend?.(e, node.id, pname)}
        ></div>
        <span class="port-wd-label port-wd-label-in">{pname}</span>
      </div>
    {/each}
  </div>

  <!-- Center -->
  <div class="wd-center">
    <div class="wd-header">
      <span class="wd-cat">watchdog</span>
      <div class="wd-header-actions">
        <button class="btn-expand" onclick={onexpand}>{isExpanded ? '▲' : '▼'}</button>
        {#if canEdit}
          <button class="btn-remove" onclick={onremove}>✕</button>
        {/if}
      </div>
    </div>

    <div class="wd-mode">
      {MODE_LABEL[node.mode?.level ?? 'good']}
    </div>

    <!-- Expanded config panel -->
    {#if isExpanded}
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <div class="wd-body">
        <div class="field-row">
          <div class="field">
            <label>modo</label>
            <select class="inp" value={node.mode?.level ?? 'good'}
              onchange={(e) => {
                const level = (e.target as HTMLSelectElement).value;
                if (level === 'good') set('mode', { level: 'good' });
                if (level === 'bad')  set('mode', { level: 'bad', expected_on_trend: 'ascending', expected_off_trend: 'descending', min_change_pct: 0.05, component: null });
                if (level === 'ugly') set('mode', { level: 'ugly', notify_message: null });
              }}>
              <option value="good">Good — feedback MQTT</option>
              <option value="bad">Bad — tendencia</option>
              <option value="ugly">Ugly — confirmación manual</option>
            </select>
          </div>
        </div>
        <div class="field-row">
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
            <div class="field-row">
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
            <div class="field-row">
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

        <div class="node-hint">
          {#if node.mode?.level === 'bad'}
            Verifica que la señal cambie en la dirección esperada cuando el actuador recibe ON u OFF.
          {:else if node.mode?.level === 'ugly'}
            Pide confirmación manual antes de actuar. El actuador queda en Hold hasta que un operador confirme.
          {:else}
            Verifica que el actuador reportó el estado correcto vía MQTT Subscriber. Conectar mqtt_ret_in desde el MQTT Subscriber correspondiente.
          {/if}
        </div>

        <!-- Resize handle -->
        {#if onresize}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="resize-handle" onmousedown={(e) => { e.stopPropagation(); onresize?.(e); }}>⌟</div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Right ports -->
  <div class="ports-right">
    {#each OUTPUTS as pname (pname)}
      <div class="port-wd-wrap">
        <span class="port-wd-label port-wd-label-out">{pname}</span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="port-wd port-wd-out"
          style="--pc:{PORT_COLOR[pname]}"
          title={pname}
          bind:this={portEls[pname]}

          onmousedown={(e) => onconnectstart?.(e, node.id, pname)}
        ></div>
      </div>
    {/each}
  </div>
</div>

<style>
  .wd-block {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    border: 2px solid var(--nc);
    border-radius: 10px;
    background: var(--bg-surface);
    min-width: 220px;
    min-height: 90px;
    cursor: grab;
    user-select: none;
    position: relative;
  }
  .wd-block:active { cursor: grabbing; }
  .wd-block.expanded { min-width: 280px; min-height: 120px; cursor: default; }

  /* Left/right port columns */
  .ports-left, .ports-right {
    display: flex;
    flex-direction: column;
    justify-content: space-around;
    padding: 8px 0;
    gap: 4px;
    flex-shrink: 0;
  }
  .ports-left  { padding-left: 0; }
  .ports-right { padding-right: 0; }

  .port-wd-wrap {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .port-wd {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--pc);
    background: var(--bg-surface);
    cursor: crosshair;
    flex-shrink: 0;
    transition: background .1s;
  }
  .port-wd:hover { background: var(--pc); }
  .port-wd-in  { cursor: crosshair; }
  .port-wd-out { cursor: cell; }

  .port-wd-label {
    font-size: 7.5px;
    font-family: 'DM Mono', monospace;
    white-space: nowrap;
    padding: 1px 4px;
    border-radius: 2px;
    line-height: 1.3;
  }
  .port-wd-label-in  { background: #EFF6FF; color: #1D4ED8; }
  .port-wd-label-out { background: #F0FDF4; color: #166534; }

  /* Center */
  .wd-center {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 6px 4px;
    border-left:  0.5px solid color-mix(in srgb, var(--nc) 20%, transparent);
    border-right: 0.5px solid color-mix(in srgb, var(--nc) 20%, transparent);
  }

  .wd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
  }
  .wd-cat {
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: .08em;
    color: var(--nc);
    font-family: 'DM Mono', monospace;
  }
  .wd-header-actions { display: flex; gap: 2px; }
  .btn-expand, .btn-remove {
    width: 16px; height: 16px;
    border: none; background: none;
    cursor: pointer; font-size: 9px;
    color: var(--text-muted);
    border-radius: 3px; padding: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .btn-expand:hover { background: var(--interactive-hover); }
  .btn-remove:hover { background: var(--error-bg); color: var(--error-color); }

  .wd-mode {
    font-size: 13px;
    font-weight: 600;
    color: var(--nc);
    text-align: center;
    padding: 4px 0;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Expanded body */
  .wd-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 6px;
    border-top: 0.5px solid color-mix(in srgb, var(--nc) 20%, transparent);
    overflow-y: auto;
    position: relative;
  }

  .field { display: flex; flex-direction: column; gap: 2px; }
  .field-row { display: flex; gap: 6px; }
  .field-row .field { flex: 1; min-width: 0; }
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

  .node-hint {
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