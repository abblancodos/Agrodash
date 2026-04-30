<!-- src/lib/components/experiments/EntriesTab.svelte -->
<script lang="ts">
  import { experimentStore, canEdit, canAdmin, activeEvents, groups } from '$lib/stores/experiment';
  import type { ExperimentColumn, Definition } from '$lib/stores/experiment';
  import ConflictBanner from './ConflictBanner.svelte';
  import CsvImporter from './CsvImporter.svelte';
  import CorrectionForm from './CorrectionForm.svelte';
  import type { ExperimentEvent } from '$lib/stores/experiment';

  let showImporter = $state(false);
  const API = import.meta.env.VITE_API_BASE ?? '';

  // ── Modo: 'view' | 'edit' ────────────────────────────────────────────────
  let mode = $state<'view' | 'edit'>('view');

  // ── Datos derivados ──────────────────────────────────────────────────────
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

  const addedKeys   = $derived(new Set(columns.map(c => c.key)));
  const hasGroupCol = $derived(columns.some(c => c.type === 'group'));
  const entryValues = $derived($experimentStore.entryValues);
  const events      = $derived($activeEvents);

  // ── Sort (solo en modo view) ─────────────────────────────────────────────
  let sortKey = $state<string | null>(null);   // null = timestamp
  let sortDir = $state<'asc' | 'desc'>('asc');

  function toggleSort(key: string | null) {
    if (sortKey === key) sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    else { sortKey = key; sortDir = 'asc'; }
  }

  const sortedEvents = $derived(() => {
    const evs = [...events];
    if (!sortKey) {
      return sortDir === 'asc' ? evs : evs.reverse();
    }
    return evs.sort((a, b) => {
      const va = getNumeric(a.id, sortKey!);
      const vb = getNumeric(b.id, sortKey!);
      if (va === null && vb === null) return 0;
      if (va === null) return 1;
      if (vb === null) return -1;
      return sortDir === 'asc' ? va - vb : vb - va;
    });
  });

  function getNumeric(entryId: string, key: string): number | null {
    const v = entryValues[entryId]?.[key];
    if (typeof v === 'number') return v;
    // intentar calcular expresión
    const def = $experimentStore.definitions.find(d => d.key === key && d.type === 'expression');
    if (def) {
      const r = calcExpression(entryId, def);
      const n = parseFloat(r);
      return isNaN(n) ? null : n;
    }
    return null;
  }

  // ── Resize de columnas ────────────────────────────────────────────────────
  // ancho local en px, se sincroniza con el store al soltar
  let colWidths = $state<Record<string, number>>({});

  $effect(() => {
    // inicializar desde el store cuando cambian columns
    const w: Record<string, number> = {};
    for (const c of columns) {
      w[c.key] = (c as any).width ?? 120;
    }
    colWidths = w;
  });

  let resizing = $state<string | null>(null);  // key de columna en resize
  let resizeStartX = 0;
  let resizeStartW = 0;

  function onResizeStart(e: MouseEvent, key: string) {
    e.preventDefault();
    e.stopPropagation();
    resizing = key;
    resizeStartX = e.clientX;
    resizeStartW = colWidths[key] ?? 120;

    function onMove(ev: MouseEvent) {
      if (!resizing) return;
      const delta = ev.clientX - resizeStartX;
      colWidths = { ...colWidths, [resizing]: Math.max(60, resizeStartW + delta) };
    }

    function onUp() {
      if (resizing) saveWidths();
      resizing = null;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  let saveWidthTimer: ReturnType<typeof setTimeout> | null = null;
  function saveWidths() {
    if (saveWidthTimer) clearTimeout(saveWidthTimer);
    saveWidthTimer = setTimeout(async () => {
      const exp = $experimentStore.experiment!;
      const updated = columns.map(c => ({ ...c, width: colWidths[c.key] ?? (c as any).width ?? 120 }));
      await fetch(`${API}/api/v1/experiments/${exp.id}/columns`, {
        method: 'PATCH', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ columns: updated }),
      });
      experimentStore.updateColumns(updated);
    }, 400);
  }

  // ── Drag reorder (solo en modo edit) ─────────────────────────────────────
  let dragCol           = $state<number | null>(null);
  let dragOver          = $state<number | null>(null);
  let draggingOverTrash = $state(false);

  function onDragStart(i: number) { dragCol = i; draggingOverTrash = false; }
  function onDragEnd()  { dragCol = null; dragOver = null; draggingOverTrash = false; }

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

  async function removeColumn(i: number) {
    const exp = $experimentStore.experiment!;
    const updated = columns
      .filter((_, idx) => idx !== i)
      .map((c, idx) => ({ ...c, order: idx }));
    await fetch(`${API}/api/v1/experiments/${exp.id}/columns`, {
      method: 'PATCH', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ columns: updated }),
    });
    experimentStore.updateColumns(updated);
    dragCol = null; dragOver = null; draggingOverTrash = false;
  }

  async function onDropTrash() {
    if (dragCol === null) return;
    await removeColumn(dragCol);
  }

  // ── Dropdown añadir columna ───────────────────────────────────────────────
  let dropdownOpen     = $state(false);
  let dropdownStyle    = $state('');
  let dropdownBtnEl    = $state<HTMLButtonElement | null>(null);
  let dropdownPortalEl = $state<HTMLDivElement | null>(null);

  function openDropdown() {
    if (!dropdownBtnEl) return;
    const rect = dropdownBtnEl.getBoundingClientRect();
    let left = rect.right - 220;
    if (left < 8) left = rect.left;
    dropdownStyle = `left:${left}px; top:${rect.bottom + 4}px;`;
    dropdownOpen = true;
  }
  function closeDropdown() { dropdownOpen = false; }

  $effect(() => {
    if (dropdownPortalEl) {
      document.body.appendChild(dropdownPortalEl);
      return () => { dropdownPortalEl?.remove(); };
    }
  });

  $effect(() => {
    function handler(e: MouseEvent) {
      if (!dropdownOpen) return;
      const t = e.target as Node;
      if (!dropdownBtnEl?.contains(t) && !dropdownPortalEl?.contains(t)) dropdownOpen = false;
    }
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  });

  // ── Menú de acciones de entry (···) ─────────────────────────────────────
  let actionMenuOpen     = $state(false);
  let actionMenuStyle    = $state('');
  let actionMenuEvent    = $state<ExperimentEvent | null>(null);
  let actionMenuPortalEl = $state<HTMLDivElement | null>(null);
  let showCorrection     = $state(false);
  let actionLoading      = $state(false);

  function openActionMenu(e: MouseEvent, ev: ExperimentEvent) {
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    let left = rect.right - 160;
    if (left < 8) left = rect.left;
    let top = rect.bottom + 4;
    if (top + 120 > window.innerHeight) top = rect.top - 120;
    actionMenuStyle = `left:${left}px; top:${top}px;`;
    actionMenuEvent = ev;
    actionMenuOpen = true;
  }

  function closeActionMenu() { actionMenuOpen = false; actionMenuEvent = null; }

  $effect(() => {
    if (actionMenuPortalEl) {
      document.body.appendChild(actionMenuPortalEl);
      return () => { actionMenuPortalEl?.remove(); };
    }
  });

  $effect(() => {
    function handler(e: MouseEvent) {
      if (!actionMenuOpen) return;
      if (!actionMenuPortalEl?.contains(e.target as Node)) closeActionMenu();
    }
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  });

  async function voidEntry() {
    if (!actionMenuEvent) return;
    actionLoading = true;
    const ev = actionMenuEvent;
    closeActionMenu();
    await fetch(`${API}/api/v1/experiments/${ev.experiment_id}/events/${ev.id}/void`, {
      method: 'POST', credentials: 'include',
    });
    experimentStore.voidEvent(ev.id);
    actionLoading = false;
  }

  async function deleteEntry() {
    if (!actionMenuEvent) return;
    if (!confirm('¿Eliminar esta entry permanentemente? Esta acción no se puede deshacer.')) return;
    actionLoading = true;
    const ev = actionMenuEvent;
    closeActionMenu();
    await fetch(`${API}/api/v1/experiments/${ev.experiment_id}/events/${ev.id}`, {
      method: 'DELETE', credentials: 'include',
    });
    experimentStore.deleteEvent(ev.id);
    actionLoading = false;
  }

  async function addGroupColumn() {
    const exp = $experimentStore.experiment!;
    if (hasGroupCol) return;
    const newColumns: ExperimentColumn[] = [
      ...columns,
      { key: '_group', type: 'group' as any, order: columns.length, visible: true }
    ];
    await fetch(`${API}/api/v1/experiments/${exp.id}/columns`, {
      method: 'PATCH', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ columns: newColumns }),
    });
    experimentStore.updateColumns(newColumns);
  }

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

  // ── Nueva entry ───────────────────────────────────────────────────────────
  let addingEntry  = $state(false);
  let newRowValues = $state<Record<string, string>>({});
  let saving       = $state(false);
  let error        = $state('');

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

  // ── Helpers de celda ──────────────────────────────────────────────────────
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
    for (const d of $experimentStore.definitions.filter(d => d.type === 'constant' && !d.group_id)) {
      const val = (d.payload as any).value;
      if (typeof val === 'number') ctx[d.key] = val;
    }
    const ev = events.find(e => e.id === entryId);
    if (ev?.group_id) {
      for (const d of $experimentStore.definitions.filter(d => d.type === 'constant' && d.group_id === ev.group_id)) {
        const val = (d.payload as any).value;
        if (typeof val === 'number') ctx[d.key] = val;
      }
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

  // ancho en px de cada columna como style string
  function colStyle(key: string): string {
    const w = colWidths[key] ?? 120;
    return `width:${w}px; min-width:${w}px; max-width:${w}px;`;
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
    <div class="table-outer">

      <!-- Toolbar de modo -->
      {#if $canEdit}
        <div class="mode-bar">
          <button class="mode-btn" class:active={mode === 'view'} onclick={() => mode = 'view'}>
            ↕ ver y ordenar
          </button>
          <button class="mode-btn" class:active={mode === 'edit'} onclick={() => { mode = 'edit'; addingEntry = false; }}>
            ⠿ editar columnas
          </button>
          {#if mode === 'edit'}
            <span class="mode-hint">arrastrá para reordenar · soltá en 🗑 para quitar</span>
          {:else if sortKey}
            <button class="sort-clear" onclick={() => { sortKey = null; sortDir = 'asc'; }}>
              limpiar orden ✕
            </button>
          {/if}
        </div>
      {/if}

      <!-- Trash zone (solo en modo edit mientras arrastrás) -->
      {#if mode === 'edit' && dragCol !== null}
        <div class="trash-zone" role="region" aria-label="Zona de eliminación" class:trash-over={draggingOverTrash}
          ondragover={(e) => { e.preventDefault(); draggingOverTrash = true; }}
          ondragleave={() => draggingOverTrash = false}
          ondrop={onDropTrash}>
          <span class="trash-icon">🗑</span>
          <span class="trash-label">{draggingOverTrash ? 'soltar para quitar columna' : 'arrastrá aquí para quitar'}</span>
        </div>
      {/if}

      <div class="table-scroll">
        <table class="t">
          <colgroup>
            <col style="width:110px; min-width:110px;" />
            {#each columns as col}
              <col style={colStyle(col.key)} />
            {/each}
            {#if mode === 'edit' && $canEdit}<col style="width:40px;" />{/if}
            <col style="width:50px;" />
          </colgroup>

          <thead>
            <tr>
              <!-- Timestamp header -->
              <th class="th-ts" class:th-sortable={mode === 'view'}
                  onclick={() => mode === 'view' && toggleSort(null)}>
                <span class="th-inner">
                  timestamp
                  {#if mode === 'view' && sortKey === null}
                    <span class="sort-arrow">{sortDir === 'asc' ? '↑' : '↓'}</span>
                  {/if}
                </span>
              </th>

              <!-- Columnas de datos -->
              {#each columns as col, i (col.key)}
                {@const def = variables.find(d => d.key === col.key)}
                <th class="th-col"
                    class:th-sortable={mode === 'view'}
                    class:th-draggable={mode === 'edit'}
                    class:dragging={mode === 'edit' && dragCol === i}
                    class:dragover={mode === 'edit' && dragOver === i}
                    draggable={mode === 'edit'}
                    ondragstart={() => onDragStart(i)}
                    ondragend={onDragEnd}
                    ondragover={(e) => { if (mode === 'edit') { e.preventDefault(); dragOver = i; } }}
                    ondrop={() => mode === 'edit' && onDrop(i)}
                    onclick={() => mode === 'view' && toggleSort(col.key)}>
                  <span class="th-inner">
                    {#if mode === 'edit'}<span class="drag-handle">⠿</span>{/if}
                    <span class="th-label">{def?.label ?? col.key}</span>
                    {#if def?.var_type === 'numeric' && (def.payload as any).unit}
                      <span class="unit">({(def.payload as any).unit})</span>
                    {/if}
                    {#if col.type === 'expression'}<span class="badge-expr">ƒ</span>{/if}
                    {#if mode === 'view' && sortKey === col.key}
                      <span class="sort-arrow">{sortDir === 'asc' ? '↑' : '↓'}</span>
                    {/if}
                  </span>
                  <!-- Resize handle (solo en modo view) -->
                  {#if mode === 'view'}
                    <span class="resize-handle"
                      onmousedown={(e) => onResizeStart(e, col.key)}
                      role="separator" aria-label="Redimensionar columna">
                    </span>
                  {/if}
                </th>
              {/each}

              <!-- Botón añadir columna (solo en modo edit) -->
              {#if mode === 'edit' && $canEdit}
                <th class="th-add">
                  <button bind:this={dropdownBtnEl} class="btn-plus" onclick={openDropdown}>+</button>
                </th>
              {/if}
              <th class="th-act"></th>
            </tr>
          </thead>

          <tbody>
            {#each sortedEvents() as ev (ev.id)}
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
                {#if mode === 'edit' && $canEdit}<td></td>{/if}
                <td class="td-act">
                  <button class="btn-act" onclick={(e) => openActionMenu(e, ev)}>···</button>
                </td>
              </tr>
            {/each}

            {#if addingEntry && mode === 'view'}
              <tr class="row row--new">
                <td class="td-ts muted">ahora</td>
                {#each columns as col}
                  <td class="td-val">
                    {#if col.type === 'group'}
                      <select class="cell-in" bind:value={newRowValues['_group']}>
                        <option value="">sin grupo</option>
                        {#each $groups as g}<option value={g.id}>{g.name}</option>{/each}
                      </select>
                    {:else if col.type === 'variable'}
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
                <td class="td-act">
                  <button class="btn-ok" disabled={saving} onclick={saveEntry}>{saving ? '...' : '✓'}</button>
                  <button class="btn-x" onclick={() => { addingEntry = false; newRowValues = {}; }}>✕</button>
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>

      {#if $canEdit && mode === 'view'}
        <div class="entry-actions">
          {#if !addingEntry}
            <button class="btn-add-row" onclick={() => addingEntry = true}>+ nueva entry</button>
          {/if}
          <button class="btn-import" onclick={() => showImporter = true}>↑ importar CSV</button>
        </div>
      {/if}
    </div>

  {/if}

  {#if showImporter}
    <div class="overlay" onclick={() => showImporter = false} role="presentation">
      <div class="import-panel" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1"
           onkeydown={(e) => e.key === 'Escape' && (showImporter = false)}>
        <div class="import-head">
          <span>importar entries desde CSV</span>
          <button class="btn-close-imp" onclick={() => showImporter = false}>✕</button>
        </div>
        <div class="import-body">
          <CsvImporter onClose={() => showImporter = false} />
        </div>
      </div>
    </div>
  {/if}

  <!-- Menú de acciones (···) — portal al body -->
  {#if actionMenuOpen && actionMenuEvent}
    <div bind:this={actionMenuPortalEl} class="action-menu-portal" style={actionMenuStyle}>
      {#if !actionMenuEvent.is_voided}
        <button class="amenu-opt" onclick={() => { showCorrection = true; closeActionMenu(); }}>
          <span class="amenu-icon">✎</span> corregir
        </button>
        {#if $canEdit}
          <button class="amenu-opt amenu-opt--warn" onclick={voidEntry}>
            <span class="amenu-icon">⊘</span> anular
          </button>
        {/if}
      {/if}
      {#if $canAdmin}
        <div class="amenu-sep"></div>
        <button class="amenu-opt amenu-opt--danger" onclick={deleteEntry}>
          <span class="amenu-icon">✕</span> eliminar
        </button>
      {/if}
    </div>
  {/if}

  <!-- Modal de corrección -->
  {#if showCorrection && actionMenuEvent}
    <CorrectionForm event={actionMenuEvent} onClose={() => { showCorrection = false; actionMenuEvent = null; }} />
  {/if}

  <!-- Dropdown añadir columna — portal al body -->
  {#if dropdownOpen}
    <div bind:this={dropdownPortalEl} class="dropdown-portal" style={dropdownStyle}>
      {#if !hasGroupCol && $groups.length > 0}
        <button class="opt" onclick={() => { addGroupColumn(); closeDropdown(); }}>
          <span class="opt-type opt-type--var">⊞</span>grupo de suelo
        </button>
      {/if}
      {#each variables.filter(d => !addedKeys.has(d.key)) as def}
        <button class="opt" onclick={() => { addColumn(def); closeDropdown(); }}>
          <span class="opt-type opt-type--{def.type === 'expression' ? 'expr' : def.type === 'constant' ? 'const' : 'var'}">{def.type === 'expression' ? 'ƒ' : def.type === 'constant' ? 'C' : 'χ'}</span>{def.label}
        </button>
      {:else}
        {#if hasGroupCol || $groups.length === 0}<span class="opt-empty">todas agregadas</span>{/if}
      {/each}
    </div>
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

  /* ── Mode bar ─────────────────────────────────────────────────────────── */
  .table-outer { display: flex; flex-direction: column; gap: 6px; }
  .mode-bar { display: flex; align-items: center; gap: 6px; }
  .mode-btn {
    font-size: calc(11px * var(--font-scale)); padding: 4px 10px;
    border: 0.5px solid var(--border-default); border-radius: 6px;
    background: none; cursor: pointer; color: var(--text-muted);
    font-family: 'DM Mono', monospace; transition: all .12s;
  }
  .mode-btn:hover { color: var(--text-secondary); background: var(--interactive-hover); }
  .mode-btn.active { background: var(--bg-elevated); color: var(--text-primary); border-color: var(--border-strong, var(--border-default)); }
  .mode-hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); margin-left: 4px; }
  .sort-clear { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); background: none; border: none; cursor: pointer; margin-left: 4px; }
  .sort-clear:hover { color: var(--text-primary); }

  /* ── Trash zone ───────────────────────────────────────────────────────── */
  .trash-zone {
    display: flex; align-items: center; gap: 8px;
    border: 1.5px dashed var(--border-default); border-radius: 8px;
    padding: 8px 14px; color: var(--text-muted);
    font-size: calc(12px * var(--font-scale)); transition: all .12s;
    background: var(--bg-elevated);
  }
  .trash-zone.trash-over { border-color: #A32D2D; background: #FCEBEB; color: #A32D2D; }
  .trash-icon { font-size: 16px; }
  .trash-label { flex: 1; }

  /* ── Table ────────────────────────────────────────────────────────────── */
  .table-scroll { overflow-x: auto; border: 0.5px solid var(--border-subtle); border-radius: 8px; }
  .t { width: max-content; min-width: 100%; border-collapse: collapse; font-size: calc(12px * var(--font-scale)); table-layout: fixed; }

  th {
    background: var(--bg-elevated);
    padding: calc(8px * var(--font-scale)) calc(10px * var(--font-scale));
    text-align: left; font-weight: 500;
    border-bottom: 0.5px solid var(--border-subtle);
    position: relative; overflow: hidden;
    user-select: none;
  }

  .th-inner { display: flex; align-items: center; gap: 4px; overflow: hidden; }
  .th-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0; }

  .th-ts { color: var(--text-muted); }
  .th-col { color: var(--text-secondary); }
  .th-sortable { cursor: pointer; }
  .th-sortable:hover .th-label { color: var(--text-primary); }
  .th-draggable { cursor: grab; }
  .th-col.dragging { opacity: 0.4; }
  .th-col.dragover { box-shadow: inset 2px 0 0 var(--text-primary); }

  .drag-handle { color: var(--text-muted); font-size: 12px; flex-shrink: 0; cursor: grab; }
  .sort-arrow { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); flex-shrink: 0; }
  .unit { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; flex-shrink: 0; }
  .badge-expr { font-size: calc(10px * var(--font-scale)); padding: 1px 4px; border-radius: 20px; background: #EEEDFE; color: #3C3489; flex-shrink: 0; }

  /* Resize handle — banda derecha del th */
  .resize-handle {
    position: absolute; right: 0; top: 0; bottom: 0;
    width: 5px; cursor: col-resize;
    background: transparent;
    transition: background .12s;
  }
  .resize-handle:hover { background: var(--border-default); }

  .th-add { width: 40px; text-align: center; }
  .th-act { width: 50px; }

  /* ── Rows ─────────────────────────────────────────────────────────────── */
  .row td {
    padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    vertical-align: top;
    overflow: hidden;
    word-break: break-word;
  }
  .row:last-child td { border-bottom: none; }
  .row:hover td { background: var(--interactive-hover); }
  .row.voided td { opacity: 0.4; text-decoration: line-through; }
  .row--new td { background: #EAF3DE18; }

  .td-ts { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); white-space: nowrap; vertical-align: middle !important; }
  .td-val { font-family: 'DM Mono', monospace; }
  .td-expr { color: #185FA5; }
  .td-act { white-space: nowrap; vertical-align: middle !important; }
  .muted { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }

  /* ── Entry form ───────────────────────────────────────────────────────── */
  .cell-in { width: 100%; padding: 3px 6px; border: 0.5px solid var(--border-default); border-radius: 4px; font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-surface); color: var(--text-primary); outline: none; box-sizing: border-box; }
  .cell-in:focus { border-color: var(--text-primary); }

  .btn-act { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 2px 6px; letter-spacing: .1em; }
  .btn-ok  { background: #3B6D11; color: #fff; border: none; border-radius: 4px; cursor: pointer; padding: 3px 8px; font-size: calc(12px * var(--font-scale)); }
  .btn-ok:disabled { opacity: 0.5; }
  .btn-x   { background: none; border: none; cursor: pointer; color: var(--text-muted); padding: 3px 6px; font-size: calc(12px * var(--font-scale)); }

  .entry-actions { display: flex; gap: 8px; align-items: center; }
  .btn-add-row { display: flex; align-items: center; gap: 6px; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .btn-add-row:hover { border-color: var(--text-muted); color: var(--text-primary); }
  .btn-import { display: flex; align-items: center; gap: 6px; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .btn-import:hover { background: var(--interactive-hover); color: var(--text-primary); }

  /* ── Dropdown portal ─────────────────────────────────────────────────── */
  .btn-plus { width: 24px; height: 24px; border-radius: 50%; border: 0.5px dashed var(--border-default); background: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .btn-plus:hover { border-color: var(--text-muted); color: var(--text-primary); }

  :global(.dropdown-portal) {
    position: fixed; z-index: 200; background: var(--bg-surface);
    border: 0.5px solid var(--border-default); border-radius: 8px;
    padding: 4px; box-shadow: 0 8px 24px rgba(0,0,0,0.14);
    display: flex; flex-direction: column; min-width: 220px;
    font-family: 'DM Sans', sans-serif; font-size: 13px; line-height: 1.4;
  }
  :global(.dropdown-portal .opt) {
    display: flex; align-items: center; gap: 8px; padding: 6px 10px;
    background: none; border: none; cursor: pointer;
    font-size: 12px; font-family: 'DM Sans', sans-serif;
    color: var(--text-primary); border-radius: 4px;
    text-align: left; white-space: nowrap; width: 100%;
  }
  :global(.dropdown-portal .opt:hover) { background: var(--interactive-hover); }
  :global(.dropdown-portal .opt-type) {
    font-size: 10px; width: 16px; text-align: center;
    font-family: 'DM Mono', monospace; flex-shrink: 0; font-weight: 500;
  }
  :global(.dropdown-portal .opt-type--expr)  { color: #3C3489; }
  :global(.dropdown-portal .opt-type--const) { color: #0C447C; }
  :global(.dropdown-portal .opt-type--var)   { color: #3B6D11; }
  :global(.dropdown-portal .opt-empty) {
    font-size: 11px; color: var(--text-muted); padding: 6px 10px; white-space: nowrap;
  }

  /* ── Importer ─────────────────────────────────────────────────────────── */
  .overlay { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: center; padding: 20px; }
  .import-panel { background: var(--bg-surface); border-radius: 12px; width: 100%; max-width: 600px; max-height: 90vh; overflow-y: auto; }
  .import-head { display: flex; align-items: center; justify-content: space-between; padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .btn-close-imp { background: none; border: none; cursor: pointer; font-size: 14px; color: var(--text-muted); }
  .import-body { padding: calc(16px * var(--font-scale)); }

  /* ── Action menu portal ─────────────────────────────────────────────── */
  :global(.action-menu-portal) {
    position: fixed; z-index: 300; background: var(--bg-surface);
    border: 0.5px solid var(--border-default); border-radius: 8px;
    padding: 4px; box-shadow: 0 8px 24px rgba(0,0,0,0.16);
    display: flex; flex-direction: column; min-width: 160px;
    font-family: 'DM Sans', sans-serif; font-size: 13px;
  }
  :global(.amenu-opt) {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 12px; background: none; border: none;
    cursor: pointer; font-size: 12px; font-family: 'DM Sans', sans-serif;
    color: var(--text-primary); border-radius: 4px; text-align: left; width: 100%;
    white-space: nowrap;
  }
  :global(.amenu-opt:hover) { background: var(--interactive-hover); }
  :global(.amenu-opt--warn) { color: #854F0B; }
  :global(.amenu-opt--warn:hover) { background: #FAEEDA; }
  :global(.amenu-opt--danger) { color: #A32D2D; }
  :global(.amenu-opt--danger:hover) { background: #FCEBEB; }
  :global(.amenu-icon) { font-size: 11px; width: 14px; text-align: center; flex-shrink: 0; }
  :global(.amenu-sep) { height: 0.5px; background: var(--border-subtle); margin: 3px 4px; }

  :global(.group-chip) { font-size: calc(11px * var(--font-scale)); padding: 2px 8px; border-radius: 20px; font-weight: 500; }
</style>