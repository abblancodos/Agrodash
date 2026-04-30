<!-- src/lib/components/processes/PipelineNode.svelte -->
<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data }: { data: any } = $props();
</script>

<div
  class="flow-node"
  class:active={data.active}
  style="--nc:{data.color}"
  onclick={() => data.onClick?.(data.nodeId)}
  onkeydown={(e) => e.key === 'Enter' && data.onClick?.(data.nodeId)}
  role="button"
  tabindex="0"
>
  <Handle type="target" position={Position.Left} />
  <div class="fn-category">{data.category ?? ''}</div>
  <div class="fn-type">{data.label}</div>
  <div class="fn-id">{data.nodeId}</div>
  <Handle type="source" position={Position.Right} />
</div>

<style>
  .flow-node {
    display: flex; flex-direction: column; align-items: center; gap: 3px;
    padding: 10px 16px; border-radius: 8px;
    background: var(--bg-surface, #fff); border: 2px solid var(--nc, #8a9bb0);
    cursor: pointer; min-width: 120px; text-align: center;
    transition: all .15s; box-shadow: 0 2px 8px rgba(0,0,0,0.06);
  }
  .flow-node:hover, .flow-node.active {
    background: color-mix(in srgb, var(--nc) 10%, var(--bg-surface, #fff));
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
  }
  .fn-category { font-size: 9px; color: var(--nc); text-transform: uppercase; letter-spacing: .08em; font-family: 'DM Mono', monospace; }
  .fn-type { font-size: 12px; font-weight: 600; color: var(--text-primary, #1a1a2e); }
  .fn-id { font-size: 9px; color: var(--text-muted, #8a9bb0); font-family: 'DM Mono', monospace; }
</style>