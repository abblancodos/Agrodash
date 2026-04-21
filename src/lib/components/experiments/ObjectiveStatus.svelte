<script lang="ts">
  import type { ObjectiveEvaluation } from '$lib/stores/experiment';

  let { evaluation }: { evaluation: ObjectiveEvaluation } = $props();

  const { objective: obj, status, value, goto } = $derived(evaluation);

  function conditionLabel(obj: any) {
    if (obj.condition_type === 'range') {
      const c = obj.condition;
      const parts = [];
      if (c.min != null) parts.push(`≥ ${c.min}`);
      if (c.max != null) parts.push(`≤ ${c.max}`);
      return `${c.variable} ${parts.join(' y ')}`;
    }
    return obj.condition.expr ?? '—';
  }

  function valueLabel(v: number | null, obj: any) {
    if (v === null) return '—';
    const unit = obj.condition.unit ?? '';
    return `${v.toFixed(4).replace(/\.?0+$/, '')} ${unit}`.trim();
  }

  const statusIcon = $derived(
    status === 'ok' ? '✓' :
    status === 'warning' ? '↓' :
    status === 'violation' ? '✗' : '?'
  );
</script>

<div class="obj-row obj-row--{status}">
  <div class="obj-dot obj-dot--{status}"></div>
  <div class="obj-name">{obj.name}</div>
  <div class="obj-cond">{conditionLabel(obj)}</div>
  <div class="obj-val obj-val--{status}">
    {valueLabel(value, obj)} {statusIcon}
  </div>
  {#if goto}
    <div class="obj-goto obj-goto--{status}">→ {goto}</div>
  {/if}
</div>

<style>
  .obj-row {
    display: flex; align-items: center; gap: calc(10px * var(--font-scale));
    padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-subtle);
    border-radius: 6px; margin-bottom: calc(6px * var(--font-scale));
  }
  .obj-row--ok        { border-color: #639922; background: #EAF3DE18; }
  .obj-row--warning   { border-color: #BA7517; background: #FAEEDA18; }
  .obj-row--violation { border-color: #E24B4A; background: #FCEBEB18; }
  .obj-row--unknown   { }

  .obj-dot {
    width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0;
  }
  .obj-dot--ok        { background: #639922; }
  .obj-dot--warning   { background: #BA7517; }
  .obj-dot--violation { background: #E24B4A; }
  .obj-dot--unknown   { background: var(--border-default); }

  .obj-name {
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    color: var(--text-primary); min-width: 120px;
  }
  .obj-cond {
    font-size: calc(12px * var(--font-scale)); color: var(--text-secondary);
    font-family: 'DM Mono', monospace; flex: 1;
  }
  .obj-val {
    font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace;
    min-width: 90px; text-align: right; font-weight: 500;
  }
  .obj-val--ok        { color: #3B6D11; }
  .obj-val--warning   { color: #854F0B; }
  .obj-val--violation { color: #A32D2D; }
  .obj-val--unknown   { color: var(--text-muted); }

  .obj-goto {
    font-size: calc(11px * var(--font-scale)); padding: 2px 8px;
    border-radius: 20px; white-space: nowrap;
  }
  .obj-goto--ok        { background: #EAF3DE; color: #3B6D11; }
  .obj-goto--warning   { background: #FAEEDA; color: #854F0B; }
  .obj-goto--violation { background: #FCEBEB; color: #A32D2D; }

  @media (max-width: 640px) {
    .obj-cond { display: none; }
    .obj-goto { display: none; }
  }
</style>
