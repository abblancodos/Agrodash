<!-- src/routes/processes/+page.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { auth, isLoggedIn, currentUser } from '$lib/stores/auth';
  import LoginModal from '$lib/components/LoginModal.svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let loginOpen  = $state(false);
  let processes  = $state<any[]>([]);
  let loading    = $state(true);
  let error      = $state('');
  let showNew    = $state(false);

  // Formulario nuevo proceso
  let newName       = $state('');
  let newDesc       = $state('');
  let newUrl        = $state('');
  let newKey        = $state('');
  let newSaving     = $state(false);
  let newError      = $state('');

  async function load() {
    loading = true; error = '';
    try {
      const res = await fetch(`${API}/api/v1/processes`, { credentials: 'include' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      processes = await res.json();
    } catch (e: any) {
      error = e.message;
    } finally { loading = false; }
  }

  async function createProcess() {
    if (!newName.trim() || !newUrl.trim() || !newKey.trim()) {
      newError = 'Nombre, URL y API key son obligatorios'; return;
    }
    newSaving = true; newError = '';
    try {
      const res = await fetch(`${API}/api/v1/processes`, {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: newName, description: newDesc || null,
          control_url: newUrl, api_key: newKey,
          type: 'irrigation_kalman',
        }),
      });
      if (!res.ok) { const d = await res.json(); throw new Error(d.error); }
      showNew = false; newName = ''; newDesc = ''; newUrl = ''; newKey = '';
      await load();
    } catch (e: any) {
      newError = e.message;
    } finally { newSaving = false; }
  }

  onMount(async () => {
    await auth.init();
    await load();
  });

  $effect(() => { void $isLoggedIn; load(); });

  function statusDot(s: string) {
    return s === 'running' ? '#3da85a' : s === 'error' ? '#e05454' : '#8a9bb0';
  }
  function statusLabel(s: string) {
    return s === 'running' ? 'activo' : s === 'stopped' ? 'detenido' : s === 'error' ? 'error' : 'desconocido';
  }
  function relTime(ts: string | null) {
    if (!ts) return '—';
    const diff = Date.now() - new Date(ts).getTime();
    const m = Math.floor(diff / 60000);
    if (m < 1) return 'ahora';
    if (m < 60) return `hace ${m}m`;
    const h = Math.floor(m / 60);
    if (h < 24) return `hace ${h}h`;
    return `hace ${Math.floor(h/24)}d`;
  }
</script>

