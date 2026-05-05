<!-- src/lib/components/processes/NodeCanvas.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';

  function defaultParams(type: string): Record<string, any> {
    const d: Record<string, any> = {
      postgres_sensor: { sensors: [] },
      kalman:          { Q: 1e-5, R: 1e-3, P0: 1.0, convergence_threshold: 5e-4, warmup_samples: 10 },
      moving_avg:      { window_n: 10, warmup_samples: 10 },
      ewma:            { alpha: 0.1, warmup_samples: 10 },
      lowpass:         { tau_seconds: 120, warmup_samples: 10 },
      passthrough:     {},
      concat:          {},
      weighted_mean:   { weights: [] },
      mahalanobis:     { target: [], threshold_act: 2.5, threshold_deact: 1.0, use_kalman_P: true },
      hysteresis:      { reduction: { type: 'mean' }, low: 0.08, high: 0.085, action_below_low: 'on', action_above_high: 'off' },
      sprt:            { mu_H0: 0.0, mu_H1: 1.0, sigma: 0.1, alpha: 0.05, beta: 0.05, reduction: { type: 'mean' }, reset_on_action: true },
      mqtt_actuator:   { connection: 'shared', topic: '', payload_on: 'on', payload_off: 'off' },
      http_actuator:   { connection: 'shared', path_on: '/on', path_off: '/off' },
      logger:          { tag: '' },
      select:          { indices: [] },
      linear_scale:    { a: 1.0, b: 0.0 },
    };
    return d[type] ?? {};
  }
  import PipelineBlock from './PipelineBlock.svelte';
  import type { ProcessConfig } from '$lib/stores/process';

  let {
    pipeline,
    availableSensors,
    canEdit,
    onchange,
  }: {
    pipeline: any;
    availableSensors: { id: string; label: string }[];
    canEdit: boolean;
    onchange: () => void;
  } = $props();

  // ── State ─────────────────────────────────────────────────────────────────
  let canvasEl = $state<HTMLDivElement | null>(null);
  let svgEl    = $state<SVGSVGElement | null>(null);

  // Node positions — keyed by node id
  let positions = $state<Record<string, { x: number; y: number }>>({});

  // Block element refs — for edge calculation
  let blockRefs: Record<string, HTMLDivElement> = {};
  // Track block dimensions for edge calculation
  let blockSizes = $state<Record<string, { w: number; h: number }>>({});

  // Update block size after render
  function measureBlock(nodeId: string, el: HTMLDivElement | null) {
    if (!el) return;
    blockRefs[nodeId] = el;
    // Use offsetWidth/Height which work correctly with position:absolute
    const w = el.offsetWidth  || 180;
    const h = el.offsetHeight || 36;
    if (blockSizes[nodeId]?.w !== w || blockSizes[nodeId]?.h !== h) {
      blockSizes = { ...blockSizes, [nodeId]: { w, h } };
    }
  }
  let renderTick = $state(0); // increment to force edge re-render after DOM updates

  // Dragging state
  let dragging = $state<{ nodeId: string; startX: number; startY: number; origX: number; origY: number } | null>(null);

  // Panning state
  let panning = $state<{ startX: number; startY: number; origPanX: number; origPanY: number } | null>(null);
  let panX = $state(0);
  let panY = $state(0);

  // Connecting state — drawing an edge
  let connecting = $state<{ fromId: string; fromPort: 'output'; x: number; y: number } | null>(null);
  let mouseX = $state(0);
  let mouseY = $state(0);

  // Expanded blocks
  let expanded = $state<Set<string>>(new Set());

  // ── Init positions from saved node_positions or auto-layout ───────────────
  onMount(() => {
    initPositions();
  });

  function initPositions() {
    const saved = pipeline.node_positions ?? {};
    const nodes = pipeline.nodes ?? [];
    const newPos: Record<string, { x: number; y: number }> = {};

    nodes.forEach((n: any, i: number) => {
      newPos[n.id] = saved[n.id] ?? { x: 60 + i * 260, y: 100 };
    });
    positions = newPos;
    // Delay edge render until DOM has painted the blocks
    setTimeout(() => {
      renderTick++;
      // Measure all blocks after DOM paints
      for (const nodeId of Object.keys(positions)) {
        measureBlock(nodeId, blockRefs[nodeId]);
      }
    }, 60);
  }

  // When pipeline changes (tab switch), reinit
  $effect(() => {
    const _id = pipeline.id; // track pipeline id
    initPositions();
    expanded = new Set();
  });

  // ── Node metadata ──────────────────────────────────────────────────────────
  const NODE_COLORS: Record<string, string> = {
    postgres_sensor: '#4a90d9',
    kalman: '#7c6fcd', moving_avg: '#7c6fcd', ewma: '#7c6fcd',
    lowpass: '#7c6fcd', passthrough: '#8a9bb0',
    concat: '#e8a838', weighted_mean: '#e8a838',
    mahalanobis: '#e07b54', hysteresis: '#e07b54', sprt: '#e07b54',
    mqtt_actuator: '#3da85a', http_actuator: '#3da85a',
    logger: '#8a9bb0', select: '#8a9bb0', linear_scale: '#8a9bb0',
  };

  const NODE_CATEGORY: Record<string, string> = {
    postgres_sensor: 'fuente',
    kalman: 'filtro', moving_avg: 'filtro', ewma: 'filtro',
    lowpass: 'filtro', passthrough: 'filtro',
    concat: 'combinador', weighted_mean: 'combinador',
    mahalanobis: 'decisor', hysteresis: 'decisor', sprt: 'decisor',
    mqtt_actuator: 'actuador', http_actuator: 'actuador',
    logger: 'util', select: 'util', linear_scale: 'util',
  };

  // ── Dragging ───────────────────────────────────────────────────────────────
  function startDrag(e: MouseEvent, nodeId: string) {
    if ((e.target as HTMLElement).closest('.port, button, input, select, textarea')) return;
    e.preventDefault();
    e.stopPropagation(); // prevent canvas pan from firing
    if (!canEdit) return;
    const pos = positions[nodeId] ?? { x: 0, y: 0 };
    dragging = { nodeId, startX: e.clientX, startY: e.clientY, origX: pos.x, origY: pos.y };
  }

  // Canvas panning
  function startPan(e: MouseEvent) {
    if (dragging || connecting) return;
    if ((e.target as HTMLElement).closest('.block')) return;
    e.preventDefault();
    panning = { startX: e.clientX, startY: e.clientY, origPanX: panX, origPanY: panY };
  }

  function onMouseMove(e: MouseEvent) {
    if (canvasEl) {
      const rect = canvasEl.getBoundingClientRect();
      mouseX = e.clientX - rect.left - panX;
      mouseY = e.clientY - rect.top  - panY;
    }
    if (dragging) {
      const dx = e.clientX - dragging.startX;
      const dy = e.clientY - dragging.startY;
      positions = {
        ...positions,
        [dragging.nodeId]: {
          x: Math.max(0, dragging.origX + dx),
          y: Math.max(0, dragging.origY + dy),
        }
      };
      return;
    }
    if (panning) {
      panX = panning.origPanX + (e.clientX - panning.startX);
      panY = panning.origPanY + (e.clientY - panning.startY);
    }
  }

  function onMouseUp() {
    if (dragging) {
      pipeline.node_positions = { ...pipeline.node_positions, ...positions };
      dragging = null;
      onchange();
    }
    if (panning) { panning = null; }
    connecting = null;
  }

  // ── Connecting ports ───────────────────────────────────────────────────────
  function startConnect(e: MouseEvent, fromId: string) {
    if (!canEdit) return;
    e.stopPropagation();
    if (!canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    connecting = {
      fromId,
      fromPort: 'output',
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    };
  }

  function endConnect(e: MouseEvent, toId: string) {
    if (!connecting || connecting.fromId === toId) { connecting = null; return; }
    e.stopPropagation();
    // Add edge if not duplicate
    const edges = pipeline.edges ?? [];
    const exists = edges.some((ed: any) => ed.from === connecting!.fromId && ed.to === toId);
    if (!exists && connecting.fromId !== toId) {
      pipeline.edges = [...edges, {
        id: `${connecting.fromId}-${toId}`,
        from: connecting.fromId,
        to: toId,
      }];
      setTimeout(() => { renderTick++; }, 20);
      onchange();
    }
    connecting = null;
  }

  // Drop from BlockPicker
  function onDrop(e: DragEvent) {
    e.preventDefault();
    const type = e.dataTransfer?.getData('text/plain');
    if (!type || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    // Drop position in canvas coords (accounting for pan)
    const x = e.clientX - rect.left - panX;
    const y = e.clientY - rect.top  - panY;

    const id  = `${type}_${Date.now()}`;
    const nodes = pipeline.nodes ?? [];
    pipeline.nodes = [...nodes, { id, type, ...defaultParams(type) }];
    if (!pipeline.node_positions) pipeline.node_positions = {};
    pipeline.node_positions[id] = { x: Math.max(0, x - 90), y: Math.max(0, y - 20) };
    positions = { ...positions, [id]: pipeline.node_positions[id] };
    setTimeout(() => { renderTick++; }, 50);
    onchange();
  }

  function removeEdge(edgeId: string) {
    pipeline.edges = (pipeline.edges ?? []).filter((e: any) => e.id !== edgeId);
    onchange();
  }

  function removeNode(nodeId: string) {
    pipeline.nodes = (pipeline.nodes ?? []).filter((n: any) => n.id !== nodeId);
    pipeline.edges = (pipeline.edges ?? []).filter((e: any) => e.from !== nodeId && e.to !== nodeId);
    if (pipeline.node_positions) delete pipeline.node_positions[nodeId];
    const newPos = { ...positions };
    delete newPos[nodeId];
    positions = newPos;
    onchange();
  }

  function toggleExpand(nodeId: string) {
    const next = new Set(expanded);
    if (next.has(nodeId)) next.delete(nodeId);
    else next.add(nodeId);
    expanded = next;
  }

  // ── Edge SVG paths ─────────────────────────────────────────────────────────
  function getPortCenter(nodeId: string, port: 'input' | 'output'): { x: number; y: number } | null {
    const pos  = positions[nodeId];
    const size = blockSizes[nodeId];
    if (!pos) return null;
    const w = size?.w ?? 180;
    const h = size?.h ?? 36;
    // Canvas-local coordinates (including pan offset)
    const cx = pos.x + panX;
    const cy = pos.y + panY + h / 2;
    return {
      x: port === 'input'  ? cx - 6        : cx + w + 6,
      y: cy,
    };
  }

  function cubicPath(x1: number, y1: number, x2: number, y2: number): string {
    const cx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${cx} ${y1}, ${cx} ${y2}, ${x2} ${y2}`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="canvas"
  bind:this={canvasEl}
  onmousedown={startPan}
  onmousemove={onMouseMove}
  onmouseup={onMouseUp}
  onmouseleave={onMouseUp}
  ondragover={(e) => { e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'; }}
  ondrop={onDrop}
  style="cursor:{panning ? 'grabbing' : dragging ? 'grabbing' : 'default'}"
>

  <!-- SVG layer for edges -->
  <svg class="edge-svg" bind:this={svgEl}>
    {#each (pipeline.edges ?? []) as edge (edge.id)}
      {@const p1 = getPortCenter(edge.from, 'output')}
      {@const p2 = getPortCenter(edge.to, 'input')}
      {#if p1 && p2}
        <!-- Edge line -->
        <path
          d={cubicPath(p1.x, p1.y, p2.x, p2.y)}
          stroke="var(--text-muted)"
          stroke-width="2.5"
          fill="none"
          opacity="0.7"
        />
        <!-- Arrow at target -->
        <polygon
          points="{p2.x},{p2.y} {p2.x - 8},{p2.y - 4} {p2.x - 8},{p2.y + 4}"
          fill="var(--text-muted)"
          opacity="0.7"
        />
        <!-- Edge delete button -->
        {#if canEdit}
          <circle
            cx={(p1.x + p2.x) / 2}
            cy={(p1.y + p2.y) / 2}
            r="7"
            fill="var(--bg-elevated)"
            stroke="var(--border-default)"
            stroke-width="1"
            style="cursor:pointer"
            role="button"
            tabindex="0"
            onclick={() => removeEdge(edge.id)}
            onkeydown={(e) => e.key === 'Enter' && removeEdge(edge.id)}
          />
          <text
            x={(p1.x + p2.x) / 2}
            y={(p1.y + p2.y) / 2 + 4}
            text-anchor="middle"
            font-size="10"
            fill="var(--text-muted)"
            style="cursor:pointer;pointer-events:none"
          >✕</text>
        {/if}
      {/if}
    {/each}

    <!-- In-progress connection line -->
    {#if connecting}
      {@const p1 = getPortCenter(connecting.fromId, 'output')}
      {#if p1}
        <path
          d={cubicPath(p1.x, p1.y, mouseX, mouseY)}
          stroke="var(--nc, #4a90d9)"
          stroke-width="2"
          stroke-dasharray="6 3"
          fill="none"
          pointer-events="none"
          opacity="0.8"
        />
      {/if}
    {/if}
  </svg>

  <!-- Blocks — translated by pan offset -->
  <div class="blocks-layer" style="transform:translate({panX}px,{panY}px)">
  {#each (pipeline.nodes ?? []) as node (node.id)}
    {@const pos = positions[node.id] ?? { x: 60, y: 100 }}
    {@const color = NODE_COLORS[node.type] ?? '#8a9bb0'}
    {@const category = NODE_CATEGORY[node.type] ?? ''}
    {@const isExpanded = expanded.has(node.id)}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="block"
      class:expanded={isExpanded}
      style="left:{pos.x}px; top:{pos.y}px; --nc:{color}"
      bind:this={blockRefs[node.id]}
      onmouseenter={() => measureBlock(node.id, blockRefs[node.id])}
    >
      <!-- Input port -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="port port-input"
        title="input"
        onmouseup={(e) => endConnect(e, node.id)}
      ></div>

      <!-- Block content -->
      <PipelineBlock
        {node}
        {color}
        {category}
        {isExpanded}
        {availableSensors}
        {canEdit}
        onexpand={() => toggleExpand(node.id)}
        onremove={() => removeNode(node.id)}
        onchange={onchange}
        onstartdrag={(e: MouseEvent) => startDrag(e, node.id)}
      />

      <!-- Output port -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="port port-output"
        title="output"
        onmousedown={(e) => startConnect(e, node.id)}
      ></div>
    </div>
  {/each}

  </div><!-- end blocks-layer -->

{#if (pipeline.nodes ?? []).length === 0}
    <div class="empty-hint">
      Usá el panel izquierdo para agregar bloques al pipeline
    </div>
  {/if}
</div>

<style>
  .canvas {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background-image: radial-gradient(var(--border-subtle) 1px, transparent 1px);
    background-size: 24px 24px;
    cursor: default;
  }

  .edge-svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    overflow: visible;
  }
  .edge-svg circle { pointer-events: all; }

  .block {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 0;
    cursor: grab;
    user-select: none;
    filter: drop-shadow(0 2px 8px rgba(0,0,0,0.08));
    transition: filter .15s;
  }
  .block:active { cursor: grabbing; filter: drop-shadow(0 4px 16px rgba(0,0,0,0.14)); }
  .block.expanded { z-index: 10; }

  .port {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--bg-surface);
    border: 2px solid var(--nc, #8a9bb0);
    flex-shrink: 0;
    z-index: 2;
    transition: transform .12s, background .12s;
  }
  .port-input  { cursor: crosshair; }
  .port-output { cursor: crosshair; }
  .port:hover  { transform: scale(1.4); background: var(--nc, #8a9bb0); }

  .empty-hint {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    color: var(--text-muted);
    font-size: calc(13px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    pointer-events: none;
  }
</style>