<!-- CsvImporter.svelte -->
<!-- Wizard para importar entries desde CSV.
     Modo A: sin definitions — detecta columnas, el usuario las clasifica, crea definitions + entries.
     Modo B: con definitions — solo mapea columnas a definitions existentes e importa entries. -->
<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import SymbolPicker from './SymbolPicker.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  // ── Tipos ─────────────────────────────────────────────────────────────────
  type ColRole = 'timestamp' | 'variable_numeric' | 'variable_text' | 'variable_qualitative' | 'constant' | 'ignore';

  interface ColConfig {
    csvHeader:  string;
    example:    string;
    role:       ColRole;
    // Para variables y constantes
    key:        string;
    label:      string;
    unit:       string;
    // Para constantes — valor fijo
    constValue: string;
    // Para qualitative — opciones detectadas
    detectedOptions: string[];
  }

  // ── Estado ────────────────────────────────────────────────────────────────
  type Step = 'upload' | 'classify' | 'configure' | 'preview' | 'importing';
  let step = $state<Step>('upload');
  let error = $state('');

  let csvHeaders = $state<string[]>([]);
  let csvRows = $state<Record<string, string>[]>([]);
  let colConfigs = $state<ColConfig[]>([]);

  // Modo: A = sin definitions, B = con definitions
  const modeA = $derived($experimentStore.definitions.filter(d => d.type === 'variable').length === 0);
  const existingDefs = $derived($experimentStore.definitions.filter(d => d.type === 'variable'));

  // Mapeo Modo B: csvHeader → definition_key | '_timestamp' | '_ignore'
  let mappingB = $state<Record<string, string>>({});

  let importProgress = $state(0);
  let importTotal = $state(0);
  let importDone = $state(false);
  let importSuccess = $state(0);

  // ── CSV parsing ───────────────────────────────────────────────────────────
  function parseCsv(text: string) {
    error = '';
    const lines = text.replace(/\r/g, '').split('\n').filter(l => l.trim());
    if (lines.length < 2) { error = 'El CSV necesita al menos un header y una fila'; return; }

    const sep = lines[0].includes(';') ? ';' : ',';
    const headers = lines[0].split(sep).map(h => h.trim().replace(/^"|"$/g, ''));
    const rows = lines.slice(1).map(line => {
      const cells = line.split(sep).map(c => c.trim().replace(/^"|"$/g, ''));
      return Object.fromEntries(headers.map((h, i) => [h, cells[i] ?? '']));
    }).filter(r => Object.values(r).some(v => v !== ''));

    csvHeaders = headers;
    csvRows = rows;

    if (modeA) {
      // Modo A — auto-clasificar
      colConfigs = headers.map(h => {
        const hLow = h.toLowerCase().replace(/[\s\-_\.]/g, '');
        const examples = rows.slice(0, 10).map(r => r[h]).filter(v => v);
        const example = examples[0] ?? '';

        // Detectar timestamp
        let role: ColRole = 'variable_numeric';
        if (['fecha','date','datetime','timestamp','time','hora'].some(k => hLow.includes(k))) {
          role = 'timestamp';
        } else {
          // Detectar si es numérico
          const numericCount = examples.filter(v => !isNaN(parseFloat(v.replace(',', '.')))).length;
          if (numericCount < examples.length * 0.5) {
            // Mayoría no numérica → detectar si es cualitativo
            const unique = new Set(examples).size;
            role = unique <= 8 ? 'variable_qualitative' : 'variable_text';
          }
        }

        // Detectar opciones para qualitative
        const detectedOptions = role === 'variable_qualitative'
          ? [...new Set(examples)].filter(Boolean)
          : [];

        // Auto key: limpiar header
        const autoKey = h.toLowerCase()
          .replace(/[áàä]/g, 'a').replace(/[éèë]/g, 'e')
          .replace(/[íìï]/g, 'i').replace(/[óòö]/g, 'o')
          .replace(/[úùü]/g, 'u').replace(/ñ/g, 'n')
          .replace(/[^a-z0-9_]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '');

        return { csvHeader: h, example, role, key: autoKey, label: h, unit: '', constValue: '', detectedOptions };
      });
      step = 'classify';
    } else {
      // Modo B — auto-mapear a definitions existentes
      const newMapping: Record<string, string> = {};
      for (const h of headers) {
        const hLow = h.toLowerCase().replace(/[\s\-_\.]/g, '');
        if (['fecha','date','datetime','timestamp','time','hora'].some(k => hLow.includes(k))) {
          newMapping[h] = '_timestamp'; continue;
        }
        const match = existingDefs.find(d => {
          const kl = d.key.toLowerCase().replace(/[\s\-_\.]/g, '');
          const ll = d.label.toLowerCase().replace(/[\s\-_\.]/g, '');
          return hLow.includes(kl) || hLow.includes(ll) || kl.includes(hLow) || ll.includes(hLow);
        });
        newMapping[h] = match ? match.key : '_ignore';
      }
      mappingB = newMapping;
      step = 'classify';
    }
  }

  function handleFile(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = ev => parseCsv(ev.target?.result as string);
    reader.readAsText(file);
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = ev => parseCsv(ev.target?.result as string);
    reader.readAsText(file);
  }

  // ── Preview entries ───────────────────────────────────────────────────────
  function buildEntryFromRowA(row: Record<string, string>) {
    let recorded_at: string | null = null;
    const values: { key: string; label: string; numVal: number | null; textVal: string | null }[] = [];

    for (const cfg of colConfigs) {
      if (cfg.role === 'ignore' || cfg.role === 'constant') continue;
      const raw = row[cfg.csvHeader]?.trim() ?? '';
      if (!raw) continue;

      if (cfg.role === 'timestamp') {
        recorded_at = parseDate(raw);
      } else if (cfg.role === 'variable_numeric') {
        const n = parseFloat(raw.replace(',', '.'));
        if (!isNaN(n)) values.push({ key: cfg.key, label: cfg.label, numVal: n, textVal: null });
      } else {
        values.push({ key: cfg.key, label: cfg.label, numVal: null, textVal: raw });
      }
    }
    return { recorded_at, values, hasData: values.length > 0 };
  }

  function buildEntryFromRowB(row: Record<string, string>) {
    let recorded_at: string | null = null;
    const values: { definition_key: string; value_numeric: number | null; value_text: string | null }[] = [];

    for (const [csvCol, defKey] of Object.entries(mappingB)) {
      if (defKey === '_ignore' || !defKey) continue;
      const raw = row[csvCol]?.trim() ?? '';
      if (!raw) continue;

      if (defKey === '_timestamp') {
        recorded_at = parseDate(raw); continue;
      }
      const def = existingDefs.find(d => d.key === defKey);
      if (!def) continue;
      if (def.var_type === 'numeric') {
        const n = parseFloat(raw.replace(',', '.'));
        if (!isNaN(n)) values.push({ definition_key: defKey, value_numeric: n, value_text: null });
      } else {
        values.push({ definition_key: defKey, value_numeric: null, value_text: raw });
      }
    }
    return { recorded_at, values, hasData: values.length > 0 };
  }

  function parseDate(raw: string): string | null {
    const d = new Date(raw);
    if (!isNaN(d.getTime())) return d.toISOString();
    // dd/mm/yyyy hh:mm
    const m = raw.match(/(\d{1,2})[\/\-](\d{1,2})[\/\-](\d{2,4})[\sT,]+(\d{1,2}):(\d{2})/);
    if (m) {
      const year = m[3].length === 2 ? `20${m[3]}` : m[3];
      const dt = new Date(`${year}-${m[2].padStart(2,'0')}-${m[1].padStart(2,'0')}T${m[4].padStart(2,'0')}:${m[5]}:00`);
      if (!isNaN(dt.getTime())) return dt.toISOString();
    }
    return null;
  }

  function fmtDate(iso: string | null) {
    if (!iso) return '—';
    return new Date(iso).toLocaleString('es-CR', { day:'2-digit', month:'short', hour:'2-digit', minute:'2-digit', hour12: false });
  }

  // Columnas con datos reales para preview
  const previewCols = $derived(
    modeA
      ? colConfigs.filter(c => c.role !== 'ignore' && c.role !== 'constant' && c.role !== 'timestamp')
      : existingDefs.filter(d => Object.values(mappingB).includes(d.key))
  );

  // ── Import ────────────────────────────────────────────────────────────────
  async function runImport() {
    step = 'importing';
    importTotal = csvRows.length;
    importProgress = 0;
    importDone = false;
    importSuccess = 0;
    error = '';

    const expId = $experimentStore.experiment!.id;

    // Modo A: crear definitions primero
    if (modeA) {
      const variableCols = colConfigs.filter(c => c.role.startsWith('variable'));
      const constantCols = colConfigs.filter(c => c.role === 'constant');

      // Crear variables
      for (const cfg of variableCols) {
        const varType = cfg.role === 'variable_numeric' ? 'numeric'
          : cfg.role === 'variable_qualitative' ? 'qualitative' : 'text';
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            key: cfg.key, label: cfg.label, type: 'variable',
            payload: { unit: cfg.unit },
            var_type: varType,
            options: cfg.detectedOptions,
          }),
        });
        if (res.ok) experimentStore.addDefinition(await res.json());
      }

      // Crear constantes
      for (const cfg of constantCols) {
        const n = parseFloat(cfg.constValue);
        const res = await fetch(`${API}/api/v1/experiments/${expId}/definitions`, {
          method: 'POST', credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            key: cfg.key, label: cfg.label, type: 'constant',
            payload: { value: isNaN(n) ? cfg.constValue : n, unit: cfg.unit },
          }),
        });
        if (res.ok) experimentStore.addDefinition(await res.json());
      }

      // Configurar columns del experimento
      const columns = variableCols.map((cfg, i) => ({
        key: cfg.key, type: 'variable' as const, order: i, visible: true,
      }));
      await fetch(`${API}/api/v1/experiments/${expId}/columns`, {
        method: 'PATCH', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ columns }),
      });
      experimentStore.updateColumns(columns);
    }

    // Importar entries
    for (const row of csvRows) {
      const entry = modeA ? buildEntryFromRowA(row) : buildEntryFromRowB(row);
      if (!entry.hasData) { importProgress++; continue; }

      try {
        const evBody: Record<string, unknown> = {
          step_key: 'entry', event_type: 'measurement', data: {},
        };
        if (entry.recorded_at) evBody.recorded_at = entry.recorded_at;

        const evRes = await fetch(`${API}/api/v1/experiments/${expId}/events`, {
          method: 'POST', credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(evBody),
        });
        if (!evRes.ok) { importProgress++; continue; }
        const event = await evRes.json();

        const vals = modeA
          ? (entry as any).values.map((v: any) => ({
              definition_key: v.key,
              value_numeric: v.numVal,
              value_text: v.textVal,
              value_csv_data: null,
            }))
          : (entry as any).values;

        if (vals.length > 0) {
          await fetch(`${API}/api/v1/experiments/${expId}/events/${event.id}/values`, {
            method: 'POST', credentials: 'include',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(vals),
          });
          experimentStore.setEntryValues(event.id, Object.fromEntries(
            vals.map((v: any) => [v.definition_key, v.value_numeric ?? v.value_text])
          ));
        }

        experimentStore.addEvent(event);
        importSuccess++;
      } catch {}
      importProgress++;
    }

    importDone = true;
  }
