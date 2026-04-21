<script lang="ts">
  import DefinitionForms from './DefinitionForms.svelte';

  let { onClose }: { onClose: () => void } = $props();

  type DefType = 'constant' | 'expression' | 'objective' | 'step' | 'collaborator' | 'csv_schema';
  let selected = $state<DefType | null>(null);

  const options: { type: DefType; label: string; desc: string; icon: string }[] = [
    { type: 'constant',    label: 'constante',        desc: 'Valor fijo con unidad y comentario',          icon: 'C' },
    { type: 'expression',  label: 'expresión',        desc: 'Cálculo automático sobre constantes/vars',    icon: 'ƒ' },
    { type: 'step',        label: 'paso',             desc: 'Campos a registrar + script Rhai opcional',   icon: '→' },
    { type: 'objective',   label: 'objetivo',         desc: 'Condición de alerta sobre una variable',      icon: '◎' },
    { type: 'csv_schema',  label: 'schema CSV',       desc: 'Columnas esperadas para upload de archivos',  icon: '⊞' },
    { type: 'collaborator',label: 'colaborador',      desc: 'Agregar usuario con rol al experimento',      icon: '+' },
  ];
</script>

<div class="overlay" onclick={onClose} role="presentation">
  <div class="panel" onclick={(e) => e.stopPropagation()}
       onkeydown={(e) => e.stopPropagation()}
       role="dialog" aria-modal="true" tabindex="-1">

    <div class="panel-head">
      {#if selected}
        <button class="btn-back" onclick={() => selected = null}>←</button>
        <span class="panel-title">agregar {selected}</span>
      {:else}
        <span class="panel-title">¿qué querés agregar?</span>
      {/if}
      <button class="btn-close" onclick={onClose}>✕</button>
    </div>

    <div class="panel-body">
      {#if !selected}
        <div class="options-grid">
          {#each options as opt}
            <button class="option-btn" onclick={() => selected = opt.type}>
              <span class="option-icon">{opt.icon}</span>
              <span class="option-label">{opt.label}</span>
              <span class="option-desc">{opt.desc}</span>
            </button>
          {/each}
        </div>
      {:else}
        <DefinitionForms type={selected} onClose={onClose} />
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: center; padding: 20px; }
  .panel { background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 12px; width: 100%; max-width: 460px; max-height: 90vh; overflow-y: auto; }
  .panel-head { display: flex; align-items: center; gap: 8px; padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); }
  .panel-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .btn-back { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); padding: 0 4px; }
  .btn-close { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .panel-body { padding: calc(16px * var(--font-scale)); }
  .options-grid { display: grid; grid-template-columns: 1fr 1fr; gap: calc(8px * var(--font-scale)); }
  .option-btn { display: flex; flex-direction: column; align-items: flex-start; gap: 3px; padding: calc(12px * var(--font-scale)); border: 0.5px solid var(--border-subtle); border-radius: 8px; background: none; cursor: pointer; text-align: left; transition: all .12s; }
  .option-btn:hover { border-color: var(--border-default); background: var(--interactive-hover); }
  .option-icon { font-size: calc(16px * var(--font-scale)); color: var(--text-secondary); font-family: 'DM Mono', monospace; font-weight: 500; }
  .option-label { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .option-desc { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); line-height: 1.4; }
</style>
