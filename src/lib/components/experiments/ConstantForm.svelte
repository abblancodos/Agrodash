<!-- ConstantForm.svelte -->
<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import SymbolPicker from './SymbolPicker.svelte';

  let { onClose }: { onClose: () => void } = $props();
  const API = import.meta.env.VITE_API_BASE ?? '';

  let key = $state(''); let label = $state(''); let value = $state('');
  let unit = $state(''); let comment = $state('');
  let loading = $state(false); let error = $state('');

  async function save() {
    if (!key || !label || value === '') { error = 'key, label y valor son requeridos'; return; }
    loading = true; error = '';
    const n = parseFloat(value);
    if (isNaN(n)) { error = 'El valor debe ser numérico'; loading = false; return; }
    try {
            const res = await fetch(`${API}/api/v1/experiments/${$experimentStore.experiment?.id}/definitions`, {
        credentials: 'include',
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ key, label, type: 'constant', payload: { value: n, unit, comment } }),
      });
      const data = await res.json();
      if (!res.ok) { error = data.error ?? 'Error'; return; }
      experimentStore.addDefinition(data);
      onClose();
    } catch (e: any) { error = e.message; } finally { loading = false; }
  }
</script>

<div class="form-section">
  <h3 class="form-title">nueva constante</h3>
  <div class="form-hint">Valor fijo que no cambia durante el experimento. Ej: masa de sólidos, volumen de referencia.</div>
  {#if error}<div class="err">{error}</div>{/if}
  <div class="field">
    <label for="cf-key">key <span class="field-hint">identificador único, solo letras/números/guión bajo, sin espacios ni símbolos</span></label>
    <input id="cf-key" class="mono" bind:value={key} placeholder="M_solidos" />
  </div>
  <div class="field">
    <label for="cf-label">nombre <span class="field-hint">nombre legible, puede tener símbolos y tildes</span></label>
    <div class="input-row">
      <input id="cf-label" bind:value={label} placeholder="Masa de sólidos" />
      <SymbolPicker onPick={(s) => label += s} />
    </div>
  </div>
  <div class="field row">
    <div class="field-grow"><label for="cf-value">valor numérico</label><input id="cf-value" class="mono" bind:value={value} inputmode="decimal" placeholder="8033.7" /></div>
    <div class="field-unit">
      <label for="cf-unit">unidad</label>
      <div class="input-row">
        <input id="cf-unit" bind:value={unit} placeholder="g" />
        <SymbolPicker onPick={(s) => unit += s} />
      </div>
    </div>
  </div>
  <div class="field"><label for="cf-comment">comentario <span class="muted">(recomendado)</span></label><textarea id="cf-comment" rows="2" bind:value={comment} placeholder="Ej: Pesaje realizado el 15 abr con suelo seco al aire"></textarea></div>
  <div class="btn-row">
    <button class="btn-cancel" onclick={onClose}>cancelar</button>
    <button class="btn-save" disabled={loading} onclick={save}>{loading ? 'guardando...' : 'guardar constante'}</button>
  </div>
</div>

<style>
.form-section { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
.form-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); margin-bottom: 2px; }
.form-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }
.err { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }
.field { display: flex; flex-direction: column; gap: 4px; }
.field label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
.field.row { flex-direction: row; gap: 10px; align-items: flex-end; }
.field-grow { flex: 1; display: flex; flex-direction: column; gap: 4px; }
.field-unit { flex: 0 0 auto; min-width: 80px; max-width: 120px; display: flex; flex-direction: column; gap: 4px; }
.muted { color: var(--text-muted); }
.field-hint { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-weight: 400; display: block; margin-top: 1px; }
.input-row { display: flex; gap: 6px; align-items: center; }
.input-row input { flex: 1; }
input, textarea {
  padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale));
  border: 0.5px solid var(--border-default); border-radius: 6px;
  font-size: calc(13px * var(--font-scale)); background: var(--bg-surface);
  color: var(--text-primary); outline: none; font-family: inherit;
}
input.mono { font-family: 'DM Mono', monospace; }
textarea { resize: vertical; }
.btn-row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.btn-cancel { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); background: none; border: none; cursor: pointer; padding: 6px 12px; }
.btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }

</style>