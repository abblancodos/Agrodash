<!-- src/lib/components/experiments/EntriesTab.svelte -->
<script lang="ts">
  import { experimentStore, canEdit, canAdmin, activeEvents } from '$lib/stores/experiment';
  import type { ExperimentColumn, Definition } from '$lib/stores/experiment';
  import ConflictBanner from './ConflictBanner.svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  const columns = $derived(
    ($experimentStore.experiment?.columns ?? [])
      .filter(c => c.visible)
      .sort((a, b) => a.order - b.order)
  );

  const variables = $derived(
    $experimentStore.definitions.filter(d =>
      d.type === 'variable' || d.type === 'expression' || d.type === 'constant'
    )
  );

  const addedKeys = $derived(new Set(columns.map(c => c.key)));
  const entryValues = $derived($experimentStore.entryValues);
  const events = $derived($activeEvents);

  let addingEntry = $state(false);
  let newRowValues = $state<Record<string, string>>({});
  let saving = $state(false);
  let error = $state('');
  let dragOver = $state<number | null>(null);
  let dragCol = $state<number | null>(null);

  async function addColumn(def: Definition) {
    const exp = $experimentStore.experiment!;
    const newColumns: ExperimentColumn[] = [
      ...columns,
      { key: def.key, type: def.type === 'variable' ? 'variable' : def.type === 'expression' ? 'expression' : 'constant', order: columns.length, visible: true }
    ];
    await fetch(`${API}/api/v1/experiments/${exp.id}/columns`, {
      method: 'PATCH', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ columns: newColumns }),
    });
    experimentStore.updateColumns(newColumns);
  }

  async function saveEntry() {
    saving = true; error = '';
    const exp = $experimentStore.experiment!;
    try {
      const evRes = await fetch(`${API}/api/v1/experiments/${exp.id}/events`, {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ step_key: 'entry', event_type: 'measurement', data: {} }),
      });
      if (!evRes.ok) { error = 'Error creando entry'; return; }
      const event = await evRes.json();

      const values = columns
        .filter(c => c.type === 'variable')
        .map(c => {
          const def = variables.find(d => d.key === c.key);
          const raw = newRowValues[c.key] ?? '';
          if (def?.var_type === 'numeric') return { definition_key: c.key, value_numeric: parseFloat(raw) || null, value_text: null, value_csv_data: null };
          return { definition_key: c.key, value_numeric: null, value_text: raw || null, value_csv_data: null };
        })
        .filter(v => v.value_numeric !== null || (v.value_text !== null && v.value_text !== ''));

      if (values.length > 0) {
        await fetch(`${API}/api/v1/experiments/${exp.id}/events/${event.id}/values`, {
          method: 'POST', credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(values),
        });
        experimentStore.setEntryValues(event.id, Object.fromEntries(
          values.map(v => [v.definition_key, v.value_numeric ?? v.value_text])
        ));
      }

      experimentStore.addEvent(event);
      newRowValues = {};
      addingEntry = false;
    } catch (e: any) { error = e.message; }
    finally { saving = false; }
  }

  function onDragStart(i: number) { dragCol = i; }
  function onDragEnd() { dragCol = null; dragOver = null; }
  async function onDrop(targetI: number) {
    if (dragCol === null || dragCol === targetI) return;
    const reordered = [...columns];
    const [moved] = reordered.splice(dragCol, 1);
    reordered.splice(targetI, 0, moved);
    const updated = reordered.map((c, i) => ({ ...c, order: i }));
    const exp = $experimentStore.experiment!;
    await fetch(`${API}/api/v1/experiments/${exp.id}/columns`, {
      method: 'PATCH', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ columns: updated }),
    });
    experimentStore.updateColumns(updated);
    dragCol = null; dragOver = null;
  }

  function getCellValue(entryId: string, key: string): string {
    const v = entryValues[entryId]?.[key];
    if (v === null || v === undefined) return '—';
    if (typeof v === 'number') return v.toFixed(6).replace(/\.?0+$/, '');
    return String(v);
  }

  function calcExpression(entryId: string, def: Definition): string {
    const ctx: Record<string, number> = {};
    const consts = $experimentStore.experiment?.constants ?? {};
    for (const [k, v] of Object.entries(consts)) if (typeof v === 'number') ctx[k] = v;
    for (const d of $experimentStore.definitions.filter(d => d.type === 'constant')) {
      const val = (d.payload as any).value;
      if (typeof val === 'number') ctx[d.key] = val;
    }
    const vals = entryValues[entryId] ?? {};
    for (const [k, v] of Object.entries(vals)) if (typeof v === 'number') ctx[k] = v;
    const formula = (def.payload as any).formula as string;
    if (!formula) return '—';
    const keys = Object.keys(ctx).sort((a, b) => b.length - a.length);
    let expr = formula;
    for (const k of keys) expr = expr.replace(new RegExp(`\\b${k}\\b`, 'g'), String(ctx[k]));
    if (!/^[\d\s\+\-\*\/\.\(\)\^%]+$/.test(expr)) return '?';
    try {
      const result = new Function(`return (${expr.replace(/\^/g, '**')})`)();
      return typeof result === 'number' && isFinite(result) ? result.toFixed(6).replace(/\.?0+$/, '') : '?';
    } catch { return '?'; }
  }
