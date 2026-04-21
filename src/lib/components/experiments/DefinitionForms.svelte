<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import ScriptEditor from './ScriptEditor.svelte';

  let { type, onClose }:
    { type: 'expression' | 'objective' | 'step' | 'collaborator' | 'csv_schema'; onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let key = $state(''); let label = $state('');
  let formula = $state(''); let unit = $state(''); let comment = $state('');
  let script = $state('');
  // objective
  let condType = $state<'range' | 'expression'>('range');
  let variable = $state(''); let minVal = $state(''); let maxVal = $state('');
  let expr = $state(''); let severity = $state('warning');
  let gotoOk = $state(''); let gotoViol = $state('');
  // step fields
  let fields = $state<{ key: string; label: string; unit: string }[]>([{ key: '', label: '', unit: '' }]);
  // collaborator
  let searchQ = $state(''); let searchResults = $state<any[]>([]);
  let selectedUser = $state<any>(null); let role = $state('editor');
  // csv schema
  let columns = $state<{ key: string; label: string; unit: string }[]>([{ key: '', label: '', unit: '' }]);

  let loading = $state(false); let error = $state('');

  async function searchUsers() {
    if (searchQ.length < 2) return;
    const token = auth.getToken();
    const res = await fetch(`${API}/api/v1/users/search?q=${encodeURIComponent(searchQ)}`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (res.ok) searchResults = await res.json();
  }

  async function save() {
    loading = true; error = '';
    const expId = $experimentStore.experiment?.id;
    const token = auth.getToken();
    const headers = { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) };

    try {
      if (type === 'expression') {
        if (!key || !label || !formula) { error = 'key, label y fórmula requeridos'; return; }
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', headers,
          body: JSON.stringify({ key, label, type: 'expression', payload: { formula, unit, comment } }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addDefinition(d);

      } else if (type === 'objective') {
        if (!label) { error = 'label requerido'; return; }
        const condition = condType === 'range'
          ? { variable, min: minVal ? parseFloat(minVal) : null, max: maxVal ? parseFloat(maxVal) : null, unit }
          : { expr };
        const res = await fetch(`${API}/api/v1/experiments/${expId}/objectives`, {
          method: 'POST', headers,
          body: JSON.stringify({ name: label, condition_type: condType, condition, severity, goto_ok: gotoOk || null, goto_violation: gotoViol || null }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addObjective(d);

      } else if (type === 'step') {
        if (!key || !label) { error = 'key y label requeridos'; return; }
        const validFields = fields.filter(f => f.key && f.label);
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', headers,
          body: JSON.stringify({ key, label, type: 'step', payload: { fields: validFields, script } }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addDefinition(d);

      } else if (type === 'collaborator') {
        if (!selectedUser) { error = 'Seleccioná un usuario'; return; }
        const res = await fetch(`${API}/api/v1/experiments/${expId}/collaborators`, {
          method: 'POST', headers,
          body: JSON.stringify({ user_id: selectedUser.id, role }),
        });
        if (!res.ok) { const d = await res.json(); error = d.error; return; }
        experimentStore.addCollaborator({ ...selectedUser, user_id: selectedUser.id, role, added_at: new Date().toISOString() });

      } else if (type === 'csv_schema') {
        if (!key || !label) { error = 'key y label requeridos'; return; }
        const validCols = columns.filter(c => c.key && c.label);
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', headers,
          body: JSON.stringify({ key, label, type: 'csv_schema', payload: { columns: validCols } }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addDefinition(d);
      }

      onClose();
    } catch (e: any) { error = e.message; } finally { loading = false; }
  }
</script>

<div class="form-section">
  {#if error}<div class="err">{error}</div>{/if}

  {#if type === 'expression'}
    <h3 class="form-title">nueva expresión</h3>
    <div class="form-hint">Cálculo automático a partir de constantes, otras expresiones o variables de entries. Ej: <code>(masa - M_solidos) / V_suelo</code></div>
    <div class="field"><label for="df-key">key</label><input id="df-key" class="mono" bind:value={key} placeholder="theta_grav" /></div>
    <div class="field"><label for="df-label">label</label><input id="df-label" bind:value={label} placeholder="θ gravimétrico" /></div>
    <div class="field"><label for="df-formula">fórmula</label><input id="df-formula" class="mono" bind:value={formula} placeholder="(masa_pesaje - M_solidos) / V_suelo" /></div>
    <div class="field row">
      <div class="field-grow"><label for="df-unit">unidad</label><input id="df-unit" bind:value={unit} placeholder="m³/m³" /></div>
    </div>
    <div class="field"><label for="df-comment">comentario</label><textarea id="df-comment" rows="2" bind:value={comment}></textarea></div>

  {:else if type === 'objective'}
    <h3 class="form-title">nuevo objetivo</h3>
    <div class="field"><label for="df-nombre">nombre</label><input id="df-nombre" bind:value={label} placeholder="θ dentro del rango" /></div>
    <div class="field">
      <label for="df-condtype">tipo de condición</label>
      <select id="df-condtype" bind:value={condType}>
        <option value="range">rango numérico</option>
        <option value="expression">expresión</option>
      </select>
    </div>
    {#if condType === 'range'}
      <div class="field"><label for="df-variable">variable</label><input id="df-variable" class="mono" bind:value={variable} placeholder="theta_grav" /></div>
      <div class="field row">
        <div class="field-grow"><label for="df-min">mínimo (opcional)</label><input id="df-min" class="mono" bind:value={minVal} placeholder="0.28" inputmode="decimal" /></div>
        <div class="field-grow"><label for="df-max">máximo (opcional)</label><input id="df-max" class="mono" bind:value={maxVal} placeholder="0.32" inputmode="decimal" /></div>
        <div class="field-unit"><label for="df-unit">unidad</label><input id="df-unit" bind:value={unit} placeholder="m³/m³" /></div>
      </div>
    {:else}
      <div class="field"><label for="df-expr">expresión (resultado > 0 = ok)</label><input id="df-expr" class="mono" bind:value={expr} placeholder="theta_grav - 0.25" /></div>
    {/if}
    <div class="field">
      <label for="df-severity">severidad</label>
      <select id="df-severity" bind:value={severity}>
        <option value="info">info</option>
        <option value="warning">advertencia</option>
        <option value="critical">crítica</option>
      </select>
    </div>
    <div class="field row">
      <div class="field-grow"><label for="df-goto-ok">ir a (si OK)</label><input id="df-goto-ok" class="mono" bind:value={gotoOk} placeholder="monitoreo_continuo" /></div>
      <div class="field-grow"><label for="df-goto-viol">ir a (si viola)</label><input id="df-goto-viol" class="mono" bind:value={gotoViol} placeholder="irrigar" /></div>
    </div>

  {:else if type === 'step'}
    <h3 class="form-title">nuevo paso</h3>
    <div class="form-hint">Define qué datos se registran en este paso y qué script se ejecuta al guardarlo.</div>
    <div class="field"><label for="df-key">key</label><input id="df-key" class="mono" bind:value={key} placeholder="pesaje" /></div>
    <div class="field"><label for="df-label">label</label><input id="df-label" bind:value={label} placeholder="Pesaje de la maceta" /></div>
    <div class="fields-title">campos</div>
    {#each fields as f, i}
      <div class="field row">
        <div class="field-grow"><label>key</label><input aria-label="key" class="mono" bind:value={f.key} placeholder="masa" /></div>
        <div class="field-grow"><label>label</label><input aria-label="label" bind:value={f.label} placeholder="Masa" /></div>
        <div class="field-unit"><label>unidad</label><input aria-label="unidad" bind:value={f.unit} placeholder="g" /></div>
        <button class="btn-remove-field" onclick={() => fields = fields.filter((_, j) => j !== i)}>✕</button>
      </div>
    {/each}
    <button class="btn-add-field" onclick={() => fields = [...fields, { key: '', label: '', unit: '' }]}>+ campo</button>
    <div class="field"><label for="df-script">script (opcional)</label><ScriptEditor bind:value={script} /></div>

  {:else if type === 'collaborator'}
    <h3 class="form-title">agregar colaborador</h3>
    <div class="field">
      <label for="df-search">buscar usuario</label>
      <div class="search-row">
        <input id="df-search" bind:value={searchQ} oninput={searchUsers} placeholder="nombre o email" />
        <button class="btn-search" onclick={searchUsers}>buscar</button>
      </div>
    </div>
    {#if searchResults.length > 0}
      <div class="search-results">
        {#each searchResults as u}
          <button class="search-result" class:selected={selectedUser?.id === u.id}
                  onclick={() => selectedUser = u}>
            <span class="result-name">{u.display_name}</span>
            <span class="result-email">{u.email}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if selectedUser}
      <div class="field">
        <label for="df-role">rol</label>
        <select id="df-role" bind:value={role}>
          <option value="viewer">viewer — solo lectura</option>
          <option value="editor">editor — puede agregar entries</option>
          <option value="admin">admin — acceso completo</option>
        </select>
      </div>
    {/if}

  {:else if type === 'csv_schema'}
    <h3 class="form-title">schema de CSV</h3>
    <div class="form-hint">Define las columnas esperadas cuando se sube un archivo CSV en este paso.</div>
    <div class="field"><label for="df-csv-key">key del paso</label><input id="df-csv-key" class="mono" bind:value={key} placeholder="upload_rigol" /></div>
    <div class="field"><label for="df-label">label</label><input id="df-label" bind:value={label} placeholder="Upload Rigol CSV" /></div>
    <div class="fields-title">columnas esperadas</div>
    {#each columns as c, i}
      <div class="field row">
        <div class="field-grow"><label>key</label><input aria-label="key" class="mono" bind:value={c.key} placeholder="freq_hz" /></div>
        <div class="field-grow"><label>label</label><input aria-label="label col" bind:value={c.label} placeholder="Frecuencia" /></div>
        <div class="field-unit"><label>unidad</label><input aria-label="unidad col" bind:value={c.unit} placeholder="Hz" /></div>
        <button class="btn-remove-field" onclick={() => columns = columns.filter((_, j) => j !== i)}>✕</button>
      </div>
    {/each}
    <button class="btn-add-field" onclick={() => columns = [...columns, { key: '', label: '', unit: '' }]}>+ columna</button>
  {/if}

  <div class="btn-row">
    <button class="btn-cancel" onclick={onClose}>cancelar</button>
    <button class="btn-save" disabled={loading} onclick={save}>
      {loading ? 'guardando...' : 'guardar'}
    </button>
  </div>
</div>

<style>
  @import './forms.css';
  .fields-title { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; margin-top: 4px; }
  .btn-add-field { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); background: none; border: 0.5px dashed var(--border-default); border-radius: 4px; padding: 4px 10px; cursor: pointer; }
  .btn-remove-field { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 12px; padding: 0 4px; align-self: flex-end; margin-bottom: 6px; }
  .search-row { display: flex; gap: 6px; }
  .search-row input { flex: 1; }
  .btn-search { padding: calc(7px * var(--font-scale)) calc(12px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; color: var(--text-secondary); cursor: pointer; font-size: calc(12px * var(--font-scale)); }
  .search-results { display: flex; flex-direction: column; gap: 4px; max-height: 200px; overflow-y: auto; border: 0.5px solid var(--border-subtle); border-radius: 6px; padding: 4px; }
  .search-result { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px; border-radius: 4px; background: none; border: none; cursor: pointer; text-align: left; transition: background .1s; }
  .search-result:hover, .search-result.selected { background: var(--interactive-hover); }
  .result-name { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); }
  .result-email { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  code { font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale)); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; }
</style>