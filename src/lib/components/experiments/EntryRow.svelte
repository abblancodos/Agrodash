<script lang="ts">
  import type { ExperimentEvent } from '$lib/stores/experiment';
  import { experimentContext, canEdit } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import CorrectionForm from './CorrectionForm.svelte';
  import EntryRowSelf from './EntryRow.svelte';

  let { event, correction = null }:
    { event: ExperimentEvent; correction: ExperimentEvent | null } = $props();

  let correcting = $state(false);
  let expanded   = $state(false);

  const isVoided     = $derived(event.is_voided);
  const isCorrection = $derived(!!event.corrects_event_id);
  const canCorrect   = $derived(
    $canEdit &&
    !isVoided &&
    !isCorrection &&
    (event.recorded_by === $auth.user?.id || $auth.user !== null)
  );

  function relTime(iso: string) {
    const secs = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs/60)} min`;
    if (secs < 86400) return `${Math.floor(secs/3600)} h`;
    return new Date(iso).toLocaleDateString('es-CR', { day:'2-digit', month:'short' });
  }

  function formatTs(iso: string) {
    return new Date(iso).toLocaleString('es-CR', {
      day:'2-digit', month:'short', hour:'2-digit', minute:'2-digit', hour12: false,
    });
  }

  // Mostrar los campos más relevantes del data
  function dataPreview(data: Record<string, unknown>) {
    return Object.entries(data)
      .filter(([k]) => k !== '_meta')
      .map(([k, v]) => {
        if (typeof v === 'number') return `${k}: ${v.toFixed(4).replace(/\.?0+$/, '')}`;
        if (typeof v === 'string') return `${k}: ${v}`;
        return null;
      })
      .filter(Boolean)
      .slice(0, 3)
      .join(' · ');
  }
</script>

<!-- Si está anulada, mostrar tachada con conector a su corrección -->
{#if isVoided}
  <div class="entry-row entry-row--voided">
    <span class="cell-muted ts">{formatTs(event.recorded_at)}</span>
    <span class="cell-strike">{event.step_key}</span>
    <span class="cell-strike data">{dataPreview(event.data)}</span>
    <span class="badge badge-voided">anulada</span>
  </div>
  {#if correction}
    <div class="correction-connector">
      <div class="connector-line"></div>
      <EntryRowSelf event={correction} correction={null} />
    </div>
  {/if}

{:else}
  <div class="entry-row" class:entry-row--correction={isCorrection}
       class:entry-row--expanded={expanded}>
    <span class="cell-muted ts">{formatTs(event.recorded_at)}</span>
    <span class="cell step">{event.step_key}{isCorrection ? ' ✎' : ''}</span>
    <span class="cell data">{dataPreview(event.data)}</span>
    <span class="cell-muted by">{event.recorded_by_name ?? '—'}</span>
    {#if isCorrection}
      <span class="badge badge-correction">corrección</span>
    {:else}
      <span class="badge badge-active">activa</span>
    {/if}

    <!-- Menú -->
    {#if canCorrect || Object.keys(event.data).length > 3}
      <button class="btn-menu" onclick={() => expanded = !expanded} aria-label="opciones">
        ···
      </button>
    {:else}
      <span></span>
    {/if}
  </div>

  <!-- Dropdown de acciones -->
  {#if expanded}
    <div class="entry-detail">
      <div class="detail-data">
        {#each Object.entries(event.data) as [k, v]}
          {#if typeof v === 'number' || typeof v === 'string'}
            <div class="detail-row">
              <span class="detail-key">{k}</span>
              <span class="detail-val">{typeof v === 'number' ? v.toFixed(6).replace(/\.?0+$/, '') : v}</span>
            </div>
          {/if}
        {/each}
        {#if event.note}
          <div class="detail-note">nota: {event.note}</div>
        {/if}
        {#if event.correction_reason}
          <div class="detail-note">motivo corrección: {event.correction_reason}</div>
        {/if}
      </div>
      {#if canCorrect}
        <button class="btn-correct" onclick={() => { correcting = true; expanded = false; }}>
          corregir esta entry
        </button>
      {/if}
    </div>
  {/if}

  {#if correcting}
    <CorrectionForm
      {event}
      onClose={() => correcting = false}
    />
  {/if}
{/if}

<style>
  .entry-row {
    display: grid;
    grid-template-columns: 110px 120px 1fr 80px 70px 32px;
    gap: calc(8px * var(--font-scale));
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-subtle);
    border-radius: 6px; margin-bottom: calc(3px * var(--font-scale));
    align-items: center;
  }
  .entry-row--correction { border-color: #639922; background: #EAF3DE10; }
  .entry-row--voided { opacity: 0.45; background: var(--bg-elevated); grid-template-columns: 110px 120px 1fr 70px; }
  .entry-row--expanded { border-bottom-left-radius: 0; border-bottom-right-radius: 0; }

  .ts    { font-size: calc(11px * var(--font-scale)); }
  .step  { font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-primary); }
  .data  { font-size: calc(11px * var(--font-scale)); font-family: 'DM Mono', monospace; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .by    { font-size: calc(11px * var(--font-scale)); }
  .cell-muted { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .cell-strike { font-size: calc(12px * var(--font-scale)); text-decoration: line-through; color: var(--text-muted); }

  .badge { font-size: calc(10px * var(--font-scale)); padding: 2px 6px; border-radius: 20px; justify-self: start; }
  .badge-active     { background: #EAF3DE; color: #3B6D11; }
  .badge-voided     { background: #F1EFE8; color: #5F5E5A; }
  .badge-correction { background: #EAF3DE; color: #3B6D11; }

  .btn-menu {
    background: none; border: none; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); color: var(--text-muted);
    padding: 0 4px; letter-spacing: .1em;
  }

  .entry-detail {
    border: 0.5px solid var(--border-subtle); border-top: none;
    border-bottom-left-radius: 6px; border-bottom-right-radius: 6px;
    padding: calc(10px * var(--font-scale)) calc(12px * var(--font-scale));
    margin-bottom: calc(3px * var(--font-scale));
    background: var(--bg-elevated);
  }
  .detail-row { display: flex; justify-content: space-between; padding: 3px 0; font-size: calc(12px * var(--font-scale)); }
  .detail-key { color: var(--text-secondary); font-family: 'DM Mono', monospace; }
  .detail-val { color: var(--text-primary); font-family: 'DM Mono', monospace; font-weight: 500; }
  .detail-note { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); margin-top: 6px; }
  .btn-correct {
    margin-top: calc(10px * var(--font-scale));
    font-size: calc(12px * var(--font-scale)); color: #A32D2D;
    background: none; border: 0.5px solid rgba(162,45,45,0.3);
    border-radius: 4px; padding: 4px 10px; cursor: pointer;
  }

  .correction-connector { margin-left: calc(16px * var(--font-scale)); border-left: 2px solid #97C459; padding-left: calc(10px * var(--font-scale)); }
  .connector-line { display: none; }

  @media (max-width: 640px) {
    .entry-row { display: block !important; }
    .entry-row .by { display: none; }
    .ts { font-size: calc(10px * var(--font-scale)); }
  }
</style>