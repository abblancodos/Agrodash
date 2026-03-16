<script lang="ts">
  import { sensorColor, normaliseSensorLabel, type Box } from '$lib/api';

  interface Props {
    boxes: Box[];
    selected: Set<string>;
    onchange: (selected: Set<string>) => void;
  }
  let { boxes, selected, onchange }: Props = $props();

  let open = $state(false);
  let panelEl: HTMLDivElement;
  let btnEl: HTMLButtonElement;
  let pulse = $state(true);

  // Stop pulsing after first open
  function toggle() {
    open = !open;
    pulse = false;
  }

  function handleOutside(e: MouseEvent) {
    if (open && panelEl && !panelEl.contains(e.target as Node) && !btnEl.contains(e.target as Node)) {
      open = false;
    }
  }

  function toggleBox(id: string) {
    const next = new Set(selected);
    if (next.has(id)) {
      if (next.size > 1) next.delete(id); // keep at least one
    } else {
      next.add(id);
    }
    onchange(next);
  }

  function selectAll() { onchange(new Set(boxes.map(b => b.id))); }
  function selectNone() {
    // keep first one selected
    if (boxes.length) onchange(new Set([boxes[0].id]));
  }

  const selectedCount = $derived(selected.size);
</script>

<svelte:window onclick={handleOutside} />

<div class="bsel">
  <button
    bind:this={btnEl}
    class="bsel__btn"
    class:bsel__btn--pulse={pulse}
    class:bsel__btn--active={open}
    onclick={toggle}
    title="Seleccionar cajas"
  >
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="3" width="7" height="7"/>
      <rect x="14" y="3" width="7" height="7"/>
      <rect x="3" y="14" width="7" height="7"/>
      <rect x="14" y="14" width="7" height="7"/>
    </svg>
    <span class="bsel__count">{selectedCount}/{boxes.length}</span>
    {#if pulse}<span class="bsel__pulse-ring"></span>{/if}
  </button>

  {#if open}
    <div class="bsel__panel" bind:this={panelEl}>
      <div class="bsel__header">
        <span class="bsel__title">CAJAS VISIBLES</span>
        <div class="bsel__actions">
          <button class="bsel__action" onclick={selectAll}>todas</button>
          <span class="bsel__sep">·</span>
          <button class="bsel__action" onclick={selectNone}>ninguna</button>
        </div>
      </div>
      <div class="bsel__list">
        {#each boxes as box (box.id)}
          {@const isOn = selected.has(box.id)}
          <button
            class="bsel__item"
            class:bsel__item--on={isOn}
            onclick={() => toggleBox(box.id)}
          >
            <span class="bsel__check">{isOn ? '✓' : ''}</span>
            <span class="bsel__name">{box.name}</span>
            <span class="bsel__sensors">{box.sensors.length} sen.</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .bsel { position:relative; }

  .bsel__btn {
    display:flex; align-items:center; gap:5px; position:relative;
    padding:5px 9px; height:28px;
    background:var(--interactive-bg); border:1px solid var(--border-default);
    border-radius:4px; color:var(--text-muted);
    font-family:'DM Mono',monospace; font-size:10px; letter-spacing:.06em;
    cursor:pointer; transition:all .12s;
  }
  .bsel__btn:hover { background:var(--interactive-hover); color:var(--text-secondary); border-color:var(--border-strong); }
  .bsel__btn--active { background:var(--accent-bg); border-color:var(--accent-border); color:var(--accent-text); }

  .bsel__count { font-variant-numeric:tabular-nums; }

  /* Pulse animation to attract attention on first load */
  .bsel__btn--pulse { border-color:var(--accent-border); }
  .bsel__pulse-ring {
    position:absolute; inset:-3px; border-radius:6px;
    border:2px solid var(--accent-border);
    animation:ring 1.5s ease-out infinite;
    pointer-events:none;
  }
  @keyframes ring {
    0% { opacity:.8; transform:scale(1); }
    100% { opacity:0; transform:scale(1.18); }
  }

  /* Panel */
  .bsel__panel {
    position:absolute; top:calc(100% + 6px); left:0; z-index:300;
    width:240px; max-height:400px;
    background:var(--bg-overlay); border:1px solid var(--border-default);
    border-radius:6px; box-shadow:0 8px 32px rgba(0,0,0,.15);
    display:flex; flex-direction:column; overflow:hidden;
  }

  .bsel__header {
    display:flex; align-items:center; justify-content:space-between;
    padding:10px 12px 8px; border-bottom:1px solid var(--border-subtle); flex-shrink:0;
  }
  .bsel__title { font-family:'DM Mono',monospace; font-size:8.5px; letter-spacing:.14em; color:var(--text-faint); }
  .bsel__actions { display:flex; align-items:center; gap:4px; }
  .bsel__action {
    background:none; border:none; font-family:'DM Mono',monospace; font-size:9px;
    letter-spacing:.06em; color:var(--text-muted); cursor:pointer; padding:0;
    transition:color .12s;
  }
  .bsel__action:hover { color:var(--text-secondary); }
  .bsel__sep { color:var(--border-default); font-size:10px; }

  .bsel__list { overflow-y:auto; padding:6px; display:flex; flex-direction:column; gap:2px; }

  .bsel__item {
    display:flex; align-items:center; gap:8px; padding:6px 8px;
    background:none; border:1px solid transparent; border-radius:3px;
    cursor:pointer; transition:all .1s; text-align:left; width:100%;
  }
  .bsel__item:hover { background:var(--interactive-hover); border-color:var(--border-subtle); }
  .bsel__item--on { background:var(--accent-bg); border-color:var(--accent-border); }

  .bsel__check {
    width:12px; font-size:9px; color:var(--accent-text); flex-shrink:0;
    font-family:'DM Mono',monospace;
  }
  .bsel__name {
    font-family:'DM Mono',monospace; font-size:10px; letter-spacing:.04em;
    color:var(--text-secondary); flex:1;
  }
  .bsel__item--on .bsel__name { color:var(--accent-text); }
  .bsel__sensors {
    font-family:'DM Mono',monospace; font-size:8.5px;
    color:var(--text-faint); flex-shrink:0;
  }
</style>