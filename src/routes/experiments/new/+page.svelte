<!-- src/routes/experiments/new/+page.svelte -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { auth, isLoggedIn } from '$lib/stores/auth';
  import { onMount } from 'svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let title       = $state('');
  let description = $state('');
  let isPublic    = $state(false);
  let loading     = $state(false);
  let error       = $state('');

  onMount(async () => {
    await auth.init();
    if (!$isLoggedIn) goto('/experiments');
  });

  async function create() {
    if (!title.trim()) { error = 'El título es requerido'; return; }
    loading = true; error = '';
    try {
      const res = await fetch(`${API}/api/v1/experiments`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          title: title.trim(),
          description: description.trim() || null,
          public: isPublic,
          constants: {},
        }),
      });
      const data = await res.json();
      if (!res.ok) { error = data.error ?? 'Error al crear'; return; }
      goto(`/experiments/${data.id}`);
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <a href="/experiments" class="back">← experimentos</a>
    <h1 class="page-title">nuevo experimento</h1>
  </div>

  <div class="form-card">
    {#if error}
      <div class="error-box">{error}</div>
    {/if}

    <div class="field">
      <label class="field-label" for="exp-title">título <span class="req">*</span></label>
      <input id="exp-title" class="field-input" bind:value={title}
             placeholder="ej: Retención de agua — Maceta FA #1" />
    </div>

    <div class="field">
      <label class="field-label" for="exp-desc">descripción <span class="muted">(opcional)</span></label>
      <textarea id="exp-desc" class="field-input" rows="3" bind:value={description}
                placeholder="Breve descripción del experimento y sus objetivos"></textarea>
    </div>

    <label class="toggle-row">
      <input type="checkbox" bind:checked={isPublic} />
      <span class="toggle-label">experimento público</span>
      <span class="toggle-hint">cualquier persona puede ver los datos sin necesidad de login</span>
    </label>

    <div class="form-foot">
      <a href="/experiments" class="btn-cancel">cancelar</a>
      <button class="btn-create" disabled={loading} onclick={create}>
        {loading ? 'creando...' : 'crear experimento'}
      </button>
    </div>
  </div>
</div>

<style>
  .page { max-width: 560px; margin: 0 auto; padding: calc(32px * var(--font-scale)) calc(20px * var(--font-scale)); }
  .page-head { margin-bottom: calc(24px * var(--font-scale)); }
  .back { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-decoration: none; display: block; margin-bottom: calc(8px * var(--font-scale)); }
  .back:hover { color: var(--text-primary); }
  .page-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }

  .form-card {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-subtle);
    border-radius: 12px;
    padding: calc(24px * var(--font-scale));
    display: flex; flex-direction: column; gap: calc(16px * var(--font-scale));
  }
  .error-box { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 10px 14px; font-size: calc(13px * var(--font-scale)); }
  .field { display: flex; flex-direction: column; gap: calc(5px * var(--font-scale)); }
  .field-label { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .req { color: #A32D2D; }
  .muted { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }
  .field-input {
    padding: calc(9px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 8px;
    font-size: calc(14px * var(--font-scale)); background: var(--bg-surface);
    color: var(--text-primary); outline: none; font-family: inherit;
    transition: border-color .12s;
  }
  .field-input:focus { border-color: var(--text-primary); }
  textarea.field-input { resize: vertical; }

  .toggle-row { display: flex; align-items: flex-start; gap: calc(10px * var(--font-scale)); cursor: pointer; }
  .toggle-row input { margin-top: 2px; flex-shrink: 0; }
  .toggle-label { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); font-weight: 500; }
  .toggle-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-top: 2px; display: block; }

  .form-foot { display: flex; justify-content: flex-end; gap: calc(10px * var(--font-scale)); margin-top: calc(8px * var(--font-scale)); }
  .btn-cancel { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); text-decoration: none; padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; }
  .btn-create {
    padding: calc(9px * var(--font-scale)) calc(20px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    transition: opacity .12s;
  }
  .btn-create:hover:not(:disabled) { opacity: 0.85; }
  .btn-create:disabled { opacity: 0.5; cursor: not-allowed; }
</style><!-- src/routes/experiments/new/+page.svelte -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { auth, isLoggedIn } from '$lib/stores/auth';
  import { onMount } from 'svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let title       = $state('');
  let description = $state('');
  let isPublic    = $state(false);
  let loading     = $state(false);
  let error       = $state('');

  onMount(async () => {
    await auth.init();
    if (!$isLoggedIn) goto('/experiments');
  });

  async function create() {
    if (!title.trim()) { error = 'El título es requerido'; return; }
    loading = true; error = '';
    try {
      const res = await fetch(`${API}/api/v1/experiments`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          title: title.trim(),
          description: description.trim() || null,
          public: isPublic,
          constants: {},
        }),
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        const data = (() => { try { return JSON.parse(text); } catch { return {}; } })();
        error = data.error ?? `Error ${res.status}: ${text.slice(0, 100)}`;
        return;
      }
      const data = await res.json();
      goto(`/experiments/${data.id}`);
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <a href="/experiments" class="back">← experimentos</a>
    <h1 class="page-title">nuevo experimento</h1>
  </div>

  <div class="form-card">
    {#if error}
      <div class="error-box">{error}</div>
    {/if}

    <div class="field">
      <label class="field-label" for="exp-title">título <span class="req">*</span></label>
      <input id="exp-title" class="field-input" bind:value={title}
             placeholder="ej: Retención de agua — Maceta FA #1" />
    </div>

    <div class="field">
      <label class="field-label" for="exp-desc">descripción <span class="muted">(opcional)</span></label>
      <textarea id="exp-desc" class="field-input" rows="3" bind:value={description}
                placeholder="Breve descripción del experimento y sus objetivos"></textarea>
    </div>

    <label class="toggle-row">
      <input type="checkbox" bind:checked={isPublic} />
      <span class="toggle-label">experimento público</span>
      <span class="toggle-hint">cualquier persona puede ver los datos sin necesidad de login</span>
    </label>

    <div class="form-foot">
      <a href="/experiments" class="btn-cancel">cancelar</a>
      <button class="btn-create" disabled={loading} onclick={create}>
        {loading ? 'creando...' : 'crear experimento'}
      </button>
    </div>
  </div>
</div>

<style>
  .page { max-width: 560px; margin: 0 auto; padding: calc(32px * var(--font-scale)) calc(20px * var(--font-scale)); }
  .page-head { margin-bottom: calc(24px * var(--font-scale)); }
  .back { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); text-decoration: none; display: block; margin-bottom: calc(8px * var(--font-scale)); }
  .back:hover { color: var(--text-primary); }
  .page-title { font-size: calc(20px * var(--font-scale)); font-weight: 500; color: var(--text-primary); }

  .form-card {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-subtle);
    border-radius: 12px;
    padding: calc(24px * var(--font-scale));
    display: flex; flex-direction: column; gap: calc(16px * var(--font-scale));
  }
  .error-box { background: var(--error-bg); color: var(--error-color); border-radius: 6px; padding: 10px 14px; font-size: calc(13px * var(--font-scale)); }
  .field { display: flex; flex-direction: column; gap: calc(5px * var(--font-scale)); }
  .field-label { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); }
  .req { color: #A32D2D; }
  .muted { color: var(--text-muted); font-size: calc(11px * var(--font-scale)); }
  .field-input {
    padding: calc(9px * var(--font-scale)) calc(12px * var(--font-scale));
    border: 0.5px solid var(--border-default); border-radius: 8px;
    font-size: calc(14px * var(--font-scale)); background: var(--bg-surface);
    color: var(--text-primary); outline: none; font-family: inherit;
    transition: border-color .12s;
  }
  .field-input:focus { border-color: var(--text-primary); }
  textarea.field-input { resize: vertical; }

  .toggle-row { display: flex; align-items: flex-start; gap: calc(10px * var(--font-scale)); cursor: pointer; }
  .toggle-row input { margin-top: 2px; flex-shrink: 0; }
  .toggle-label { font-size: calc(13px * var(--font-scale)); color: var(--text-primary); font-weight: 500; }
  .toggle-hint { font-size: calc(12px * var(--font-scale)); color: var(--text-muted); margin-top: 2px; display: block; }

  .form-foot { display: flex; justify-content: flex-end; gap: calc(10px * var(--font-scale)); margin-top: calc(8px * var(--font-scale)); }
  .btn-cancel { font-size: calc(13px * var(--font-scale)); color: var(--text-secondary); text-decoration: none; padding: calc(8px * var(--font-scale)) calc(14px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; }
  .btn-create {
    padding: calc(9px * var(--font-scale)) calc(20px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 6px; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    transition: opacity .12s;
  }
  .btn-create:hover:not(:disabled) { opacity: 0.85; }
  .btn-create:disabled { opacity: 0.5; cursor: not-allowed; }
</style>