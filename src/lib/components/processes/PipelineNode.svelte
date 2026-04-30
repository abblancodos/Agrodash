<!-- src/lib/components/processes/PipelineNode.svelte -->
<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import { getContext } from 'svelte';
  import type { Writable } from 'svelte/store';

  let { data }: { data: any } = $props();

  // Read active state from context — never from props
  // This way the parent never needs to reassign flowNodes to update highlight
  const activeNodeStore = getContext<Writable<string | null>>('activeNode');
  let isActive = $state(false);
  activeNodeStore?.subscribe(id => { isActive = id === data.nodeId; });
</script>

<div
  class="flow-node"
  class:active={isActive}
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
    background: var(--bg-surface, #fff);
    border: 2px solid var(--nc, #8a9bb0);
    cursor: pointer; min-width: 120px; text-align: center;
    transition: background .15s, box-shadow .15s;
    box-shadow: 0 2px 8px rgba(0,0,0,0.06);
  }
  .flow-node:hover {
    background: color-mix(in srgb, var(--nc) 8%, var(--bg-surface, #fff));
    box-shadow: 0 4px 16px rgba(0,0,0,0.10);
  }
  .flow-node.active {
    background: color-mix(in srgb, var(--nc) 14%, var(--bg-surface, #fff));
    box-shadow: 0 4px 16px rgba(0,0,0,0.14);
    border-width: 2.5px;
  }
  .fn-category { font-size: 9px; color: var(--nc); text-transform: uppercase; letter-spacing: .08em; font-family: 'DM Mono', monospace; }
  .fn-type { font-size: 12px; font-weight: 600; color: var(--text-primary, #1a1a2e); }
  .fn-id { font-size: 9px; color: var(--text-muted, #8a9bb0); font-family: 'DM Mono', monospace; }
</style>