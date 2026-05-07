<!-- src/lib/components/processes/ProcessCollaborators.svelte -->
<!--
  Card de colaboradores para el ConfigTab del proceso.
  Uso:
    <ProcessCollaborators {processId} />

  Roles:
    admin   — puede editar config, agregar/quitar colaboradores, iniciar/detener
    operator — puede ver monitor y enviar comandos (override, clear)
    viewer   — solo lectura
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { processStore } from '$lib/stores/process.svelte';

  let { processId }: { processId: string } = $props();

  const API = (import.meta as any).env?.VITE_API_BASE ?? '';

  // ── State ─────────────────────────────────────────────────────────────────
  interface Collab {
    user_id: string;
    display_name: string;
    email: string;
    role: 'admin' | 'operator' | 'viewer';
  }

  let collabs    = $state<Collab[]>([]);
  let loading    = $state(true);
  let error      = $state('');

  // Búsqueda de usuario para agregar
  let searchQ    = $state('');
  let searchRes  = $state<{ id: string; display_name: string; email: string }[]>([]);
  let searching  = $state(false);
  let addOpen    = $state(false);
  let newRole    = $state<'admin' | 'operator' | 'viewer'>('operator');
  let addError   = $state('');
  let addLoading = $state(false);

  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  const canAdmin = $derived(processStore.canAdmin);

  // ── Load ──────────────────────────────────────────────────────────────────
  onMount(loadCollabs);

  async function loadCollabs() {
    loading = true; error = '';
    try {
      const r = await fetch(`${API}/api/v1/processes/${processId}/collaborators`, {
        credentials: 'include',
      });
      if (!r.ok) throw new Error(`${r.status}`);
      collabs = await r.json();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  // ── Search users ──────────────────────────────────────────────────────────
  function onSearchInput() {
    if (searchTimer) clearTimeout(searchTimer);
    if (searchQ.trim().length < 2) { searchRes = []; return; }
    searchTimer = setTimeout(doSearch, 300);
  }

  async function doSearch() {
    searching = true;
    try {
      const r = await fetch(
        `${API}/api/v1/users/search?q=${encodeURIComponent(searchQ)}`,
        { credentials: 'include' }
      );
      if (r.ok) {
        const data = await r.json();
        searchRes = Array.isArray(data) ? data : (data.users ?? data.results ?? []);
      }
    } finally { searching = false; }
  }

  // ── Add collaborator ──────────────────────────────────────────────────────
  async function addCollab(userId: string, name: string, email: string) {
    addLoading = true; addError = '';
    try {
      const r = await fetch(`${API}/api/v1/processes/${processId}/collaborators`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ user_id: userId, role: newRole }),
      });
      if (!r.ok) {
        const j = await r.json().catch(() => ({}));
        throw new Error(j.error ?? `${r.status}`);
      }
      collabs = [...collabs, { user_id: userId, display_name: name, email, role: newRole }];
      searchQ = ''; searchRes = []; addOpen = false;
    } catch (e: any) {
      addError = e.message;
    } finally { addLoading = false; }
  }

  // ── Remove collaborator ───────────────────────────────────────────────────
  async function removeCollab(userId: string) {
    if (!confirm('¿Quitar acceso a este usuario?')) return;
    try {
      await fetch(`${API}/api/v1/processes/${processId}/collaborators/${userId}`, {
        method: 'DELETE',
        credentials: 'include',
      });
      collabs = collabs.filter(c => c.user_id !== userId);
    } catch {}
  }

  // ── Helpers ───────────────────────────────────────────────────────────────
  function initials(name: string) {
    return name.split(' ').map(w => w[0]).join('').slice(0, 2).toUpperCase();
  }

  const ROLE_COLOR: Record<string, string> = {
    admin:    '#085041',
    operator: '#0C447C',
    viewer:   '#5F5E5A',
  };
  const ROLE_BG: Record<string, string> = {
    admin:    '#E1F5EE',
    operator: '#E6F1FB',
    viewer:   '#F1EFE8',
  };
  const ROLE_LABEL: Record<string, string> = {
    admin:    'admin',
    operator: 'operador',
    viewer:   'visor',
  };

  // Ya está en collabs?
  function alreadyAdded(userId: string) {
    return collabs.some(c => c.user_id === userId);
  }
</script>

