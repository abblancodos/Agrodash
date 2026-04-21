<!-- src/routes/auth/callback/+page.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/stores/auth';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let status = $state<'loading' | 'ok' | 'error'>('loading');
  let errorMsg = $state('');
  let debugInfo = $state('');

  onMount(async () => {
    // Leer token directamente de window.location — más confiable en SPA
    const params = new URLSearchParams(window.location.search);
    const token = params.get('token');

    debugInfo = `search: ${window.location.search} | hash: ${window.location.hash}`;

    if (!token) {
      // Intentar también desde el hash por si acaso
      const hashParams = new URLSearchParams(window.location.hash.replace('#', '').replace('/auth/callback?', ''));
      const hashToken = hashParams.get('token') ?? window.location.hash.split('token=')[1]?.split('&')[0];

      if (!hashToken) {
        status = 'error';
        errorMsg = `No se recibió token. Debug: ${debugInfo}`;
        return;
      }

      await doLogin(hashToken);
      return;
    }

    await doLogin(token);
  });

  async function doLogin(token: string) {
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
  }
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
  .error { color: var(--error-color); font-family: monospace; font-size: 12px; max-width: 500px; text-align: center; word-break: break-all; }
  a { color: var(--text-primary); font-size: 13px; }
</style>