<script lang="ts">
  import { auth } from '$lib/stores/auth';

  let { experimentId, experimentTitle, onDeleted }:
    { experimentId: string; experimentTitle: string; onDeleted: () => void } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  // Generar código aleatorio de 6 chars al montar
  const confirmCode = Math.random().toString(36).slice(2, 8).toUpperCase();

  let expanded   = $state(false);
  let inputCode  = $state('');
  let loading    = $state(false);
  let error      = $state('');

  const codeMatch = $derived(inputCode.trim().toUpperCase() === confirmCode);

  async function deleteExperiment() {
    if (!codeMatch) return;
    loading = true; error = '';
    try {
      const token = auth.getToken();
      const res = await fetch(`${API}/api/v1/experiments/${experimentId}`, {
        method: 'DELETE',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });

      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        error = data.error ?? `Error ${res.status}`;
        return;
      }

      // El backend devuelve el CSV completo como body — descargarlo automáticamente
      const csv  = await res.text();
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `backup-${experimentTitle.replace(/\s+/g, '-').toLowerCase()}.csv`;
      a.click();
      URL.revokeObjectURL(url);

      onDeleted();
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

<div class="danger-zone">
  <div class="danger-zone__head" role="button" tabindex="0"
       onclick={() => expanded = !expanded}
       onkeydown={(e) => e.key === 'Enter' && (expanded = !expanded)}>
    <span class="danger-zone__label">zona de peligro</span>
    <span class="danger-zone__chevron" class:open={expanded}>▶</span>
  </div>

  {#if expanded}
    <div class="danger-zone__body">
      <div class="danger-zone__warning">
        <div class="warning-icon">!</div>
        <div class="warning-text">
          <strong>Esta acción es irreversible.</strong> Se borrarán el experimento,
          todas las entries, definiciones, objetivos y colaboradores.
          Antes de borrar se descargará automáticamente un CSV con todo el contenido
          (incluyendo entries anuladas) como respaldo.
        </div>
      </div>

      <div class="confirm-block">
        <p class="confirm-hint">
          Para confirmar, escribí el siguiente código:
        </p>
        <div class="confirm-code">{confirmCode}</div>
        <input
          class="confirm-input"
          class:match={codeMatch}
          bind:value={inputCode}
          placeholder="escribí el código acá"
          maxlength="6"
          spellcheck="false"
          autocomplete="off"
        />
        {#if error}
          <div class="confirm-error">{error}</div>
        {/if}
      </div>

      <button
        class="btn-delete"
        disabled={!codeMatch || loading}
        onclick={deleteExperiment}
      >
        {#if loading}
          descargando backup y borrando...
        {:else}
          descargar backup y borrar experimento
        {/if}
      </button>
    </div>
  {/if}
</div>

<style>
  .danger-zone {
    margin-top: calc(40px * var(--font-scale));
    border: 0.5px solid rgba(162,45,45,0.3);
    border-radius: 8px;
    overflow: hidden;
  }

  .danger-zone__head {
    display: flex; align-items: center; justify-content: space-between;
    padding: calc(12px * var(--font-scale)) calc(16px * var(--font-scale));
    cursor: pointer; user-select: none;
    background: rgba(162,45,45,0.05);
  }
  .danger-zone__label {
    font-size: calc(12px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    letter-spacing: .08em;
    color: #A32D2D;
  }
  .danger-zone__chevron {
    font-size: calc(10px * var(--font-scale));
    color: #A32D2D;
    transition: transform .15s;
    display: inline-block;
  }
  .danger-zone__chevron.open { transform: rotate(90deg); }

  .danger-zone__body {
    padding: calc(16px * var(--font-scale)) calc(16px * var(--font-scale));
    border-top: 0.5px solid rgba(162,45,45,0.2);
    display: flex; flex-direction: column;
    gap: calc(16px * var(--font-scale));
  }

  .danger-zone__warning {
    display: flex; gap: calc(10px * var(--font-scale));
    align-items: flex-start;
    background: #FCEBEB;
    border: 0.5px solid rgba(162,45,45,0.3);
    border-radius: 6px;
    padding: calc(12px * var(--font-scale));
  }
  .warning-icon {
    width: 20px; height: 20px; border-radius: 50%;
    background: #A32D2D; color: #fff;
    display: flex; align-items: center; justify-content: center;
    font-size: 12px; font-weight: 500; flex-shrink: 0;
  }
  .warning-text {
    font-size: calc(12px * var(--font-scale));
    color: #791F1F; line-height: 1.5;
  }
  .warning-text strong { color: #501313; }

  .confirm-block { display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); }
  .confirm-hint {
    font-size: calc(12px * var(--font-scale));
    color: var(--text-secondary);
  }
  .confirm-code {
    font-size: calc(18px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    font-weight: 500;
    letter-spacing: .2em;
    color: #A32D2D;
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    background: #FCEBEB;
    border-radius: 6px;
    text-align: center;
    width: fit-content;
  }
  .confirm-input {
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 6px;
    font-size: calc(15px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    letter-spacing: .15em;
    text-transform: uppercase;
    color: var(--text-primary);
    background: var(--bg-surface);
    outline: none;
    transition: border-color .12s;
    width: 160px;
    text-align: center;
  }
  .confirm-input:focus { border-color: var(--border-default); }
  .confirm-input.match { border-color: #A32D2D; color: #A32D2D; }
  .confirm-error {
    font-size: calc(12px * var(--font-scale));
    color: #A32D2D;
  }

  .btn-delete {
    padding: calc(9px * var(--font-scale)) calc(16px * var(--font-scale));
    background: #A32D2D; color: #fff;
    border: none; border-radius: 6px; cursor: pointer;
    font-size: calc(13px * var(--font-scale));
    transition: opacity .12s;
    width: fit-content;
  }
  .btn-delete:hover:not(:disabled) { opacity: 0.85; }
  .btn-delete:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