</script>

<div class="entries-tab">
  <ConflictBanner />
  {#if error}<div class="err">{error}</div>{/if}

  {#if columns.length === 0}
    <div class="empty-state">
      <p class="empty-title">no hay columnas configuradas</p>
      <p class="empty-sub">Definí variables en <em>definitions</em> y agregalas como columnas.</p>
      {#if $canEdit && variables.length > 0}
        <div class="col-picker">
          {#each variables as def}
            <button class="col-chip" onclick={() => addColumn(def)}>+ {def.label}</button>
          {/each}
        </div>
      {/if}
    </div>

  {:else}
    <div class="table-wrap">
      <table class="t">
        <thead>
          <tr>
            <th class="th-ts">timestamp</th>
            {#each columns as col, i (col.key)}
              {@const def = variables.find(d => d.key === col.key)}
              <th class="th-col" class:dragging={dragCol === i} class:dragover={dragOver === i}
                  draggable={$canEdit}
                  ondragstart={() => onDragStart(i)} ondragend={onDragEnd}
                  ondragover={(e) => { e.preventDefault(); dragOver = i; }}
                  ondrop={() => onDrop(i)}>
                {def?.label ?? col.key}
                {#if def?.var_type === 'numeric' && (def.payload as any).unit}
                  <span class="unit">({(def.payload as any).unit})</span>
                {/if}
                {#if col.type === 'expression'}<span class="badge-expr">ƒ</span>{/if}
              </th>
            {/each}
            {#if $canEdit}
              <th class="th-add">
                <div class="add-wrap">
                  <button class="btn-plus">+</button>
                  <div class="dropdown">
                    {#each variables.filter(d => !addedKeys.has(d.key)) as def}
                      <button class="opt" onclick={() => addColumn(def)}>
                        <span class="opt-type">{def.var_type ?? def.type}</span>{def.label}
                      </button>
                    {:else}
                      <span class="opt-empty">todas agregadas</span>
                    {/each}
                  </div>
                </div>
              </th>
            {/if}
            <th class="th-act"></th>
          </tr>
        </thead>
        <tbody>
          {#each events as ev (ev.id)}
            <tr class="row" class:voided={ev.is_voided}>
              <td class="td-ts">{new Date(ev.recorded_at).toLocaleString('es-CR',{day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit',hour12:false})}</td>
              {#each columns as col}
                <td class="td-val" class:td-expr={col.type === 'expression'}>
                  {#if col.type === 'expression'}
                    {@const def = $experimentStore.definitions.find(d => d.key === col.key)}
                    {def ? calcExpression(ev.id, def) : '—'}
                  {:else}
                    {getCellValue(ev.id, col.key)}
                  {/if}
                </td>
              {/each}
              {#if $canEdit}<td></td>{/if}
              <td class="td-act">
                {#if $canAdmin}<button class="btn-act">···</button>{/if}
              </td>
            </tr>
          {/each}

          {#if addingEntry}
            <tr class="row row--new">
              <td class="td-ts muted">ahora</td>
              {#each columns as col}
                <td class="td-val">
                  {#if col.type === 'variable'}
                    {@const def = variables.find(d => d.key === col.key)}
                    {#if def?.var_type === 'qualitative' && def.options?.length}
                      <select class="cell-in" bind:value={newRowValues[col.key]}>
                        <option value="">—</option>
                        {#each def.options as opt}<option value={opt}>{opt}</option>{/each}
                      </select>
                    {:else if def?.var_type === 'vector_csv'}
                      <input type="file" class="cell-in" accept=".csv" />
                    {:else}
                      <input class="cell-in" type="text" bind:value={newRowValues[col.key]} placeholder="0.000" inputmode="decimal" />
                    {/if}
                  {:else}
                    <span class="muted">auto</span>
                  {/if}
                </td>
              {/each}
              {#if $canEdit}<td></td>{/if}
              <td class="td-act">
                <button class="btn-ok" disabled={saving} onclick={saveEntry}>{saving ? '...' : '✓'}</button>
                <button class="btn-x" onclick={() => { addingEntry = false; newRowValues = {}; }}>✕</button>
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    {#if $canEdit && !addingEntry}
      <button class="btn-add-row" onclick={() => addingEntry = true}>+ nueva entry</button>
    {/if}
  {/if}
</div>

<style>
  .entries-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }
  .err { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }

  .empty-state { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 40px 0; text-align: center; }
  .empty-title { font-size: calc(15px * var(--font-scale)); color: var(--text-secondary); }
  .empty-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); }
  .col-picker { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .col-chip { font-size: calc(12px * var(--font-scale)); padding: 4px 10px; border: 0.5px solid var(--border-default); border-radius: 20px; background: none; cursor: pointer; color: var(--text-secondary); }
  .col-chip:hover { background: var(--interactive-hover); }

  .table-wrap { overflow-x: auto; border: 0.5px solid var(--border-subtle); border-radius: 8px; }
  .t { width: 100%; border-collapse: collapse; font-size: calc(12px * var(--font-scale)); }
  th { background: var(--bg-elevated); padding: calc(8px * var(--font-scale)) calc(10px * var(--font-scale)); text-align: left; font-weight: 500; border-bottom: 0.5px solid var(--border-subtle); white-space: nowrap; }
  .th-ts { color: var(--text-muted); min-width: 110px; }
  .th-col { color: var(--text-secondary); cursor: grab; user-select: none; }
  .th-col.dragging { opacity: 0.5; }
  .th-col.dragover { border-left: 2px solid var(--text-primary); }
  .unit { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-left: 3px; }
  .badge-expr { font-size: calc(10px * var(--font-scale)); padding: 1px 5px; border-radius: 20px; background: #EEEDFE; color: #3C3489; margin-left: 4px; }
  .th-add { width: 40px; text-align: center; }
  .th-act { width: 60px; }

  .add-wrap { position: relative; display: inline-block; }
  .btn-plus { width: 24px; height: 24px; border-radius: 50%; border: 0.5px dashed var(--border-default); background: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .btn-plus:hover { border-color: var(--text-muted); color: var(--text-primary); }
  .dropdown { display: none; position: absolute; top: 100%; right: 0; z-index: 20; background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 8px; min-width: 200px; padding: 4px; }
  .add-wrap:hover .dropdown { display: flex; flex-direction: column; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 6px 10px; background: none; border: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-primary); border-radius: 4px; text-align: left; }
  .opt:hover { background: var(--interactive-hover); }
  .opt-type { font-size: calc(10px * var(--font-scale)); padding: 1px 5px; border-radius: 20px; background: var(--bg-elevated); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .opt-empty { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); padding: 6px 10px; }

  .row td { padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); vertical-align: middle; }
  .row:last-child td { border-bottom: none; }
  .row:hover td { background: var(--interactive-hover); }
  .row.voided td { opacity: 0.4; text-decoration: line-through; }
  .row--new td { background: #EAF3DE18; }

  .td-ts { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); white-space: nowrap; }
  .td-val { font-family: 'DM Mono', monospace; }
  .td-expr { color: #185FA5; }
  .td-act { white-space: nowrap; }
  .muted { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }

  .cell-in { width: 100%; min-width: 80px; padding: 3px 6px; border: 0.5px solid var(--border-default); border-radius: 4px; font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-surface); color: var(--text-primary); outline: none; }
  .cell-in:focus { border-color: var(--text-primary); }
  .btn-act { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 2px 6px; letter-spacing: .1em; }
  .btn-ok { background: #3B6D11; color: #fff; border: none; border-radius: 4px; cursor: pointer; padding: 3px 8px; font-size: calc(12px * var(--font-scale)); }
  .btn-ok:disabled { opacity: 0.5; }
  .btn-x { background: none; border: none; cursor: pointer; color: var(--text-muted); padding: 3px 6px; font-size: calc(12px * var(--font-scale)); }
  .btn-add-row { display: flex; align-items: center; gap: 6px; width: fit-content; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .btn-add-row:hover { border-color: var(--text-muted); color: var(--text-primary); }
</style>