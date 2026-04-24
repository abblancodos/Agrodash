<script lang="ts">
  import { experimentStore, canEdit, canAdmin } from '$lib/stores/experiment';
  import type { DefinitionGroup } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import AddDefinitionMenu from './AddDefinitionMenu.svelte';
  import ScriptEditor from './ScriptEditor.svelte';
  import DangerZone from '$lib/components/experiments/DangerZone.svelte';
  import SymbolPicker from './SymbolPicker.svelte';

  // ── Grupos ───────────────────────────────────────────────────────────────
  let showNewGroup = $state(false);
  let newGroupName = $state('');
  let newGroupColor = $state('#4a90d9');
  let editingGroupId = $state<string | null>(null);
  let editGroupName = $state('');
  let editGroupColor = $state('');
  let groupSaving = $state(false);

  const groupColors = ['#4a90d9','#3da85a','#e07b54','#7c6fcd','#e8a838','#d47cb0','#78c4b8','#8a9bb0'];

  async function createGroup() {
    if (!newGroupName.trim()) return;
    groupSaving = true;
    const res = await fetch(`${API}/api/v1/experiments/${exp.id}/groups`, {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newGroupName.trim(), color: newGroupColor }),
    });
    if (res.ok) {
      experimentStore.addGroup(await res.json());
      newGroupName = ''; showNewGroup = false;
    }
    groupSaving = false;
  }

  async function deleteGroup(gid: string) {
    if (!confirm('¿Borrar grupo? Las definitions quedan sin grupo.')) return;
    await fetch(`${API}/api/v1/experiments/${exp.id}/groups/${gid}`, {
      method: 'DELETE', credentials: 'include',
    });
    experimentStore.removeGroup(gid);
  }

  async function moveDefToGroup(defId: string, groupId: string | null) {
    await fetch(`${API}/api/v1/experiments/${exp.id}/definitions/${defId}/group`, {
      method: 'PATCH', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ group_id: groupId }),
    });
    // Update local state
    const def = $experimentStore.definitions.find(d => d.id === defId);
    if (def) experimentStore.updateDefinition({ ...def, group_id: groupId });
  }

  // ── Edición de definitions ────────────────────────────────────────────────
  let editingId = $state<string | null>(null);
  let editLabel = $state('');
  let editUnit = $state('');
  let editValue = $state('');
  let editFormula = $state('');
  let editComment = $state('');
  let editSaving = $state(false);
  let editError = $state('');
  let editFormulaEl = $state<HTMLInputElement | null>(null);
  let showEditPicker = $state(false);

  function getFilteredEditDefs() {
    if (!showEditPicker) return [];
    const pos = editFormulaEl?.selectionStart ?? editFormula.length;
    const m = editFormula.slice(0, pos).match(/[a-zA-Z_][a-zA-Z0-9_]*$/);
    const word = m ? m[0].toLowerCase() : '';
    return defs.filter(d =>
      (d.type === 'variable' || d.type === 'constant' || d.type === 'expression') &&
      (word.length < 2 || d.key.toLowerCase().includes(word) || d.label.toLowerCase().includes(word))
    );
  }

  function insertEditKey(key: string) {
    if (!editFormulaEl) { editFormula += key; return; }
    const start = editFormulaEl.selectionStart ?? editFormula.length;
    const end = editFormulaEl.selectionEnd ?? editFormula.length;
    editFormula = editFormula.slice(0, start) + key + editFormula.slice(end);
    showEditPicker = false;
    setTimeout(() => {
      editFormulaEl?.focus();
      editFormulaEl?.setSelectionRange(start + key.length, start + key.length);
    }, 10);
  }

  function startEdit(def: any) {
    editingId = def.id;
    editLabel = def.label;
    editUnit = def.payload?.unit ?? '';
    editValue = def.payload?.value?.toString() ?? '';
    editFormula = def.payload?.formula ?? '';
    editComment = def.payload?.comment ?? '';
    editError = '';
  }

  function cancelEdit() {
    editingId = null;
    editError = '';
  }

  async function saveEdit(def: any) {
    editSaving = true; editError = '';
    try {
      const payload = def.type === 'constant'
        ? { value: parseFloat(editValue) || 0, unit: editUnit, comment: editComment }
        : def.type === 'expression'
        ? { formula: editFormula, unit: editUnit, comment: editComment }
        : { ...def.payload, unit: editUnit, comment: editComment };

      const res = await fetch(`${API}/api/v1/experiments/${exp.id}/definitions/${def.id}`, {
        method: 'PUT',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ label: editLabel, payload, sort_order: def.sort_order }),
      });
      const data = await res.json();
      if (!res.ok) { editError = data.error ?? 'Error'; return; }
      experimentStore.updateDefinition(data);
      editingId = null;
    } catch (e: any) { editError = e.message; }
    finally { editSaving = false; }
  }

  let { onDeleted }: { onDeleted: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let addOpen = $state(false);

  const defs = $derived($experimentStore.definitions);
  const objs = $derived($experimentStore.objectives);
  const colls= $derived($experimentStore.collaborators);
  const exp  = $derived($experimentStore.experiment!);

  const variables   = $derived(defs.filter(d => d.type === 'variable'));
  const constants   = $derived(defs.filter(d => d.type === 'constant'));
  const expressions = $derived(defs.filter(d => d.type === 'expression'));
  const steps       = $derived(defs.filter(d => d.type === 'step'));
  const csvSchemas  = $derived(defs.filter(d => d.type === 'csv_schema'));

  const isEmpty = $derived(defs.length === 0 && objs.length === 0 && colls.length === 0);

  async function deleteDef(id: string) {
        await fetch(`${API}/api/v1/experiments/${exp.id}/definitions/${id}`, {
        credentials: 'include',
      method: 'DELETE',     });
    experimentStore.removeDefinition(id);
  }

  async function deleteObj(id: string) {
        await fetch(`${API}/api/v1/experiments/${exp.id}/objectives/${id}`, {
        credentials: 'include',
      method: 'DELETE',     });
    experimentStore.removeObjective(id);
  }
</script>

<div class="def-tab">
  <div class="def-header">
    {#if $canEdit}
      <button class="btn-add" onclick={() => addOpen = true}>
        <span class="plus-icon">+</span> agregar
      </button>
    {/if}
  </div>

  {#if isEmpty}
    <div class="empty-state">
      <p class="empty-title">el experimento no tiene definiciones todavía</p>
      <p class="empty-sub">
        Empezá agregando <strong>constantes</strong> (valores fijos como masas o volúmenes),
        luego <strong>expresiones</strong> (fórmulas calculadas), después los
        <strong>pasos</strong> (qué datos registrar) y finalmente los
        <strong>objetivos</strong> (condiciones a monitorear).
      </p>
      {#if $canEdit}
        <button class="btn-add-first" onclick={() => addOpen = true}>+ agregar primera definición</button>
      {/if}
    </div>

  {:else}
    <!-- Constantes -->
    {#if constants.length > 0}
      <section class="def-section">
        <div class="section-head">constantes</div>
        {#each constants as d (d.id)}
          <div class="def-card">
            <div class="def-card-head">
              <span class="def-type def-type--constant">constante</span>
              <span class="def-key">{d.key}</span>
              <span class="def-label">{d.label}</span>
              {#if $canAdmin}<button class="btn-del" onclick={() => deleteDef(d.id)}>✕</button>{/if}
            </div>
            <div class="def-card-body">
              <span class="def-val">{(d.payload as any).value}</span>
              {#if (d.payload as any).unit}<span class="def-unit">{(d.payload as any).unit}</span>{/if}
              {#if (d.payload as any).comment}<p class="def-comment">{(d.payload as any).comment}</p>{/if}
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <!-- Expresiones -->
    {#if expressions.length > 0}
      <section class="def-section">
        <div class="section-head">expresiones</div>
        {#each expressions as d (d.id)}
          <div class="def-card">
            <div class="def-card-head">
              <span class="def-type def-type--expression">expresión</span>
              <span class="def-key">{d.key}</span>
              <span class="def-label">{d.label}</span>
              {#if $canAdmin}<button class="btn-del" onclick={() => deleteDef(d.id)}>✕</button>{/if}
            </div>
            <div class="def-card-body">
              <code class="def-formula">{(d.payload as any).formula}</code>
              {#if (d.payload as any).unit}<span class="def-unit">{(d.payload as any).unit}</span>{/if}
              {#if (d.payload as any).comment}<p class="def-comment">{(d.payload as any).comment}</p>{/if}
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <!-- Objetivos -->
    {#if objs.length > 0}
      <section class="def-section">
        <div class="section-head">objetivos</div>
        {#each objs as o (o.id)}
          <div class="def-card">
            <div class="def-card-head">
              <span class="def-type def-type--objective">objetivo</span>
              <span class="def-label">{o.name}</span>
              <span class="sev-badge sev-{o.severity}">{o.severity}</span>
              {#if $canAdmin}<button class="btn-del" onclick={() => deleteObj(o.id)}>✕</button>{/if}
            </div>
            <div class="def-card-body">
              {#if o.condition_type === 'range'}
                {@const c = o.condition as any}
                <code class="def-formula">
                  {c.variable}
                  {c.min != null ? `≥ ${c.min}` : ''}
                  {c.max != null ? `≤ ${c.max}` : ''}
                  {c.unit ?? ''}
                </code>
              {:else}
                <code class="def-formula">{(o.condition as any).expr}</code>
              {/if}
              {#if o.goto_ok || o.goto_violation}
                <div class="goto-row">
                  {#if o.goto_ok}<span class="goto-ok">ok → {o.goto_ok}</span>{/if}
                  {#if o.goto_violation}<span class="goto-viol">viola → {o.goto_violation}</span>{/if}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <!-- Pasos -->
    {#if steps.length > 0}
      <section class="def-section">
        <div class="section-head">pasos</div>
        {#each steps as d (d.id)}
          {@const p = d.payload as any}
          <div class="def-card">
            <div class="def-card-head">
              <span class="def-type def-type--step">paso</span>
              <span class="def-key">{d.key}</span>
              <span class="def-label">{d.label}</span>
              {#if $canAdmin}<button class="btn-del" onclick={() => deleteDef(d.id)}>✕</button>{/if}
            </div>
            <div class="def-card-body">
              {#if p.fields?.length > 0}
                <div class="fields-list">
                  {#each p.fields as f}<span class="field-chip">{f.key} ({f.unit || '—'})</span>{/each}
                </div>
              {/if}
              {#if p.script}
                <ScriptEditor value={p.script} readonly={true} />
              {/if}
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <!-- CSV Schemas -->
    {#if csvSchemas.length > 0}
      <section class="def-section">
        <div class="section-head">schemas CSV</div>
        {#each csvSchemas as d (d.id)}
          {@const p = d.payload as any}
          <div class="def-card">
            <div class="def-card-head">
              <span class="def-type def-type--csv">csv schema</span>
              <span class="def-key">{d.key}</span>
              <span class="def-label">{d.label}</span>
              {#if $canAdmin}<button class="btn-del" onclick={() => deleteDef(d.id)}>✕</button>{/if}
            </div>
            <div class="def-card-body">
              <div class="fields-list">
                {#each (p.columns ?? []) as c}
                  <span class="field-chip">{c.key}{c.unit ? ` (${c.unit})` : ''}</span>
                {/each}
              </div>
            </div>
          </div>
        {/each}
      </section>
    {/if}
  {/if}

  <!-- Zona de peligro — solo admins -->
  {#if $canAdmin}
    <DangerZone
      experimentId={exp.id}
      experimentTitle={exp.title}
      {onDeleted}
    />
  {/if}
</div>

{#if addOpen}
  <AddDefinitionMenu onClose={() => addOpen = false} />
{/if}

<style>
  .def-tab { display: flex; flex-direction: column; gap: calc(24px * var(--font-scale)); }
  .def-header { display: flex; justify-content: flex-end; }
  .btn-add { display: flex; align-items: center; gap: 6px; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; color: var(--text-secondary); cursor: pointer; font-size: calc(13px * var(--font-scale)); transition: all .12s; }
  .btn-add:hover { background: var(--interactive-hover); color: var(--text-primary); }
  .plus-icon { font-size: calc(16px * var(--font-scale)); line-height: 1; }

  .empty-state { display: flex; flex-direction: column; align-items: center; gap: calc(12px * var(--font-scale)); padding: calc(48px * var(--font-scale)) 0; text-align: center; }
  .empty-title { font-size: calc(15px * var(--font-scale)); color: var(--text-secondary); }
  .empty-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); line-height: 1.6; max-width: 440px; }
  .btn-add-first { padding: calc(8px * var(--font-scale)) calc(18px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); margin-top: 4px; }

  .def-section { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .section-head { font-size: calc(11px * var(--font-scale)); font-weight: 500; color: var(--text-secondary); letter-spacing: .06em; text-transform: uppercase; margin-bottom: 2px; }

  .def-card { border: 0.5px solid var(--border-subtle); border-radius: 8px; overflow: hidden; }
  .def-card-head { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)); background: var(--bg-elevated); border-bottom: 0.5px solid var(--border-subtle); flex-wrap: wrap; }
  .def-card-body { padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }

  .def-type { font-size: calc(10px * var(--font-scale)); padding: 2px 6px; border-radius: 20px; flex-shrink: 0; }
  .def-type--constant   { background: #E6F1FB; color: #0C447C; }
  .def-type--expression { background: #EEEDFE; color: #3C3489; }
  .def-type--objective  { background: #FAEEDA; color: #854F0B; }
  .def-type--step       { background: #E1F5EE; color: #085041; }
  .def-type--csv        { background: #F1EFE8; color: #5F5E5A; }

  .def-key   { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); }
  .def-label { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .def-val   { font-size: calc(15px * var(--font-scale)); font-family: 'DM Mono', monospace; font-weight: 500; color: var(--text-primary); }
  .def-unit  { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-left: 4px; }
  .def-formula { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: calc(6px * var(--font-scale)) calc(8px * var(--font-scale)); border-radius: 4px; color: #3C3489; display: block; }
  .def-comment { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); line-height: 1.4; }

  .sev-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 6px; border-radius: 20px; }
  .sev-info     { background: #E6F1FB; color: #0C447C; }
  .sev-warning  { background: #FAEEDA; color: #854F0B; }
  .sev-critical { background: #FCEBEB; color: #A32D2D; }

  .goto-row { display: flex; gap: 10px; font-size: calc(11px * var(--font-scale)); }
  .goto-ok   { color: #3B6D11; font-family: 'DM Mono', monospace; }
  .goto-viol { color: #A32D2D; font-family: 'DM Mono', monospace; }

  .fields-list { display: flex; flex-wrap: wrap; gap: 4px; }
  .field-chip { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 2px 7px; border-radius: 4px; color: var(--text-secondary); }

  .btn-del { background: none; border: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); padding: 2px 4px; margin-left: auto; }
  .btn-del:hover { color: #A32D2D; }

  .def-type-badge { font-size: calc(10px * var(--font-scale)); padding: 1px 6px; border-radius: 20px; }
  .def-type-badge--variable { background: #EAF3DE; color: #3B6D11; }
  .edit-form { display: flex; flex-direction: column; gap: 8px; padding: 8px; background: var(--bg-elevated); border-radius: 6px; }
  .edit-err { font-size: calc(11px * var(--font-scale)); color: var(--error-color); }
  .edit-row { display: flex; gap: 8px; flex-wrap: wrap; }
  .edit-field { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 120px; }
  .edit-field--sm { flex: 0 0 80px; }
  :global(.edit-field label) { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  :global(.edit-field input) { padding: 4px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; font-size: calc(12px * var(--font-scale)); background: var(--bg-surface); color: var(--text-primary); outline: none; }
  .input-row { display: flex; gap: 4px; align-items: center; }
  :global(.input-row input) { flex: 1; }
  .edit-actions { display: flex; justify-content: flex-end; gap: 6px; }
  .btn-edit-cancel { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); background: none; border: none; cursor: pointer; padding: 4px 8px; }
  .btn-edit-save { font-size: calc(12px * var(--font-scale)); padding: 4px 12px; background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 4px; cursor: pointer; }
  .btn-edit-save:disabled { opacity: 0.5; }
  .btn-def-edit { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); background: none; border: 0.5px solid var(--border-subtle); border-radius: 4px; cursor: pointer; padding: 2px 8px; }
  .btn-def-edit:hover { color: var(--text-primary); border-color: var(--border-default); }

  .formula-autocomplete { position: relative; }
  :global(.formula-autocomplete input) { width: 100%; }
  .autocomplete-panel {
    position: absolute; top: calc(100% + 2px); left: 0; right: 0; z-index: 50;
    background: var(--bg-surface); border: 0.5px solid var(--border-default);
    border-radius: 6px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);
    max-height: 180px; overflow-y: auto; padding: 3px;
  }
  .ac-item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 5px 7px; border: none; background: none; cursor: pointer; border-radius: 4px; text-align: left; }
  .ac-item:hover { background: var(--interactive-hover); }
  .ac-type { font-size: calc(11px * var(--font-scale)); width: 14px; text-align: center; font-family: 'DM Mono', monospace; }
  .ac-type--variable { color: #3B6D11; }
  .ac-type--constant { color: #92400E; }
  .ac-type--expression { color: #3C3489; }
  .ac-key { font-size: calc(11px * var(--font-scale)); color: var(--text-primary); min-width: 80px; font-family: 'DM Mono', monospace; }
  .ac-label { font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); flex: 1; }
  .ac-val { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }

  /* Groups */
  .groups-section { display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); margin-bottom: calc(8px * var(--font-scale)); }
  .groups-head { display: flex; align-items: center; justify-content: space-between; }
  .btn-new-group { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); background: none; border: 0.5px dashed var(--border-default); border-radius: 4px; cursor: pointer; padding: 2px 8px; }
  .btn-new-group:hover { color: var(--text-primary); border-color: var(--text-muted); }
  .new-group-form { display: flex; flex-direction: column; gap: 8px; padding: 10px; background: var(--bg-elevated); border-radius: 8px; border: 0.5px solid var(--border-subtle); }
  :global(.new-group-form input) { padding: 6px 10px; border: 0.5px solid var(--border-default); border-radius: 4px; font-size: calc(13px * var(--font-scale)); background: var(--bg-surface); color: var(--text-primary); outline: none; }
  .color-picker { display: flex; gap: 6px; }
  .color-swatch { width: 20px; height: 20px; border-radius: 50%; border: 2px solid transparent; cursor: pointer; }
  .color-swatch.selected { border-color: var(--text-primary); }
  .btn-row-sm { display: flex; justify-content: flex-end; gap: 6px; }
  .btn-sec-sm { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); background: none; border: none; cursor: pointer; padding: 4px 8px; }
  .btn-pri-sm { font-size: calc(12px * var(--font-scale)); padding: 4px 12px; background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 4px; cursor: pointer; }
  .btn-pri-sm:disabled { opacity: 0.5; }
  .groups-list { display: flex; flex-direction: column; gap: 6px; }
  .group-card { border: 0.5px solid var(--border-subtle); border-radius: 8px; overflow: hidden; }
  .group-card-head { display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: var(--bg-elevated); }
  .group-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
  .group-name { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .btn-del-group { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 12px; }
  .btn-del-group:hover { color: var(--error-color); }
  .group-defs { display: flex; flex-direction: column; padding: 6px 8px; gap: 2px; }
  .group-def-item { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 4px; }
  .group-def-item:hover { background: var(--interactive-hover); }
  .group-def-type { font-size: calc(10px * var(--font-scale)); width: 14px; text-align: center; color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .group-def-key { font-size: calc(11px * var(--font-scale)); color: var(--text-primary); min-width: 80px; }
  .group-def-label { font-size: calc(11px * var(--font-scale)); color: var(--text-secondary); flex: 1; }
  .group-def-val { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .btn-ungroup { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 11px; padding: 0 4px; }
  .btn-ungroup:hover { color: var(--text-primary); }
  .group-empty { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); padding: 4px 6px; font-style: italic; }
  .group-sel { font-size: calc(11px * var(--font-scale)); padding: 2px 4px; border: 0.5px solid var(--border-subtle); border-radius: 4px; background: var(--bg-surface); color: var(--text-muted); cursor: pointer; }
</style>