<div class="page">
  <a href="/" class="back">← dashboard</a>

  <div class="page-head">
    <div>
      <h1 class="page-title">procesos</h1>
      <p class="page-sub">automatización y control en tiempo real</p>
    </div>
    <div class="page-head__right">
      {#if $isLoggedIn}
        <div class="user-pill">
          <span class="avatar">{$currentUser?.display_name?.[0]?.toUpperCase() ?? '?'}</span>
          <span class="user-name">{$currentUser?.display_name}</span>
          <button class="btn-logout" onclick={() => auth.logout()}>salir</button>
        </div>
        <button class="btn-new" onclick={() => showNew = !showNew}>+ nuevo proceso</button>
      {:else}
        <button class="btn-login" onclick={() => loginOpen = true}>acceder</button>
      {/if}
    </div>
  </div>

  {#if showNew}
    <div class="new-form">
      <div class="new-form__head">
        <span class="new-form__title">nuevo proceso</span>
        <button class="close-btn" onclick={() => showNew = false}>✕</button>
      </div>
      {#if newError}<div class="form-err">{newError}</div>{/if}
      <div class="form-grid">
        <div class="field">
          <label>nombre</label>
          <input bind:value={newName} placeholder="Riego Invernadero" />
        </div>
        <div class="field">
          <label>descripción <span class="opt">(opcional)</span></label>
          <input bind:value={newDesc} placeholder="Control de 6 líneas con Kalman" />
        </div>
        <div class="field">
          <label>URL del control</label>
          <input bind:value={newUrl} placeholder="http://100.104.63.6:8090" class="mono" />
        </div>
        <div class="field">
          <label>API key</label>
          <input bind:value={newKey} type="password" placeholder="••••••••" />
        </div>
      </div>
      <div class="form-actions">
        <button class="btn-cancel" onclick={() => showNew = false}>cancelar</button>
        <button class="btn-save" disabled={newSaving} onclick={createProcess}>
          {newSaving ? 'creando...' : 'crear proceso'}
        </button>
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="loading-row"><div class="spinner"></div>cargando...</div>
  {:else if error}
    <div class="error-banner">{error}</div>
  {:else if processes.length === 0}
    <div class="empty-state">
      <p class="empty-title">no hay procesos configurados</p>
      <p class="empty-sub">
        {#if $isLoggedIn}
          <button class="btn-link" onclick={() => showNew = true}>crear el primer proceso</button>
        {:else}
          <button class="btn-link" onclick={() => loginOpen = true}>accedé</button> para ver o crear procesos
        {/if}
      </p>
    </div>
  {:else}
    <div class="process-grid">
      {#each processes as p (p.id)}
        <a href="/processes/{p.id}" class="process-card">
          <div class="card-head">
            <span class="status-dot" style="background:{statusDot(p.status)}"></span>
            <span class="card-name">{p.name}</span>
            <span class="card-role">{p.user_role ?? ''}</span>
          </div>
          {#if p.description}
            <p class="card-desc">{p.description}</p>
          {/if}
          <div class="card-foot">
            <span class="card-status" style="color:{statusDot(p.status)}">{statusLabel(p.status)}</span>
            <span class="card-seen">último dato {relTime(p.last_seen_at)}</span>
          </div>
          {#if p.last_state?.kalman}
            <div class="card-kalman">
              <span class="kalman-badge" class:convergido={p.last_state.kalman.convergido}>
                Kalman {p.last_state.kalman.convergido ? 'convergido ✓' : 'convergiendo...'}
              </span>
            </div>
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>

<LoginModal bind:open={loginOpen} />

<style>
  .page { max-width: 960px; margin: 0 auto; padding: calc(24px * var(--font-scale)) calc(20px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(20px * var(--font-scale)); }

  .back { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-decoration: none; }
  .back:hover { color: var(--text-primary); }

  .page-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
  .page-head__right { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); }
  .page-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .page-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); margin-top: 2px; }

  .btn-new { padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(13px * var(--font-scale)); color: var(--text-primary); }
  .btn-new:hover { background: var(--interactive-hover); }
  .btn-login { padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .user-pill { display: flex; align-items: center; gap: 8px; }
  .avatar { width: 28px; height: 28px; border-radius: 50%; background: var(--interactive-hover); display: flex; align-items: center; justify-content: center; font-size: calc(12px * var(--font-scale)); font-weight: 500; }
  .user-name { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .btn-logout { background: none; border: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }

  /* Formulario nuevo proceso */
  .new-form { background: var(--bg-elevated); border: 0.5px solid var(--border-default); border-radius: 10px; padding: calc(16px * var(--font-scale)); display: flex; flex-direction: column; gap: calc(14px * var(--font-scale)); }
  .new-form__head { display: flex; align-items: center; justify-content: space-between; }
  .new-form__title { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-primary); font-family: 'DM Mono', monospace; letter-spacing: .04em; }
  .close-btn { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: 14px; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: calc(10px * var(--font-scale)); }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .field input { padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(13px * var(--font-scale)); outline: none; }
  .field input:focus { border-color: var(--text-primary); }
  .mono { font-family: 'DM Mono', monospace; }
  .opt { color: var(--text-muted); font-weight: 400; }
  .form-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn-cancel { background: none; border: 0.5px solid var(--border-default); border-radius: 6px; padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale)); cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); }
  .btn-save { background: var(--text-primary); color: var(--bg-surface); border: none; border-radius: 6px; padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale)); cursor: pointer; font-size: calc(13px * var(--font-scale)); font-weight: 500; }
  .btn-save:disabled { opacity: 0.5; }
  .form-err { background: var(--error-bg); color: var(--error-color); padding: 8px 12px; border-radius: 6px; font-size: calc(12px * var(--font-scale)); }

  /* Grid de procesos */
  .process-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: calc(12px * var(--font-scale)); }

  .process-card { display: flex; flex-direction: column; gap: calc(8px * var(--font-scale)); padding: calc(14px * var(--font-scale)); background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 10px; text-decoration: none; transition: all .12s; }
  .process-card:hover { background: var(--interactive-hover); border-color: var(--border-strong, var(--border-default)); }

  .card-head { display: flex; align-items: center; gap: 8px; }
  .status-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .card-name { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); flex: 1; }
  .card-role { font-size: calc(10px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 1px 6px; border-radius: 10px; }
  .card-desc { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.5; }
  .card-foot { display: flex; align-items: center; justify-content: space-between; }
  .card-status { font-size: calc(11px * var(--font-scale)); font-weight: 500; font-family: 'DM Mono', monospace; }
  .card-seen { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .card-kalman { margin-top: 2px; }
  .kalman-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 10px; background: var(--bg-elevated); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .kalman-badge.convergido { background: #EAF3DE; color: #3B6D11; }

  .empty-state { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 60px 0; text-align: center; }
  .empty-title { font-size: calc(15px * var(--font-scale)); color: var(--text-secondary); }
  .empty-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); }
  .btn-link { background: none; border: none; cursor: pointer; color: var(--text-primary); text-decoration: underline; font-size: inherit; padding: 0; }

  .loading-row { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); }
  .spinner { width: 16px; height: 16px; border: 2px solid var(--border-subtle); border-top-color: var(--text-muted); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error-banner { background: var(--error-bg); color: var(--error-color); padding: 10px 14px; border-radius: 6px; font-size: calc(13px * var(--font-scale)); }

  @media (max-width: 640px) {
    .form-grid { grid-template-columns: 1fr; }
    .process-grid { grid-template-columns: 1fr; }
  }
</style>