</script>

<div class="imp">
  {#if error}<div class="err">{error}</div>{/if}

  <!-- ── Step 1: Upload ── -->
  {#if step === 'upload'}
    <div class="drop-zone" ondragover={(e) => e.preventDefault()} ondrop={handleDrop} role="button" tabindex="0">
      <div class="drop-icon">📄</div>
      <p class="drop-title">arrastrá tu CSV acá</p>
      <p class="drop-sub">o hacé clic para seleccionar</p>
      <input type="file" accept=".csv,.tsv,.txt" class="file-input" onchange={handleFile} />
    </div>
    <p class="hint">Soporta CSV con separador coma o punto y coma. Fechas: ISO, dd/mm/yyyy hh:mm.</p>

  <!-- ── Step 2: Clasificar / mapear columnas ── -->
  {:else if step === 'classify'}
    <h3 class="step-title">
      {modeA ? '¿qué es cada columna?' : 'mapear columnas'}
    </h3>
    <p class="step-sub">
      {modeA
        ? `${csvRows.length} filas detectadas. Clasificá cada columna.`
        : `${csvRows.length} filas. Asigná columnas a variables del experimento.`}
    </p>

    <div class="classify-table">
      <div class="classify-header">
        <span>columna CSV</span>
        <span>ejemplo</span>
        <span>{modeA ? 'tipo' : 'variable'}</span>
      </div>

      {#if modeA}
        {#each colConfigs as cfg, i}
          <div class="classify-row">
            <span class="col-name">{cfg.csvHeader}</span>
            <span class="col-ex">{cfg.example}</span>
            <select class="col-sel" bind:value={colConfigs[i].role}>
              <option value="timestamp">📅 fecha/hora</option>
              <option value="variable_numeric">χ numérica</option>
              <option value="variable_text">T texto</option>
              <option value="variable_qualitative">≡ cualitativa</option>
              <option value="constant">C constante</option>
              <option value="ignore">— ignorar</option>
            </select>
          </div>
        {/each}
      {:else}
        {#each csvHeaders as h}
          <div class="classify-row">
            <span class="col-name">{h}</span>
            <span class="col-ex">{csvRows[0]?.[h] ?? '—'}</span>
            <select class="col-sel" bind:value={mappingB[h]}>
              <option value="_ignore">— ignorar —</option>
              <option value="_timestamp">📅 fecha/hora</option>
              {#each existingDefs as def}
                <option value={def.key}>{def.label}</option>
              {/each}
            </select>
          </div>
        {/each}
      {/if}
    </div>

    <div class="btn-row">
      <button class="btn-sec" onclick={() => { step = 'upload'; csvHeaders = []; csvRows = []; }}>volver</button>
      <button class="btn-pri" onclick={() => modeA ? step = 'configure' : step = 'preview'}>
        {modeA ? 'configurar →' : 'previsualizar →'}
      </button>
    </div>

  <!-- ── Step 3: Configurar (solo Modo A) ── -->
  {:else if step === 'configure'}
    <h3 class="step-title">configurar variables y constantes</h3>
    <p class="step-sub">Definí key, label y unidad para cada columna.</p>

    <div class="config-list">
      {#each colConfigs.filter(c => c.role !== 'ignore' && c.role !== 'timestamp') as cfg, i}
        {@const idx = colConfigs.indexOf(cfg)}
        <div class="config-card">
          <div class="config-card-head">
            <span class="config-badge config-badge--{cfg.role}">{cfg.role === 'constant' ? 'C' : cfg.role === 'variable_qualitative' ? '≡' : cfg.role === 'variable_text' ? 'T' : 'χ'}</span>
            <span class="config-csv-col">{cfg.csvHeader}</span>
            <span class="config-ex">ej: {cfg.example}</span>
          </div>
          <div class="config-fields">
            <div class="cfg-field">
              <label>key</label>
              <div class="input-row">
                <input class="mono" bind:value={colConfigs[idx].key} placeholder="masa_maceta" />
                <SymbolPicker onPick={(s) => colConfigs[idx].key += s} />
              </div>
            </div>
            <div class="cfg-field">
              <label>label</label>
              <div class="input-row">
                <input bind:value={colConfigs[idx].label} placeholder="Masa maceta" />
                <SymbolPicker onPick={(s) => colConfigs[idx].label += s} />
              </div>
            </div>
            {#if cfg.role !== 'variable_qualitative' && cfg.role !== 'variable_text'}
              <div class="cfg-field cfg-field--sm">
                <label>unidad</label>
                <input bind:value={colConfigs[idx].unit} placeholder="g" />
              </div>
            {/if}
            {#if cfg.role === 'constant'}
              <div class="cfg-field cfg-field--sm">
                <label>valor</label>
                <input class="mono" bind:value={colConfigs[idx].constValue} placeholder="8033.7" inputmode="decimal" />
              </div>
            {/if}
            {#if cfg.role === 'variable_qualitative' && cfg.detectedOptions.length > 0}
              <div class="cfg-field">
                <label>opciones detectadas</label>
                <div class="opt-chips">
                  {#each cfg.detectedOptions as opt}
                    <span class="opt-chip">{opt}</span>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    <div class="btn-row">
      <button class="btn-sec" onclick={() => step = 'classify'}>volver</button>
      <button class="btn-pri" onclick={() => step = 'preview'}>previsualizar →</button>
    </div>

  <!-- ── Step 4: Preview ── -->
  {:else if step === 'preview'}
    <h3 class="step-title">previsualización</h3>
    <p class="step-sub">Primeras 5 filas de {csvRows.length} total.</p>

    <div class="prev-wrap">
      <table class="prev-table">
        <thead>
          <tr>
            <th>timestamp</th>
            {#each previewCols as col}
              <th>{modeA ? (col as any).label : (col as any).label}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each csvRows.slice(0, 5) as row}
            {@const entry = modeA ? buildEntryFromRowA(row) : buildEntryFromRowB(row)}
            <tr class:empty-row={!entry.hasData}>
              <td class="td-ts">{fmtDate(entry.recorded_at)}</td>
              {#each previewCols as col}
                {@const key = modeA ? (col as any).key : (col as any).key}
                {@const val = (entry as any).values?.find((v: any) =>
                  modeA ? v.key === key : v.definition_key === key
                )}
                <td>{val?.numVal ?? val?.value_numeric ?? val?.textVal ?? val?.value_text ?? '—'}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if !colConfigs.some(c => c.role === 'timestamp') && !Object.values(mappingB).includes('_timestamp')}
      <div class="warn">⚠ Sin columna de fecha — se usará la hora actual para todas las entries.</div>
    {/if}

    <div class="btn-row">
      <button class="btn-sec" onclick={() => step = modeA ? 'configure' : 'classify'}>volver</button>
      <button class="btn-pri" onclick={runImport}>importar {csvRows.length} entries</button>
    </div>

  <!-- ── Step 5: Importing ── -->
  {:else if step === 'importing'}
    <div class="prog-section">
      <h3 class="step-title">{importDone ? '✓ listo' : 'importando...'}</h3>
      <div class="prog-wrap">
        <div class="prog-bar" style="width: {importTotal > 0 ? (importProgress/importTotal*100) : 0}%"></div>
      </div>
      <p class="prog-label">
        {importDone
          ? `${importSuccess} de ${importTotal} filas importadas correctamente`
          : `${importProgress} / ${importTotal}`}
      </p>
      {#if importDone}
        <button class="btn-pri" onclick={onClose}>cerrar</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .imp { display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }
  .err { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }

  /* Upload */
  .drop-zone { position: relative; border: 1.5px dashed var(--border-default); border-radius: 10px; padding: calc(32px * var(--font-scale)); text-align: center; cursor: pointer; }
  .drop-zone:hover { border-color: var(--text-muted); }
  .drop-icon { font-size: 32px; margin-bottom: 8px; }
  .drop-title { font-size: calc(14px * var(--font-scale)); color: var(--text-primary); font-weight: 500; }
  .drop-sub { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-top: 4px; }
  .file-input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }

  /* Classify */
  .step-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .step-sub { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }
  .classify-table { display: flex; flex-direction: column; gap: 4px; }
  .classify-header { display: grid; grid-template-columns: 1fr 1fr 1.4fr; gap: 8px; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); border-bottom: 0.5px solid var(--border-subtle); padding-bottom: 4px; }
  .classify-row { display: grid; grid-template-columns: 1fr 1fr 1.4fr; gap: 8px; align-items: center; }
  .col-name { font-family: 'DM Mono', monospace; font-size: calc(12px * var(--font-scale)); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .col-ex { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .col-sel { font-size: calc(12px * var(--font-scale)); padding: 4px 6px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); width: 100%; }

  /* Configure */
  .config-list { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); max-height: 400px; overflow-y: auto; }
  .config-card { border: 0.5px solid var(--border-subtle); border-radius: 8px; padding: calc(12px * var(--font-scale)); display: flex; flex-direction: column; gap: 10px; }
  .config-card-head { display: flex; align-items: center; gap: 8px; }
  .config-badge { font-size: calc(11px * var(--font-scale)); padding: 2px 8px; border-radius: 20px; font-family: 'DM Mono', monospace; font-weight: 600; }
  .config-badge--variable_numeric { background: #EAF3DE; color: #3B6D11; }
  .config-badge--variable_text { background: #E8F0FE; color: #185FA5; }
  .config-badge--variable_qualitative { background: #EEEDFE; color: #3C3489; }
  .config-badge--constant { background: #FEF3C7; color: #92400E; }
  .config-csv-col { font-size: calc(12px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .config-ex { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .config-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .cfg-field { display: flex; flex-direction: column; gap: 3px; }
  .cfg-field--sm { grid-column: span 1; }
  .cfg-field label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .cfg-field input { padding: 5px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; font-size: calc(12px * var(--font-scale)); background: var(--bg-surface); color: var(--text-primary); outline: none; width: 100%; }
  .cfg-field input:focus { border-color: var(--text-primary); }
  .cfg-field input.mono { font-family: 'DM Mono', monospace; }
  .input-row { display: flex; gap: 4px; align-items: center; }
  .input-row input { flex: 1; }
  .opt-chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .opt-chip { font-size: calc(11px * var(--font-scale)); padding: 2px 8px; border-radius: 20px; background: var(--bg-elevated); color: var(--text-secondary); border: 0.5px solid var(--border-subtle); }

  /* Preview */
  .prev-wrap { overflow-x: auto; border: 0.5px solid var(--border-subtle); border-radius: 6px; }
  .prev-table { width: 100%; border-collapse: collapse; font-size: calc(11px * var(--font-scale)); }
  .prev-table th { background: var(--bg-elevated); padding: 6px 10px; text-align: left; border-bottom: 0.5px solid var(--border-subtle); color: var(--text-secondary); white-space: nowrap; }
  .prev-table td { padding: 5px 10px; border-bottom: 0.5px solid var(--border-subtle); font-family: 'DM Mono', monospace; }
  .prev-table tr.empty-row td { color: var(--text-muted); }
  .td-ts { color: var(--text-muted) !important; white-space: nowrap; }
  .warn { font-size: calc(12px * var(--font-scale)); color: #92400E; background: #FEF3C7; border-radius: 6px; padding: 8px 12px; }

  /* Progress */
  .prog-section { display: flex; flex-direction: column; align-items: center; gap: 16px; padding: 20px 0; }
  .prog-wrap { width: 100%; height: 6px; background: var(--bg-elevated); border-radius: 3px; overflow: hidden; }
  .prog-bar { height: 100%; background: #3B6D11; border-radius: 3px; transition: width .15s; }
  .prog-label { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }

  /* Buttons */
  .btn-row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  .btn-sec { padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .btn-pri { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); border: none; border-radius: 6px; background: var(--text-primary); color: var(--bg-surface); cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-pri:hover { opacity: 0.85; }
</style>