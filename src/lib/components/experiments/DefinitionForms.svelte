<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import ScriptEditor from './ScriptEditor.svelte';
  import SymbolPicker from './SymbolPicker.svelte';

  let { type, onClose }:
    { type: 'variable' | 'expression' | 'objective' | 'step' | 'collaborator' | 'csv_schema'; onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let key = $state(''); let label = $state('');
  let formula = $state(''); let unit = $state(''); let comment = $state('');
  let varType = $state<'numeric'|'vector_csv'|'text'|'qualitative'>('numeric');
  let qualOptions = $state('');
  let formulaEl = $state<HTMLInputElement | null>(null);
  let showDefPicker = $state(false);

  // Insertar key en la fórmula en la posición del cursor
  function insertKey(key: string) {
    if (!formulaEl) { formula += key; return; }
    const start = formulaEl.selectionStart ?? formula.length;
    const end = formulaEl.selectionEnd ?? formula.length;
    formula = formula.slice(0, start) + key + formula.slice(end);
    showDefPicker = false;
    // Restaurar foco y cursor después del texto insertado
    setTimeout(() => {
      formulaEl?.focus();
      formulaEl?.setSelectionRange(start + key.length, start + key.length);
    }, 10);
  }

  // Definitions disponibles para el picker
  const pickerDefs = $derived([
    ...$experimentStore.definitions.filter(d => d.type === 'variable'),
    ...$experimentStore.definitions.filter(d => d.type === 'constant'),
    ...$experimentStore.definitions.filter(d => d.type === 'expression'),
  ]);
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
        const res = await fetch(`${API}/api/v1/users/search?q=${encodeURIComponent(searchQ)}`, {
          });
    if (res.ok) searchResults = await res.json();
  }

  async function save() {
    loading = true; error = '';
    const expId = $experimentStore.experiment?.id;
        const headers = { 'Content-Type': 'application/json' };

    try {
      if (type === 'variable') {
        if (!key || !label) { error = 'key y label requeridos'; return; }
        const options = varType === 'qualitative'
          ? qualOptions.split(',').map(s => s.trim()).filter(Boolean)
          : [];
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', headers, credentials: 'include',
          body: JSON.stringify({
            key, label, type: 'variable',
            payload: { unit, comment },
            var_type: varType,
            options,
          }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addDefinition(d);

      } else if (type === 'expression') {
        if (!key || !label || !formula) { error = 'key, label y fórmula requeridos'; return; }
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
        credentials: 'include',
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
        credentials: 'include',
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
        credentials: 'include',
          method: 'POST', headers,
          body: JSON.stringify({ key, label, type: 'step', payload: { fields: validFields, script } }),
        });
        const d = await res.json();
        if (!res.ok) { error = d.error; return; }
        experimentStore.addDefinition(d);

      } else if (type === 'collaborator') {
        if (!selectedUser) { error = 'Seleccioná un usuario'; return; }
        const res = await fetch(`${API}/api/v1/experiments/${expId}/collaborators`, {
        credentials: 'include',
          method: 'POST', headers,
          body: JSON.stringify({ user_id: selectedUser.id, role }),
        });
        if (!res.ok) { const d = await res.json(); error = d.error; return; }
        experimentStore.addCollaborator({ ...selectedUser, user_id: selectedUser.id, role, added_at: new Date().toISOString() });

      } else if (type === 'csv_schema') {
        if (!key || !label) { error = 'key y label requeridos'; return; }
        const validCols = columns.filter(c => c.key && c.label);
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
        credentials: 'include',
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
    <div class="form-hint">Cálculo automático a partir de constantes y variables. Operadores: <code>+ - * / ^ ( )</code></div>
    <div class="field">
      <label for="ef-key">key <span class="field-hint">solo letras/números/guión bajo — se usa en otras expresiones</span></label>
      <input id="ef-key" class="mono" bind:value={key} placeholder="theta_grav" />
    </div>
    <div class="field">
      <label for="ef-label">nombre <span class="field-hint">nombre legible, puede tener símbolos como θ</span></label>
      <div class="input-row">
        <input id="ef-label" bind:value={label} placeholder="θ gravimétrico" />
        <SymbolPicker onPick={(s) => label += s} />
      </div>
    </div>
    <div class="field formula-field">
      <label for="ef-formula">fórmula <span class="field-hint">hacé clic para ver variables y constantes disponibles</span></label>
      <div class="formula-autocomplete">
        <input id="ef-formula" class="mono" bind:this={formulaEl} bind:value={formula}
               placeholder="(masa_pesaje - M_solidos) / V_suelo"
               onfocus={() => showDefPicker = true}
               oninput={() => showDefPicker = true}
               onblur={() => setTimeout(() => showDefPicker = false, 150)} />
        {#if showDefPicker && pickerDefs.length > 0}
          {@const word = (() => {
            const pos = formulaEl?.selectionStart ?? formula.length;
            const m = formula.slice(0, pos).match(/[a-zA-Z_][a-zA-Z0-9_]*$/);
            return m ? m[0].toLowerCase() : '';
          })()}
          {@const filtered = word.length >= 1
            ? pickerDefs.filter(d => d.key.toLowerCase().includes(word) || d.label.toLowerCase().includes(word))
            : pickerDefs}
          {#if filtered.length > 0}
            <div class="autocomplete-panel">
              {#each filtered as def}
                <button class="ac-item" type="button"
                        onmousedown={(e) => { e.preventDefault(); insertKey(def.key); }}>
                  <span class="ac-type ac-type--{def.type}">{def.type === 'variable' ? 'χ' : def.type === 'constant' ? 'C' : 'ƒ'}</span>
                  <span class="ac-key mono">{def.key}</span>
                  <span class="ac-label">{def.label}</span>
                  {#if def.payload?.value !== undefined}<span class="ac-val mono">{def.payload.value}</span>{/if}
                  {#if def.payload?.unit}<span class="ac-unit">{def.payload.unit}</span>{/if}
                </button>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </div>
    <div class="field row">
      <div class="field-grow">
        <label for="ef-unit">unidad <span class="field-hint">opcional</span></label>
        <div class="input-row">
          <input id="ef-unit" bind:value={unit} placeholder="%" />
          <SymbolPicker onPick={(s) => unit += s} />
        </div>
      </div>
    </div>
    <div class="field"><label for="ef-comment">comentario</label><textarea id="ef-comment" rows="2" bind:value={comment}></textarea></div>

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

  {:else if type === 'variable'}
    <h3 class="form-title">nueva variable</h3>
    <div class="form-hint">Dato que el usuario introduce en cada entry.</div>
    <div class="field">
      <label for="df-key">key <span class="field-hint">solo letras/números/guión bajo — se usa en expresiones</span></label>
      <input id="df-key" class="mono" bind:value={key} placeholder="masa_maceta" />
    </div>
    <div class="field">
      <label for="df-label">nombre <span class="field-hint">nombre legible, puede tener símbolos</span></label>
      <div class="input-row">
        <input id="df-label" bind:value={label} placeholder="Masa maceta + suelo" />
        <SymbolPicker onPick={(s) => label += s} />
      </div>
    </div>
    <div class="field">
      <label for="df-vartype">tipo</label>
      <select id="df-vartype" bind:value={varType}>
        <option value="numeric">numérico — un número por entry</option>
        <option value="vector_csv">vectorial CSV — un archivo CSV por entry (graficable)</option>
        <option value="text">texto libre</option>
        <option value="qualitative">cualitativo — selección de opciones</option>
      </select>
    </div>
    {#if varType === 'numeric'}
      <div class="field row">
        <div class="field-grow">
          <label for="df-unit">unidad <span class="field-hint">opcional, puede tener símbolos</span></label>
          <div class="input-row">
            <input id="df-unit" bind:value={unit} placeholder="g" />
            <SymbolPicker onPick={(s) => unit += s} />
          </div>
        </div>
      </div>
    {:else if varType === 'qualitative'}
      <div class="field">
        <label for="df-options">opciones (separadas por coma)</label>
        <input id="df-options" bind:value={qualOptions} placeholder="seco, húmedo, saturado" />
      </div>
    {/if}
    <div class="field"><label for="df-comment">comentario</label><textarea id="df-comment" rows="2" bind:value={comment}></textarea></div>

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
  .input-row { display: flex; gap: 6px; align-items: center; }
.input-row input { flex: 1; }
.field-hint { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-weight: 400; display: block; margin-top: 1px; }
.form-section { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
.form-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); margin-bottom: 2px; }
.form-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }
.err { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }
.field { display: flex; flex-direction: column; gap: 4px; }
.field label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
.field.row { flex-direction: row; gap: 10px; align-items: flex-end; }
.field-grow { flex: 1; display: flex; flex-direction: column; gap: 4px; }
.field-unit { width: 80px; display: flex; flex-direction: column; gap: 4px; }
.muted { color: var(--text-muted); }
input, textarea, select {
  padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale));
  border: 0.5px solid var(--border-default); border-radius: 6px;
  font-size: calc(13px * var(--font-scale)); background: var(--bg-surface);
  color: var(--text-primary); outline: none; font-family: inherit;
}
input.mono, textarea.mono { font-family: 'DM Mono', monospace; }
textarea { resize: vertical; }
.btn-row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.btn-cancel { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); background: none; border: none; cursor: pointer; padding: 6px 12px; }
.btn-save { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }

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

  .formula-autocomplete { position: relative; }
  .formula-autocomplete input { width: 100%; }
  .autocomplete-panel {
    position: absolute; top: calc(100% + 4px); left: 0; right: 0;
    z-index: 50;
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
    max-height: 220px; overflow-y: auto;
    padding: 4px;
  }
  .ac-item {
    display: flex; align-items: center; gap: 8px;
    width: 100%; padding: 6px 8px;
    border: none; background: none; cursor: pointer;
    border-radius: 4px; text-align: left;
  }
  .ac-item:hover { background: var(--interactive-hover); }
  .ac-type { font-size: calc(11px * var(--font-scale)); width: 16px; text-align: center; font-family: 'DM Mono', monospace; }
  .ac-type--variable { color: #3B6D11; }
  .ac-type--constant { color: #92400E; }
  .ac-type--expression { color: #3C3489; }
  .ac-key { font-size: calc(12px * var(--font-scale)); color: var(--text-primary); min-width: 100px; }
  .ac-label { font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); flex: 1; }
  .ac-val { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .ac-unit { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); }
</style>