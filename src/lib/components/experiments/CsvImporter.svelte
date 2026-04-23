<!-- CsvImporter.svelte -->
<!-- Importa entries desde un CSV. -->
<!-- Flujo: subir CSV → mapear columnas → previsualizar → confirmar → enviar entries -->
<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';

  let { onClose }: { onClose: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  // ── Estado ────────────────────────────────────────────────────────────────
  type Step = 'upload' | 'map' | 'preview' | 'importing';
  let step = $state<Step>('upload');
  let error = $state('');

  // CSV parseado
  let csvHeaders = $state<string[]>([]);
  let csvRows = $state<Record<string, string>[]>([]);

  // Mapeo: header CSV → definition_key del experimento (o 'timestamp' o '_ignore')
  let mapping = $state<Record<string, string>>({});

  // Variables disponibles del experimento
  const definitions = $derived(
    $experimentStore.definitions.filter(d => d.type === 'variable')
  );

  // Column que el usuario eligió como timestamp
  const timestampCol = $derived(
    Object.entries(mapping).find(([_, v]) => v === '_timestamp')?.[0] ?? null
  );

  // Preview de entries procesadas
  const previewEntries = $derived(() => {
    return csvRows.slice(0, 5).map(row => buildEntry(row));
  });

  // Progress durante import
  let importProgress = $state(0);
  let importTotal = $state(0);
  let importDone = $state(false);

  // ── CSV parsing ───────────────────────────────────────────────────────────
  function parseCsv(text: string) {
    const lines = text.replace(/\r/g, '').split('\n').filter(l => l.trim());
    if (lines.length < 2) { error = 'El CSV debe tener al menos un header y una fila de datos'; return; }

    // Detectar separador
    const sep = lines[0].includes(';') ? ';' : ',';
    const headers = lines[0].split(sep).map(h => h.trim().replace(/^"|"$/g, ''));
    const rows = lines.slice(1).map(line => {
      const cells = line.split(sep).map(c => c.trim().replace(/^"|"$/g, ''));
      return Object.fromEntries(headers.map((h, i) => [h, cells[i] ?? '']));
    }).filter(r => Object.values(r).some(v => v !== ''));

    csvHeaders = headers;
    csvRows = rows;

    // Auto-mapeo: intentar hacer match por nombre similar
    const newMapping: Record<string, string> = {};
    for (const h of headers) {
      const hLower = h.toLowerCase().replace(/[\s\-_\.]/g, '');
      // Intentar match con timestamp
      if (['fecha','date','datetime','timestamp','time','hora'].some(k => hLower.includes(k))) {
        newMapping[h] = '_timestamp';
        continue;
      }
      // Intentar match con definitions
      const match = definitions.find(d => {
        const kLower = d.key.toLowerCase().replace(/[\s\-_\.]/g, '');
        const lLower = d.label.toLowerCase().replace(/[\s\-_\.]/g, '');
        return hLower.includes(kLower) || hLower.includes(lLower) ||
               kLower.includes(hLower) || lLower.includes(hLower);
      });
      newMapping[h] = match ? match.key : '_ignore';
    }
    mapping = newMapping;
    step = 'map';
    error = '';
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

  // ── Build entry desde fila CSV ────────────────────────────────────────────
  function buildEntry(row: Record<string, string>): {
    recorded_at: string | null;
    values: { definition_key: string; value_numeric: number | null; value_text: string | null }[];
    hasData: boolean;
  } {
    let recorded_at: string | null = null;
    const values: { definition_key: string; value_numeric: number | null; value_text: string | null }[] = [];

    for (const [csvCol, defKey] of Object.entries(mapping)) {
      if (defKey === '_ignore' || defKey === '') continue;
      const raw = row[csvCol]?.trim() ?? '';
      if (!raw) continue;

      if (defKey === '_timestamp') {
        // Intentar parsear la fecha
        const d = new Date(raw);
        if (!isNaN(d.getTime())) {
          recorded_at = d.toISOString();
        } else {
          // Intentar formato dd/mm/yyyy hh:mm
          const match = raw.match(/(\d{1,2})[\/\-](\d{1,2})[\/\-](\d{2,4})[\s,T]+(\d{1,2}):(\d{2})/);
          if (match) {
            const [_, d, m, y, hh, mm] = match;
            const year = y.length === 2 ? `20${y}` : y;
            const dt = new Date(`${year}-${m.padStart(2,'0')}-${d.padStart(2,'0')}T${hh.padStart(2,'0')}:${mm}:00`);
            if (!isNaN(dt.getTime())) recorded_at = dt.toISOString();
          }
        }
        continue;
      }

      const def = definitions.find(d => d.key === defKey);
      if (!def) continue;

      if (def.var_type === 'numeric') {
        // Limpiar separadores de miles y decimales europeos
        const cleaned = raw.replace(/[,\s]/g, '').replace(',', '.');
        const n = parseFloat(cleaned);
        if (!isNaN(n)) values.push({ definition_key: defKey, value_numeric: n, value_text: null });
      } else {
        values.push({ definition_key: defKey, value_numeric: null, value_text: raw });
      }
    }

    return { recorded_at, values, hasData: values.length > 0 };
  }

  // ── Import ────────────────────────────────────────────────────────────────
  async function runImport() {
    step = 'importing';
    importTotal = csvRows.length;
    importProgress = 0;
    importDone = false;
    error = '';

    const expId = $experimentStore.experiment!.id;
    let successCount = 0;

    for (const row of csvRows) {
      const entry = buildEntry(row);
      if (!entry.hasData) { importProgress++; continue; }

      try {
        // 1. Crear el evento con fecha opcional
        const evBody: Record<string, unknown> = {
          step_key: 'entry',
          event_type: 'measurement',
          data: {},
        };
        if (entry.recorded_at) evBody.recorded_at = entry.recorded_at;

        const evRes = await fetch(`${API}/api/v1/experiments/${expId}/events`, {
          method: 'POST', credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(evBody),
        });
        if (!evRes.ok) { importProgress++; continue; }
        const event = await evRes.json();

        // 2. Guardar valores
        if (entry.values.length > 0) {
          await fetch(`${API}/api/v1/experiments/${expId}/events/${event.id}/values`, {
            method: 'POST', credentials: 'include',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(entry.values),
          });
          experimentStore.setEntryValues(event.id, Object.fromEntries(
            entry.values.map(v => [v.definition_key, v.value_numeric ?? v.value_text])
          ));
        }

        experimentStore.addEvent(event);
        successCount++;
      } catch {}
      importProgress++;
    }

    importDone = true;
  }

  // ── Helpers UI ────────────────────────────────────────────────────────────
  function formatDate(iso: string | null) {
    if (!iso) return '—';
    return new Date(iso).toLocaleString('es-CR', { day:'2-digit', month:'short', hour:'2-digit', minute:'2-digit', hour12: false });
  }
</script>

<div class="importer">
  {#if error}<div class="err">{error}</div>{/if}

  <!-- ── Step 1: Upload ── -->
  {#if step === 'upload'}
    <div class="drop-zone"
         ondragover={(e) => e.preventDefault()}
         ondrop={handleDrop}
         role="button" tabindex="0">
      <div class="drop-icon">📄</div>
      <p class="drop-title">arrastrá tu CSV acá</p>
      <p class="drop-sub">o hacé clic para seleccionar</p>
      <input type="file" accept=".csv,.tsv,.txt" class="file-input" onchange={handleFile} />
    </div>
    <p class="hint">Soporta CSV con separador coma o punto y coma. Las fechas pueden estar en formato ISO, dd/mm/yyyy, o mm/dd/yyyy.</p>

  <!-- ── Step 2: Mapeo de columnas ── -->
  {:else if step === 'map'}
    <div class="map-section">
      <h3 class="map-title">mapear columnas</h3>
      <p class="map-sub">{csvRows.length} filas detectadas. Asigná cada columna del CSV a una variable del experimento.</p>

      <div class="map-table">
        <div class="map-row map-header">
          <span>columna CSV</span>
          <span>ejemplo</span>
          <span>variable</span>
        </div>
        {#each csvHeaders as h}
          <div class="map-row">
            <span class="map-col-name">{h}</span>
            <span class="map-example">{csvRows[0]?.[h] ?? '—'}</span>
            <select class="map-select" bind:value={mapping[h]}>
              <option value="_ignore">— ignorar —</option>
              <option value="_timestamp">📅 fecha/hora</option>
              {#each definitions as def}
                <option value={def.key}>{def.label}</option>
              {/each}
            </select>
          </div>
        {/each}
      </div>

      <div class="btn-row">
        <button class="btn-cancel" onclick={() => { step = 'upload'; csvHeaders = []; csvRows = []; }}>volver</button>
        <button class="btn-primary" onclick={() => step = 'preview'}>previsualizar →</button>
      </div>
    </div>

  <!-- ── Step 3: Preview ── -->
  {:else if step === 'preview'}
    <div class="preview-section">
      <h3 class="map-title">previsualización</h3>
      <p class="map-sub">Primeras 5 filas de {csvRows.length} total.</p>

      <div class="preview-wrap">
        <table class="preview-table">
          <thead>
            <tr>
              <th>timestamp</th>
              {#each definitions.filter(d => Object.values(mapping).includes(d.key)) as def}
                <th>{def.label}</th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each csvRows.slice(0, 5) as row}
              {@const entry = buildEntry(row)}
              <tr class:empty={!entry.hasData}>
                <td class="td-ts">{formatDate(entry.recorded_at)}</td>
                {#each definitions.filter(d => Object.values(mapping).includes(d.key)) as def}
                  {@const val = entry.values.find(v => v.definition_key === def.key)}
                  <td>{val?.value_numeric ?? val?.value_text ?? '—'}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      {#if !timestampCol}
        <div class="warn">⚠ Sin columna de fecha — se usará la hora actual para todas las entries.</div>
      {/if}

      <div class="btn-row">
        <button class="btn-cancel" onclick={() => step = 'map'}>← volver</button>
        <button class="btn-primary" onclick={runImport}>importar {csvRows.length} entries</button>
      </div>
    </div>

  <!-- ── Step 4: Importing ── -->
  {:else if step === 'importing'}
    <div class="progress-section">
      <h3 class="map-title">{importDone ? '¡listo!' : 'importando...'}</h3>
      <div class="progress-bar-wrap">
        <div class="progress-bar" style="width: {importTotal > 0 ? (importProgress/importTotal*100) : 0}%"></div>
      </div>
      <p class="progress-label">{importProgress} / {importTotal} filas procesadas</p>
      {#if importDone}
        <button class="btn-primary" onclick={onClose}>cerrar</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .importer { display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }
  .err { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }

  .drop-zone {
    position: relative; border: 1.5px dashed var(--border-default); border-radius: 10px;
    padding: calc(32px * var(--font-scale)); text-align: center; cursor: pointer;
    transition: border-color .15s;
  }
  .drop-zone:hover { border-color: var(--text-muted); }
  .drop-icon { font-size: 32px; margin-bottom: 8px; }
  .drop-title { font-size: calc(14px * var(--font-scale)); color: var(--text-primary); font-weight: 500; }
  .drop-sub { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-top: 4px; }
  .file-input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .hint { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; }

  .map-title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .map-sub { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }
  .map-table { display: flex; flex-direction: column; gap: 4px; }
  .map-header { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-weight: 500; border-bottom: 0.5px solid var(--border-subtle); padding-bottom: 4px; }
  .map-row { display: grid; grid-template-columns: 1fr 1fr 1.5fr; gap: 8px; align-items: center; }
  .map-col-name { font-family: 'DM Mono', monospace; font-size: calc(12px * var(--font-scale)); color: var(--text-primary); }
  .map-example { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .map-select { font-size: calc(12px * var(--font-scale)); padding: 4px 6px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); width: 100%; }

  .preview-wrap { overflow-x: auto; border: 0.5px solid var(--border-subtle); border-radius: 6px; }
  .preview-table { width: 100%; border-collapse: collapse; font-size: calc(11px * var(--font-scale)); }
  .preview-table th { background: var(--bg-elevated); padding: 6px 10px; text-align: left; border-bottom: 0.5px solid var(--border-subtle); color: var(--text-secondary); font-weight: 500; white-space: nowrap; }
  .preview-table td { padding: 5px 10px; border-bottom: 0.5px solid var(--border-subtle); font-family: 'DM Mono', monospace; }
  .preview-table tr.empty td { color: var(--text-muted); }
  .td-ts { color: var(--text-muted) !important; white-space: nowrap; }

  .warn { font-size: calc(12px * var(--font-scale)); color: #b45309; background: #fef3c7; border-radius: 6px; padding: 8px 12px; }

  .progress-section { display: flex; flex-direction: column; align-items: center; gap: 16px; padding: 20px 0; }
  .progress-bar-wrap { width: 100%; height: 6px; background: var(--bg-elevated); border-radius: 3px; overflow: hidden; }
  .progress-bar { height: 100%; background: #3B6D11; border-radius: 3px; transition: width .2s; }
  .progress-label { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }

  .btn-row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  .btn-cancel { padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .btn-primary { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); border: none; border-radius: 6px; background: var(--text-primary); color: var(--bg-surface); cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-primary:hover { opacity: 0.85; }
</style>