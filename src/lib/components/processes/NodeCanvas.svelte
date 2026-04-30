<!-- src/lib/components/processes/NodeCanvas.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
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

  // Dragging state
  let dragging: { nodeId: string; startX: number; startY: number; origX: number; origY: number } | null = null;

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
    if (!canEdit) return;
    if ((e.target as HTMLElement).closest('.port, button, input, select, textarea')) return;
    e.preventDefault();
    const pos = positions[nodeId] ?? { x: 0, y: 0 };
    dragging = { nodeId, startX: e.clientX, startY: e.clientY, origX: pos.x, origY: pos.y };
  }

  function onMouseMove(e: MouseEvent) {
    if (canvasEl) {
      const rect = canvasEl.getBoundingClientRect();
      mouseX = e.clientX - rect.left;
      mouseY = e.clientY - rect.top;
    }
    if (!dragging) return;
    const dx = e.clientX - dragging.startX;
    const dy = e.clientY - dragging.startY;
    positions = {
      ...positions,
      [dragging.nodeId]: {
        x: Math.max(0, dragging.origX + dx),
        y: Math.max(0, dragging.origY + dy),
      }
    };
  }

  function onMouseUp() {
    if (dragging) {
      // Save positions to pipeline
      pipeline.node_positions = { ...pipeline.node_positions, ...positions };
      dragging = null;
      onchange();
    }
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
    if (!exists) {
      pipeline.edges = [...edges, {
        id: `${connecting.fromId}-${toId}`,
        from: connecting.fromId,
        to: toId,
      }];
      onchange();
    }
    connecting = null;
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
    const el = blockRefs[nodeId];
    if (!el || !canvasEl) return null;
    const cr = canvasEl.getBoundingClientRect();
    const nr = el.getBoundingClientRect();
    const y = nr.top - cr.top + nr.height / 2;
    const x = port === 'input'
      ? nr.left - cr.left
      : nr.left - cr.left + nr.width;
    return { x, y };
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
  onmousemove={onMouseMove}
  onmouseup={onMouseUp}
  onmouseleave={onMouseUp}
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
          stroke="var(--border-default)"
          stroke-width="2"
          fill="none"
          stroke-dasharray={connecting ? '4 2' : 'none'}
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
          stroke="var(--text-muted)"
          stroke-width="1.5"
          stroke-dasharray="4 2"
          fill="none"
          pointer-events="none"
        />
      {/if}
    {/if}
  </svg>

  <!-- Blocks -->
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
      onmousedown={(e) => startDrag(e, node.id)}
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