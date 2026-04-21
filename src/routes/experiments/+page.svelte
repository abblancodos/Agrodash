<script lang="ts">
  import { onMount } from 'svelte';
  import { auth, isLoggedIn, currentUser } from '$lib/stores/auth';
  import LoginModal from '$lib/components/LoginModal.svelte';
  import ExperimentCard from '$lib/components/experiments/ExperimentCard.svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let loginOpen  = $state(false);
  let experiments = $state<any[]>([]);
  let loading    = $state(true);
  let error      = $state('');

  async function loadExperiments() {
    loading = true; error = '';
    try {
      const token = auth.getToken();
      const res = await fetch(`${API}/api/v1/experiments`, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      experiments = await res.json();
    } catch (e: any) {
      error = e.message ?? 'Error cargando experimentos';
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await auth.init();
    await loadExperiments();
  });

  // Recargar cuando el usuario se loguea
  $effect(() => {
    void $isLoggedIn;
    loadExperiments();
  });

  const publicExps = $derived(experiments.filter(e => e.public));
  const myExps     = $derived(experiments.filter(e => e.user_role && e.user_role !== 'viewer'));

  function statusColor(s: string) {
    return s === 'active' ? 'var(--live-color)' : s === 'completed' ? 'var(--text-muted)' : 'var(--text-faint)';
  }
</script>

<div class="page">
  <!-- Header -->
  <div class="page-head">
    <div class="page-head__left">
      <h1 class="page-title">experimentos</h1>
      <p class="page-sub">datos de laboratorio y experimentos de campo</p>
    </div>
    <div class="page-head__right">
      {#if $isLoggedIn}
        <div class="user-pill">
          <span class="user-avatar">{$currentUser?.display_name?.[0]?.toUpperCase() ?? '?'}</span>
          <span class="user-name">{$currentUser?.display_name}</span>
          <button class="btn-logout" onclick={() => { auth.logout(); loadExperiments(); }}>
            salir
          </button>
        </div>
        <a href="/experiments/new" class="btn-new">+ nuevo experimento</a>
      {:else}
        <button class="btn-login" onclick={() => loginOpen = true}>
          acceder
        </button>
      {/if}
    </div>
  </div>

  <!-- Estado vacío global -->
  {#if !loading && experiments.length === 0 && !$isLoggedIn}
    <div class="empty-state">
      <div class="empty-state__icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32">
          <path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2"/>
          <rect x="9" y="3" width="6" height="4" rx="1"/>
          <line x1="9" y1="12" x2="15" y2="12"/><line x1="9" y1="16" x2="13" y2="16"/>
        </svg>
      </div>
      <p class="empty-state__title">no hay experimentos públicos todavía</p>
      <p class="empty-state__sub">
        <button class="btn-link" onclick={() => loginOpen = true}>accedé</button>
        para ver los experimentos a los que tenés acceso o crear uno nuevo
      </p>
    </div>

  {:else if !loading && experiments.length === 0 && $isLoggedIn}
    <div class="empty-state">
      <p class="empty-state__title">todavía no tenés experimentos</p>
      <p class="empty-state__sub">
        <a href="/experiments/new" class="btn-link">creá tu primer experimento</a>
        o pedile a alguien que te agregue como colaborador
      </p>
    </div>

  {:else}
    <!-- Mis experimentos (con rol de editor/admin) -->
    {#if $isLoggedIn && myExps.length > 0}
      <section class="section">
        <h2 class="section-title">mis experimentos</h2>
        <div class="exp-grid">
          {#each myExps as exp (exp.id)}
            <ExperimentCard experiment={exp} />
          {/each}
        </div>
      </section>
    {/if}

    <!-- Experimentos públicos -->
    {#if publicExps.length > 0}
      <section class="section">
        <h2 class="section-title">públicos</h2>
        {#if !$isLoggedIn}
          <p class="section-hint">
            solo visualización —
            <button class="btn-link" onclick={() => loginOpen = true}>accedé</button>
            para crear o colaborar
          </p>
        {/if}
        <div class="exp-grid">
          {#each publicExps as exp (exp.id)}
            <ExperimentCard experiment={exp} />
          {/each}
        </div>
      </section>
    {/if}

    {#if loading}
      <div class="loading-row">
        <div class="spinner"></div>
        cargando experimentos...
      </div>
    {/if}

    {#if error}
      <div class="error-banner">{error}</div>
    {/if}
  {/if}
</div>

<LoginModal bind:open={loginOpen} />

<style>
  .page { max-width: 960px; margin: 0 auto; padding: calc(24px * var(--font-scale)) calc(20px * var(--font-scale)); }

  .page-head { display: flex; align-items: flex-start; justify-content: space-between; margin-bottom: calc(28px * var(--font-scale)); gap: 16px; flex-wrap: wrap; }
  .page-head__right { display: flex; align-items: center; gap: calc(10px * var(--font-scale)); flex-shrink: 0; }
  .page-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }
  .page-sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); margin-top: 2px; }

  .btn-login {
    padding: calc(7px * var(--font-scale)) calc(16px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    transition: opacity .12s;
  }
  .btn-login:hover { opacity: 0.85; }

  .btn-new {
    padding: calc(7px * var(--font-scale)) calc(14px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 6px;
    color: var(--text-primary); text-decoration: none;
    font-size: calc(13px * var(--font-scale)); transition: all .12s;
  }
  .btn-new:hover { background: var(--interactive-hover); }

  .user-pill { display: flex; align-items: center; gap: calc(8px * var(--font-scale)); }
  .user-avatar {
    width: 28px; height: 28px; border-radius: 50%;
    background: var(--interactive-hover); color: var(--text-primary);
    display: flex; align-items: center; justify-content: center;
    font-size: calc(12px * var(--font-scale)); font-weight: 500;
  }
  .user-name { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .btn-logout {
    background: none; border: none; cursor: pointer;
    font-size: calc(12px * var(--font-scale)); color: var(--text-muted);
  }

  .section { margin-bottom: calc(32px * var(--font-scale)); }
  .section-title { font-size: calc(13px * var(--font-scale)); font-weight: 500; color: var(--text-muted); letter-spacing: .06em; text-transform: uppercase; margin-bottom: calc(12px * var(--font-scale)); }
  .section-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-bottom: calc(12px * var(--font-scale)); }

  .exp-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: calc(12px * var(--font-scale)); }

  .exp-card {
    display: flex; flex-direction: column; gap: calc(8px * var(--font-scale));
    padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale));
    background: var(--bg-surface); border: 0.5px solid var(--border-subtle);
    border-radius: 10px; text-decoration: none; transition: all .12s;
  }
  .exp-card:hover { border-color: var(--border-default); background: var(--interactive-hover); }
  .exp-card__head { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
  .exp-card__title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); line-height: 1.3; }
  .exp-card__status { font-size: calc(11px * var(--font-scale)); flex-shrink: 0; margin-top: 2px; }
  .exp-card__desc { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.4; }
  .exp-card__foot { display: flex; align-items: center; justify-content: space-between; margin-top: auto; }
  .exp-card__date { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .exp-card__role { font-size: calc(11px * var(--font-scale)); padding: 2px 8px; border-radius: 20px; }
  .role-admin  { background: rgba(29,158,117,0.12); color: var(--live-color); }
  .role-editor { background: rgba(56,138,221,0.12); color: #185FA5; }
  .role-viewer { background: var(--bg-elevated); color: var(--text-muted); }
  .exp-badge { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px; background: var(--bg-elevated); color: var(--text-muted); flex-shrink: 0; }

  .empty-state { display: flex; flex-direction: column; align-items: center; gap: calc(12px * var(--font-scale)); padding: calc(60px * var(--font-scale)) 0; text-align: center; }
  .empty-state__icon { color: var(--text-muted); opacity: 0.5; }
  .empty-state__title { font-size: calc(15px * var(--font-scale)); color: var(--text-secondary); }
  .empty-state__sub { font-size: calc(13px * var(--font-scale)); color: var(--text-muted); }

  .btn-link { background: none; border: none; cursor: pointer; color: var(--text-primary); text-decoration: underline; font-size: inherit; padding: 0; }

  .loading-row { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: calc(13px * var(--font-scale)); padding: 20px 0; }
  .spinner { width: 16px; height: 16px; border: 2px solid var(--border-subtle); border-top-color: var(--text-muted); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .error-banner { background: var(--error-bg); color: var(--error-color); padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale)); border-radius: 6px; font-size: calc(13px * var(--font-scale)); }

  @media (max-width: 640px) {
    .page { padding: 16px 12px; }
    .exp-grid { grid-template-columns: 1fr; }
    .page-head { flex-direction: column; }
  }
</style>
