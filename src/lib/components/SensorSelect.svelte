<script lang="ts">
  import { sensorColor, normaliseSensorLabel } from '$lib/api';

  interface SensorRow {
    number: number;
    types: { id: string; type: string }[];
  }

  interface Props {
    rows: SensorRow[];
    active: Map<number, Set<string>>;
    onchange: (active: Map<number, Set<string>>) => void;
  }
  let { rows, active, onchange }: Props = $props();

  let selectedNum = $state(rows[0]?.number ?? 0);
  let open = $state(false);
  let btnEl: HTMLButtonElement;
  let panelEl: HTMLDivElement;
  let panelStyle = $state('');

  // Which sensors are fully hidden (eye toggled off)
  let hiddenSensors = $state<Set<number>>(new Set());

  function openDrop() {
    open = !open;
    if (open && btnEl) {
      const r = btnEl.getBoundingClientRect();
      const panelWidth = 200;
      const left = Math.min(r.left, window.innerWidth - panelWidth - 8);
      panelStyle = `top:${r.bottom + 6}px;left:${left}px;width:${panelWidth}px;`;
    }
  }

  function handleOutside(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node;
    if (btnEl?.contains(t) || panelEl?.contains(t)) return;
    open = false;
  }

  function selectSensor(num: number) {
    selectedNum = num;
    open = false;
  }

  function toggleEye(num: number, e: MouseEvent) {
    e.stopPropagation();
    const next = new Set(hiddenSensors);
    if (next.has(num)) { next.delete(num); } else { next.add(num); }
    hiddenSensors = next;
    // Propagate: hidden sensors get empty type sets
    const nextActive = new Map(active);
    if (next.has(num)) {
      nextActive.set(num, new Set());
    } else {
      // Restore all types for that sensor
      const row = rows.find(r => r.number === num);
      if (row) nextActive.set(num, new Set(row.types.map(t => t.type.toLowerCase())));
    }
    onchange(nextActive);
  }

  function toggleType(typeKey: string) {
    const next = new Map(active);
    const cur = new Set(next.get(selectedNum) ?? []);
    if (cur.has(typeKey)) { cur.delete(typeKey); } else { cur.add(typeKey); }
    next.set(selectedNum, cur);
    // If any type is active, sensor is visible
    if (cur.size > 0) {
      const h = new Set(hiddenSensors);
      h.delete(selectedNum);
      hiddenSensors = h;
    }
    onchange(next);
  }

  function isTypeActive(num: number, typeKey: string) {
    return active.get(num)?.has(typeKey) ?? true;
  }

  const selectedRow = $derived(rows.find(r => r.number === selectedNum));
</script>

<svelte:window onclick={handleOutside} />

