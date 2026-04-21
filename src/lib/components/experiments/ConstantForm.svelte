<!-- ConstantForm.svelte -->
<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';

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
      const token = auth.getToken();
      const res = await fetch(`${API}/api/v1/experiments/${$experimentStore.experiment?.id}/definitions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
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
  <div class="field"><label>key (identificador)</label><input class="mono" bind:value={key} placeholder="M_solidos" /></div>
  <div class="field"><label>label (para el usuario)</label><input bind:value={label} placeholder="Masa de sólidos" /></div>
  <div class="field row">
    <div class="field-grow"><label>valor numérico</label><input class="mono" bind:value={value} inputmode="decimal" placeholder="8033.7" /></div>
    <div class="field-unit"><label>unidad</label><input bind:value={unit} placeholder="g" /></div>
  </div>
  <div class="field"><label>comentario <span class="muted">(recomendado)</span></label><textarea rows="2" bind:value={comment} placeholder="Ej: Pesaje realizado el 15 abr con suelo seco al aire"></textarea></div>
  <div class="btn-row">
    <button class="btn-cancel" onclick={onClose}>cancelar</button>
    <button class="btn-save" disabled={loading} onclick={save}>{loading ? 'guardando...' : 'guardar constante'}</button>
  </div>
</div>

<style src="./forms.css"></style>
