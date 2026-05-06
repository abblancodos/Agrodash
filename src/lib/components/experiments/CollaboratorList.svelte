<script lang="ts">
  import { experimentStore, canAdmin } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';
  import AddDefinitionMenu from './AddDefinitionMenu.svelte';

  let { onClose: _onClose }: { onClose?: () => void } = $props();
  let addOpen = $state(false);

  const owner = $derived($experimentStore.experiment?.owner_id);
  const collabs = $derived($experimentStore.collaborators);

  function initials(name: string) {
    return name.split(' ').map(w => w[0]).join('').slice(0, 2).toUpperCase();
  }

  function roleColor(role: string) {
    return role === 'admin' ? '#085041' : role === 'editor' ? '#0C447C' : '#5F5E5A';
  }
  function roleBg(role: string) {
    return role === 'admin' ? '#E1F5EE' : role === 'editor' ? '#E6F1FB' : '#F1EFE8';
  }
</script>

<div class="collab-list">
  <div class="collab-head">
    <span class="collab-title">colaboradores</span>
    {#if $canAdmin}
      <button class="btn-add" onclick={() => addOpen = true}>+ agregar</button>
    {/if}
  </div>

  <!-- Owner siempre primero -->
  {#if owner}
    <div class="collab-row">
      <div class="avatar" style="background:#E1F5EE;color:#085041">
        {initials($experimentStore.experiment?.title ?? '?')}
      </div>
      <div class="collab-info">
        <span class="collab-name">dueño del experimento</span>
      </div>
      <span class="role-badge" style="background:#E1F5EE;color:#085041">admin</span>
    </div>
  {/if}

  {#each collabs as c (c.user_id)}
    <div class="collab-row">
      <div class="avatar" style="background:{roleBg(c.role)};color:{roleColor(c.role)}">
        {initials(c.display_name)}
      </div>
      <div class="collab-info">
        <span class="collab-name">{c.display_name}</span>
        <span class="collab-email">{c.email}</span>
      </div>
      <span class="role-badge" style="background:{roleBg(c.role)};color:{roleColor(c.role)}">
        {c.role}
      </span>
      {#if $canAdmin && c.user_id !== $auth.user?.id}
        <button class="btn-remove" onclick={async () => {
          const API = import.meta.env.VITE_API_BASE ?? '';
          const token = auth.getToken();
          await fetch(`${API}/api/v1/experiments/${$experimentStore.experiment?.id}/collaborators/${c.user_id}`, {
            method: 'DELETE', headers: token ? { Authorization: `Bearer ${token}` } : {},
          });
          experimentStore.removeCollaborator(c.user_id);
        }}>✕</button>
      {/if}
    </div>
  {/each}

  {#if collabs.length === 0 && !$canAdmin}
    <p class="empty-hint">sin colaboradores adicionales</p>
  {/if}
</div>

{#if addOpen}
  <AddDefinitionMenu onClose={() => addOpen = false} />
{/if}

<style>
  .collab-list { }
  .collab-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: calc(10px * var(--font-scale)); }
  .collab-title { font-size: calc(11px * var(--font-scale)); font-weight: 500; color: var(--text-secondary); letter-spacing: .06em; text-transform: uppercase; }
  .btn-add { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); background: none; border: 0.5px solid var(--border-default); border-radius: 4px; padding: 3px 8px; cursor: pointer; }
  .collab-row { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); padding: calc(8px * var(--font-scale)) 0; border-bottom: 0.5px solid var(--border-subtle); }
  .collab-row:last-child { border-bottom: none; }
  .avatar { width: 30px; height: 30px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: calc(11px * var(--font-scale)); font-weight: 500; flex-shrink: 0; }
  .collab-info { flex: 1; min-width: 0; }
  .collab-name { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); display: block; }
  .collab-email { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); display: block; }
  .role-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px; flex-shrink: 0; }
  .btn-remove { background: none; border: none; cursor: pointer; font-size: calc(11px * var(--font-scale)); color: var(--text-muted); padding: 2px 4px; }
  .btn-remove:hover { color: #A32D2D; }
  .empty-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); padding: 8px 0; }
</style>