<script lang="ts">
  import { sensorColor, normaliseSensorLabel, type Box } from '$lib/api';

  interface Props {
    boxes: Box[];
    selected: Set<string>; // lowercase type keys
    onchange: (selected: Set<string>) => void;
  }
  let { boxes, selected, onchange }: Props = $props();

  let open = $state(false);
  let panelEl: HTMLDivElement;
  let btnEl: HTMLButtonElement;

  // All unique types across all boxes
  const allTypes = $derived(
    [...new Set(boxes.flatMap(b => b.sensors.map(s => s.type.toLowerCase())))]
      .sort()
  );

  function handleOutside(e: MouseEvent) {
    if (open && panelEl && !panelEl.contains(e.target as Node) && !btnEl.contains(e.target as Node)) {
      open = false;
    }
  }

  function toggle() {
    open = !open;
    if (open) requestAnimationFrame(() => reposition());
  }

  function reposition() {
    if (!panelEl || !btnEl) return;
    const btn   = btnEl.getBoundingClientRect();
    const panel = panelEl.getBoundingClientRect();
    const vw    = window.innerWidth;
    const margin = 8;
    let left = btn.left + btn.width / 2 - panel.width / 2;
    left = Math.max(margin, Math.min(left, vw - panel.width - margin));
    const parentLeft = (btnEl.closest('.tsel') as HTMLElement)?.getBoundingClientRect().left ?? 0;
    panelEl.style.left = (left - parentLeft) + 'px';
    panelEl.style.transform = 'none';
  }

  function toggleType(type: string) {
    const next = new Set(selected);
    if (next.has(type)) {
      next.delete(type);
    } else {
      next.add(type);
    }
    onchange(next);
  }

  function selectAll() { onchange(new Set(allTypes)); }
  function selectNone() { onchange(new Set()); }

  const selectedCount = $derived(selected.size);
</script>

<svelte:window onclick={handleOutside} />

<div class="tsel">
  <button
    bind:this={btnEl}
    class="tsel__btn"
    class:tsel__btn--active={open}
    bind:this={btnEl} onclick={toggle}
    title="Filtrar tipos de sensor"
  >
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
      <line x1="4" y1="6" x2="20" y2="6"/>
      <line x1="8" y1="12" x2="16" y2="12"/>
      <line x1="11" y1="18" x2="13" y2="18"/>
    </svg>
    <span class="tsel__count">{selectedCount}/{allTypes.length}</span>
  </button>

  {#if open}
    <div class="tsel__panel" bind:this={panelEl}>
      <div class="tsel__header">
        <span class="tsel__title">TIPOS DE SENSOR</span>
        <div class="tsel__actions">
          <button class="tsel__action" onclick={selectAll}>todos</button>
          <span class="tsel__sep">·</span>
          <button class="tsel__action" onclick={selectNone}>ninguno</button>
        </div>
      </div>
      <div class="tsel__list">
        {#each allTypes as type (type)}
          {@const isOn = selected.has(type)}
          {@const color = sensorColor(type)}
          <button
            class="tsel__item"
            class:tsel__item--on={isOn}
            onclick={() => toggleType(type)}
            style="--c:{color}"
          >
            <span class="tsel__check">{isOn ? '✓' : ''}</span>
            <span class="tsel__dot" style="background:{color}"></span>
            <span class="tsel__name">{normaliseSensorLabel(type)}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .tsel { position: relative; }

  .tsel__btn {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 9px; height: 28px;
    background: var(--interactive-bg); border: 1px solid var(--border-default);
    border-radius: 4px; color: var(--text-muted);
    font-family: 'DM Mono', monospace; font-size: 10px; letter-spacing: .06em;
    cursor: pointer; transition: all .12s;
  }
  .tsel__btn:hover { background: var(--interactive-hover); color: var(--text-secondary); border-color: var(--border-strong); }
  .tsel__btn--active { background: var(--accent-bg); border-color: var(--accent-border); color: var(--accent-text); }
  .tsel__count { font-variant-numeric: tabular-nums; }

  .tsel__panel {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 300;
    width: 220px; max-height: 380px;
    background: var(--bg-overlay); border: 1px solid var(--border-default);
    border-radius: 6px; box-shadow: 0 8px 32px rgba(0,0,0,.15);
    display: flex; flex-direction: column; overflow: hidden;
  }

  .tsel__header {
    display: flex; align-items: center; justify-content: space-between;
    padding:12px 14px 10px; border-bottom: 1px solid var(--border-subtle); flex-shrink: 0;
  }
  .tsel__title { font-family: 'DM Mono', monospace; font-size: 8.5px; letter-spacing: .14em; color: var(--text-faint); }
  .tsel__actions { display: flex; align-items: center; gap: 4px; }
  .tsel__action {
    background: none; border: none; font-family: 'DM Mono', monospace; font-size: 9px;
    letter-spacing: .06em; color: var(--text-muted); cursor: pointer; padding: 0; transition: color .12s;
  }
  .tsel__action:hover { color: var(--text-secondary); }
  .tsel__sep { color: var(--border-default); font-size: 10px; }

  .tsel__list { overflow-y: auto; padding:8px; display: flex; flex-direction: column; gap: 2px; }

  .tsel__item {
    display: flex; align-items: center; gap: 7px; padding: 8px 12px;
    background: none; border: 1px solid transparent; border-radius: 3px;
    cursor: pointer; transition: all .1s; text-align: left; width: 100%;
  }
  .tsel__item:hover { background: var(--interactive-hover); border-color: var(--border-subtle); }
  .tsel__item--on { background: color-mix(in srgb, var(--c) 8%, var(--bg-elevated)); border-color: color-mix(in srgb, var(--c) 25%, transparent); }

  .tsel__check { width: 12px; font-size: 9px; color: var(--accent-text); flex-shrink: 0; font-family: 'DM Mono', monospace; }
  .tsel__dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .tsel__name { font-family: 'DM Mono', monospace; font-size: 10px; letter-spacing: .04em; color: var(--text-secondary); flex: 1; }
</style>