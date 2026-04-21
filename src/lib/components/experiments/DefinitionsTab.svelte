<script lang="ts">
  import { experimentStore, canEdit, canAdmin } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import AddDefinitionMenu from './AddDefinitionMenu.svelte';
  import ScriptEditor from './ScriptEditor.svelte';
  import DangerZone from '$lib/components/experiments/DangerZone.svelte';

  let { onDeleted }: { onDeleted: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let addOpen = $state(false);

  const defs = $derived($experimentStore.definitions);
  const objs = $derived($experimentStore.objectives);
  const colls= $derived($experimentStore.collaborators);
  const exp  = $derived($experimentStore.experiment!);

  const constants   = $derived(defs.filter(d => d.type === 'constant'));
  const expressions = $derived(defs.filter(d => d.type === 'expression'));
  const steps       = $derived(defs.filter(d => d.type === 'step'));
  const csvSchemas  = $derived(defs.filter(d => d.type === 'csv_schema'));

  const isEmpty = $derived(defs.length === 0 && objs.length === 0 && colls.length === 0);

  async function deleteDef(id: string) {
    const token = auth.getToken();
    await fetch(`${API}/api/v1/experiments/${exp.id}/definitions/${id}`, {
      method: 'DELETE', headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    experimentStore.removeDefinition(id);
  }

  async function deleteObj(id: string) {
    const token = auth.getToken();
    await fetch(`${API}/api/v1/experiments/${exp.id}/objectives/${id}`, {
      method: 'DELETE', headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
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
</style>
