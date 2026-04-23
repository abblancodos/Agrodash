<!-- SymbolPicker.svelte -->
<script lang="ts">
  let { onPick }: { onPick: (s: string) => void } = $props();

  const groups = [
    { label: 'griegas', symbols: ['α','β','γ','δ','ε','ζ','η','θ','ι','κ','λ','μ','ν','ξ','π','ρ','σ','τ','φ','χ','ψ','ω','Γ','Δ','Θ','Λ','Σ','Φ','Ψ','Ω'] },
    { label: 'matemáticas', symbols: ['∂','∑','∫','∏','√','∞','≈','≠','≤','≥','±','×','÷','·','°','′','″','‰','∝','∇','∈','∉','⊂','⊃','∪','∩'] },
    { label: 'unidades', symbols: ['µ','Å','℃','℉','Ω','℃','㎝','㎞','㎡','㎥','㎎','㎏','㎖','㎗','㎘','ℓ','㏄','㏎','㏑','㏒','㏔','㎐','㎒','㎓','㎔'] },
    { label: 'subíndices', symbols: ['₀','₁','₂','₃','₄','₅','₆','₇','₈','₉','ₐ','ₑ','ₒ','ₓ','ₙ','ₘ','ᵢ','ⱼ','ₖ'] },
    { label: 'superíndices', symbols: ['⁰','¹','²','³','⁴','⁵','⁶','⁷','⁸','⁹','ⁿ','ˢ','ᵗ','ʰ'] },
  ];

  let open = $state(false);
</script>

<div class="sym-wrap">
  <button class="btn-sym" type="button" onclick={() => open = !open} title="insertar símbolo">Ω</button>
  {#if open}
    <div class="sym-panel">
      {#each groups as g}
        <div class="sym-group">
          <span class="sym-label">{g.label}</span>
          <div class="sym-row">
            {#each g.symbols as s}
              <button class="sym-btn" type="button" onclick={() => { onPick(s); open = false; }}>{s}</button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sym-wrap { position: relative; display: inline-block; }
  .btn-sym {
    padding: 3px 8px; border: 0.5px solid var(--border-default);
    border-radius: 4px; background: var(--bg-elevated); cursor: pointer;
    font-size: calc(13px * var(--font-scale)); color: var(--text-secondary);
    font-family: 'DM Mono', monospace;
  }
  .btn-sym:hover { border-color: var(--text-muted); color: var(--text-primary); }
  .sym-panel {
    position: absolute; bottom: 100%; left: 0; z-index: 100;
    background: var(--bg-surface); border: 0.5px solid var(--border-default);
    border-radius: 8px; padding: 10px; min-width: 280px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
    display: flex; flex-direction: column; gap: 8px;
  }
  .sym-group { display: flex; flex-direction: column; gap: 4px; }
  .sym-label { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); text-transform: uppercase; letter-spacing: .05em; }
  .sym-row { display: flex; flex-wrap: wrap; gap: 2px; }
  .sym-btn {
    width: 26px; height: 26px; border: none; background: none;
    cursor: pointer; font-size: calc(13px * var(--font-scale));
    border-radius: 4px; color: var(--text-primary);
  }
  .sym-btn:hover { background: var(--interactive-hover); }
</style>