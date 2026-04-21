<script lang="ts">
  import { experimentStore } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';

  let { stepKey, onUploaded }:
    { stepKey: string; onUploaded: (rows: any[], columns: string[]) => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let file    = $state<File | null>(null);
  let loading = $state(false);
  let error   = $state('');
  let preview = $state<{ columns: string[]; rows: any[] } | null>(null);

  // Expected schema from definitions
  const csvSchema = $derived(
    $experimentStore.definitions.find(
      d => d.type === 'csv_schema' && d.key === stepKey
    )?.payload as { columns?: { key: string; label: string; unit?: string }[] } | undefined
  );

  async function handleFile(e: Event) {
    const input = e.target as HTMLInputElement;
    file = input.files?.[0] ?? null;
    if (!file) return;
    error = '';

    // Preview local del CSV antes de subir
    const text = await file.text();
    const lines = text.split('\n').filter(l => l.trim());
    if (lines.length < 2) { error = 'El archivo parece vacío'; return; }
    const columns = lines[0].split(',').map(h => h.trim());
    const rows = lines.slice(1, 6).map(line => {
      const vals = line.split(',');
      return Object.fromEntries(columns.map((c, i) => [c, vals[i]?.trim() ?? '']));
    });
    preview = { columns, rows };
  }

  async function upload() {
    if (!file) return;
    loading = true; error = '';
    try {
      const token = auth.getToken();
      const expId = $experimentStore.experiment?.id;
      const formData = new FormData();
      formData.append('file', file);
      formData.append('step_key', stepKey);
      const res = await fetch(`${API}/api/v1/experiments/${expId}/upload-csv`, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: formData,
      });
      const data = await res.json();
      if (!res.ok) { error = data.error ?? 'Error al subir'; return; }
      onUploaded(data.parsed_data ?? [], data.columns ?? []);
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

<div class="csv-upload">
  {#if csvSchema?.columns}
    <div class="schema-hint">
      <span class="schema-label">columnas esperadas:</span>
      {#each csvSchema.columns as col}
        <span class="schema-col">{col.key}{col.unit ? ` (${col.unit})` : ''}</span>
      {/each}
    </div>
  {/if}

  <label class="file-drop" class:has-file={!!file}>
    <input type="file" accept=".csv,.txt" onchange={handleFile} style="display:none" />
    {#if file}
      <span class="file-name">{file.name}</span>
      <span class="file-size">{(file.size / 1024).toFixed(1)} KB</span>
    {:else}
      <span class="drop-hint">clic para seleccionar CSV</span>
    {/if}
  </label>

  {#if preview}
    <div class="preview">
      <div class="preview-label">primeras 5 filas</div>
      <div class="preview-scroll">
        <table class="preview-table">
          <thead>
            <tr>{#each preview.columns as col}<th>{col}</th>{/each}</tr>
          </thead>
          <tbody>
            {#each preview.rows as row}
              <tr>{#each preview.columns as col}<td>{row[col] ?? ''}</td>{/each}</tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="error-box">{error}</div>
  {/if}

  {#if file}
    <button class="btn-upload" disabled={loading} onclick={upload}>
      {loading ? 'subiendo...' : 'subir CSV'}
    </button>
  {/if}
</div>

<style>
  .csv-upload { display: flex; flex-direction: column; gap: calc(10px * var(--font-scale)); }
  .schema-hint { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .schema-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .schema-col { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 2px 6px; border-radius: 4px; color: var(--text-secondary); }
  .file-drop { display: flex; align-items: center; justify-content: center; gap: 8px; padding: calc(16px * var(--font-scale)); border: 0.5px dashed var(--border-default); border-radius: 6px; cursor: pointer; transition: all .12s; }
  .file-drop:hover, .file-drop.has-file { border-color: var(--text-muted); background: var(--bg-elevated); }
  .file-name { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); font-family: 'DM Mono', monospace; }
  .file-size { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .drop-hint { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); }
  .preview { }
  .preview-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); margin-bottom: 4px; }
  .preview-scroll { overflow-x: auto; }
  .preview-table { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; border-collapse: collapse; white-space: nowrap; }
  .preview-table th { background: var(--bg-elevated); padding: 4px 8px; text-align: left; color: var(--text-secondary); font-weight: 500; }
  .preview-table td { padding: 3px 8px; border-top: 0.5px solid var(--border-subtle); color: var(--text-primary); }
  .error-box { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 8px 12px; font-size: calc(12px * var(--font-scale)); }
  .btn-upload { padding: calc(8px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); }
  .btn-upload:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