<div class="ss">
  <!-- Dropdown trigger -->
  <button bind:this={btnEl} class="ss__btn" class:ss__btn--open={open} onclick={openDrop}>
    <span class="ss__btn-label">Sensor #{selectedNum}</span>
    <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
      <polyline points="6 9 12 15 18 9"/>
    </svg>
  </button>

  <!-- Type pills for selected sensor -->
  {#if selectedRow}
    <div class="ss__types">
      {#each selectedRow.types as t (t.id)}
        {@const key = t.type.toLowerCase()}
        {@const on = isTypeActive(selectedNum, key)}
        <button
          class="ss__type"
          class:ss__type--off={!on}
          style="--c:{sensorColor(t.type)}"
          onclick={() => toggleType(key)}
        >
          <span class="ss__type-dot"></span>
          <span class="ss__type-label">{normaliseSensorLabel(t.type)}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<!-- Panel rendered in body to avoid overflow clipping -->
{#if open}
  <div class="ss__panel" bind:this={panelEl} style={panelStyle}>
    <div class="ss__panel-header">
      <span class="ss__panel-title">SENSORES</span>
      <div class="ss__panel-acts">
        <button class="ss__panel-act" onclick={() => {
          hiddenSensors = new Set();
          const next = new Map(rows.map(r => [r.number, new Set(r.types.map(t => t.type.toLowerCase()))]));
          onchange(next);
        }}>todos</button>
        <span class="ss__panel-sep">·</span>
        <button class="ss__panel-act" onclick={() => {
          hiddenSensors = new Set(rows.map(r => r.number));
          onchange(new Map(rows.map(r => [r.number, new Set()])));
        }}>ninguno</button>
      </div>
    </div>
    {#each rows as row (row.number)}
      {@const hidden = hiddenSensors.has(row.number)}
      <div class="ss__option" class:ss__option--active={row.number === selectedNum} class:ss__option--hidden={hidden}>
        <button class="ss__opt-select" onclick={() => selectSensor(row.number)}>
          <span class="ss__opt-num">Sensor #{row.number}</span>
          <span class="ss__opt-dots">
            {#each row.types as t (t.id)}
              <span class="ss__opt-dot" style="background:{sensorColor(t.type)}" title={normaliseSensorLabel(t.type)}></span>
            {/each}
          </span>
        </button>
        <button class="ss__eye" onclick={(e) => toggleEye(row.number, e)} title={hidden ? 'Mostrar' : 'Ocultar'}>
          {#if !hidden}
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
              <circle cx="12" cy="12" r="3"/>
            </svg>
          {:else}
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/>
              <line x1="1" y1="1" x2="23" y2="23"/>
            </svg>
          {/if}
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .ss { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }

  .ss__btn {
    display: flex; align-items: center; gap: 0.3rem;
    padding: 0.25rem 0.5rem; height: 1.625rem;
    background: var(--interactive-bg); border: 1px solid var(--border-default);
    border-radius: 4px; color: var(--text-secondary);
    font-family: 'DM Mono', monospace; font-size: 0.625rem; letter-spacing: .06em;
    cursor: pointer; transition: all .12s; white-space: nowrap;
  }
  .ss__btn:hover, .ss__btn--open {
    background: var(--interactive-hover); border-color: var(--border-strong); color: var(--text-primary);
  }
  .ss__btn-label { font-variant-numeric: tabular-nums; }

  /* Panel — fixed position to escape overflow:hidden */
  .ss__panel {
    position: fixed; z-index: 9999;
    background: var(--bg-overlay); border: 1px solid var(--border-default);
    border-radius: 6px; box-shadow: 0 8px 24px rgba(0,0,0,.2);
    padding: 0.3rem; display: flex; flex-direction: column; gap: 2px;
    max-height: 280px; overflow-y: auto;
  }

  .ss__panel-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 0.375rem 0.5rem 0.25rem;
    border-bottom: 1px solid var(--border-subtle); margin-bottom: 0.2rem;
  }
  .ss__panel-title { font-family: 'DM Mono', monospace; font-size: 0.5rem; letter-spacing: .14em; color: var(--text-faint); }
  .ss__panel-acts { display: flex; align-items: center; gap: 0.25rem; }
  .ss__panel-act {
    background: none; border: none; font-family: 'DM Mono', monospace;
    font-size: 0.5625rem; letter-spacing: .06em; color: var(--text-muted);
    cursor: pointer; padding: 0; transition: color .12s;
  }
  .ss__panel-act:hover { color: var(--text-secondary); }
  .ss__panel-sep { color: var(--border-default); font-size: 0.625rem; }

  .ss__option {
    display: flex; align-items: center; border-radius: 3px;
    transition: background .1s;
  }
  .ss__option:hover { background: var(--interactive-hover); }
  .ss__option--active { background: var(--accent-bg); }
  .ss__option--hidden { opacity: .45; }

  .ss__opt-select {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.3rem 0.5rem; border: none; background: none;
    cursor: pointer; flex: 1; text-align: left;
  }
  .ss__opt-num {
    font-family: 'DM Mono', monospace; font-size: 0.625rem; letter-spacing: .06em;
    color: var(--text-secondary); white-space: nowrap;
  }
  .ss__option--active .ss__opt-num { color: var(--accent-text); }

  .ss__opt-dots { display: flex; gap: 3px; flex-wrap: wrap; }
  .ss__opt-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }

  .ss__eye {
    background: none; border: none; cursor: pointer; padding: 0.25rem 0.5rem;
    color: var(--text-muted); display: flex; align-items: center;
    border-radius: 3px; transition: all .12s; flex-shrink: 0;
  }
  .ss__eye:hover { color: var(--text-primary); background: var(--interactive-hover); }

  /* Type pills */
  .ss__types { display: flex; gap: 0.3rem; flex-wrap: wrap; }

  .ss__type {
    display: flex; align-items: center; gap: 0.25rem;
    padding: 0.125rem 0.5rem 0.125rem 0.375rem;
    background: color-mix(in srgb, var(--c) 10%, var(--bg-elevated));
    border: 1px solid color-mix(in srgb, var(--c) 30%, transparent);
    border-radius: 3px; cursor: pointer; transition: all .12s;
  }
  .ss__type:hover { background: color-mix(in srgb, var(--c) 18%, var(--bg-elevated)); }
  .ss__type--off { background: var(--bg-elevated); border-color: var(--border-subtle); opacity: .4; }
  .ss__type-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--c); flex-shrink: 0; }
  .ss__type--off .ss__type-dot { background: var(--border-strong); }
  .ss__type-label {
    font-family: 'DM Mono', monospace; font-size: 0.5625rem; letter-spacing: .05em;
    color: color-mix(in srgb, var(--c) 80%, var(--text-primary)); white-space: nowrap;
  }
  .ss__type--off .ss__type-label { color: var(--text-faint); }
</style>