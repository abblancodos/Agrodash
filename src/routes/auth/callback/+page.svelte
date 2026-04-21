<!-- src/routes/auth/callback/+page.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { auth } from '$lib/stores/auth';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let status = $state<'loading' | 'ok' | 'error'>('loading');
  let errorMsg = $state('');

  onMount(async () => {
    const token = $page.url.searchParams.get('token');

    if (!token) {
      status = 'error';
      errorMsg = 'No se recibió token';
      return;
    }

    try {
      const res = await fetch(`${API}/api/v1/auth/me`, {
        headers: { Authorization: `Bearer ${token}` },
      });

      if (!res.ok) {
        status = 'error';
        errorMsg = `Error ${res.status} validando sesión`;
        return;
      }

      const user = await res.json();
      auth.setToken(token, user);
      status = 'ok';
      goto('/experiments');
    } catch (e: any) {
      status = 'error';
      errorMsg = e.message ?? 'Error de red';
    }
  });
</script>

<div class="callback-shell">
  {#if status === 'loading'}
    <div class="spinner"></div>
    <p>Iniciando sesión...</p>
  {:else if status === 'ok'}
    <p>Sesión iniciada, redirigiendo...</p>
  {:else}
    <p class="error">Error: {errorMsg}</p>
    <a href="/experiments">← volver a experimentos</a>
  {/if}
</div>

<style>
  .callback-shell {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    min-height: 60vh; gap: 16px;
    font-size: 14px; color: var(--text-secondary);
  }
  .spinner {
    width: 24px; height: 24px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--text-muted);
    border-radius: 50%;
    animation: spin .8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error { color: var(--error-color); }
  a { color: var(--text-primary); font-size: 13px; }
</style>