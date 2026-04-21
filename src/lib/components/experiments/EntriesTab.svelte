<script lang="ts">
  import { eventsWithCorrections, canEdit } from '$lib/stores/experiment';
  import EntryRow from './EntryRow.svelte';
  import EntryForm from './EntryForm.svelte';
  import ConflictBanner from './ConflictBanner.svelte';

  let addOpen = $state(false);

  const rows = $derived($eventsWithCorrections);
  const isEmpty = $derived(rows.length === 0);
</script>

<div class="entries-tab">
  <ConflictBanner />

  {#if isEmpty}
    <div class="empty-state">
      <p class="empty-title">sin entries todavía</p>
      <p class="empty-sub">
        Antes de registrar datos, definí las constantes, expresiones y pasos del experimento
        en la pestaña <em>definitions</em>.
      </p>
      {#if $canEdit}
        <button class="btn-add-first" onclick={() => addOpen = true}>
          + registrar primera entry
        </button>
      {/if}
    </div>
  {:else}
    <!-- Cabecera de columnas -->
    <div class="entries-head">
      <span class="col-h">timestamp</span>
      <span class="col-h">paso</span>
      <span class="col-h">datos</span>
      <span class="col-h">por</span>
      <span class="col-h">estado</span>
      <span></span>
    </div>

    {#each rows as { event, correction } (event.id)}
      <EntryRow {event} {correction} />
    {/each}

    {#if $canEdit}
      <button class="btn-add" onclick={() => addOpen = true}>
        <span class="plus">+</span>
        nueva entry
      </button>
    {/if}
  {/if}
</div>

{#if addOpen}
  <EntryForm onClose={() => addOpen = false} />
{/if}

<style>
  .entries-tab { }

  .empty-state { display: flex; flex-direction: column; align-items: center; gap: calc(12px * var(--font-scale)); padding: calc(48px * var(--font-scale)) 0; text-align: center; }
  .empty-title { font-size: calc(15px * var(--font-scale)); color: var(--text-secondary); }
  .empty-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); line-height: 1.5; max-width: 400px; }
  .btn-add-first { padding: calc(8px * var(--font-scale)) calc(18px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); margin-top: 4px; }

  .entries-head {
    display: grid;
    grid-template-columns: 110px 120px 1fr 80px 70px 32px;
    gap: calc(8px * var(--font-scale));
    padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale));
    background: var(--bg-elevated);
    border-radius: 6px;
    margin-bottom: calc(6px * var(--font-scale));
  }
  .col-h { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-weight: 500; }

  .btn-add {
    display: flex; align-items: center; gap: 6px;
    padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale));
    border: 0.5px dashed var(--border-default);
    border-radius: 6px;
    background: none; cursor: pointer;
    font-size: calc(12px * var(--font-scale));
    color: var(--text-secondary);
    margin-top: calc(8px * var(--font-scale));
    transition: all .12s;
  }
  .btn-add:hover { border-color: var(--text-muted); color: var(--text-primary); }
  .plus { font-size: calc(16px * var(--font-scale)); line-height: 1; }

  @media (max-width: 640px) {
    .entries-head { display: none; }
  }
</style>
