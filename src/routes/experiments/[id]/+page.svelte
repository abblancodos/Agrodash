<!-- src/routes/experiments/[id]/+page.svelte -->
<script lang="ts">
  import { auth } from '$lib/stores/auth';
  import { experimentStore, canEdit, canAdmin } from '$lib/stores/experiment';
  import OverviewTab from '$lib/components/experiments/OverviewTab.svelte';
  import EntriesTab from '$lib/components/experiments/EntriesTab.svelte';
  import DefinitionsTab from '$lib/components/experiments/DefinitionsTab.svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  type Tab = 'overview' | 'entries' | 'definitions';
  let activeTab = $state<Tab>('overview');

  const exp = $derived($experimentStore.experiment!);

  function statusLabel(s: string) {
    return s === 'active' ? 'activo' : s === 'completed' ? 'completado' : 'archivado';
  }
  function statusColor(s: string) {
    return s === 'active' ? 'var(--live-color)' : 'var(--text-muted)';
  }

  async function downloadCsv() {
        const res = await fetch(`${API}/api/v1/experiments/${exp.id}/export-csv`, {
          });
    if (!res.ok) return;
    const csv  = await res.text();
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement('a');
    a.href     = url;
    a.download = `${exp.title.replace(/\s+/g, '-').toLowerCase()}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="page">
  <!-- Topbar -->
  <div class="topbar">
    <div class="topbar__left">
      <div class="breadcrumb">
        <a href="/" class="back-link">dashboard</a>
        <span class="breadcrumb-sep">/</span>
        <a href="/experiments" class="back-link">experimentos</a>
      </div>
      <div class="topbar__info">
        <h1 class="exp-title">{exp.title}</h1>
        <div class="exp-meta">
          {#if exp.description}<span>{exp.description}</span>{/if}
          <span class="status-dot" style="color:{statusColor(exp.status)}">
            {statusLabel(exp.status)}
          </span>
          {#if exp.public}<span class="badge-pub">público</span>{/if}
          {#if exp.user_role}
            <span class="badge-role badge-role--{exp.user_role}">{exp.user_role}</span>
          {/if}
        </div>
      </div>
    </div>
    <div class="topbar__right">
      <button class="btn-csv" onclick={downloadCsv} title="Descargar CSV">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5"
             stroke-linecap="round" stroke-linejoin="round" width="13" height="13">
          <path d="M2 10v2h10v-2M7 2v7M4 6l3 3 3-3"/>
        </svg>
        CSV
      </button>
    </div>
  </div>

  <!-- Tabs -->
  <div class="tabs">
    {#each ([['overview','overview'],['entries','entries'],['definitions','definitions']] as const) as [id, label]}
      <button class="tab" class:active={activeTab === id}
              onclick={() => activeTab = id}>
        {label}
      </button>
    {/each}
  </div>

  <!-- Tab content -->
  <div class="content">
    {#if activeTab === 'overview'}
      <OverviewTab />
    {:else if activeTab === 'entries'}
      <EntriesTab />
    {:else}
      <DefinitionsTab onDeleted={() => { window.location.href = '/experiments'; }} />
    {/if}
  </div>
</div>

<style>
  .page { max-width: 1100px; margin: 0 auto; padding: 0; }

  .topbar {
    display: flex; align-items: flex-start; justify-content: space-between;
    gap: 16px; padding: calc(16px * var(--font-scale)) calc(20px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
    flex-wrap: wrap;
  }
  .topbar__left { display: flex; flex-direction: column; gap: calc(4px * var(--font-scale)); }
  .topbar__right { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); flex-shrink: 0; }

  .breadcrumb { display: flex; align-items: center; gap: calc(6px * var(--font-scale)); }
  .breadcrumb-sep { font-size: calc(12px * var(--font-scale)); color: var(--border-default); }
  .back-link {
    font-size: calc(12px * var(--font-scale)); color: var(--text-muted);
    text-decoration: none; transition: color .12s;
  }
  .back-link:hover { color: var(--text-primary); }

  .exp-title {
    font-size: calc(18px * var(--font-scale)); font-weight: 500;
    color: var(--text-primary); line-height: 1.2;
  }
  .exp-meta {
    display: flex; align-items: center; gap: calc(10px * var(--font-scale));
    flex-wrap: wrap; font-size: calc(12px * var(--font-scale));
    color: var(--text-secondary); margin-top: 2px;
  }
  .status-dot { font-size: calc(11px * var(--font-scale)); }
  .badge-pub {
    font-size: calc(10px * var(--font-scale)); padding: 2px 7px;
    border-radius: 20px; background: var(--bg-elevated); color: var(--text-muted);
  }
  .badge-role {
    font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px;
  }
  .badge-role--admin  { background: rgba(29,158,117,0.12); color: var(--live-color); }
  .badge-role--editor { background: rgba(56,138,221,0.12); color: #185FA5; }
  .badge-role--viewer { background: var(--bg-elevated); color: var(--text-muted); }

  .btn-csv {
    display: flex; align-items: center; gap: 6px;
    padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 6px;
    background: none; color: var(--text-secondary); cursor: pointer;
    font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace;
    transition: all .12s;
  }
  .btn-csv:hover { background: var(--interactive-hover); color: var(--text-primary); }

  .tabs {
    display: flex; border-bottom: 0.5px solid var(--border-subtle);
    padding: 0 calc(20px * var(--font-scale));
    background: var(--bg-surface);
  }
  .tab {
    padding: calc(10px * var(--font-scale)) calc(16px * var(--font-scale));
    font-size: calc(13px * var(--font-scale)); color: var(--text-secondary);
    background: none; border: none; border-bottom: 2px solid transparent;
    cursor: pointer; transition: all .12s;
  }
  .tab:hover { color: var(--text-primary); }
  .tab.active {
    color: var(--text-primary);
    border-bottom-color: var(--text-primary);
  }

  .content {
    padding: calc(20px * var(--font-scale)) calc(20px * var(--font-scale));
  }

  @media (max-width: 640px) {
    .topbar { padding: 12px; }
    .content { padding: 12px; }
    .tabs { padding: 0 12px; }
    .exp-title { font-size: calc(15px * var(--font-scale)); }
  }
</style>