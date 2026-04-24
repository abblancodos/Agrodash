<!-- SymbolPicker.svelte -->
<script lang="ts">
  import { onDestroy } from 'svelte';

  let { onPick }: { onPick: (s: string) => void } = $props();

  const groups = [
    { label: 'griegas min', symbols: ['α','β','γ','δ','ε','ζ','η','θ','ι','κ','λ','μ','ν','ξ','π','ρ','σ','τ','φ','χ','ψ','ω'] },
    { label: 'griegas may', symbols: ['Γ','Δ','Θ','Λ','Σ','Φ','Ψ','Ω'] },
    { label: 'matemáticas', symbols: ['∂','∑','∫','∏','√','∞','≈','≠','≤','≥','±','×','÷','·','°','‰','∝','∇'] },
    { label: 'subíndices',  symbols: ['₀','₁','₂','₃','₄','₅','₆','₇','₈','₉','ₐ','ₑ','ₒ','ₓ','ₙ','ₘ'] },
    { label: 'superíndices',symbols: ['⁰','¹','²','³','⁴','⁵','⁶','⁷','⁸','⁹','ⁿ'] },
  ];

  let open = $state(false);
  let btnEl = $state<HTMLButtonElement | null>(null);
  let panelEl = $state<HTMLDivElement | null>(null);

  // Posición del panel — se calcula cuando se abre
  let panelStyle = $state('');

  function toggle() {
    open = !open;
    if (open && btnEl) {
      // Calcular posición relativa al viewport
      const rect = btnEl.getBoundingClientRect();
      const panelW = 300;
      const panelH = 280;
      let left = rect.left;
      let top = rect.top - panelH - 6;
      // Si se sale por la derecha
      if (left + panelW > window.innerWidth - 8) left = window.innerWidth - panelW - 8;
      // Si se sale por arriba, poner debajo
      if (top < 8) top = rect.bottom + 6;
      panelStyle = `left:${left}px; top:${top}px; width:${panelW}px;`;
    }
  }

  function pick(s: string) {
    onPick(s);
    open = false;
  }

  function onDocClick(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node;
    if (!btnEl?.contains(t) && !panelEl?.contains(t)) open = false;
  }

  $effect(() => {
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  });
</script>

<button bind:this={btnEl} class="btn-sym" type="button" onclick={toggle} title="insertar símbolo">Ω</button>

{#if open}
  <!-- Portal al body para evitar overflow:hidden del modal -->
  <div bind:this={panelEl} class="sym-panel" style={panelStyle}>
    {#each groups as g}
      <div class="sym-group">
        <span class="sym-label">{g.label}</span>
        <div class="sym-row">
          {#each g.symbols as s}
            <button class="sym-btn" type="button" onclick={() => pick(s)}>{s}</button>
          {/each}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .btn-sym {
    padding: 3px 8px; border: 0.5px solid var(--border-default);
    border-radius: 4px; background: var(--bg-elevated); cursor: pointer;
    font-size: calc(13px * var(--font-scale)); color: var(--text-secondary);
    font-family: 'DM Mono', monospace; flex-shrink: 0;
  }
  .btn-sym:hover { border-color: var(--text-muted); color: var(--text-primary); }

  /* El panel va fixed al viewport, encima de todo */
  .sym-panel {
    position: fixed;
    z-index: 9999;
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 8px;
    padding: 10px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.18);
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 280px;
    overflow-y: auto;
  }
  .sym-group { display: flex; flex-direction: column; gap: 3px; }
  .sym-label { font-size: 9px; color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; }
  .sym-row { display: flex; flex-wrap: wrap; gap: 2px; }
  .sym-btn {
    width: 26px; height: 26px; border: none; background: none;
    cursor: pointer; font-size: calc(13px * var(--font-scale));
    border-radius: 4px; color: var(--text-primary);
  }
  .sym-btn:hover { background: var(--interactive-hover); }
</style>