<div class="collab-card">
  <div class="card-header">
    <div class="card-title">acceso al proceso</div>
    {#if canAdmin}
      <button class="btn-add" onclick={() => { addOpen = !addOpen; addError = ''; }}>
        {addOpen ? '✕ cancelar' : '+ agregar'}
      </button>
    {/if}
  </div>

  <!-- Add panel -->
  {#if addOpen && canAdmin}
    <div class="add-panel">
      <div class="search-row">
        <input
          class="inp"
          placeholder="buscar por nombre o email..."
          bind:value={searchQ}
          oninput={onSearchInput}
          autofocus
        />
        <select class="inp inp--role" bind:value={newRole}>
          <option value="admin">admin</option>
          <option value="operator">operador</option>
          <option value="viewer">visor</option>
        </select>
      </div>

      {#if addError}
        <div class="add-error">{addError}</div>
      {/if}

      {#if searching}
        <div class="search-hint">buscando...</div>
      {:else if searchRes.length > 0}
        <div class="search-results">
          {#each searchRes as u (u.id)}
            {@const added = alreadyAdded(u.id)}
            <button
              class="search-row-result"
              class:already={added}
              disabled={added || addLoading}
              onclick={() => addCollab(u.id, u.display_name, u.email)}
            >
              <div class="avatar-sm" style="background:{ROLE_BG[newRole]};color:{ROLE_COLOR[newRole]}">
                {initials(u.display_name)}
              </div>
              <div class="result-info">
                <span class="result-name">{u.display_name}</span>
                <span class="result-email">{u.email}</span>
              </div>
              <span class="result-action">
                {added ? '✓ ya tiene acceso' : addLoading ? '...' : '+ agregar'}
              </span>
            </button>
          {/each}
        </div>
      {:else if searchQ.length >= 2}
        <div class="search-hint">sin resultados para "{searchQ}"</div>
      {:else}
        <div class="search-hint">escribí al menos 2 caracteres</div>
      {/if}

      <!-- Role descriptions -->
      <div class="role-desc">
        <span style="color:{ROLE_COLOR.admin}; background:{ROLE_BG.admin}" class="role-pill">admin</span>
        editar config, agregar/quitar acceso, iniciar/detener ·
        <span style="color:{ROLE_COLOR.operator}; background:{ROLE_BG.operator}" class="role-pill">operador</span>
        ver monitor, enviar comandos ·
        <span style="color:{ROLE_COLOR.viewer}; background:{ROLE_BG.viewer}" class="role-pill">visor</span>
        solo lectura
      </div>
    </div>
  {/if}

  <!-- Collaborator list -->
  {#if loading}
    <div class="hint">cargando...</div>
  {:else if error}
    <div class="hint err">{error}</div>
  {:else if collabs.length === 0}
    <div class="hint">solo vos tenés acceso a este proceso.</div>
  {:else}
    <div class="collab-list">
      {#each collabs as c (c.user_id)}
        <div class="collab-row">
          <div class="avatar"
            style="background:{ROLE_BG[c.role] ?? ROLE_BG.viewer};color:{ROLE_COLOR[c.role] ?? ROLE_COLOR.viewer}">
            {initials(c.display_name)}
          </div>
          <div class="collab-info">
            <span class="collab-name">{c.display_name}</span>
            <span class="collab-email">{c.email}</span>
          </div>
          <span class="role-badge"
            style="background:{ROLE_BG[c.role] ?? ROLE_BG.viewer};color:{ROLE_COLOR[c.role] ?? ROLE_COLOR.viewer}">
            {ROLE_LABEL[c.role] ?? c.role}
          </span>
          {#if canAdmin}
            <button class="btn-remove" onclick={() => removeCollab(c.user_id)} title="quitar acceso">✕</button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .collab-card {
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-subtle);
    border-radius: 8px;
    padding: calc(12px * var(--font-scale)) calc(14px * var(--font-scale));
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .card-title {
    font-size: calc(11px * var(--font-scale));
    font-family: 'DM Mono', monospace;
    color: var(--text-muted);
    letter-spacing: .05em;
  }
  .btn-add {
    font-size: calc(12px * var(--font-scale));
    color: var(--text-secondary);
    background: none;
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .btn-add:hover { background: var(--interactive-hover); }

  /* Add panel */
  .add-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 6px;
  }
  .search-row {
    display: flex;
    gap: 6px;
  }
  .inp {
    padding: calc(5px * var(--font-scale)) calc(8px * var(--font-scale));
    border: 0.5px solid var(--border-default);
    border-radius: 5px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: calc(12px * var(--font-scale));
    outline: none;
    flex: 1;
  }
  .inp:focus { border-color: var(--text-primary); }
  .inp--role { flex: 0 0 100px; font-size: calc(11px * var(--font-scale)); }

  .search-hint {
    font-size: calc(11px * var(--font-scale));
    color: var(--text-muted);
    padding: 2px 0;
  }
  .add-error {
    font-size: calc(11px * var(--font-scale));
    color: var(--error-color);
  }

  .search-results {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
  }
  .search-row-result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: none;
    border-radius: 5px;
    background: none;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background .1s;
  }
  .search-row-result:hover:not(:disabled) { background: var(--interactive-hover); }
  .search-row-result.already { opacity: 0.5; cursor: default; }
  .search-row-result:disabled { cursor: default; }

  .avatar-sm {
    width: 26px; height: 26px;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: calc(10px * var(--font-scale));
    font-weight: 600;
    flex-shrink: 0;
  }
  .result-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .result-name { font-size: calc(12px * var(--font-scale)); color: var(--text-primary); }
  .result-email { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); }
  .result-action { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); flex-shrink: 0; }

  .role-desc {
    font-size: calc(9px * var(--font-scale));
    color: var(--text-muted);
    line-height: 1.8;
  }
  .role-pill {
    padding: 1px 6px;
    border-radius: 10px;
    font-size: calc(9px * var(--font-scale));
    font-weight: 500;
  }

  /* Collab list */
  .collab-list { display: flex; flex-direction: column; }
  .collab-row {
    display: flex;
    align-items: center;
    gap: calc(10px * var(--font-scale));
    padding: calc(7px * var(--font-scale)) 0;
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .collab-row:last-child { border-bottom: none; }
  .avatar {
    width: 30px; height: 30px;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: calc(11px * var(--font-scale));
    font-weight: 600;
    flex-shrink: 0;
  }
  .collab-info { flex: 1; min-width: 0; }
  .collab-name { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); display: block; }
  .collab-email { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); display: block; }
  .role-badge {
    font-size: calc(10px * var(--font-scale));
    padding: 2px 8px;
    border-radius: 20px;
    flex-shrink: 0;
    font-weight: 500;
  }
  .btn-remove {
    background: none; border: none;
    cursor: pointer;
    font-size: calc(11px * var(--font-scale));
    color: var(--text-muted);
    padding: 2px 4px;
    flex-shrink: 0;
  }
  .btn-remove:hover { color: #A32D2D; }

  .hint {
    font-size: calc(12px * var(--font-scale));
    color: var(--text-muted);
    padding: 4px 0;
  }
  .hint.err { color: var(--error-color); }
</style>